use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};
use unicode_normalization::UnicodeNormalization;

use crate::{
    Actor, ApiError, AppState, Body, Id, Path, Query, Result, db, files,
    sync::{self, Scope},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users/search", get(search))
        .route("/friends/requests", post(request_friend))
        .route("/friends/requests/{request_id}", patch(resolve_request))
        .route("/friends/{target_user_id}", delete(remove_friend))
        .route("/blocks", post(block))
        .route("/blocks/{target_user_id}", delete(unblock))
        .route("/dms", post(open_dm))
}

#[derive(Clone, Copy)]
struct Pair {
    lo: Id,
    hi: Id,
}

impl Pair {
    fn new(actor: Id, target: Id) -> Result<Self> {
        if actor == target {
            return Err(ApiError::bad(
                "Không thể thực hiện thao tác này với chính mình.",
            ));
        }
        let (lo, hi) = if actor.0 < target.0 {
            (actor, target)
        } else {
            (target, actor)
        };
        Ok(Self { lo, hi })
    }

    fn scope(self) -> Scope {
        Scope::Users(vec![self.lo, self.hi])
    }

    fn peer(self, user: Id) -> Id {
        if user == self.lo { self.hi } else { self.lo }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    q: String,
    cursor: Option<Id>,
    limit: Option<u32>,
}

#[derive(FromRow)]
struct PublicProfile {
    id: i64,
    username: String,
    full_name: String,
    avatar_upload_id: Option<i64>,
    relationship: String,
}

async fn search(
    State(state): State<AppState>,
    actor: Actor,
    Query(input): Query<Search>,
) -> Result<Json<Value>> {
    let term: String = input.q.trim().nfc().collect();
    if term.is_empty() || term.chars().count() > 100 || term.chars().any(char::is_control) {
        return Err(ApiError::bad("Từ khóa tìm kiếm không hợp lệ."));
    }
    let limit = input.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(ApiError::bad("Số kết quả phải từ 1 đến 50."));
    }
    let mut rows = sqlx::query_as::<_, PublicProfile>(
        "SELECT u.id, u.username, d.full_name, d.avatar_upload_id, CASE \
            WHEN b.blocker_id IS NOT NULL THEN 'BLOCKED' \
            WHEN r.status = 'FRIEND' THEN 'FRIEND' \
            WHEN r.requester_id = $1 THEN 'PENDING_OUTGOING' \
            WHEN r.requester_id IS NOT NULL THEN 'PENDING_INCOMING' \
            ELSE 'NONE' END AS relationship \
         FROM users u JOIN user_details d ON d.user_id = u.id \
         LEFT JOIN relations r ON r.lo = least($1, u.id) AND r.hi = greatest($1, u.id) \
         LEFT JOIN user_blocks b ON b.blocker_id = $1 AND b.blocked_id = u.id \
         WHERE u.id <> $1 AND u.id > $2 AND u.is_available \
            AND (strpos(identity_key(u.username), identity_key($3)) > 0 OR strpos(identity_key(d.full_name), identity_key($3)) > 0) \
         ORDER BY u.id LIMIT $4",
    )
    .bind(actor.user_id.0)
    .bind(input.cursor.map_or(0, |id| id.0))
    .bind(term)
    .bind(i64::from(limit) + 1)
    .fetch_all(&state.pool)
    .await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = rows.last().filter(|_| has_more).map(|row| Id(row.id));
    let items = rows
        .into_iter()
        .map(|row| {
            json!({
                "user_id": Id(row.id), "username": row.username, "full_name": row.full_name,
                "avatar_url": row.avatar_upload_id.map(|id| files::image_url(Id(id))),
                "relationship": row.relationship,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"items": items, "next_cursor": next_cursor, "has_more": has_more}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    target_user_id: Id,
}

#[derive(FromRow)]
struct Relation {
    id: i64,
    requester_id: i64,
    status: String,
    created_at: DateTime<Utc>,
}

impl Relation {
    fn view(&self, pair: Pair, status: &str) -> Value {
        let requester = Id(self.requester_id);
        json!({
            "request_id": Id(self.id), "requester_id": requester, "target_user_id": pair.peer(requester),
            "status": status, "created_at": self.created_at,
        })
    }
}

fn pair_conflict() -> ApiError {
    ApiError::conflict("Không thể thực hiện thao tác này với tài khoản được chọn.")
}

async fn can_relate(conn: &mut PgConnection, pair: Pair, peer: Id) -> Result<bool> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users WHERE id = $3 AND is_available) AND NOT EXISTS ( \
         SELECT 1 FROM user_blocks WHERE (blocker_id = $1 AND blocked_id = $2) OR (blocker_id = $2 AND blocked_id = $1))",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .bind(peer.0)
    .fetch_one(conn)
    .await?;
    Ok(allowed)
}

async fn request_friend(
    State(state): State<AppState>,
    actor: Actor,
    Body(input): Body<Target>,
) -> Result<(StatusCode, Json<Value>)> {
    let pair = Pair::new(actor.user_id, input.target_user_id)?;
    let id = state.id().await?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    if !can_relate(&mut tx, pair, input.target_user_id).await? {
        return Err(pair_conflict());
    }
    let request = sqlx::query_as::<_, Relation>(
        "INSERT INTO relations (id, lo, hi, requester_id, status) VALUES ($1, $2, $3, $4, 'PENDING') \
         ON CONFLICT (lo, hi) DO NOTHING RETURNING id, requester_id, status, created_at",
    )
    .bind(id.0)
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .bind(actor.user_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(pair_conflict)?;
    let view = request.view(pair, "PENDING");
    sync::emit(
        &mut tx,
        Scope::Users(vec![input.target_user_id]),
        "FRIEND_REQUEST",
        view.clone(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Decision {
    Accept,
    Decline,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveRequest {
    decision: Decision,
}

async fn resolve_request(
    State(state): State<AppState>,
    actor: Actor,
    Path(request_id): Path<Id>,
    Body(input): Body<ResolveRequest>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (lo, hi) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT lo, hi FROM relations WHERE id = $1 AND $2 IN (lo, hi) AND requester_id <> $2",
    )
    .bind(request_id.0)
    .bind(actor.user_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let pair = Pair {
        lo: Id(lo),
        hi: Id(hi),
    };
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    let request = sqlx::query_as::<_, Relation>(
        "SELECT id, requester_id, status, created_at FROM relations WHERE id = $1",
    )
    .bind(request_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(pair_conflict)?;
    if request.status != "PENDING" || !can_relate(&mut tx, pair, Id(request.requester_id)).await? {
        return Err(pair_conflict());
    }
    let response = match input.decision {
        Decision::Accept => {
            sqlx::query("UPDATE relations SET status = 'FRIEND' WHERE id = $1")
                .bind(request_id.0)
                .execute(&mut *tx)
                .await?;
            let view = request.view(pair, "FRIEND");
            sync::emit(&mut tx, pair.scope(), "FRIEND_ADD", view.clone()).await?;
            emit_dm_state(&mut tx, pair, false).await?;
            view
        }
        Decision::Decline => {
            sqlx::query("DELETE FROM relations WHERE id = $1")
                .bind(request_id.0)
                .execute(&mut *tx)
                .await?;
            let view = request.view(pair, "DECLINED");
            sync::emit(&mut tx, pair.scope(), "FRIEND_REQUEST_UPDATE", view.clone()).await?;
            view
        }
    };
    tx.commit().await?;
    Ok(Json(response))
}

async fn remove_friend(
    State(state): State<AppState>,
    actor: Actor,
    Path(target): Path<Id>,
) -> Result<StatusCode> {
    let pair = Pair::new(actor.user_id, target)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    let removed = sqlx::query_as::<_, Relation>(
        "DELETE FROM relations WHERE lo = $1 AND hi = $2 AND status = 'FRIEND' \
         RETURNING id, requester_id, status, created_at",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(relation) = removed {
        cancel_invitations(&mut tx, pair).await?;
        emit_removed_relation(&mut tx, pair, relation).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn block(
    State(state): State<AppState>,
    actor: Actor,
    Body(input): Body<Target>,
) -> Result<StatusCode> {
    let target = input.target_user_id;
    let pair = Pair::new(actor.user_id, target)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)")
        .bind(target.0)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(ApiError::not_found());
    }
    let inserted = sqlx::query(
        "INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(actor.user_id.0)
    .bind(target.0)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() > 0 {
        let removed = sqlx::query_as::<_, Relation>(
            "DELETE FROM relations WHERE lo = $1 AND hi = $2 RETURNING id, requester_id, status, created_at",
        )
        .bind(pair.lo.0)
        .bind(pair.hi.0)
        .fetch_optional(&mut *tx)
        .await?;
        cancel_invitations(&mut tx, pair).await?;
        if let Some(relation) = removed {
            emit_removed_relation(&mut tx, pair, relation).await?;
        }
        emit_block(&mut tx, actor.user_id, target, true).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unblock(
    State(state): State<AppState>,
    actor: Actor,
    Path(target): Path<Id>,
) -> Result<StatusCode> {
    let pair = Pair::new(actor.user_id, target)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    let deleted = sqlx::query("DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2")
        .bind(actor.user_id.0)
        .bind(target.0)
        .execute(&mut *tx)
        .await?;
    if deleted.rows_affected() > 0 {
        emit_block(&mut tx, actor.user_id, target, false).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn emit_block(
    conn: &mut PgConnection,
    actor: Id,
    target: Id,
    is_blocked: bool,
) -> Result<()> {
    sync::emit(
        conn,
        Scope::Users(vec![actor]),
        "BLOCK_LIST_UPDATE",
        json!({
            "target_user_id": target, "is_blocked": is_blocked,
        }),
    )
    .await
}

async fn emit_removed_relation(
    conn: &mut PgConnection,
    pair: Pair,
    relation: Relation,
) -> Result<()> {
    if relation.status == "FRIEND" {
        sync::emit(
            conn,
            pair.scope(),
            "FRIEND_REMOVE",
            relation.view(pair, "REMOVED"),
        )
        .await?;
        emit_dm_state(conn, pair, true).await?;
    } else {
        sync::emit(
            conn,
            pair.scope(),
            "FRIEND_REQUEST_UPDATE",
            relation.view(pair, "CANCELLED"),
        )
        .await?;
    }
    Ok(())
}

#[derive(FromRow)]
struct CancelledInvitation {
    id: i64,
    channel_id: i64,
    inviter_id: i64,
    invitee_id: i64,
    created_at: DateTime<Utc>,
    resolved_at: DateTime<Utc>,
}

async fn cancel_invitations(conn: &mut PgConnection, pair: Pair) -> Result<()> {
    let channels = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM channels WHERE id IN ( \
            SELECT channel_id FROM channel_invitations WHERE status = 'PENDING' AND \
            ((inviter_id = $1 AND invitee_id = $2) OR (inviter_id = $2 AND invitee_id = $1)) \
         ) ORDER BY id FOR UPDATE",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .fetch_all(&mut *conn)
    .await?;
    if channels.is_empty() {
        return Ok(());
    }
    let cancelled = sqlx::query_as::<_, CancelledInvitation>(
        "UPDATE channel_invitations SET status = 'CANCELLED', resolved_at = now() \
         WHERE status = 'PENDING' AND channel_id = ANY($3) AND \
         ((inviter_id = $1 AND invitee_id = $2) OR (inviter_id = $2 AND invitee_id = $1)) \
         RETURNING id, channel_id, inviter_id, invitee_id, created_at, resolved_at",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .bind(channels)
    .fetch_all(&mut *conn)
    .await?;
    for invitation in cancelled {
        sync::emit(conn, pair.scope(), "INVITE_UPDATE", json!({
            "invitation_id": Id(invitation.id), "channel_id": Id(invitation.channel_id),
            "inviter_id": Id(invitation.inviter_id), "invitee_id": Id(invitation.invitee_id),
            "created_at": invitation.created_at, "resolved_at": invitation.resolved_at, "status": "CANCELLED",
        })).await?;
    }
    Ok(())
}

async fn emit_dm_state(conn: &mut PgConnection, pair: Pair, is_read_only: bool) -> Result<()> {
    let id = sqlx::query_scalar::<_, i64>("SELECT room_id FROM dm_pairs WHERE lo = $1 AND hi = $2")
        .bind(pair.lo.0)
        .bind(pair.hi.0)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(id) = id {
        sync::emit(
            conn,
            Scope::Room(Id(id)),
            "DM_STATE_UPDATE",
            json!({
                "room_id": Id(id), "is_read_only": is_read_only,
            }),
        )
        .await?;
    }
    Ok(())
}

fn dm_view(pair: Pair, room_id: Id, room_name: &str, last_sequence: i64) -> Value {
    json!({
        "room_id": room_id, "channel_id": null, "room_name": room_name, "type": "DM", "is_private": true,
        "member_ids": [pair.lo, pair.hi], "is_read_only": false, "last_message_sequence": last_sequence.to_string(),
    })
}

async fn open_dm(
    State(state): State<AppState>,
    actor: Actor,
    Body(input): Body<Target>,
) -> Result<(StatusCode, Json<Value>)> {
    let pair = Pair::new(actor.user_id, input.target_user_id)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_pair(&mut tx, pair.lo, pair.hi).await?;
    let friends: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relations WHERE lo = $1 AND hi = $2 AND status = 'FRIEND')",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .fetch_one(&mut *tx)
    .await?;
    if !friends || !can_relate(&mut tx, pair, input.target_user_id).await? {
        return Err(pair_conflict());
    }
    let existing = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT r.id, r.name, r.last_sequence FROM dm_pairs p JOIN rooms r ON r.id = p.room_id \
         WHERE p.lo = $1 AND p.hi = $2 AND r.is_available",
    )
    .bind(pair.lo.0)
    .bind(pair.hi.0)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((id, name, last_sequence)) = existing {
        tx.commit().await?;
        return Ok((
            StatusCode::OK,
            Json(dm_view(pair, Id(id), &name, last_sequence)),
        ));
    }
    let room = state.id().await?;
    let room_name = "Tin nhắn riêng";
    sqlx::query(
        "INSERT INTO rooms (id, name, kind, private, position) VALUES ($1, $2, 'DM', true, 0)",
    )
    .bind(room.0)
    .bind(room_name)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO dm_pairs (lo, hi, room_id) VALUES ($1, $2, $3)")
        .bind(pair.lo.0)
        .bind(pair.hi.0)
        .bind(room.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO room_members (room_id, user_id) VALUES ($1, $2), ($1, $3)")
        .bind(room.0)
        .bind(pair.lo.0)
        .bind(pair.hi.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO message_reads (room_id, user_id) VALUES ($1, $2), ($1, $3)")
        .bind(room.0)
        .bind(pair.lo.0)
        .bind(pair.hi.0)
        .execute(&mut *tx)
        .await?;
    let view = dm_view(pair, room, room_name, 0);
    sync::emit(&mut tx, Scope::Room(room), "ROOM_CREATE", view.clone()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}
