use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, patch, post, put},
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{Actor, ApiError, AppState, Body, Id, Path, Result, db, files, sync, voice};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/channels", post(create_channel))
        .route(
            "/channels/{channel_id}",
            patch(update_channel).delete(delete_channel),
        )
        .route("/channels/{channel_id}/invitations", post(invite))
        .route("/invitations/{invitation_id}", patch(resolve_invitation))
        .route("/channel-links/{invite_code}", get(preview_link))
        .route("/channel-links/{invite_code}/join", post(join_link))
        .route(
            "/channels/{channel_id}/members/{user_id}",
            delete(remove_member),
        )
        .route(
            "/channels/{channel_id}/members/{user_id}/role",
            patch(update_role),
        )
        .route(
            "/channels/{channel_id}/transfer-ownership",
            post(transfer_ownership),
        )
        .route("/channels/{channel_id}/categories", post(create_category))
        .route(
            "/channels/{channel_id}/categories/{category_id}",
            patch(update_category).delete(delete_category),
        )
        .route(
            "/channels/{channel_id}/categories/order",
            put(order_categories),
        )
        .route("/channels/{channel_id}/rooms", post(create_room))
        .route(
            "/channels/{channel_id}/rooms/{room_id}",
            patch(update_room).delete(delete_room),
        )
        .route("/channels/{channel_id}/rooms/order", put(order_rooms))
}

// Serde's default Option handling treats null and a missing field alike.
fn supplied<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn name(input: &str) -> Result<String> {
    let name: String = input.trim().nfc().collect();
    if name.is_empty() || name.contains('\0') {
        return Err(ApiError::bad("INVALID_NAME"));
    }
    Ok(name)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChannelRole {
    Creator,
    Moderator,
    Member,
}

impl ChannelRole {
    fn manages_channel(self) -> bool {
        self != Self::Member
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum EditableRole {
    Moderator,
    Member,
}

impl EditableRole {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Moderator => "MODERATOR",
            Self::Member => "MEMBER",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum JoinMode {
    InviteOnly,
    PublicLink,
}

impl JoinMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::InviteOnly => "INVITE_ONLY",
            Self::PublicLink => "PUBLIC_LINK",
        }
    }
}

#[derive(FromRow)]
struct Channel {
    id: i64,
    creator_id: i64,
    name: String,
    description: Option<String>,
    join_mode: String,
    invite_code: String,
    is_available: bool,
    icon_upload_id: Option<i64>,
    background_upload_id: Option<i64>,
}

impl Channel {
    fn preview(&self) -> Value {
        json!({
            "channel_id": Id(self.id),
            "channel_name": self.name,
            "description": self.description,
            "icon_url": self.icon_upload_id.map(|id| files::image_url(Id(id))),
            "background_url": self.background_upload_id.map(|id| files::image_url(Id(id)))
        })
    }

    fn view(&self, manager: bool) -> Value {
        let mut view = self.preview();
        view["creator_id"] = json!(Id(self.creator_id));
        view["join_mode"] = json!(self.join_mode);
        if manager {
            view["invite_code"] = json!(self.invite_code);
        }
        view
    }
}

async fn locked_channel(conn: &mut PgConnection, channel: Id) -> Result<Channel> {
    sqlx::query_as("SELECT * FROM channels WHERE id = $1 FOR UPDATE")
        .bind(channel.0)
        .fetch_optional(conn)
        .await?
        .ok_or_else(ApiError::not_found)
}

async fn role(conn: &mut PgConnection, channel: Id, user: Id) -> Result<Option<ChannelRole>> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM channel_members WHERE channel_id = $1 AND user_id = $2",
    )
    .bind(channel.0)
    .bind(user.0)
    .fetch_optional(conn)
    .await?;
    match role.as_deref() {
        Some("CREATOR") => Ok(Some(ChannelRole::Creator)),
        Some("MODERATOR") => Ok(Some(ChannelRole::Moderator)),
        Some("MEMBER") => Ok(Some(ChannelRole::Member)),
        None => Ok(None),
        _ => Err(ApiError::internal()),
    }
}

async fn channel_member(
    conn: &mut PgConnection,
    channel: Id,
    user: Id,
) -> Result<(Channel, ChannelRole)> {
    let data = locked_channel(conn, channel).await?;
    if !data.is_available {
        return Err(ApiError::not_found());
    }
    let role = role(conn, channel, user)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok((data, role))
}

async fn channel_manager(conn: &mut PgConnection, channel: Id, user: Id) -> Result<Channel> {
    let (data, role) = channel_member(conn, channel, user).await?;
    if !role.manages_channel() {
        return Err(ApiError::forbidden());
    }
    Ok(data)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateChannel {
    channel_name: String,
    #[serde(default)]
    invitee_ids: Vec<Id>,
}

async fn create_channel(
    State(state): State<AppState>,
    actor: Actor,
    Body(body): Body<CreateChannel>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = name(&body.channel_name)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let invitees: BTreeSet<_> = body.invitee_ids.iter().map(|id| id.0).collect();
    for invitee in invitees {
        if invitee != actor.user_id.0 {
            db::lock_pair(&mut tx, actor.user_id, Id(invitee)).await?;
        }
    }
    let channel = state.id().await?;
    let created: Channel = sqlx::query_as(
        "INSERT INTO channels (id, creator_id, name, join_mode, invite_code)
         VALUES ($1, $2, $3, 'INVITE_ONLY', $4) RETURNING *",
    )
    .bind(channel.0)
    .bind(actor.user_id.0)
    .bind(name)
    .bind(Uuid::new_v4().simple().to_string())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO channel_members (channel_id, user_id, role) VALUES ($1, $2, 'CREATOR')",
    )
    .bind(channel.0)
    .bind(actor.user_id.0)
    .execute(&mut *tx)
    .await?;
    sync::emit(
        &mut tx,
        sync::Scope::Channel(channel),
        "CHANNEL_UPDATE",
        created.view(false),
    )
    .await?;

    let mut invitations = Vec::with_capacity(body.invitee_ids.len());
    for invitee in body.invitee_ids {
        let outcome = if let Some(invitation) =
            create_invitation(&state, &mut tx, channel, actor.user_id, invitee).await?
        {
            json!({ "target_user_id": invitee, "invitation": invitation.view() })
        } else {
            json!({ "target_user_id": invitee, "error": { "code": "INVITATION_UNAVAILABLE" } })
        };
        invitations.push(outcome);
    }
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "channel": created.view(true), "invitations": invitations })),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateChannel {
    #[serde(default, deserialize_with = "supplied")]
    channel_name: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "supplied")]
    join_mode: Option<JoinMode>,
    #[serde(default, deserialize_with = "supplied")]
    icon_upload_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "supplied")]
    background_upload_id: Option<Option<Id>>,
}

async fn update_channel(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<UpdateChannel>,
) -> Result<Json<Value>> {
    let new_name = body.channel_name.as_deref().map(name).transpose()?;
    if body
        .description
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|text| text.chars().count() > 2_000 || text.contains('\0'))
    {
        return Err(ApiError::bad("INVALID_DESCRIPTION"));
    }
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let current = channel_manager(&mut tx, channel, actor.user_id).await?;
    let icon = body
        .icon_upload_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(current.icon_upload_id);
    let background = body
        .background_upload_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(current.background_upload_id);
    for (previous, next, purpose) in [
        (current.icon_upload_id, icon, "ICON"),
        (current.background_upload_id, background, "BACKGROUND"),
    ] {
        if previous == next {
            continue;
        }
        if let Some(next) = next {
            files::attach_image(
                &state,
                &mut tx,
                actor.user_id,
                Some(channel),
                Id(next),
                purpose,
            )
            .await?;
        }
        if let Some(previous) = previous {
            files::discard_image(&mut tx, Id(previous)).await?;
        }
    }
    let updated: Channel = sqlx::query_as(
        "UPDATE channels SET name = COALESCE($2, name), description = $3,
         join_mode = COALESCE($4, join_mode), icon_upload_id = $5, background_upload_id = $6
         WHERE id = $1 RETURNING *",
    )
    .bind(channel.0)
    .bind(new_name)
    .bind(body.description.unwrap_or(current.description))
    .bind(body.join_mode.as_ref().map(JoinMode::as_str))
    .bind(icon)
    .bind(background)
    .fetch_one(&mut *tx)
    .await?;
    sync::emit(
        &mut tx,
        sync::Scope::Channel(channel),
        "CHANNEL_UPDATE",
        updated.view(false),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(updated.view(true)))
}

async fn delete_channel(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (_, role) = channel_member(&mut tx, channel, actor.user_id).await?;
    if role != ChannelRole::Creator {
        return Err(ApiError::forbidden());
    }
    let members =
        sqlx::query_scalar::<_, i64>("SELECT user_id FROM channel_members WHERE channel_id = $1")
            .bind(channel.0)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(Id)
            .collect();
    sqlx::query("UPDATE channels SET is_available = false WHERE id = $1")
        .bind(channel.0)
        .execute(&mut *tx)
        .await?;
    cancel_invitations(&mut tx, channel, None).await?;
    voice::revoke_invalid(&mut tx).await?;
    sync::emit(
        &mut tx,
        sync::Scope::Revocation(members),
        "CHANNEL_DELETE",
        json!({ "channel_id": channel }),
    )
    .await?;
    tx.commit().await?;
    voice::flush_after_commit(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(FromRow)]
struct Invitation {
    id: i64,
    channel_id: i64,
    inviter_id: i64,
    invitee_id: i64,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Invitation {
    fn view(&self) -> Value {
        json!({
            "invitation_id": Id(self.id),
            "channel_id": Id(self.channel_id),
            "inviter_id": Id(self.inviter_id),
            "invitee_id": Id(self.invitee_id),
            "status": self.status,
            "created_at": self.created_at,
            "resolved_at": self.resolved_at
        })
    }

    async fn emit(&self, conn: &mut PgConnection, kind: &str) -> Result<()> {
        sync::emit(
            conn,
            sync::Scope::Users(vec![Id(self.inviter_id), Id(self.invitee_id)]),
            kind,
            self.view(),
        )
        .await
    }
}

async fn can_invite(conn: &mut PgConnection, inviter: Id, invitee: Id) -> Result<bool> {
    let lo = inviter.0.min(invitee.0);
    let hi = inviter.0.max(invitee.0);
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relations WHERE lo = $1 AND hi = $2 AND status = 'FRIEND')
         AND NOT EXISTS (SELECT 1 FROM user_blocks WHERE (blocker_id, blocked_id) IN (($1, $2), ($2, $1)))
         AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND is_available)
         AND EXISTS (SELECT 1 FROM users WHERE id = $2 AND is_available)",
    )
    .bind(lo)
    .bind(hi)
    .fetch_one(conn)
    .await?)
}

async fn create_invitation(
    state: &AppState,
    conn: &mut PgConnection,
    channel: Id,
    inviter: Id,
    invitee: Id,
) -> Result<Option<Invitation>> {
    if inviter == invitee
        || !can_invite(conn, inviter, invitee).await?
        || role(conn, channel, invitee).await?.is_some()
    {
        return Ok(None);
    }
    let id = state.id().await?;
    let invitation: Option<Invitation> = sqlx::query_as(
        "INSERT INTO channel_invitations (id, channel_id, inviter_id, invitee_id, status)
         VALUES ($1, $2, $3, $4, 'PENDING')
         ON CONFLICT (channel_id, invitee_id) WHERE status = 'PENDING' DO NOTHING RETURNING *",
    )
    .bind(id.0)
    .bind(channel.0)
    .bind(inviter.0)
    .bind(invitee.0)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(invitation) = &invitation {
        invitation.emit(conn, "INVITE_CREATE").await?;
    }
    Ok(invitation)
}

async fn cancel_invitations(
    conn: &mut PgConnection,
    channel: Id,
    inviter: Option<Id>,
) -> Result<()> {
    let cancelled: Vec<Invitation> = sqlx::query_as(
        "UPDATE channel_invitations SET status = 'CANCELLED', resolved_at = now()
         WHERE channel_id = $1 AND status = 'PENDING' AND ($2::bigint IS NULL OR inviter_id = $2)
         RETURNING *",
    )
    .bind(channel.0)
    .bind(inviter.map(|id| id.0))
    .fetch_all(&mut *conn)
    .await?;
    for invitation in cancelled {
        invitation.emit(conn, "INVITE_UPDATE").await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    target_user_id: Id,
}

async fn invite(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<Target>,
) -> Result<(StatusCode, Json<Value>)> {
    let target = body.target_user_id;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    if actor.user_id != target {
        db::lock_pair(&mut tx, actor.user_id, target).await?;
    }
    channel_manager(&mut tx, channel, actor.user_id).await?;
    let invitation = create_invitation(&state, &mut tx, channel, actor.user_id, target)
        .await?
        .ok_or_else(|| ApiError::conflict("INVITATION_UNAVAILABLE"))?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(invitation.view())))
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Decision {
    Accept,
    Decline,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveInvitation {
    decision: Decision,
}

async fn resolve_invitation(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Id>,
    Body(body): Body<ResolveInvitation>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (inviter, channel): (i64, i64) = sqlx::query_as(
        "SELECT inviter_id, channel_id FROM channel_invitations WHERE id = $1 AND invitee_id = $2",
    )
    .bind(id.0)
    .bind(actor.user_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let (inviter, channel) = (Id(inviter), Id(channel));
    db::lock_pair(&mut tx, actor.user_id, inviter).await?;
    let channel_data = locked_channel(&mut tx, channel).await?;
    let status: String =
        sqlx::query_scalar("SELECT status FROM channel_invitations WHERE id = $1 FOR UPDATE")
            .bind(id.0)
            .fetch_one(&mut *tx)
            .await?;
    if status != "PENDING" {
        return Err(ApiError::conflict("INVITATION_UNAVAILABLE"));
    }
    let resolution = match body.decision {
        Decision::Decline => "DECLINED",
        Decision::Accept
            if channel_data.is_available
                && role(&mut tx, channel, inviter)
                    .await?
                    .is_some_and(ChannelRole::manages_channel)
                && can_invite(&mut tx, inviter, actor.user_id).await? =>
        {
            "ACCEPTED"
        }
        Decision::Accept => "CANCELLED",
    };
    let invitation: Invitation = sqlx::query_as(
        "UPDATE channel_invitations SET status = $2, resolved_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id.0)
    .bind(resolution)
    .fetch_one(&mut *tx)
    .await?;
    invitation.emit(&mut tx, "INVITE_UPDATE").await?;
    let mut view = invitation.view();
    if resolution == "ACCEPTED" {
        add_member(&mut tx, channel, actor.user_id).await?;
        let manager = role(&mut tx, channel, actor.user_id)
            .await?
            .is_some_and(ChannelRole::manages_channel);
        view["channel"] = channel_data.view(manager);
    }
    tx.commit().await?;
    if resolution == "CANCELLED" {
        return Err(ApiError::conflict("INVITATION_UNAVAILABLE"));
    }
    Ok(Json(view))
}

async fn public_channel(conn: &mut PgConnection, code: &str, lock: bool) -> Result<Channel> {
    let query = if lock {
        "SELECT * FROM channels WHERE invite_code = $1 AND is_available AND join_mode = 'PUBLIC_LINK' FOR UPDATE"
    } else {
        "SELECT * FROM channels WHERE invite_code = $1 AND is_available AND join_mode = 'PUBLIC_LINK'"
    };
    sqlx::query_as(query)
        .bind(code)
        .fetch_optional(conn)
        .await?
        .ok_or_else(ApiError::not_found)
}

async fn preview_link(
    State(state): State<AppState>,
    _actor: Actor,
    Path(code): Path<String>,
) -> Result<Json<Value>> {
    let channel = public_channel(&mut *state.pool.acquire().await?, &code, false).await?;
    Ok(Json(channel.preview()))
}

async fn join_link(
    State(state): State<AppState>,
    actor: Actor,
    Path(code): Path<String>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let data = public_channel(&mut tx, &code, true).await?;
    let channel = Id(data.id);
    add_member(&mut tx, channel, actor.user_id).await?;
    let manager = role(&mut tx, channel, actor.user_id)
        .await?
        .is_some_and(ChannelRole::manages_channel);
    tx.commit().await?;
    Ok(Json(data.view(manager)))
}

async fn add_member(conn: &mut PgConnection, channel: Id, user: Id) -> Result<()> {
    let inserted = sqlx::query(
        "INSERT INTO channel_members (channel_id, user_id, role) VALUES ($1, $2, 'MEMBER')
         ON CONFLICT DO NOTHING",
    )
    .bind(channel.0)
    .bind(user.0)
    .execute(&mut *conn)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO message_reads (room_id, user_id, last_sequence)
         SELECT id, $2, last_sequence FROM rooms
         WHERE channel_id = $1 AND is_available AND kind = 'TEXT' AND NOT private
         ON CONFLICT (room_id, user_id) DO UPDATE
         SET last_sequence = GREATEST(message_reads.last_sequence, EXCLUDED.last_sequence),
             read_at = CASE WHEN message_reads.last_sequence < EXCLUDED.last_sequence THEN now()
                       ELSE message_reads.read_at END",
    )
    .bind(channel.0)
    .bind(user.0)
    .execute(&mut *conn)
    .await?;
    sync::emit(
        conn,
        sync::Scope::Channel(channel),
        "MEMBER_ADD",
        json!({ "channel_id": channel, "user_id": user, "role": "MEMBER" }),
    )
    .await
}

async fn remove_member(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, user)): Path<(Id, String)>,
) -> Result<StatusCode> {
    let target = if user == "me" {
        actor.user_id
    } else {
        user.parse::<Id>()
            .map_err(|_| ApiError::bad("INVALID_ID"))?
    };
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (_, actor_role) = channel_member(&mut tx, channel, actor.user_id).await?;
    if user == "me" {
        if actor_role == ChannelRole::Creator {
            return Err(ApiError::conflict("CREATOR_MUST_TRANSFER"));
        }
    } else {
        if actor.user_id == target || !actor_role.manages_channel() {
            return Err(ApiError::forbidden());
        }
        let target_role = role(&mut tx, channel, target)
            .await?
            .ok_or_else(ApiError::not_found)?;
        if target_role == ChannelRole::Creator
            || (actor_role == ChannelRole::Moderator && target_role == ChannelRole::Moderator)
        {
            return Err(ApiError::forbidden());
        }
    }
    sqlx::query(
        "DELETE FROM room_members USING rooms
         WHERE room_members.room_id = rooms.id AND rooms.channel_id = $1 AND room_members.user_id = $2",
    )
    .bind(channel.0)
    .bind(target.0)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM channel_members WHERE channel_id = $1 AND user_id = $2")
        .bind(channel.0)
        .bind(target.0)
        .execute(&mut *tx)
        .await?;
    cancel_invitations(&mut tx, channel, Some(target)).await?;
    voice::revoke_invalid(&mut tx).await?;
    let event = json!({ "channel_id": channel, "user_id": target });
    sync::emit(
        &mut tx,
        sync::Scope::Channel(channel),
        "MEMBER_REMOVE",
        event.clone(),
    )
    .await?;
    sync::emit(
        &mut tx,
        sync::Scope::Revocation(vec![target]),
        "MEMBER_REMOVE",
        event,
    )
    .await?;
    tx.commit().await?;
    voice::flush_after_commit(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRole {
    role: EditableRole,
}

async fn update_role(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, target)): Path<(Id, Id)>,
    Body(body): Body<UpdateRole>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (_, actor_role) = channel_member(&mut tx, channel, actor.user_id).await?;
    if actor_role != ChannelRole::Creator || actor.user_id == target {
        return Err(ApiError::forbidden());
    }
    let previous = role(&mut tx, channel, target)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let next = body.role.as_str();
    let event = json!({ "channel_id": channel, "roles": [{ "user_id": target, "role": next }] });
    let promote = matches!(body.role, EditableRole::Moderator);
    if promote != (previous == ChannelRole::Moderator) {
        set_role(&mut tx, channel, target, next).await?;
        if !promote {
            cancel_invitations(&mut tx, channel, Some(target)).await?;
        }
        sync::emit(
            &mut tx,
            sync::Scope::Channel(channel),
            "ROLE_UPDATE",
            event.clone(),
        )
        .await?;
        change_role_visibility(&mut tx, channel, target, promote).await?;
        if !promote {
            voice::revoke_invalid(&mut tx).await?;
        }
    }
    tx.commit().await?;
    if !promote && previous == ChannelRole::Moderator {
        voice::flush_after_commit(&state).await;
    }
    Ok(Json(event))
}

async fn transfer_ownership(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<Target>,
) -> Result<Json<Value>> {
    let target = body.target_user_id;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (_, actor_role) = channel_member(&mut tx, channel, actor.user_id).await?;
    if actor_role != ChannelRole::Creator || actor.user_id == target {
        return Err(ApiError::forbidden());
    }
    let previous = role(&mut tx, channel, target)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let available: bool = sqlx::query_scalar("SELECT is_available FROM users WHERE id = $1")
        .bind(target.0)
        .fetch_one(&mut *tx)
        .await?;
    if !available {
        return Err(ApiError::conflict("MEMBER_UNAVAILABLE"));
    }
    set_role(&mut tx, channel, actor.user_id, "MODERATOR").await?;
    set_role(&mut tx, channel, target, "CREATOR").await?;
    sqlx::query("UPDATE channels SET creator_id = $2 WHERE id = $1")
        .bind(channel.0)
        .bind(target.0)
        .execute(&mut *tx)
        .await?;
    let event = json!({
        "channel_id": channel,
        "roles": [
            { "user_id": actor.user_id, "role": "MODERATOR" },
            { "user_id": target, "role": "CREATOR" }
        ]
    });
    sync::emit(
        &mut tx,
        sync::Scope::Channel(channel),
        "ROLE_UPDATE",
        event.clone(),
    )
    .await?;
    if previous == ChannelRole::Member {
        change_role_visibility(&mut tx, channel, target, true).await?;
    }
    tx.commit().await?;
    Ok(Json(event))
}

async fn set_role(conn: &mut PgConnection, channel: Id, user: Id, role: &str) -> Result<()> {
    sqlx::query("UPDATE channel_members SET role = $3 WHERE channel_id = $1 AND user_id = $2")
        .bind(channel.0)
        .bind(user.0)
        .bind(role)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(FromRow)]
struct Category {
    id: i64,
    channel_id: i64,
    name: String,
    position: i32,
}

impl Category {
    fn view(&self) -> Value {
        json!({
            "category_id": Id(self.id),
            "channel_id": Id(self.channel_id),
            "category_name": self.name,
            "position": self.position
        })
    }

    async fn emit(&self, conn: &mut PgConnection, kind: &str) -> Result<()> {
        sync::emit(
            conn,
            sync::Scope::Channel(Id(self.channel_id)),
            kind,
            self.view(),
        )
        .await
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCategory {
    category_name: String,
    position: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCategory {
    #[serde(default, deserialize_with = "supplied")]
    category_name: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    position: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderCategories {
    category_ids: Vec<Id>,
}

async fn category_order(conn: &mut PgConnection, channel: Id) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM room_categories WHERE channel_id = $1 ORDER BY position, id",
    )
    .bind(channel.0)
    .fetch_all(conn)
    .await?)
}

fn place(ids: &mut Vec<i64>, id: i64, requested: Option<u32>) -> Result<i32> {
    let position = requested
        .map(|position| position as usize)
        .unwrap_or(ids.len());
    if position > ids.len() {
        return Err(ApiError::bad("INVALID_POSITION"));
    }
    let index = i32::try_from(position).map_err(|_| ApiError::bad("INVALID_POSITION"))?;
    ids.insert(position, id);
    Ok(index)
}

fn check_order(previous: &[i64], next: &[i64]) -> Result<()> {
    let previous: BTreeSet<_> = previous.iter().collect();
    let next_set: BTreeSet<_> = next.iter().collect();
    if previous != next_set || next_set.len() != next.len() {
        return Err(ApiError::bad("INVALID_ORDER"));
    }
    Ok(())
}

async fn write_category_order(
    conn: &mut PgConnection,
    ids: &[i64],
    skip_event_for: Option<Id>,
) -> Result<()> {
    let updated: Vec<Category> = sqlx::query_as(
        "UPDATE room_categories c SET position = sort.ordinal - 1
         FROM unnest($1::bigint[]) WITH ORDINALITY AS sort(id, ordinal)
         WHERE c.id = sort.id AND c.position <> sort.ordinal - 1 RETURNING c.*",
    )
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    for category in updated {
        if skip_event_for != Some(Id(category.id)) {
            category.emit(conn, "ROOM_CATEGORY_UPDATE").await?;
        }
    }
    Ok(())
}

async fn create_category(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<CreateCategory>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = name(&body.category_name)?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    let mut ids = category_order(&mut tx, channel).await?;
    let category_id = state.id().await?;
    let position = place(&mut ids, category_id.0, body.position)?;
    let category: Category = sqlx::query_as(
        "INSERT INTO room_categories (id, channel_id, name, position)
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(category_id.0)
    .bind(channel.0)
    .bind(name)
    .bind(position)
    .fetch_one(&mut *tx)
    .await?;
    category.emit(&mut tx, "ROOM_CATEGORY_CREATE").await?;
    write_category_order(&mut tx, &ids, Some(category_id)).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(category.view())))
}

async fn update_category(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, category)): Path<(Id, Id)>,
    Body(body): Body<UpdateCategory>,
) -> Result<Json<Value>> {
    let new_name = body.category_name.as_deref().map(name).transpose()?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    if let Some(position) = body.position {
        let mut ids = category_order(&mut tx, channel).await?;
        let current = ids
            .iter()
            .position(|id| *id == category.0)
            .ok_or_else(ApiError::not_found)?;
        ids.remove(current);
        place(&mut ids, category.0, Some(position))?;
        write_category_order(&mut tx, &ids, Some(category)).await?;
    }
    let category: Category = sqlx::query_as(
        "UPDATE room_categories SET name = COALESCE($3, name)
         WHERE channel_id = $1 AND id = $2 RETURNING *",
    )
    .bind(channel.0)
    .bind(category.0)
    .bind(new_name)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    category.emit(&mut tx, "ROOM_CATEGORY_UPDATE").await?;
    tx.commit().await?;
    Ok(Json(category.view()))
}

async fn delete_category(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, category)): Path<(Id, Id)>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    require_category(&mut tx, channel, Some(category.0)).await?;
    let mut ungrouped = room_order(&mut tx, channel, None).await?;
    ungrouped.extend(room_order(&mut tx, channel, Some(category.0)).await?);
    write_room_order(&mut tx, None, &ungrouped).await?;
    sqlx::query("UPDATE rooms SET category_id = NULL WHERE channel_id = $1 AND category_id = $2")
        .bind(channel.0)
        .bind(category.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM room_categories WHERE channel_id = $1 AND id = $2")
        .bind(channel.0)
        .bind(category.0)
        .execute(&mut *tx)
        .await?;
    sync::emit(
        &mut tx,
        sync::Scope::Channel(channel),
        "ROOM_CATEGORY_DELETE",
        json!({ "channel_id": channel, "category_id": category }),
    )
    .await?;
    let ids = category_order(&mut tx, channel).await?;
    write_category_order(&mut tx, &ids, None).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn order_categories(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<OrderCategories>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    let previous = category_order(&mut tx, channel).await?;
    let ids: Vec<_> = body.category_ids.iter().map(|id| id.0).collect();
    check_order(&previous, &ids)?;
    write_category_order(&mut tx, &ids, None).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_category(
    conn: &mut PgConnection,
    channel: Id,
    category: Option<i64>,
) -> Result<()> {
    if let Some(category) = category {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM room_categories WHERE channel_id = $1 AND id = $2)",
        )
        .bind(channel.0)
        .bind(category)
        .fetch_one(conn)
        .await?;
        if !exists {
            return Err(ApiError::not_found());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum RoomKind {
    Text,
    Voice,
}

impl RoomKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Voice => "VOICE",
        }
    }
}

#[derive(FromRow)]
struct Room {
    id: i64,
    channel_id: i64,
    category_id: Option<i64>,
    name: String,
    kind: String,
    private: bool,
    position: i32,
    last_sequence: i64,
}

impl Room {
    fn view(&self) -> Value {
        json!({
            "room_id": Id(self.id),
            "channel_id": Id(self.channel_id),
            "category_id": self.category_id.map(Id),
            "room_name": self.name,
            "type": self.kind,
            "is_private": self.private,
            "position": self.position,
            "last_message_sequence": self.last_sequence.to_string()
        })
    }

    async fn emit(&self, conn: &mut PgConnection, kind: &str, viewers: Vec<Id>) -> Result<()> {
        if viewers.is_empty() {
            return Ok(());
        }
        sync::emit(
            conn,
            sync::Scope::RoomUsers(Id(self.id), viewers),
            kind,
            self.view(),
        )
        .await
    }

    async fn revoke(&self, conn: &mut PgConnection, users: Vec<Id>) -> Result<()> {
        if users.is_empty() {
            return Ok(());
        }
        sync::emit(
            conn,
            sync::Scope::Revocation(users),
            "ROOM_DELETE",
            json!({ "room_id": Id(self.id), "channel_id": Id(self.channel_id) }),
        )
        .await
    }

    async fn initialize_reads(&self, conn: &mut PgConnection, users: &[Id]) -> Result<()> {
        if self.kind != "TEXT" || users.is_empty() {
            return Ok(());
        }
        let ids: Vec<_> = users.iter().map(|id| id.0).collect();
        sqlx::query(
            "INSERT INTO message_reads (room_id, user_id, last_sequence)
             SELECT $1, user_id, $2 FROM unnest($3::bigint[]) AS users(user_id)
             ON CONFLICT (room_id, user_id) DO UPDATE
             SET last_sequence = GREATEST(message_reads.last_sequence, EXCLUDED.last_sequence),
                 read_at = CASE WHEN message_reads.last_sequence < EXCLUDED.last_sequence THEN now()
                           ELSE message_reads.read_at END",
        )
        .bind(self.id)
        .bind(self.last_sequence)
        .bind(ids)
        .execute(conn)
        .await?;
        Ok(())
    }
}

async fn change_role_visibility(
    conn: &mut PgConnection,
    channel: Id,
    user: Id,
    gained: bool,
) -> Result<()> {
    let rooms: Vec<Room> = sqlx::query_as(
        "SELECT r.* FROM rooms r WHERE r.channel_id = $1 AND r.private AND r.is_available
         AND NOT EXISTS (SELECT 1 FROM room_members m WHERE m.room_id = r.id AND m.user_id = $2)
         ORDER BY r.id FOR UPDATE OF r",
    )
    .bind(channel.0)
    .bind(user.0)
    .fetch_all(&mut *conn)
    .await?;
    for room in rooms {
        if gained {
            room.initialize_reads(conn, &[user]).await?;
            room.emit(conn, "ROOM_CREATE", vec![user]).await?;
        } else {
            room.revoke(conn, vec![user]).await?;
        }
    }
    Ok(())
}

async fn room_order(
    conn: &mut PgConnection,
    channel: Id,
    category: Option<i64>,
) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM rooms WHERE channel_id = $1 AND is_available
         AND category_id IS NOT DISTINCT FROM $2 ORDER BY position, id",
    )
    .bind(channel.0)
    .bind(category)
    .fetch_all(conn)
    .await?)
}

async fn write_room_order(
    conn: &mut PgConnection,
    category: Option<i64>,
    ids: &[i64],
) -> Result<()> {
    let updated: Vec<Room> = sqlx::query_as(
        "UPDATE rooms r SET category_id = $1, position = sort.ordinal - 1
         FROM unnest($2::bigint[]) WITH ORDINALITY AS sort(id, ordinal)
         WHERE r.id = sort.id AND (r.category_id IS DISTINCT FROM $1 OR r.position <> sort.ordinal - 1)
         RETURNING r.*",
    )
    .bind(category)
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    for room in updated {
        sync::emit(
            conn,
            sync::Scope::Room(Id(room.id)),
            "ROOM_UPDATE",
            room.view(),
        )
        .await?;
    }
    Ok(())
}

async fn locked_room(conn: &mut PgConnection, channel: Id, room: Id) -> Result<Room> {
    sqlx::query_as(
        "SELECT * FROM rooms WHERE channel_id = $1 AND id = $2 AND is_available FOR UPDATE",
    )
    .bind(channel.0)
    .bind(room.0)
    .fetch_optional(conn)
    .await?
    .ok_or_else(ApiError::not_found)
}

type RoomMembers = BTreeMap<i64, bool>;

fn member_ids(ids: &[Id]) -> Result<BTreeSet<i64>> {
    let unique: BTreeSet<_> = ids.iter().map(|id| id.0).collect();
    if unique.len() != ids.len() {
        return Err(ApiError::bad("DUPLICATE_MEMBER"));
    }
    Ok(unique)
}

async fn assigned_roles(
    conn: &mut PgConnection,
    channel: Id,
    users: &BTreeSet<i64>,
) -> Result<BTreeMap<i64, String>> {
    let ids: Vec<_> = users.iter().copied().collect();
    let roles: Vec<(i64, String)> = sqlx::query_as(
        "SELECT user_id, role FROM channel_members WHERE channel_id = $1 AND user_id = ANY($2)",
    )
    .bind(channel.0)
    .bind(ids)
    .fetch_all(conn)
    .await?;
    if roles.len() != users.len() {
        return Err(ApiError::bad("INVALID_ROOM_MEMBER"));
    }
    Ok(roles.into_iter().collect())
}

async fn write_room_members(
    conn: &mut PgConnection,
    room: Id,
    members: &RoomMembers,
) -> Result<()> {
    let (users, moderators): (Vec<_>, Vec<_>) =
        members.iter().map(|(&id, &role)| (id, role)).unzip();
    sqlx::query("DELETE FROM room_members WHERE room_id = $1 AND user_id <> ALL($2)")
        .bind(room.0)
        .bind(&users)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "INSERT INTO room_members (room_id, user_id, is_room_mod)
         SELECT $1, user_id, is_room_mod FROM unnest($2::bigint[], $3::boolean[]) AS members(user_id, is_room_mod)
         ON CONFLICT (room_id, user_id) DO UPDATE SET is_room_mod = EXCLUDED.is_room_mod
         WHERE room_members.is_room_mod IS DISTINCT FROM EXCLUDED.is_room_mod",
    )
    .bind(room.0)
    .bind(users)
    .bind(moderators)
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRoom {
    room_name: String,
    #[serde(rename = "type")]
    kind: RoomKind,
    is_private: bool,
    category_id: Option<Id>,
    position: Option<u32>,
    #[serde(default)]
    member_ids: Vec<Id>,
}

async fn create_room(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<CreateRoom>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = name(&body.room_name)?;
    let members = member_ids(&body.member_ids)?;
    if !body.is_private && !members.is_empty() {
        return Err(ApiError::bad("PUBLIC_ROOM_MEMBERS"));
    }
    let category = body.category_id.map(|id| id.0);
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    require_category(&mut tx, channel, category).await?;
    assigned_roles(&mut tx, channel, &members).await?;
    let mut order = room_order(&mut tx, channel, category).await?;
    let room_id = state.id().await?;
    let position = place(&mut order, room_id.0, body.position)?;
    let room: Room = sqlx::query_as(
        "INSERT INTO rooms (id, channel_id, category_id, name, kind, private, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(room_id.0)
    .bind(channel.0)
    .bind(category)
    .bind(name)
    .bind(body.kind.as_str())
    .bind(body.is_private)
    .bind(position)
    .fetch_one(&mut *tx)
    .await?;
    if !members.is_empty() {
        let members = members.into_iter().map(|id| (id, false)).collect();
        write_room_members(&mut tx, room_id, &members).await?;
    }
    let viewers = db::room_viewers(&mut tx, room_id).await?;
    room.initialize_reads(&mut tx, &viewers).await?;
    room.emit(&mut tx, "ROOM_CREATE", viewers).await?;
    write_room_order(&mut tx, category, &order).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(room.view())))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRoom {
    #[serde(default, deserialize_with = "supplied")]
    room_name: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    is_private: Option<bool>,
    #[serde(default, deserialize_with = "supplied")]
    category_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "supplied")]
    position: Option<u32>,
    #[serde(default, deserialize_with = "supplied")]
    member_ids: Option<Vec<Id>>,
    #[serde(default, deserialize_with = "supplied")]
    room_mod_ids: Option<Vec<Id>>,
}

async fn update_room_members(
    conn: &mut PgConnection,
    room: &Room,
    manager: bool,
    previous: &RoomMembers,
    body: &UpdateRoom,
) -> Result<()> {
    let next_private = body.is_private.unwrap_or(room.private);
    if !next_private {
        if body.member_ids.as_ref().is_some_and(|ids| !ids.is_empty())
            || body
                .room_mod_ids
                .as_ref()
                .is_some_and(|ids| !ids.is_empty())
        {
            return Err(ApiError::bad("PUBLIC_ROOM_MEMBERS"));
        }
        if room.private {
            write_room_members(conn, Id(room.id), &RoomMembers::new()).await?;
        }
        return Ok(());
    }
    if !room.private && body.member_ids.is_none() {
        return Err(ApiError::bad("ROOM_MEMBERS_REQUIRED"));
    }
    if body.member_ids.is_none() && body.room_mod_ids.is_none() {
        return Ok(());
    }

    let mut users = match &body.member_ids {
        Some(ids) => member_ids(ids)?,
        None => previous.keys().copied().collect(),
    };
    let mods = if let Some(ids) = &body.room_mod_ids {
        let ids = member_ids(ids)?;
        if !ids.is_subset(&users) {
            return Err(ApiError::bad("INVALID_ROOM_MODERATOR"));
        }
        ids
    } else {
        previous
            .iter()
            .filter_map(|(&id, &moderator)| moderator.then_some(id))
            .collect()
    };
    if manager {
        assigned_roles(conn, Id(room.channel_id), &users).await?;
    } else {
        let candidates: BTreeSet<_> = users
            .iter()
            .copied()
            .chain(previous.keys().copied())
            .collect();
        let roles = assigned_roles(conn, Id(room.channel_id), &candidates).await?;
        for (id, role) in roles {
            if role != "MEMBER" {
                if previous.contains_key(&id) {
                    users.insert(id);
                } else {
                    return Err(ApiError::forbidden());
                }
            }
        }
        users.extend(mods.iter().copied());
    }
    let next = users
        .into_iter()
        .map(|id| (id, mods.contains(&id)))
        .collect();
    write_room_members(conn, Id(room.id), &next).await
}

async fn update_room(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, id)): Path<(Id, Id)>,
    Body(body): Body<UpdateRoom>,
) -> Result<Json<Value>> {
    let new_name = body.room_name.as_deref().map(name).transpose()?;
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let (_, role) = channel_member(&mut tx, channel, actor.user_id).await?;
    let room = locked_room(&mut tx, channel, id).await?;
    let members: Vec<(i64, bool)> =
        sqlx::query_as("SELECT user_id, is_room_mod FROM room_members WHERE room_id = $1")
            .bind(id.0)
            .fetch_all(&mut *tx)
            .await?;
    let members: RoomMembers = members.into_iter().collect();
    let manager = role.manages_channel();
    if !manager {
        let assignment = members.get(&actor.user_id.0);
        if room.private && assignment.is_none() {
            return Err(ApiError::not_found());
        }
        if !room.private
            || assignment != Some(&true)
            || body.is_private.is_some()
            || body.category_id.is_some()
            || body.position.is_some()
            || body.room_mod_ids.is_some()
        {
            return Err(ApiError::forbidden());
        }
    }
    let before = viewers(&mut tx, id).await?;
    let category = body
        .category_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(room.category_id);
    require_category(&mut tx, channel, category).await?;
    update_room_members(&mut tx, &room, manager, &members, &body).await?;
    let moving = category != room.category_id;
    let new_order = if moving || body.position.is_some() {
        let mut ids = room_order(&mut tx, channel, category).await?;
        ids.retain(|existing| *existing != id.0);
        let position = place(&mut ids, id.0, body.position)?;
        Some((ids, position))
    } else {
        None
    };
    let updated: Room = sqlx::query_as(
        "UPDATE rooms SET name = COALESCE($2, name), private = COALESCE($3, private),
         category_id = $4, position = $5 WHERE id = $1 RETURNING *",
    )
    .bind(id.0)
    .bind(new_name)
    .bind(body.is_private)
    .bind(category)
    .bind(new_order.as_ref().map(|(_, p)| *p).unwrap_or(room.position))
    .fetch_one(&mut *tx)
    .await?;
    let after = viewers(&mut tx, id).await?;
    let gained: Vec<_> = after.difference(&before).copied().map(Id).collect();
    let retained: Vec<_> = after.intersection(&before).copied().map(Id).collect();
    let lost: Vec<_> = before.difference(&after).copied().map(Id).collect();
    updated.initialize_reads(&mut tx, &gained).await?;
    updated.emit(&mut tx, "ROOM_CREATE", gained).await?;
    updated.emit(&mut tx, "ROOM_UPDATE", retained).await?;
    let revoked = !lost.is_empty();
    if revoked {
        updated.revoke(&mut tx, lost).await?;
        voice::revoke_invalid(&mut tx).await?;
    }
    if let Some((ids, _)) = new_order {
        write_room_order(&mut tx, category, &ids).await?;
    }
    if moving {
        let old_order = room_order(&mut tx, channel, room.category_id).await?;
        write_room_order(&mut tx, room.category_id, &old_order).await?;
    }
    tx.commit().await?;
    if revoked {
        voice::flush_after_commit(&state).await;
    }
    Ok(Json(updated.view()))
}

async fn viewers(conn: &mut PgConnection, room: Id) -> Result<BTreeSet<i64>> {
    Ok(db::room_viewers(conn, room)
        .await?
        .into_iter()
        .map(|id| id.0)
        .collect())
}

async fn delete_room(
    State(state): State<AppState>,
    actor: Actor,
    Path((channel, id)): Path<(Id, Id)>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    let room = locked_room(&mut tx, channel, id).await?;
    let viewers = db::room_viewers(&mut tx, id).await?;
    sqlx::query("UPDATE rooms SET is_available = false WHERE id = $1")
        .bind(id.0)
        .execute(&mut *tx)
        .await?;
    let remaining = room_order(&mut tx, channel, room.category_id).await?;
    write_room_order(&mut tx, room.category_id, &remaining).await?;
    room.revoke(&mut tx, viewers).await?;
    voice::revoke_invalid(&mut tx).await?;
    tx.commit().await?;
    voice::flush_after_commit(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomGroup {
    #[serde(deserialize_with = "Deserialize::deserialize")]
    category_id: Option<Id>,
    room_ids: Vec<Id>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderRooms {
    groups: Vec<RoomGroup>,
}

async fn order_rooms(
    State(state): State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(body): Body<OrderRooms>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    channel_manager(&mut tx, channel, actor.user_id).await?;
    let mut groups = BTreeSet::new();
    let mut supplied = 0;
    for group in body.groups {
        let category = group.category_id.map(|id| id.0);
        if !groups.insert(category) {
            return Err(ApiError::bad("INVALID_ORDER"));
        }
        require_category(&mut tx, channel, category).await?;
        let previous = room_order(&mut tx, channel, category).await?;
        let ids: Vec<_> = group.room_ids.into_iter().map(|id| id.0).collect();
        check_order(&previous, &ids)?;
        supplied += ids.len();
        write_room_order(&mut tx, category, &ids).await?;
    }
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM rooms WHERE channel_id = $1 AND is_available")
            .bind(channel.0)
            .fetch_one(&mut *tx)
            .await?;
    if !groups.contains(&None) || supplied as i64 != total {
        return Err(ApiError::bad("INVALID_ORDER"));
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
