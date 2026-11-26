use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_postgres::{Client, GenericClient, Row, Transaction, types::Type};
use unicode_normalization::UnicodeNormalization;
use worker::{Method, Request, Response};

use crate::{
    auth::{self, ApiError},
    db,
    events::{self, Id, Scope},
};

type Result<T> = std::result::Result<T, ApiError>;

pub(crate) enum Route {
    Search,
    Request,
    Resolve(i64),
    Remove(i64),
    Block,
    Unblock(i64),
    Dm,
}

pub(crate) fn route(req: &Request) -> Result<Option<Route>> {
    let path = req.path();
    let Some(path) = path.strip_prefix("/api/v1/") else {
        return Ok(None);
    };
    let id = |raw: &str| {
        Id::try_from(raw.to_owned())
            .map(|id| id.0)
            .map_err(|_| bad("INVALID_PATH"))
    };
    Ok(Some(
        match (req.method(), path.split('/').collect::<Vec<_>>().as_slice()) {
            (Method::Get, ["users", "search"]) => Route::Search,
            (Method::Post, ["friends", "requests"]) => Route::Request,
            (Method::Patch, ["friends", "requests", req]) => Route::Resolve(id(req)?),
            (Method::Delete, ["friends", user]) => Route::Remove(id(user)?),
            (Method::Post, ["blocks"]) => Route::Block,
            (Method::Delete, ["blocks", user]) => Route::Unblock(id(user)?),
            (Method::Post, ["dms"]) => Route::Dm,
            _ => return Ok(None),
        },
    ))
}

#[derive(Clone, Copy)]
struct Pair {
    lo: i64,
    hi: i64,
}

impl Pair {
    fn new(user: i64, target: i64) -> Result<Self> {
        if target == user {
            return Err(ApiError::new(
                400,
                "BAD_REQUEST",
                "Không thể thực hiện thao tác này với chính mình.",
            ));
        }
        Ok(Self {
            lo: user.min(target),
            hi: user.max(target),
        })
    }

    fn scope(self) -> Scope {
        Scope::Users(vec![self.lo, self.hi])
    }

    async fn lock(self, tx: &Transaction<'_>) -> Result<()> {
        db::lock_pair(tx, self.lo, self.hi).await
    }
}

fn bad(code: &'static str) -> ApiError {
    ApiError::new(400, code, "Yêu cầu không hợp lệ.")
}
fn conflict() -> ApiError {
    ApiError::new(
        409,
        "CONFLICT",
        "Không thể thực hiện thao tác này với tài khoản được chọn.",
    )
}
fn not_found() -> ApiError {
    ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên.")
}

fn respond(status: u16, data: Value) -> Result<Response> {
    Response::from_json(&data)
        .map(|response| response.with_status(status))
        .map_err(|_| ApiError::internal())
}

fn relation_view(row: &Row, pair: Pair, status: &str) -> Value {
    let requester: i64 = row.get("requester_id");
    json!({"request_id": Id(row.get("id")), "requester_id": Id(requester),
       "target_user_id": Id(if requester == pair.lo { pair.hi } else { pair.lo }),
       "status": status, "created_at": row.get::<_,DateTime<Utc>>("created_at")})
}

async fn can_relate<C: GenericClient + Sync>(client: &C, pair: Pair, target: i64) -> Result<bool> {
    let row = client
        .query_typed_one(
            "SELECT EXISTS(SELECT FROM users WHERE id=$3 AND is_available) AND NOT EXISTS
         (SELECT FROM user_blocks WHERE (blocker_id,blocked_id) IN (($1,$2),($2,$1)))",
            &[
                (&pair.lo, Type::INT8),
                (&pair.hi, Type::INT8),
                (&target, Type::INT8),
            ],
        )
        .await?;
    Ok(row.try_get(0)?)
}

async fn search(req: Request, tx: &Transaction<'_>, user: i64) -> Result<Response> {
    let url = req.url().map_err(|_| bad("INVALID_QUERY"))?;
    let mut term = None;
    let mut cursor = None;
    let mut limit = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "q" if term.is_none() => term = Some(value.trim().nfc().collect::<String>()),
            "cursor" if cursor.is_none() => {
                cursor = Some(
                    Id::try_from(value.into_owned())
                        .map_err(|_| bad("INVALID_QUERY"))?
                        .0,
                )
            }
            "limit" if limit.is_none() => {
                limit = Some(value.parse::<i64>().map_err(|_| bad("INVALID_QUERY"))?)
            }
            _ => return Err(bad("INVALID_QUERY")),
        }
    }
    let term = term.ok_or_else(|| bad("INVALID_QUERY"))?;
    if term.is_empty() || term.chars().count() > 100 || term.chars().any(char::is_control) {
        return Err(bad("BAD_REQUEST"));
    }
    let limit = limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(bad("BAD_REQUEST"));
    }
    let rows = tx.query_typed(
        "SELECT u.id, u.username, u.full_name, d.avatar_upload_id, CASE
           WHEN b.blocker_id IS NOT NULL THEN 'BLOCKED' WHEN r.status='FRIEND' THEN 'FRIEND'
           WHEN r.requester_id=$1 THEN 'PENDING_OUTGOING' WHEN r.requester_id IS NOT NULL THEN 'PENDING_INCOMING'
           ELSE 'NONE' END AS relationship
         FROM users u JOIN user_details d ON d.user_id=u.id
         LEFT JOIN relations r ON r.lo=least($1,u.id) AND r.hi=greatest($1,u.id)
         LEFT JOIN user_blocks b ON b.blocker_id=$1 AND b.blocked_id=u.id
         WHERE u.id<>$1 AND u.id>$2 AND u.is_available AND
          (strpos(identity_key(u.username),identity_key($3))>0 OR strpos(identity_key(u.full_name),identity_key($3))>0)
         ORDER BY u.id LIMIT $4",
        &[(&user, Type::INT8), (&cursor.unwrap_or(0), Type::INT8), (&term, Type::TEXT), (&(limit+1), Type::INT8)],
    ).await?;
    let has_more = rows.len() > limit as usize;
    let items: Vec<_> = rows.into_iter().take(limit as usize).map(|row| {
        let avatar: Option<i64> = row.get("avatar_upload_id");
        json!({"user_id": Id(row.get("id")), "username":row.get::<_,String>("username"),
          "full_name":row.get::<_,String>("full_name"), "avatar_url":avatar.map(|id|format!("/api/v1/images/{id}/url")),
          "relationship":row.get::<_,String>("relationship")})
    }).collect();
    let next_cursor = items
        .last()
        .filter(|_| has_more)
        .map(|item| &item["user_id"]);
    respond(
        200,
        json!({"items":items,"next_cursor":next_cursor,"has_more":has_more}),
    )
}

async fn request_friend(
    tx: &Transaction<'_>,
    user: i64,
    target: i64,
    pair: Pair,
) -> Result<Response> {
    if !can_relate(tx, pair, target).await? {
        return Err(conflict());
    }
    let id = db::snowflake(tx).await?;
    let row = tx
        .query_typed_opt(
            "INSERT INTO relations(id,lo,hi,requester_id,status) VALUES($1,$2,$3,$4,'PENDING')
         ON CONFLICT(lo,hi) DO NOTHING RETURNING id,requester_id,created_at",
            &[
                (&id, Type::INT8),
                (&pair.lo, Type::INT8),
                (&pair.hi, Type::INT8),
                (&user, Type::INT8),
            ],
        )
        .await?
        .ok_or_else(conflict)?;
    let view = relation_view(&row, pair, "PENDING");
    events::emit(tx, Scope::User(target), "FRIEND_REQUEST", view.clone()).await?;
    respond(201, view)
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Decision {
    Accept,
    Decline,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionInput {
    decision: Decision,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    target_user_id: Id,
}

async fn resolve(
    tx: &Transaction<'_>,
    user: i64,
    request: i64,
    input: DecisionInput,
) -> Result<Response> {
    let row = tx
        .query_typed_opt(
            "SELECT lo,hi FROM relations WHERE id=$1 AND $2 IN(lo,hi) AND requester_id<>$2",
            &[(&request, Type::INT8), (&user, Type::INT8)],
        )
        .await?
        .ok_or_else(not_found)?;
    let pair = Pair {
        lo: row.get(0),
        hi: row.get(1),
    };
    pair.lock(tx).await?;
    let row = tx
        .query_typed_opt(
            "SELECT id,requester_id,status,created_at FROM relations WHERE id=$1",
            &[(&request, Type::INT8)],
        )
        .await?
        .ok_or_else(conflict)?;
    let requester: i64 = row.get("requester_id");
    if row.get::<_, String>("status") != "PENDING" || !can_relate(tx, pair, requester).await? {
        return Err(conflict());
    }
    let (sql, status, kind) = match input.decision {
        Decision::Accept => (
            "UPDATE relations SET status='FRIEND' WHERE id=$1",
            "FRIEND",
            "FRIEND_ADD",
        ),
        Decision::Decline => (
            "DELETE FROM relations WHERE id=$1",
            "DECLINED",
            "FRIEND_REQUEST_UPDATE",
        ),
    };
    tx.query_typed(sql, &[(&request, Type::INT8)]).await?;
    let view = relation_view(&row, pair, status);
    events::emit(tx, pair.scope(), kind, view.clone()).await?;
    if matches!(input.decision, Decision::Accept) {
        dm_state(tx, pair, false).await?;
    }
    respond(200, view)
}

async fn dm_state(tx: &Transaction<'_>, pair: Pair, is_read_only: bool) -> Result<()> {
    if let Some(row) = tx
        .query_typed_opt(
            "SELECT room_id FROM dm_pairs WHERE lo=$1 AND hi=$2",
            &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8)],
        )
        .await?
    {
        let room = row.get(0);
        events::emit(
            tx,
            Scope::Room(room),
            "DM_STATE_UPDATE",
            json!({"room_id":Id(room),"is_read_only":is_read_only}),
        )
        .await?;
    }
    Ok(())
}

async fn remove_relation(tx: &Transaction<'_>, pair: Pair, only_friends: bool) -> Result<()> {
    let row = tx.query_typed_opt(
        "DELETE FROM relations WHERE lo=$1 AND hi=$2 AND (NOT $3 OR status='FRIEND') RETURNING id,requester_id,status,created_at",
        &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8), (&only_friends, Type::BOOL)],
    ).await?;
    if row.is_some() || !only_friends {
        let channels: Vec<i64> = tx.query_typed(
            "SELECT id FROM channels WHERE id IN (SELECT channel_id FROM channel_invitations WHERE status='PENDING'
             AND (inviter_id,invitee_id) IN (($1,$2),($2,$1))) ORDER BY id FOR UPDATE",
            &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8)],
        ).await?.iter().map(|row| row.get(0)).collect();
        if !channels.is_empty() {
            let invitations = tx.query_typed(
                "UPDATE channel_invitations SET status='CANCELLED',resolved_at=now()
                 WHERE status='PENDING' AND channel_id=ANY($3) AND (inviter_id,invitee_id) IN (($1,$2),($2,$1))
                 RETURNING jsonb_build_object('invitation_id',id::text,'channel_id',channel_id::text,
                  'inviter_id',inviter_id::text,'invitee_id',invitee_id::text,'status',status,'created_at',created_at,'resolved_at',resolved_at)",
                &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8), (&channels, Type::INT8_ARRAY)],
            ).await?;
            for invite in invitations {
                events::emit(tx, pair.scope(), "INVITE_UPDATE", invite.try_get(0)?).await?;
            }
        }
    }
    if let Some(row) = row {
        if row.get::<_, String>("status") == "FRIEND" {
            events::emit(
                tx,
                pair.scope(),
                "FRIEND_REMOVE",
                relation_view(&row, pair, "REMOVED"),
            )
            .await?;
            dm_state(tx, pair, true).await?;
        } else {
            events::emit(
                tx,
                pair.scope(),
                "FRIEND_REQUEST_UPDATE",
                relation_view(&row, pair, "CANCELLED"),
            )
            .await?;
        }
    }
    Ok(())
}

async fn set_block(
    tx: &Transaction<'_>,
    user: i64,
    target: i64,
    pair: Pair,
    enabled: bool,
) -> Result<()> {
    if enabled
        && tx
            .query_typed_opt("SELECT id FROM users WHERE id=$1", &[(&target, Type::INT8)])
            .await?
            .is_none()
    {
        return Err(not_found());
    }
    let sql = if enabled {
        "INSERT INTO user_blocks(blocker_id,blocked_id) VALUES($1,$2) ON CONFLICT DO NOTHING RETURNING blocked_id"
    } else {
        "DELETE FROM user_blocks WHERE blocker_id=$1 AND blocked_id=$2 RETURNING blocked_id"
    };
    if tx
        .query_typed_opt(sql, &[(&user, Type::INT8), (&target, Type::INT8)])
        .await?
        .is_some()
    {
        if enabled {
            remove_relation(tx, pair, false).await?;
        }
        events::emit(
            tx,
            Scope::User(user),
            "BLOCK_LIST_UPDATE",
            json!({"target_user_id":Id(target),"is_blocked":enabled}),
        )
        .await?;
    }
    Ok(())
}

async fn open_dm(tx: &Transaction<'_>, pair: Pair, target: i64) -> Result<Response> {
    if !can_relate(tx, pair, target).await?
        || tx
            .query_typed_opt(
                "SELECT id FROM relations WHERE lo=$1 AND hi=$2 AND status='FRIEND'",
                &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8)],
            )
            .await?
            .is_none()
    {
        return Err(conflict());
    }
    let (room, sequence, status) = if let Some(row) = tx.query_typed_opt(
        "SELECT r.id,r.last_sequence FROM dm_pairs p JOIN rooms r ON r.id=p.room_id WHERE p.lo=$1 AND p.hi=$2 AND r.is_available",
        &[(&pair.lo, Type::INT8), (&pair.hi, Type::INT8)],
    ).await? {
        (row.get(0),row.get(1),200)
    } else {
        let room = db::snowflake(tx).await?;
        tx.query_typed("INSERT INTO rooms(id,kind,private) VALUES($1,'DM',true)", &[(&room, Type::INT8)]).await?;
        let params = [(&room as &(dyn tokio_postgres::types::ToSql + Sync), Type::INT8), (&pair.lo, Type::INT8), (&pair.hi, Type::INT8)];
        tx.query_typed("INSERT INTO dm_pairs(room_id,lo,hi) VALUES($1,$2,$3)", &params).await?;
        tx.query_typed("INSERT INTO room_members(room_id,user_id) VALUES($1,$2),($1,$3)", &params).await?;
        tx.query_typed("INSERT INTO message_reads(room_id,user_id) VALUES($1,$2),($1,$3)", &params).await?;
        (room,0_i64,201)
    };
    let view = json!({"room_id":Id(room),"channel_id":null,"room_name":"Tin nhắn riêng","type":"DM","is_private":true,
        "member_ids":[Id(pair.lo),Id(pair.hi)],"is_read_only":false,"last_message_sequence":sequence.to_string()});
    if status == 201 {
        events::emit(tx, Scope::Room(room), "ROOM_CREATE", view.clone()).await?;
    }
    respond(status, view)
}

pub(crate) async fn handle(
    route: Route,
    req: Request,
    client: &mut Client,
    key: &[u8],
) -> Result<Response> {
    let actor = auth::actor(&req, key)?;
    let user = actor.user_id;
    let read_only = matches!(route, Route::Search);
    let tx = client
        .build_transaction()
        .read_only(read_only)
        .start()
        .await?;
    if read_only {
        auth::validate_session(&tx, &actor).await?;
    } else {
        auth::lock_session(&tx, &actor, false).await?;
    }
    let no_content = || {
        Response::empty()
            .map(|response| response.with_status(204))
            .map_err(|_| ApiError::internal())
    };
    let response = match route {
        Route::Search => search(req, &tx, user).await?,
        Route::Resolve(request) => resolve(&tx, user, request, auth::json_body(req).await?).await?,
        Route::Remove(target) | Route::Unblock(target) => {
            let pair = Pair::new(user, target)?;
            pair.lock(&tx).await?;
            if matches!(route, Route::Remove(_)) {
                remove_relation(&tx, pair, true).await?;
            } else {
                set_block(&tx, user, target, pair, false).await?;
            }
            no_content()?
        }
        Route::Request | Route::Dm | Route::Block => {
            let target = auth::json_body::<Target>(req).await?.target_user_id.0;
            let pair = Pair::new(user, target)?;
            pair.lock(&tx).await?;
            match route {
                Route::Request => request_friend(&tx, user, target, pair).await?,
                Route::Dm => open_dm(&tx, pair, target).await?,
                _ => {
                    set_block(&tx, user, target, pair, true).await?;
                    no_content()?
                }
            }
        }
    };
    tx.commit().await?;
    Ok(response)
}
