use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, patch, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::{
    Actor, ApiError, AppState, Body, Id, Path, Query, Result, db, files,
    sync::{self, Scope},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/messages", get(history).post(send))
        .route(
            "/rooms/{room_id}/messages/{message_id}",
            patch(edit).delete(remove),
        )
        .route(
            "/rooms/{room_id}/messages/{message_id}/reactions/me",
            put(react).delete(unreact),
        )
        .route(
            "/rooms/{room_id}/messages/{message_id}/reactions",
            get(reactions),
        )
        .route("/rooms/{room_id}/read-state", put(mark_read))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendMessage {
    client_id: String,
    content: String,
    #[serde(default)]
    upload_ids: Vec<Id>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditMessage {
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryQuery {
    before_sequence: Option<String>,
    after_sequence: Option<String>,
    through_sequence: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadState {
    last_read_sequence: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reaction {
    reaction_type: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReactionsQuery {
    cursor: Option<Id>,
    limit: Option<i64>,
    reaction_type: Option<String>,
}

fn text_room(access: &db::RoomAccess) -> Result<()> {
    if access.kind == "VOICE" {
        return Err(ApiError::not_found());
    }
    Ok(())
}

fn writable(access: &db::RoomAccess) -> Result<()> {
    text_room(access)?;
    if !access.writable {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

fn limit(value: Option<i64>) -> Result<i64> {
    let value = value.unwrap_or(50);
    if !(1..=100).contains(&value) {
        return Err(ApiError::bad("INVALID_LIMIT"));
    }
    Ok(value)
}

fn sequence(value: &str) -> Result<i64> {
    if value == "0" {
        return Ok(0);
    }
    value
        .parse::<Id>()
        .map(|id| id.0)
        .map_err(|_| ApiError::bad("INVALID_SEQUENCE"))
}

fn valid_content(content: &str, attachments: bool) -> Result<()> {
    if content.chars().count() > 4_000
        || content.contains('\0')
        || (content.trim().is_empty() && !attachments)
    {
        return Err(ApiError::bad("INVALID_CONTENT"));
    }
    Ok(())
}

const MESSAGE_VIEW: &str = "SELECT jsonb_build_object(
    'message_id',m.id::text,'room_id',m.room_id::text,'sender_id',m.sender_id::text,
    'client_id',m.client_id,'room_sequence',m.sequence::text,'content',m.content,
    'time_created',m.created_at,'time_edited',m.edited_at,
    'attachments',COALESCE((SELECT jsonb_agg(jsonb_build_object('attachment_id',a.id::text,
        'file_name',u.file_name,'mime_type',u.mime_type,'file_size_bytes',u.size_bytes) ORDER BY a.id)
        FROM message_attachments a JOIN file_uploads u ON u.id=a.upload_id WHERE a.message_id=m.id),'[]'),
    'reaction_counts',COALESCE((SELECT jsonb_object_agg(reaction_type,amount) FROM
        (SELECT reaction_type,count(*) amount FROM message_reactions WHERE message_id=m.id GROUP BY reaction_type) counts),'{}'),
    'my_reaction',(SELECT reaction_type FROM message_reactions WHERE message_id=m.id AND user_id=$1))
    FROM messages m";

pub async fn message_view(
    conn: &mut PgConnection,
    user: Id,
    room: Id,
    message: Id,
) -> Result<Value> {
    Ok(
        sqlx::query_scalar(&format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND m.id=$3"))
            .bind(user.0)
            .bind(room.0)
            .bind(message.0)
            .fetch_one(conn)
            .await?,
    )
}

async fn send(
    State(state): State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
    Body(input): Body<SendMessage>,
) -> Result<(StatusCode, Json<Value>)> {
    if input.client_id.trim().is_empty()
        || input.client_id.chars().count() > 128
        || input.client_id.contains('\0')
    {
        return Err(ApiError::bad("INVALID_CLIENT_ID"));
    }
    valid_content(&input.content, !input.upload_ids.is_empty())?;
    let distinct: std::collections::HashSet<_> = input.upload_ids.iter().collect();
    if input.upload_ids.len() > 5 || distinct.len() != input.upload_ids.len() {
        return Err(ApiError::bad("INVALID_UPLOADS"));
    }
    // Upload verification uses S3, so it must happen outside the room write lock.
    let verified = if input.upload_ids.is_empty() {
        Vec::new()
    } else {
        if let Some(value) = retry_existing(&state, &actor, room, &input.client_id).await? {
            return Ok((StatusCode::OK, Json(value)));
        }
        match files::verify_uploads(&state, actor.user_id, room, &input.upload_ids).await {
            Ok(uploads) => uploads,
            Err(error) => {
                if let Some(value) = retry_existing(&state, &actor, room, &input.client_id).await? {
                    return Ok((StatusCode::OK, Json(value)));
                }
                return Err(error);
            }
        }
    };
    let mut tx = state.pool.begin().await?;
    writable(&db::lock_room_write(&mut tx, &actor, room).await?)?;
    if let Some(value) = existing(&mut tx, actor.user_id, room, &input.client_id).await? {
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(value)));
    }
    let id = state.id().await?;
    let sequence: i64 = sqlx::query_scalar(
        "UPDATE rooms SET last_sequence=last_sequence+1 WHERE id=$1 RETURNING last_sequence",
    )
    .bind(room.0)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO messages(id,room_id,sender_id,client_id,sequence,content) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id.0).bind(room.0).bind(actor.user_id.0).bind(input.client_id).bind(sequence).bind(input.content)
        .execute(&mut *tx).await?;
    files::attach_message(&state, &mut tx, actor.user_id, room, id, verified).await?;
    let view = message_view(&mut tx, actor.user_id, room, id).await?;
    sync::emit(
        &mut tx,
        Scope::Room(room),
        "MESSAGE_CREATE",
        json!({"room_id":room,"message_id":id}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn retry_existing(
    state: &AppState,
    actor: &Actor,
    room: Id,
    client_id: &str,
) -> Result<Option<Value>> {
    let mut tx = state.pool.begin().await?;
    writable(&db::lock_room_write(&mut tx, actor, room).await?)?;
    let previous = existing(&mut tx, actor.user_id, room, client_id).await?;
    tx.commit().await?;
    Ok(previous)
}

async fn existing(
    conn: &mut PgConnection,
    user: Id,
    room: Id,
    client_id: &str,
) -> Result<Option<Value>> {
    let existing = sqlx::query_scalar::<_, Id>(
        "SELECT id FROM messages WHERE room_id=$1 AND sender_id=$2 AND client_id=$3",
    )
    .bind(room.0)
    .bind(user.0)
    .bind(client_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = existing {
        return Ok(Some(message_view(conn, user, room, id).await?));
    }
    let deleted = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM deleted_message_keys WHERE room_id=$1 AND sender_id=$2 AND client_id=$3 AND expires_at>now())")
        .bind(room.0).bind(user.0).bind(client_id).fetch_one(conn).await?;
    if deleted {
        return Err(ApiError::conflict("MESSAGE_DELETED"));
    }
    Ok(None)
}

async fn history(
    State(state): State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>> {
    let before = query.before_sequence.as_deref().map(sequence).transpose()?;
    let after = query.after_sequence.as_deref().map(sequence).transpose()?;
    let through = query
        .through_sequence
        .as_deref()
        .map(sequence)
        .transpose()?;
    if (before.is_some() && after.is_some()) || (through.is_some() && after.is_none()) {
        return Err(ApiError::bad("INVALID_CURSOR"));
    }
    let limit = limit(query.limit)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let access = db::room_access(&mut tx, actor.user_id, room).await?;
    text_room(&access)?;
    let high = through.unwrap_or(access.last_sequence);
    if high > access.last_sequence || after.is_some_and(|cursor| cursor > high) {
        return Err(ApiError::bad("INVALID_CURSOR"));
    }
    let mut items: Vec<Value> = if let Some(cursor) = after {
        sqlx::query_scalar(&format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND m.sequence>$3 AND m.sequence<=$4 ORDER BY m.sequence LIMIT $5"))
            .bind(actor.user_id.0).bind(room.0).bind(cursor).bind(high).bind(limit + 1).fetch_all(&mut *tx).await?
    } else {
        sqlx::query_scalar(&format!("{MESSAGE_VIEW} WHERE m.room_id=$2 AND ($3::bigint IS NULL OR m.sequence<$3) ORDER BY m.sequence DESC LIMIT $4"))
            .bind(actor.user_id.0).bind(room.0).bind(before).bind(limit + 1).fetch_all(&mut *tx).await?
    };
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let cursor = if after.is_some() && !has_more {
        json!(high.to_string())
    } else {
        items
            .last()
            .map(|item| item["room_sequence"].clone())
            .unwrap_or(Value::Null)
    };
    let viewers: Vec<i64> = db::room_viewers(&mut tx, room)
        .await?
        .into_iter()
        .map(|id| id.0)
        .collect();
    let read_state: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('user_id',user_id::text,'last_read_sequence',last_sequence::text,'last_read_at',read_at)
         FROM message_reads WHERE room_id=$1 AND user_id=ANY($2) ORDER BY user_id",
    ).bind(room.0).bind(viewers).fetch_all(&mut *tx).await?;
    let revision = sync::revision(&mut tx, &format!("room:{room}")).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items":items,"last_message_sequence":access.last_sequence.to_string(),
        "through_sequence":high.to_string(),"next_cursor":cursor,"has_more":has_more,"read_state":read_state,"revision":revision.to_string()}),
    ))
}

async fn edit(
    State(state): State<AppState>,
    actor: Actor,
    Path((room, message)): Path<(Id, Id)>,
    Body(input): Body<EditMessage>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    writable(&db::lock_room_write(&mut tx, &actor, room).await?)?;
    let author = message_author(&mut tx, room, message).await?;
    if author != actor.user_id {
        return Err(ApiError::forbidden());
    }
    let has_upload =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM message_attachments WHERE message_id=$1)")
            .bind(message.0)
            .fetch_one(&mut *tx)
            .await?;
    valid_content(&input.content, has_upload)?;
    let changed =
        sqlx::query("UPDATE messages SET content=$2,edited_at=now() WHERE id=$1 AND content<>$2")
            .bind(message.0)
            .bind(input.content)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            > 0;
    let view = message_view(&mut tx, actor.user_id, room, message).await?;
    if changed {
        sync::emit(
            &mut tx,
            Scope::Room(room),
            "MESSAGE_UPDATE",
            json!({"room_id":room,"message_id":message}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(view))
}

async fn message_author(conn: &mut PgConnection, room: Id, message: Id) -> Result<Id> {
    Ok(
        sqlx::query_scalar("SELECT sender_id FROM messages WHERE room_id=$1 AND id=$2")
            .bind(room.0)
            .bind(message.0)
            .fetch_one(conn)
            .await?,
    )
}

async fn remove(
    State(state): State<AppState>,
    actor: Actor,
    Path((room, message)): Path<(Id, Id)>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let access = db::lock_room_write(&mut tx, &actor, room).await?;
    writable(&access)?;
    let (author, client_id, sequence): (Id, String, i64) = sqlx::query_as(
        "SELECT sender_id,client_id,sequence FROM messages WHERE room_id=$1 AND id=$2",
    )
    .bind(room.0)
    .bind(message.0)
    .fetch_one(&mut *tx)
    .await?;
    if author != actor.user_id && !access.moderate {
        return Err(ApiError::forbidden());
    }
    files::discard_message(&mut tx, message).await?;
    sqlx::query("INSERT INTO deleted_message_keys(room_id,sender_id,client_id,expires_at) VALUES ($1,$2,$3,now()+interval '24 hours')
                 ON CONFLICT(room_id,sender_id,client_id) DO UPDATE SET expires_at=excluded.expires_at")
        .bind(room.0).bind(author.0).bind(client_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM messages WHERE id=$1")
        .bind(message.0)
        .execute(&mut *tx)
        .await?;
    sync::emit(
        &mut tx,
        Scope::Room(room),
        "MESSAGE_DELETE",
        json!({"room_id":room,"message_id":message,"room_sequence":sequence.to_string()}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn valid_reaction(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 32 || value.chars().any(char::is_control)
    {
        return Err(ApiError::bad("INVALID_REACTION"));
    }
    Ok(())
}

async fn react(
    State(state): State<AppState>,
    actor: Actor,
    Path((room, message)): Path<(Id, Id)>,
    Body(input): Body<Reaction>,
) -> Result<StatusCode> {
    valid_reaction(&input.reaction_type)?;
    change_reaction(&state, &actor, room, message, Some(input.reaction_type)).await
}

async fn unreact(
    State(state): State<AppState>,
    actor: Actor,
    Path((room, message)): Path<(Id, Id)>,
) -> Result<StatusCode> {
    change_reaction(&state, &actor, room, message, None).await
}

async fn change_reaction(
    state: &AppState,
    actor: &Actor,
    room: Id,
    message: Id,
    reaction: Option<String>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    writable(&db::lock_room_write(&mut tx, actor, room).await?)?;
    message_author(&mut tx, room, message).await?;
    let changed = if let Some(value) = reaction {
        sqlx::query("INSERT INTO message_reactions(message_id,user_id,reaction_type) VALUES($1,$2,$3)
                     ON CONFLICT(message_id,user_id) DO UPDATE SET reaction_type=excluded.reaction_type
                     WHERE message_reactions.reaction_type<>excluded.reaction_type")
            .bind(message.0).bind(actor.user_id.0).bind(value).execute(&mut *tx).await?
    } else {
        sqlx::query("DELETE FROM message_reactions WHERE message_id=$1 AND user_id=$2")
            .bind(message.0)
            .bind(actor.user_id.0)
            .execute(&mut *tx)
            .await?
    };
    if changed.rows_affected() > 0 {
        sync::emit(
            &mut tx,
            Scope::Room(room),
            "MESSAGE_REACTION_UPDATE",
            json!({"room_id":room,"message_id":message}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reactions(
    State(state): State<AppState>,
    actor: Actor,
    Path((room, message)): Path<(Id, Id)>,
    Query(query): Query<ReactionsQuery>,
) -> Result<Json<Value>> {
    if let Some(value) = &query.reaction_type {
        valid_reaction(value)?;
    }
    let limit = limit(query.limit)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    text_room(&db::room_access(&mut tx, actor.user_id, room).await?)?;
    message_author(&mut tx, room, message).await?;
    let mut rows: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('user_id',u.id::text,'username',u.username,
        'full_name',d.full_name,'avatar_url',CASE WHEN d.avatar_upload_id IS NOT NULL THEN '/api/v1/images/'||d.avatar_upload_id||'/url' END,
        'reaction_type',r.reaction_type) FROM message_reactions r JOIN users u ON u.id=r.user_id AND u.is_available
        JOIN user_details d ON d.user_id=u.id WHERE r.message_id=$1 AND u.id>$2 AND ($3::text IS NULL OR r.reaction_type=$3)
        ORDER BY u.id LIMIT $4")
        .bind(message.0).bind(query.cursor.map_or(0, |id| id.0)).bind(query.reaction_type).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let cursor = more.then(|| rows.last().expect("a page has at least one row")["user_id"].clone());
    Ok(Json(
        json!({"items":rows,"next_cursor":cursor,"has_more":more}),
    ))
}

async fn mark_read(
    State(state): State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
    Body(input): Body<ReadState>,
) -> Result<Json<Value>> {
    let wanted = sequence(&input.last_read_sequence)?;
    let mut tx = state.pool.begin().await?;
    let access = db::lock_room_write(&mut tx, &actor, room).await?;
    text_room(&access)?;
    if wanted > access.last_sequence {
        return Err(ApiError::bad("INVALID_SEQUENCE"));
    }
    let updated: Option<(i64,Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "INSERT INTO message_reads(room_id,user_id,last_sequence,read_at) VALUES ($1,$2,$3,CASE WHEN $3>0 THEN now() END)
         ON CONFLICT(room_id,user_id) DO UPDATE SET last_sequence=excluded.last_sequence,read_at=excluded.read_at
         WHERE message_reads.last_sequence<excluded.last_sequence RETURNING last_sequence,read_at",
    ).bind(room.0).bind(actor.user_id.0).bind(wanted).fetch_optional(&mut *tx).await?;
    let changed = updated.is_some() && wanted > 0;
    let (sequence, read_at) =
        match updated {
            Some(row) => row,
            None => sqlx::query_as(
                "SELECT last_sequence,read_at FROM message_reads WHERE room_id=$1 AND user_id=$2",
            )
            .bind(room.0)
            .bind(actor.user_id.0)
            .fetch_one(&mut *tx)
            .await?,
        };
    let view = json!({"room_id":room,"user_id":actor.user_id,"last_read_sequence":sequence.to_string(),"last_read_at":read_at});
    if changed {
        sync::emit(
            &mut tx,
            Scope::Room(room),
            "MESSAGE_READ_UPDATE",
            view.clone(),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(view))
}
