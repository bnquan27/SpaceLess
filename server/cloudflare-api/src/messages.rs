use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_postgres::{Client, GenericClient, IsolationLevel, types::Type};
use worker::{Env, Method, Request, Response};

use crate::{
    auth::{self, Actor, ApiError},
    db, events, files,
};

type Result<T> = std::result::Result<T, ApiError>;

pub(crate) enum MessagesRoute {
    History(i64),
    Send(i64),
}

pub(crate) struct MessageResponse {
    pub response: Response,
    pub wake: bool,
}

fn bad(code: &'static str) -> ApiError {
    ApiError::new(400, code, "Yêu cầu không hợp lệ.")
}

fn not_found() -> ApiError {
    ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên.")
}

fn conflict(code: &'static str) -> ApiError {
    ApiError::new(409, code, "Trạng thái tài nguyên đã thay đổi.")
}

fn decimal(value: &str, zero: bool, code: &'static str) -> Result<i64> {
    let parsed = value.parse::<i64>().map_err(|_| bad(code))?;
    if parsed < 0 || (!zero && parsed == 0) || parsed.to_string() != value {
        return Err(bad(code));
    }
    Ok(parsed)
}

pub(crate) fn route(req: &Request) -> Result<Option<MessagesRoute>> {
    let path = req.path();
    let Some(room) = path
        .strip_prefix("/api/v1/rooms/")
        .and_then(|p| p.strip_suffix("/messages"))
    else {
        return Ok(None);
    };
    if room.contains('/') {
        return Ok(None);
    }
    match req.method() {
        Method::Get => Ok(Some(MessagesRoute::History(decimal(
            room,
            false,
            "INVALID_PATH",
        )?))),
        Method::Post => Ok(Some(MessagesRoute::Send(decimal(
            room,
            false,
            "INVALID_PATH",
        )?))),
        _ => Ok(None),
    }
}

struct RoomAccess {
    kind: String,
    channel_id: Option<i64>,
    last_sequence: i64,
    writable: bool,
}

async fn access<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    room: i64,
) -> Result<Option<RoomAccess>> {
    let row = client.query_typed(
        "SELECT r.kind, r.channel_id, r.last_sequence,
         CASE WHEN r.kind='DM' THEN EXISTS(SELECT 1 FROM relations x WHERE x.lo=p.lo AND x.hi=p.hi AND x.status='FRIEND')
           AND NOT EXISTS(SELECT 1 FROM user_blocks b WHERE (b.blocker_id,b.blocked_id) IN ((p.lo,p.hi),(p.hi,p.lo)))
           ELSE true END AS writable
         FROM rooms r
         LEFT JOIN channel_room_access access ON access.room_id=r.id AND access.user_id=$1
         LEFT JOIN dm_pairs p ON p.room_id=r.id
         WHERE r.id=$2 AND r.is_available AND
           ((r.kind='DM' AND $1 IN (p.lo,p.hi)) OR access.user_id IS NOT NULL)",
        &[(&user, Type::INT8), (&room, Type::INT8)],
    ).await?.pop();
    Ok(row.map(|row| RoomAccess {
        kind: row.get(0),
        channel_id: row.get(1),
        last_sequence: row.get(2),
        writable: row.get(3),
    }))
}

fn text_room(access: &RoomAccess) -> Result<()> {
    if access.kind == "VOICE" {
        return Err(not_found());
    }
    Ok(())
}

pub(crate) async fn lock_write<C: GenericClient + Sync>(
    client: &C,
    actor: &Actor,
    room: i64,
) -> Result<()> {
    auth::lock_session(client, actor, false).await?;
    let room_access = access(client, actor.user_id, room)
        .await?
        .ok_or_else(not_found)?;
    if let Some(channel) = room_access.channel_id {
        if client
            .query_typed(
                "SELECT id FROM channels WHERE id=$1 AND is_available FOR SHARE",
                &[(&channel, Type::INT8)],
            )
            .await?
            .is_empty()
        {
            return Err(not_found());
        }
    } else {
        let pair = client
            .query_typed(
                "SELECT lo,hi FROM dm_pairs WHERE room_id=$1",
                &[(&room, Type::INT8)],
            )
            .await?
            .pop()
            .ok_or_else(not_found)?;
        let lo: i64 = pair.get(0);
        let hi: i64 = pair.get(1);
        db::lock_pair(client, lo, hi).await?;
    }
    if client
        .query_typed(
            "SELECT id FROM rooms WHERE id=$1 AND is_available FOR UPDATE",
            &[(&room, Type::INT8)],
        )
        .await?
        .is_empty()
    {
        return Err(not_found());
    }
    let room_access = access(client, actor.user_id, room)
        .await?
        .ok_or_else(not_found)?;
    text_room(&room_access)?;
    if !room_access.writable {
        return Err(ApiError::new(
            403,
            "FORBIDDEN",
            "Bạn không có quyền thực hiện thao tác này.",
        ));
    }
    Ok(())
}

const MESSAGE_VIEW: &str = "SELECT jsonb_build_object(
    'message_id',m.id::text,'room_id',m.room_id::text,'sender_id',m.sender_id::text,
    'client_id',m.client_id,'room_sequence',m.sequence::text,'content',COALESCE(m.content,''),
    'time_created',m.created_at,'time_edited',m.edited_at,
    'attachments',COALESCE((SELECT jsonb_agg(jsonb_build_object('attachment_id',a.id::text,
        'file_name',u.file_name,'mime_type',u.mime_type,'file_size_bytes',u.size_bytes) ORDER BY a.id)
        FROM message_attachments a JOIN file_uploads u ON u.id=a.upload_id WHERE a.message_id=m.id),'[]'),
    'reaction_counts',COALESCE((SELECT jsonb_object_agg(reaction_type,amount) FROM
        (SELECT reaction_type,count(*) amount FROM message_reactions WHERE message_id=m.id GROUP BY reaction_type) counts),'{}'),
    'my_reaction',(SELECT reaction_type FROM message_reactions WHERE message_id=m.id AND user_id=$1))
    FROM messages m";

pub(crate) async fn message_view<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    room: i64,
    id: i64,
) -> Result<Option<Value>> {
    Ok(client
        .query_typed(
            &format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND m.id=$3"),
            &[(&user, Type::INT8), (&room, Type::INT8), (&id, Type::INT8)],
        )
        .await?
        .pop()
        .map(|row| row.get(0)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendMessage {
    client_id: String,
    content: Option<String>,
    #[serde(default)]
    upload_ids: Vec<String>,
}

fn validate_send(input: &SendMessage) -> Result<&str> {
    let client_len = input.client_id.chars().count();
    if input.client_id.trim().is_empty() || client_len > 128 || input.client_id.contains('\0') {
        return Err(bad("INVALID_CLIENT_ID"));
    }
    if input.upload_ids.len() > 5 {
        return Err(bad("INVALID_UPLOADS"));
    }
    let content = input.content.as_deref().unwrap_or_default();
    if content.chars().count() > 4_000
        || content.contains('\0')
        || (content.trim().is_empty() && input.upload_ids.is_empty())
    {
        return Err(bad("INVALID_CONTENT"));
    }
    Ok(content)
}

fn request_hash(content: &str, uploads: &[i64]) -> String {
    let mut hash = Sha256::new();
    hash.update((content.len() as i64).to_be_bytes());
    hash.update(content.as_bytes());
    for upload in uploads {
        hash.update(upload.to_be_bytes());
    }
    format!("{:x}", hash.finalize())
}

async fn existing<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    room: i64,
    client_id: &str,
    hash: &str,
) -> Result<Option<Value>> {
    let previous = client.query_typed(
        "SELECT r.request_hash, m.id FROM message_send_receipts r LEFT JOIN messages m ON m.id=r.message_id
         WHERE r.room_id=$1 AND r.sender_id=$2 AND r.client_id=$3",
        &[(&room, Type::INT8), (&user, Type::INT8), (&client_id, Type::TEXT)],
    ).await?.pop();
    let Some(previous) = previous else {
        return Ok(None);
    };
    let stored: Option<String> = previous.get(0);
    if stored.is_some_and(|stored| stored != hash) {
        return Err(conflict("CLIENT_ID_REUSED"));
    }
    let id: i64 = previous
        .get::<_, Option<i64>>(1)
        .ok_or_else(|| conflict("MESSAGE_DELETED"))?;
    message_view(client, user, room, id)
        .await?
        .ok_or_else(ApiError::internal)
        .map(Some)
}

async fn send(
    req: Request,
    client: &mut Client,
    actor: &Actor,
    room: i64,
    env: &Env,
) -> Result<MessageResponse> {
    let input: SendMessage = auth::json_body(req).await?;
    let content = validate_send(&input)?;
    let mut uploads = input
        .upload_ids
        .iter()
        .map(|id| decimal(id, false, "INVALID_UPLOADS"))
        .collect::<Result<Vec<_>>>()?;
    uploads.sort_unstable();
    if uploads.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err(bad("INVALID_UPLOADS"));
    }
    let hash = request_hash(content, &uploads);
    let verified = if uploads.is_empty() {
        Vec::new()
    } else {
        if let Some(response) = retry_existing(client, actor, room, &input.client_id, &hash).await?
        {
            return Ok(response);
        }
        if input.client_id.chars().count() > 64 {
            return Err(bad("INVALID_CLIENT_ID"));
        }
        match files::verify_uploads(env, client, actor.user_id, room, &uploads).await {
            Ok(verified) => verified,
            Err(error) => {
                if let Some(response) =
                    retry_existing(client, actor, room, &input.client_id, &hash).await?
                {
                    return Ok(response);
                }
                return Err(error);
            }
        }
    };
    let tx = client
        .build_transaction()
        .read_only(false)
        .isolation_level(IsolationLevel::ReadCommitted)
        .start()
        .await?;
    lock_write(&tx, actor, room).await?;
    if let Some(view) = existing(&tx, actor.user_id, room, &input.client_id, &hash).await? {
        let response = Response::from_json(&view).map_err(|_| ApiError::internal())?;
        tx.commit().await?;
        return Ok(MessageResponse {
            response,
            wake: false,
        });
    }
    if input.client_id.chars().count() > 64 {
        return Err(bad("INVALID_CLIENT_ID"));
    }
    if tx
        .query_typed(
            include_str!("../../src/sql/claim_message_send.sql"),
            &[(&actor.user_id, Type::INT8)],
        )
        .await?
        .is_empty()
    {
        return Err(ApiError::new(
            429,
            "MESSAGE_RATE_LIMITED",
            "NÀY BÌNH TĨNH, BẠN ĐANG GỬI TIN NHẮN QUÁ NHANH",
        )
        .with_retry_after(2));
    }
    let id = db::snowflake(&tx).await?;
    let row = tx
        .query_typed(
            "UPDATE rooms SET last_sequence=last_sequence+1 WHERE id=$1 RETURNING last_sequence",
            &[(&room, Type::INT8)],
        )
        .await?
        .pop()
        .ok_or_else(not_found)?;
    let sequence: i64 = row.get(0);
    tx.query_typed("INSERT INTO messages(id,room_id,sender_id,client_id,sequence,content) VALUES($1,$2,$3,$4,$5,$6)",
        &[(&id, Type::INT8), (&room, Type::INT8), (&actor.user_id, Type::INT8), (&input.client_id, Type::TEXT), (&sequence, Type::INT8), (&(!content.is_empty()).then_some(content), Type::TEXT)]).await?;
    tx.query_typed("INSERT INTO message_send_receipts(room_id,sender_id,client_id,message_id,request_hash) VALUES($1,$2,$3,$4,$5)",
        &[(&room, Type::INT8), (&actor.user_id, Type::INT8), (&input.client_id, Type::TEXT), (&id, Type::INT8), (&hash, Type::TEXT)]).await?;
    files::attach_message(&tx, actor.user_id, room, id, verified).await?;
    let view = message_view(&tx, actor.user_id, room, id)
        .await?
        .ok_or_else(ApiError::internal)?;
    events::emit(
        &tx,
        events::Scope::Room(room),
        "MESSAGE_CREATE",
        json!({"room_id": room.to_string(), "message_id": id.to_string()}),
    )
    .await?;
    let response = Response::from_json(&view)
        .map_err(|_| ApiError::internal())?
        .with_status(201);
    tx.commit().await?;
    Ok(MessageResponse {
        response,
        wake: true,
    })
}

async fn retry_existing(
    client: &mut Client,
    actor: &Actor,
    room: i64,
    client_id: &str,
    hash: &str,
) -> Result<Option<MessageResponse>> {
    let tx = client.transaction().await?;
    lock_write(&tx, actor, room).await?;
    let value = existing(&tx, actor.user_id, room, client_id, hash).await?;
    tx.commit().await?;
    value
        .map(|view| {
            Response::from_json(&view)
                .map(|response| MessageResponse {
                    response,
                    wake: false,
                })
                .map_err(|_| ApiError::internal())
        })
        .transpose()
}

#[derive(Default)]
struct HistoryQuery {
    before: Option<i64>,
    after: Option<i64>,
    through: Option<i64>,
    limit: Option<i64>,
}

fn history_query(req: &Request) -> Result<HistoryQuery> {
    let url = req.url().map_err(|_| bad("INVALID_QUERY"))?;
    let mut query = HistoryQuery::default();
    for (name, value) in url.query_pairs() {
        let field = match name.as_ref() {
            "before_sequence" => &mut query.before,
            "after_sequence" => &mut query.after,
            "through_sequence" => &mut query.through,
            "limit" => &mut query.limit,
            _ => return Err(bad("INVALID_QUERY")),
        };
        if field.is_some() {
            return Err(bad("INVALID_QUERY"));
        }
        *field = Some(if name == "limit" {
            value.parse().map_err(|_| bad("INVALID_QUERY"))?
        } else {
            decimal(&value, true, "INVALID_SEQUENCE")?
        });
    }
    if (query.before.is_some() && query.after.is_some())
        || (query.through.is_some() && query.after.is_none())
    {
        return Err(bad("INVALID_CURSOR"));
    }
    if !(1..=100).contains(&query.limit.unwrap_or(50)) {
        return Err(bad("INVALID_LIMIT"));
    }
    Ok(query)
}

pub(crate) async fn room_viewers<C: GenericClient + Sync>(
    client: &C,
    room: i64,
) -> Result<Vec<i64>> {
    Ok(client.query_typed(
        "SELECT user_id FROM channel_room_access WHERE room_id=$1
         UNION ALL SELECT u.id FROM rooms r JOIN dm_pairs p ON p.room_id=r.id
         JOIN users u ON u.id IN (p.lo,p.hi) AND u.is_available WHERE r.id=$1 AND r.is_available ORDER BY 1",
        &[(&room, Type::INT8)],
    ).await?.into_iter().map(|row| row.get(0)).collect())
}

async fn history(
    req: Request,
    client: &mut Client,
    actor: &Actor,
    room: i64,
) -> Result<MessageResponse> {
    let query = history_query(&req)?;
    let tx = client
        .build_transaction()
        .read_only(true)
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;
    auth::validate_session(&tx, actor).await?;
    let room_access = access(&tx, actor.user_id, room)
        .await?
        .ok_or_else(not_found)?;
    text_room(&room_access)?;
    let high = query.through.unwrap_or(room_access.last_sequence);
    if high > room_access.last_sequence || query.after.is_some_and(|after| after > high) {
        return Err(bad("INVALID_CURSOR"));
    }
    let limit = query.limit.unwrap_or(50);
    let fetch_limit = limit + 1;
    let rows = if let Some(after) = query.after {
        tx.query_typed(&format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND m.sequence>$3 AND m.sequence<=$4 ORDER BY m.sequence LIMIT $5"),
            &[(&actor.user_id, Type::INT8), (&room, Type::INT8), (&after, Type::INT8), (&high, Type::INT8), (&fetch_limit, Type::INT8)]).await?
    } else {
        tx.query_typed(&format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND ($3::bigint IS NULL OR m.sequence<$3) ORDER BY m.sequence DESC LIMIT $4"),
            &[(&actor.user_id, Type::INT8), (&room, Type::INT8), (&query.before, Type::INT8), (&fetch_limit, Type::INT8)]).await?
    };
    let has_more = rows.len() > limit as usize;
    let items: Vec<Value> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| row.get(0))
        .collect();
    let cursor = if query.after.is_some() && !has_more {
        json!(high.to_string())
    } else {
        items
            .last()
            .map(|item| item["room_sequence"].clone())
            .unwrap_or(Value::Null)
    };
    let viewers = room_viewers(&tx, room).await?;
    let read_state: Vec<Value> = tx.query_typed(
        "SELECT jsonb_build_object('user_id',user_id::text,'last_read_sequence',last_sequence::text,'last_read_at',read_at)
         FROM message_reads WHERE room_id=$1 AND user_id=ANY($2) ORDER BY user_id",
        &[(&room, Type::INT8), (&viewers, Type::INT8_ARRAY)],
    ).await?.into_iter().map(|row| row.get(0)).collect();
    let stream = format!("room:{room}");
    let revision = tx
        .query_typed(
            "SELECT revision FROM stream_revisions WHERE stream=$1",
            &[(&stream, Type::TEXT)],
        )
        .await?
        .pop()
        .map(|row| row.get::<_, i64>(0))
        .unwrap_or(0);
    let response = Response::from_json(
        &json!({"items": items, "last_message_sequence": room_access.last_sequence.to_string(),
        "through_sequence": high.to_string(), "next_cursor": cursor, "has_more": has_more,
        "read_state": read_state, "revision": revision.to_string()}),
    )
    .map_err(|_| ApiError::internal())?;
    tx.commit().await?;
    Ok(MessageResponse {
        response,
        wake: false,
    })
}

pub(crate) async fn handle(
    route: MessagesRoute,
    req: Request,
    client: &mut Client,
    secret: &[u8],
    env: &Env,
) -> Result<MessageResponse> {
    if secret.len() < 32 {
        return Err(ApiError::unavailable());
    }
    let actor = auth::actor(&req, secret)?;
    match route {
        MessagesRoute::Send(room) => send(req, client, &actor, room, env).await,
        MessagesRoute::History(room) => history(req, client, &actor, room).await,
    }
}

pub(crate) async fn lock_read<C: GenericClient + Sync>(
    client: &C,
    user_id: i64,
    room_id: i64,
) -> Result<bool> {
    let Some(room) = access(client, user_id, room_id).await? else {
        return Ok(false);
    };
    if let Some(channel) = room.channel_id {
        if client
            .query_typed(
                "SELECT id FROM channels WHERE id=$1 AND is_available FOR SHARE",
                &[(&channel, Type::INT8)],
            )
            .await?
            .is_empty()
        {
            return Ok(false);
        }
    }
    if client
        .query_typed(
            "SELECT id FROM rooms WHERE id=$1 AND is_available FOR SHARE",
            &[(&room_id, Type::INT8)],
        )
        .await?
        .is_empty()
    {
        return Ok(false);
    }
    Ok(access(client, user_id, room_id).await?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_hash_matches_independent_utf8_backfill_fixture() {
        assert_eq!(
            request_hash("Xin chào", &[]),
            "a8a1733e503bac7227dc7ef821e30c0b6808e16ab4522bdcb773cd82ffa75823"
        );
        assert_ne!(
            request_hash("Xin chào", &[]),
            request_hash("Xin chào ", &[])
        );
    }

    #[test]
    fn message_validation_preserves_content_and_accepts_attachment_only() {
        let mut message = SendMessage {
            client_id: "retry-1".into(),
            content: Some(" Xin chào\n".into()),
            upload_ids: Vec::new(),
        };
        assert_eq!(validate_send(&message).unwrap(), " Xin chào\n");
        message.upload_ids.push("9007199254740993".into());
        message.content = None;
        assert_eq!(validate_send(&message).unwrap(), "");
        message.upload_ids.clear();
        message.content = Some(" \n".into());
        assert!(validate_send(&message).is_err());
        message.content = Some("a\0b".into());
        assert!(validate_send(&message).is_err());
    }

    #[test]
    fn message_ids_are_canonical() {
        assert!(decimal("01", false, "INVALID_ID").is_err());
        assert!(decimal("9007199254740993", false, "INVALID_ID").is_ok());
    }
}
