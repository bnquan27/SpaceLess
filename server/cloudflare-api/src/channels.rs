use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};
use tokio_postgres::{Client, GenericClient, IsolationLevel, Row, Transaction, types::Type};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
use worker::{Env, Method, Request, Response};

use crate::{
    auth::{self, Actor, ApiError},
    db, events, files, messages, voice,
};

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Id(i64);

impl FromStr for Id {
    type Err = ();
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let digits = value.parse::<i64>().map_err(|_| ())?;
        if digits <= 0 || digits.to_string() != value {
            return Err(());
        }
        Ok(Self(digits))
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(input: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(input)?;
        value
            .parse()
            .map_err(|_| serde::de::Error::custom("INVALID_ID"))
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, output: S) -> std::result::Result<S::Ok, S::Error> {
        output.collect_str(&self.0)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ChannelRoute {
    Create,
    Update(Id),
    Delete(Id),
    Invite(Id),
    ResolveInvitation(Id),
    PreviewLink(String),
    JoinLink(String),
    RemoveMember(Id, String),
    UpdateRole(Id, Id),
    TransferOwnership(Id),
    CreateCategory(Id),
    UpdateCategory(Id, Id),
    DeleteCategory(Id, Id),
    OrderCategories(Id),
    CreateRoom(Id),
    UpdateRoom(Id, Id),
    DeleteRoom(Id, Id),
    OrderRooms(Id),
}

pub(crate) fn route(req: &Request) -> Result<Option<ChannelRoute>> {
    parse_route(req.method(), &req.path())
}

fn parse_route(method: Method, path: &str) -> Result<Option<ChannelRoute>> {
    let Some(path) = path.strip_prefix("/api/v1/") else {
        return Ok(None);
    };
    let parts = path.split('/').collect::<Vec<_>>();
    let id = |value: &str| value.parse().map_err(|_| bad("INVALID_PATH"));
    use ChannelRoute::*;
    use Method::{Delete as DeleteMethod, Get, Patch, Post, Put};
    let route = match (method, parts.as_slice()) {
        (Post, ["channels"]) => Create,
        (Patch, ["channels", channel]) => Update(id(channel)?),
        (DeleteMethod, ["channels", channel]) => Delete(id(channel)?),
        (Post, ["channels", channel, "invitations"]) => Invite(id(channel)?),
        (Patch, ["invitations", invitation]) => ResolveInvitation(id(invitation)?),
        (Get, ["channel-links", code]) if !code.is_empty() => PreviewLink((*code).to_string()),
        (Post, ["channel-links", code, "join"]) if !code.is_empty() => {
            JoinLink((*code).to_string())
        }
        (DeleteMethod, ["channels", channel, "members", target]) if !target.is_empty() => {
            RemoveMember(id(channel)?, (*target).to_string())
        }
        (Patch, ["channels", channel, "members", target, "role"]) => {
            UpdateRole(id(channel)?, id(target)?)
        }
        (Post, ["channels", channel, "transfer-ownership"]) => TransferOwnership(id(channel)?),
        (Post, ["channels", channel, "categories"]) => CreateCategory(id(channel)?),
        (Patch, ["channels", channel, "categories", category]) => {
            UpdateCategory(id(channel)?, id(category)?)
        }
        (DeleteMethod, ["channels", channel, "categories", category]) => {
            DeleteCategory(id(channel)?, id(category)?)
        }
        (Put, ["channels", channel, "categories", "order"]) => OrderCategories(id(channel)?),
        (Post, ["channels", channel, "rooms"]) => CreateRoom(id(channel)?),
        (Patch, ["channels", channel, "rooms", room]) => UpdateRoom(id(channel)?, id(room)?),
        (DeleteMethod, ["channels", channel, "rooms", room]) => DeleteRoom(id(channel)?, id(room)?),
        (Put, ["channels", channel, "rooms", "order"]) => OrderRooms(id(channel)?),
        _ => return Ok(None),
    };
    Ok(Some(route))
}

pub(crate) async fn handle(
    route: ChannelRoute,
    req: Request,
    client: &mut Client,
    jwt_secret: &[u8],
    env: &Env,
) -> Result<Response> {
    let actor = auth::actor(&req, jwt_secret)?;
    use ChannelRoute::*;
    match route {
        Create => create_channel(client, &actor, auth::json_body(req).await?).await,
        Update(channel) => {
            update_channel(client, env, &actor, channel, auth::json_body(req).await?).await
        }
        Delete(channel) => delete_channel(client, env, &actor, channel).await,
        Invite(channel) => invite(client, &actor, channel, auth::json_body(req).await?).await,
        ResolveInvitation(id) => {
            resolve_invitation(client, &actor, id, auth::json_body(req).await?).await
        }
        PreviewLink(code) => preview_link(client, &actor, code).await,
        JoinLink(code) => join_link(client, &actor, code).await,
        RemoveMember(channel, target) => {
            remove_member(client, env, &actor, (channel, target)).await
        }
        UpdateRole(channel, target) => {
            update_role(
                client,
                env,
                &actor,
                (channel, target),
                auth::json_body(req).await?,
            )
            .await
        }
        TransferOwnership(channel) => {
            transfer_ownership(client, &actor, channel, auth::json_body(req).await?).await
        }
        CreateCategory(channel) => {
            create_category(client, &actor, channel, auth::json_body(req).await?).await
        }
        UpdateCategory(channel, category) => {
            update_category(
                client,
                &actor,
                (channel, category),
                auth::json_body(req).await?,
            )
            .await
        }
        DeleteCategory(channel, category) => {
            delete_category(client, &actor, (channel, category)).await
        }
        OrderCategories(channel) => {
            order_categories(client, &actor, channel, auth::json_body(req).await?).await
        }
        CreateRoom(channel) => {
            create_room(client, &actor, channel, auth::json_body(req).await?).await
        }
        UpdateRoom(channel, room) => {
            update_room(
                client,
                env,
                &actor,
                (channel, room),
                auth::json_body(req).await?,
            )
            .await
        }
        DeleteRoom(channel, room) => delete_room(client, env, &actor, (channel, room)).await,
        OrderRooms(channel) => {
            order_rooms(client, &actor, channel, auth::json_body(req).await?).await
        }
    }
}

fn bad(code: &'static str) -> ApiError {
    ApiError::new(400, code, "Yêu cầu không hợp lệ.")
}

fn conflict(code: &'static str) -> ApiError {
    ApiError::new(409, code, "Trạng thái tài nguyên đã thay đổi.")
}

fn not_found() -> ApiError {
    ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên.")
}

fn forbidden() -> ApiError {
    ApiError::new(
        403,
        "FORBIDDEN",
        "Bạn không có quyền thực hiện thao tác này.",
    )
}

fn reply(value: Value, status: u16) -> Result<Response> {
    Response::from_json(&value)
        .map(|response| response.with_status(status))
        .map_err(|_| ApiError::internal())
}

fn empty() -> Result<Response> {
    Response::empty()
        .map(|response| response.with_status(204))
        .map_err(|_| ApiError::internal())
}

async fn room_viewers(conn: &Transaction<'_>, room: Id) -> Result<Vec<Id>> {
    Ok(messages::room_viewers(conn, room.0)
        .await?
        .into_iter()
        .map(Id)
        .collect())
}

async fn require_room_viewer(conn: &Transaction<'_>, user: Id, room: Id) -> Result<()> {
    let visible: bool = conn
        .query_typed_one(
            "SELECT EXISTS (SELECT 1 FROM channel_room_access WHERE room_id = $1 AND user_id = $2)",
            &[(&room.0, Type::INT8), (&user.0, Type::INT8)],
        )
        .await?
        .try_get(0)?;
    if !visible {
        return Err(not_found());
    }
    Ok(())
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
    if name.is_empty() || name.chars().count() > 100 || name.contains('\0') {
        return Err(bad("INVALID_NAME"));
    }
    Ok(name)
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
enum ChannelRole {
    Creator = 1,
    Moderator = 2,
    Member = 3,
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
    fn channel_role(&self) -> ChannelRole {
        match self {
            Self::Moderator => ChannelRole::Moderator,
            Self::Member => ChannelRole::Member,
        }
    }

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
    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            creator_id: row.try_get("creator_id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            join_mode: row.try_get("join_mode")?,
            invite_code: row.try_get("invite_code")?,
            is_available: row.try_get("is_available")?,
            icon_upload_id: row.try_get("icon_upload_id")?,
            background_upload_id: row.try_get("background_upload_id")?,
        })
    }
    fn preview(&self) -> Value {
        json!({
            "channel_id": Id(self.id),
            "channel_name": self.name,
            "description": self.description,
            "icon_url": self.icon_upload_id.map(|id| format!("/api/v1/images/{id}/url")),
            "background_url": self.background_upload_id.map(|id| format!("/api/v1/images/{id}/url"))
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

async fn locked_channel(conn: &Transaction<'_>, channel: Id) -> Result<Channel> {
    conn.query_typed_opt(
        "SELECT * FROM channels WHERE id = $1 FOR UPDATE",
        &[(&channel.0, Type::INT8)],
    )
    .await?
    .map(Channel::from_row)
    .transpose()?
    .ok_or_else(not_found)
}

async fn role(conn: &Transaction<'_>, channel: Id, user: Id) -> Result<Option<ChannelRole>> {
    let role: Option<i32> = conn
        .query_typed_opt(
            "SELECT role_id FROM channel_members WHERE channel_id = $1 AND user_id = $2",
            &[(&channel.0, Type::INT8), (&user.0, Type::INT8)],
        )
        .await?
        .map(|row| row.try_get(0))
        .transpose()?;
    match role {
        Some(1) => Ok(Some(ChannelRole::Creator)),
        Some(2) => Ok(Some(ChannelRole::Moderator)),
        Some(3) => Ok(Some(ChannelRole::Member)),
        None => Ok(None),
        _ => Err(ApiError::internal()),
    }
}

async fn channel_member(
    conn: &Transaction<'_>,
    channel: Id,
    user: Id,
) -> Result<(Channel, ChannelRole)> {
    let data = locked_channel(conn, channel).await?;
    if !data.is_available {
        return Err(not_found());
    }
    let role = role(conn, channel, user).await?.ok_or_else(not_found)?;
    Ok((data, role))
}

async fn channel_manager(conn: &Transaction<'_>, channel: Id, user: Id) -> Result<Channel> {
    let (data, role) = channel_member(conn, channel, user).await?;
    if !role.manages_channel() {
        return Err(forbidden());
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
    client: &mut Client,
    actor: &Actor,
    body: CreateChannel,
) -> Result<Response> {
    let name = name(&body.channel_name)?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let invitees: BTreeSet<_> = body.invitee_ids.iter().map(|id| id.0).collect();
    for invitee in invitees {
        if invitee != actor.user_id {
            db::lock_pair(&tx, actor.user_id, invitee).await?;
        }
    }
    let channel = Id(db::snowflake(&tx).await?);
    let created: Channel = Channel::from_row(
        tx.query_typed_one(
            "INSERT INTO channels (id, creator_id, name, join_mode, invite_code)
         VALUES ($1, $2, $3, 'INVITE_ONLY', $4) RETURNING *",
            &[
                (&channel.0, Type::INT8),
                (&actor.user_id, Type::INT8),
                (&name, Type::TEXT),
                (&Uuid::new_v4().simple().to_string(), Type::TEXT),
            ],
        )
        .await?,
    )?;
    tx.query_typed(
        "INSERT INTO channel_members (channel_id, user_id, role_id) VALUES ($1, $2, $3)",
        &[
            (&channel.0, Type::INT8),
            (&actor.user_id, Type::INT8),
            (&(ChannelRole::Creator as i32), Type::INT4),
        ],
    )
    .await?;
    events::emit(
        &tx,
        events::Scope::Channel(channel.0),
        "CHANNEL_UPDATE",
        created.view(false),
    )
    .await?;

    let mut invitations = Vec::with_capacity(body.invitee_ids.len());
    for invitee in body.invitee_ids {
        let outcome = if let Some(invitation) =
            create_invitation(&tx, channel, Id(actor.user_id), invitee).await?
        {
            json!({ "target_user_id": invitee, "invitation": invitation.view() })
        } else {
            json!({ "target_user_id": invitee, "error": { "code": "INVITATION_UNAVAILABLE" } })
        };
        invitations.push(outcome);
    }
    tx.commit().await?;
    reply(
        json!({ "channel": created.view(true), "invitations": invitations }),
        201,
    )
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
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    channel: Id,
    body: UpdateChannel,
) -> Result<Response> {
    let new_name = body.channel_name.as_deref().map(name).transpose()?;
    if body
        .description
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|text| text.chars().count() > 2_000 || text.contains('\0'))
    {
        return Err(bad("INVALID_DESCRIPTION"));
    }
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let current = channel_manager(&tx, channel, Id(actor.user_id)).await?;
    let icon = body
        .icon_upload_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(current.icon_upload_id);
    let background = body
        .background_upload_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(current.background_upload_id);
    for (previous, next, purpose) in [
        (current.icon_upload_id, icon, "CHANNEL_ICON"),
        (
            current.background_upload_id,
            background,
            "CHANNEL_BACKGROUND",
        ),
    ] {
        if previous == next {
            continue;
        }
        if let Some(next) = next {
            files::attach_image(env, &tx, actor.user_id, Some(channel.0), next, purpose).await?;
        }
        if let Some(previous) = previous {
            files::discard_image(&tx, previous).await?;
        }
    }
    let updated: Channel = Channel::from_row(
        tx.query_typed_one(
            "UPDATE channels SET name = COALESCE($2, name), description = $3,
         join_mode = COALESCE($4, join_mode), icon_upload_id = $5, background_upload_id = $6
         WHERE id = $1 RETURNING *",
            &[
                (&channel.0, Type::INT8),
                (&new_name, Type::TEXT),
                (&body.description.unwrap_or(current.description), Type::TEXT),
                (&body.join_mode.as_ref().map(JoinMode::as_str), Type::TEXT),
                (&icon, Type::INT8),
                (&background, Type::INT8),
            ],
        )
        .await?,
    )?;
    events::emit(
        &tx,
        events::Scope::Channel(channel.0),
        "CHANNEL_UPDATE",
        updated.view(false),
    )
    .await?;
    tx.commit().await?;
    reply(updated.view(true), 200)
}

async fn delete_channel(
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    channel: Id,
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (_, role) = channel_member(&tx, channel, Id(actor.user_id)).await?;
    if role != ChannelRole::Creator {
        return Err(forbidden());
    }
    let members = tx
        .query_typed(
            "SELECT user_id FROM channel_members WHERE channel_id = $1",
            &[(&channel.0, Type::INT8)],
        )
        .await?
        .into_iter()
        .map(|row| row.try_get(0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    tx.query_typed(
        "UPDATE channels SET is_available = false WHERE id = $1",
        &[(&channel.0, Type::INT8)],
    )
    .await?;
    cancel_invitations(&tx, channel, None).await?;
    voice::revoke_invalid(&tx).await?;
    events::emit(
        &tx,
        events::Scope::Revocation(members),
        "CHANNEL_DELETE",
        json!({ "channel_id": channel }),
    )
    .await?;
    tx.commit().await?;
    let _ = voice::flush_after_commit(client, env).await;
    empty()
}

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
    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            channel_id: row.try_get("channel_id")?,
            inviter_id: row.try_get("inviter_id")?,
            invitee_id: row.try_get("invitee_id")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            resolved_at: row.try_get("resolved_at")?,
        })
    }
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

    async fn emit(&self, conn: &Transaction<'_>, kind: &str) -> Result<()> {
        events::emit(
            conn,
            events::Scope::Users(vec![self.inviter_id, self.invitee_id]),
            kind,
            self.view(),
        )
        .await
    }
}

async fn can_invite(conn: &Transaction<'_>, inviter: Id, invitee: Id) -> Result<bool> {
    let lo = inviter.0.min(invitee.0);
    let hi = inviter.0.max(invitee.0);
    Ok(conn.query_typed_one("SELECT EXISTS (SELECT 1 FROM relations WHERE lo = $1 AND hi = $2 AND status = 'FRIEND')
         AND NOT EXISTS (SELECT 1 FROM user_blocks WHERE (blocker_id, blocked_id) IN (($1, $2), ($2, $1)))
         AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND is_available)
         AND EXISTS (SELECT 1 FROM users WHERE id = $2 AND is_available)", &[(&lo, Type::INT8), (&hi, Type::INT8)]).await?.try_get(0)?)
}

async fn create_invitation(
    conn: &Transaction<'_>,
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
    let id = Id(db::snowflake(conn).await?);
    let invitation: Option<Invitation> = conn
        .query_typed_opt(
            "INSERT INTO channel_invitations (id, channel_id, inviter_id, invitee_id, status)
         VALUES ($1, $2, $3, $4, 'PENDING')
         ON CONFLICT (channel_id, invitee_id) WHERE status = 'PENDING' DO NOTHING RETURNING *",
            &[
                (&id.0, Type::INT8),
                (&channel.0, Type::INT8),
                (&inviter.0, Type::INT8),
                (&invitee.0, Type::INT8),
            ],
        )
        .await?
        .map(Invitation::from_row)
        .transpose()?;
    if let Some(invitation) = &invitation {
        invitation.emit(conn, "INVITE_CREATE").await?;
    }
    Ok(invitation)
}

async fn cancel_invitations(
    conn: &Transaction<'_>,
    channel: Id,
    inviter: Option<Id>,
) -> Result<()> {
    let cancelled: Vec<Invitation> = conn
        .query_typed(
            "UPDATE channel_invitations SET status = 'CANCELLED', resolved_at = now()
         WHERE channel_id = $1 AND status = 'PENDING' AND ($2::bigint IS NULL OR inviter_id = $2)
         RETURNING *",
            &[
                (&channel.0, Type::INT8),
                (&inviter.map(|id| id.0), Type::INT8),
            ],
        )
        .await?
        .into_iter()
        .map(Invitation::from_row)
        .collect::<std::result::Result<Vec<_>, _>>()?;
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

async fn invite(client: &mut Client, actor: &Actor, channel: Id, body: Target) -> Result<Response> {
    let target = body.target_user_id;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    if Id(actor.user_id) != target {
        db::lock_pair(&tx, actor.user_id, target.0).await?;
    }
    let data = channel_manager(&tx, channel, Id(actor.user_id)).await?;
    if data.join_mode != "INVITE_ONLY" {
        return Err(conflict("INVITATION_UNAVAILABLE"));
    }
    let invitation = create_invitation(&tx, channel, Id(actor.user_id), target)
        .await?
        .ok_or_else(|| conflict("INVITATION_UNAVAILABLE"))?;
    tx.commit().await?;
    reply(invitation.view(), 201)
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
    client: &mut Client,
    actor: &Actor,
    id: Id,
    body: ResolveInvitation,
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (inviter, channel): (i64, i64) = tx.query_typed_opt("SELECT inviter_id, channel_id FROM channel_invitations WHERE id = $1 AND invitee_id = $2", &[(&id.0, Type::INT8), (&actor.user_id, Type::INT8)]).await?.map(|row| -> Result<_> { Ok((row.try_get(0)?, row.try_get(1)?)) }).transpose()?
    .ok_or_else(not_found)?;
    let (inviter, channel) = (Id(inviter), Id(channel));
    db::lock_pair(&tx, actor.user_id, inviter.0).await?;
    let channel_data = locked_channel(&tx, channel).await?;
    let status: String = tx
        .query_typed_one(
            "SELECT status FROM channel_invitations WHERE id = $1 FOR UPDATE",
            &[(&id.0, Type::INT8)],
        )
        .await?
        .try_get(0)?;
    if status != "PENDING" {
        return Err(conflict("INVITATION_UNAVAILABLE"));
    }
    let resolution = match body.decision {
        Decision::Decline => "DECLINED",
        Decision::Accept
            if channel_data.is_available
                && role(&tx, channel, Id(actor.user_id)).await?.is_none()
                && role(&tx, channel, inviter)
                    .await?
                    .is_some_and(ChannelRole::manages_channel)
                && can_invite(&tx, inviter, Id(actor.user_id)).await? =>
        {
            "ACCEPTED"
        }
        Decision::Accept => "CANCELLED",
    };
    let invitation: Invitation = Invitation::from_row(tx.query_typed_one("UPDATE channel_invitations SET status = $2, resolved_at = now() WHERE id = $1 RETURNING *", &[(&id.0, Type::INT8), (&resolution, Type::TEXT)]).await?)?;
    invitation.emit(&tx, "INVITE_UPDATE").await?;
    let mut view = invitation.view();
    if resolution == "ACCEPTED" {
        add_member(&tx, channel, Id(actor.user_id)).await?;
        let manager = role(&tx, channel, Id(actor.user_id))
            .await?
            .is_some_and(ChannelRole::manages_channel);
        view["channel"] = channel_data.view(manager);
    }
    tx.commit().await?;
    if resolution == "CANCELLED" {
        return Err(conflict("INVITATION_UNAVAILABLE"));
    }
    reply(view, 200)
}

async fn public_channel(
    conn: &(impl GenericClient + Sync),
    code: &str,
    lock: bool,
) -> Result<Channel> {
    let query = if lock {
        "SELECT * FROM channels WHERE invite_code = $1 AND is_available AND join_mode = 'PUBLIC_LINK' FOR UPDATE"
    } else {
        "SELECT * FROM channels WHERE invite_code = $1 AND is_available AND join_mode = 'PUBLIC_LINK'"
    };
    conn.query_typed_opt(query, &[(&code, Type::TEXT)])
        .await?
        .map(Channel::from_row)
        .transpose()?
        .ok_or_else(not_found)
}

async fn preview_link(client: &mut Client, actor: &Actor, code: String) -> Result<Response> {
    let tx = client
        .build_transaction()
        .read_only(true)
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;
    auth::validate_session(&tx, actor).await?;
    let channel = public_channel(&tx, &code, false).await?;
    tx.commit().await?;
    reply(channel.preview(), 200)
}

async fn join_link(client: &mut Client, actor: &Actor, code: String) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let data = public_channel(&tx, &code, true).await?;
    let channel = Id(data.id);
    add_member(&tx, channel, Id(actor.user_id)).await?;
    let manager = role(&tx, channel, Id(actor.user_id))
        .await?
        .is_some_and(ChannelRole::manages_channel);
    tx.commit().await?;
    reply(data.view(manager), 200)
}

async fn add_member(conn: &Transaction<'_>, channel: Id, user: Id) -> Result<()> {
    let inserted = conn
        .query_typed(
            "INSERT INTO channel_members (channel_id, user_id, role_id) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING RETURNING user_id",
            &[
                (&channel.0, Type::INT8),
                (&user.0, Type::INT8),
                (&(ChannelRole::Member as i32), Type::INT4),
            ],
        )
        .await?;
    if inserted.is_empty() {
        return Ok(());
    }
    conn.query_typed(
        "INSERT INTO message_reads (room_id, user_id, last_sequence)
         SELECT id, $2, last_sequence FROM rooms
         WHERE channel_id = $1 AND is_available AND kind = 'TEXT' AND NOT private
         ON CONFLICT (room_id, user_id) DO UPDATE
         SET last_sequence = GREATEST(message_reads.last_sequence, EXCLUDED.last_sequence),
             read_at = CASE WHEN message_reads.last_sequence < EXCLUDED.last_sequence THEN now()
                       ELSE message_reads.read_at END",
        &[(&channel.0, Type::INT8), (&user.0, Type::INT8)],
    )
    .await?;
    events::emit(
        conn,
        events::Scope::Channel(channel.0),
        "MEMBER_ADD",
        json!({ "channel_id": channel, "user_id": user, "role": "MEMBER" }),
    )
    .await
}

async fn remove_member(
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    (channel, user): (Id, String),
) -> Result<Response> {
    let target = if user == "me" {
        Id(actor.user_id)
    } else {
        user.parse::<Id>().map_err(|_| bad("INVALID_ID"))?
    };
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (_, actor_role) = channel_member(&tx, channel, Id(actor.user_id)).await?;
    if user == "me" {
        if actor_role == ChannelRole::Creator {
            return Err(conflict("CREATOR_MUST_TRANSFER"));
        }
    } else {
        if Id(actor.user_id) == target || !actor_role.manages_channel() {
            return Err(forbidden());
        }
        let target_role = role(&tx, channel, target).await?.ok_or_else(not_found)?;
        if target_role == ChannelRole::Creator
            || (actor_role == ChannelRole::Moderator && target_role == ChannelRole::Moderator)
        {
            return Err(forbidden());
        }
    }
    tx.query_typed("DELETE FROM room_members USING rooms
         WHERE room_members.room_id = rooms.id AND rooms.channel_id = $1 AND room_members.user_id = $2", &[(&channel.0, Type::INT8), (&target.0, Type::INT8)]).await?;
    clear_channel_denials(&tx, channel, target).await?;
    tx.query_typed(
        "DELETE FROM channel_members WHERE channel_id = $1 AND user_id = $2",
        &[(&channel.0, Type::INT8), (&target.0, Type::INT8)],
    )
    .await?;
    cancel_invitations(&tx, channel, Some(target)).await?;
    voice::revoke_invalid(&tx).await?;
    let event = json!({ "channel_id": channel, "user_id": target });
    events::emit(
        &tx,
        events::Scope::Channel(channel.0),
        "MEMBER_REMOVE",
        event.clone(),
    )
    .await?;
    events::emit(
        &tx,
        events::Scope::Revocation(vec![target.0]),
        "MEMBER_REMOVE",
        event,
    )
    .await?;
    tx.commit().await?;
    let _ = voice::flush_after_commit(client, env).await;
    empty()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRole {
    role: EditableRole,
}

async fn update_role(
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    (channel, target): (Id, Id),
    body: UpdateRole,
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (_, actor_role) = channel_member(&tx, channel, Id(actor.user_id)).await?;
    if actor_role != ChannelRole::Creator || Id(actor.user_id) == target {
        return Err(forbidden());
    }
    let previous = role(&tx, channel, target).await?.ok_or_else(not_found)?;
    let next = body.role.channel_role();
    let event = json!({
        "channel_id": channel, "roles": [{ "user_id": target, "role": body.role.as_str() }]
    });
    let promote = next == ChannelRole::Moderator;
    if next != previous {
        let before = visible_private_rooms(&tx, channel, target).await?;
        let removed_denials = set_role(&tx, channel, target, next).await?;
        if !promote {
            cancel_invitations(&tx, channel, Some(target)).await?;
        }
        events::emit(
            &tx,
            events::Scope::Channel(channel.0),
            "ROLE_UPDATE",
            event.clone(),
        )
        .await?;
        change_role_visibility(&tx, channel, target, &before, &removed_denials).await?;
        if !promote {
            voice::revoke_invalid(&tx).await?;
        }
    }
    tx.commit().await?;
    if !promote && previous == ChannelRole::Moderator {
        let _ = voice::flush_after_commit(client, env).await;
    }
    reply(event, 200)
}

async fn transfer_ownership(
    client: &mut Client,
    actor: &Actor,
    channel: Id,
    body: Target,
) -> Result<Response> {
    let target = body.target_user_id;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (_, actor_role) = channel_member(&tx, channel, Id(actor.user_id)).await?;
    if actor_role != ChannelRole::Creator || Id(actor.user_id) == target {
        return Err(forbidden());
    }
    role(&tx, channel, target).await?.ok_or_else(not_found)?;
    let available: bool = tx
        .query_typed_one(
            "SELECT is_available FROM users WHERE id = $1",
            &[(&target.0, Type::INT8)],
        )
        .await?
        .try_get(0)?;
    if !available {
        return Err(conflict("MEMBER_UNAVAILABLE"));
    }
    let before = visible_private_rooms(&tx, channel, target).await?;
    set_role(&tx, channel, Id(actor.user_id), ChannelRole::Moderator).await?;
    let removed_denials = set_role(&tx, channel, target, ChannelRole::Creator).await?;
    tx.query_typed(
        "UPDATE channels SET creator_id = $2 WHERE id = $1",
        &[(&channel.0, Type::INT8), (&target.0, Type::INT8)],
    )
    .await?;
    let event = json!({
        "channel_id": channel,
        "roles": [
            { "user_id": Id(actor.user_id), "role": "MODERATOR" },
            { "user_id": target, "role": "CREATOR" }
        ]
    });
    events::emit(
        &tx,
        events::Scope::Channel(channel.0),
        "ROLE_UPDATE",
        event.clone(),
    )
    .await?;
    change_role_visibility(&tx, channel, target, &before, &removed_denials).await?;
    tx.commit().await?;
    reply(event, 200)
}

async fn set_role(
    conn: &Transaction<'_>,
    channel: Id,
    user: Id,
    role: ChannelRole,
) -> Result<Vec<i64>> {
    conn.query_typed(
        "UPDATE channel_members SET role_id = $3 WHERE channel_id = $1 AND user_id = $2",
        &[
            (&channel.0, Type::INT8),
            (&user.0, Type::INT8),
            (&(role as i32), Type::INT4),
        ],
    )
    .await?;
    if role == ChannelRole::Moderator {
        return Ok(Vec::new());
    }
    clear_channel_denials(conn, channel, user).await
}

async fn clear_channel_denials(conn: &Transaction<'_>, channel: Id, user: Id) -> Result<Vec<i64>> {
    Ok(conn
        .query_typed(
            "DELETE FROM room_moderator_denials d USING rooms r
         WHERE d.room_id = r.id AND r.channel_id = $1 AND d.user_id = $2
         RETURNING d.room_id",
            &[(&channel.0, Type::INT8), (&user.0, Type::INT8)],
        )
        .await?
        .into_iter()
        .map(|row| row.try_get(0))
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

struct Category {
    id: i64,
    channel_id: i64,
    name: String,
    position: i32,
}

impl Category {
    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            channel_id: row.try_get("channel_id")?,
            name: row.try_get("name")?,
            position: row.try_get("position")?,
        })
    }
    fn view(&self) -> Value {
        json!({
            "category_id": Id(self.id),
            "channel_id": Id(self.channel_id),
            "category_name": self.name,
            "position": self.position
        })
    }

    async fn emit(&self, conn: &Transaction<'_>, kind: &str) -> Result<()> {
        events::emit(
            conn,
            events::Scope::Channel(self.channel_id),
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

async fn category_order(conn: &Transaction<'_>, channel: Id) -> Result<Vec<i64>> {
    Ok(conn
        .query_typed(
            "SELECT id FROM room_categories WHERE channel_id = $1 ORDER BY position, id",
            &[(&channel.0, Type::INT8)],
        )
        .await?
        .into_iter()
        .map(|row| row.try_get(0))
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn place(ids: &mut Vec<i64>, id: i64, requested: Option<u32>) -> Result<i32> {
    let position = requested
        .map(|position| position as usize)
        .unwrap_or(ids.len());
    if position > ids.len() {
        return Err(bad("INVALID_POSITION"));
    }
    let index = i32::try_from(position).map_err(|_| bad("INVALID_POSITION"))?;
    ids.insert(position, id);
    Ok(index)
}

fn check_order(previous: &[i64], next: &[i64]) -> Result<()> {
    let previous: BTreeSet<_> = previous.iter().collect();
    let next_set: BTreeSet<_> = next.iter().collect();
    if previous != next_set || next_set.len() != next.len() {
        return Err(bad("INVALID_ORDER"));
    }
    Ok(())
}

async fn write_category_order(
    conn: &Transaction<'_>,
    ids: &[i64],
    skip_event_for: Option<Id>,
) -> Result<()> {
    let updated: Vec<Category> = conn
        .query_typed(
            "UPDATE room_categories c SET position = sort.ordinal - 1
         FROM unnest($1::bigint[]) WITH ORDINALITY AS sort(id, ordinal)
         WHERE c.id = sort.id AND c.position <> sort.ordinal - 1 RETURNING c.*",
            &[(&ids, Type::INT8_ARRAY)],
        )
        .await?
        .into_iter()
        .map(Category::from_row)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for category in updated {
        if skip_event_for != Some(Id(category.id)) {
            category.emit(conn, "ROOM_CATEGORY_UPDATE").await?;
        }
    }
    Ok(())
}

async fn create_category(
    client: &mut Client,
    actor: &Actor,
    channel: Id,
    body: CreateCategory,
) -> Result<Response> {
    let name = name(&body.category_name)?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    let mut ids = category_order(&tx, channel).await?;
    let category_id = Id(db::snowflake(&tx).await?);
    let position = place(&mut ids, category_id.0, body.position)?;
    let category: Category = Category::from_row(
        tx.query_typed_one(
            "INSERT INTO room_categories (id, channel_id, name, position)
         VALUES ($1, $2, $3, $4) RETURNING *",
            &[
                (&category_id.0, Type::INT8),
                (&channel.0, Type::INT8),
                (&name, Type::TEXT),
                (&position, Type::INT4),
            ],
        )
        .await?,
    )?;
    category.emit(&tx, "ROOM_CATEGORY_CREATE").await?;
    write_category_order(&tx, &ids, Some(category_id)).await?;
    tx.commit().await?;
    reply(category.view(), 201)
}

async fn update_category(
    client: &mut Client,
    actor: &Actor,
    (channel, category): (Id, Id),
    body: UpdateCategory,
) -> Result<Response> {
    let new_name = body.category_name.as_deref().map(name).transpose()?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    if let Some(position) = body.position {
        let mut ids = category_order(&tx, channel).await?;
        let current = ids
            .iter()
            .position(|id| *id == category.0)
            .ok_or_else(not_found)?;
        ids.remove(current);
        place(&mut ids, category.0, Some(position))?;
        write_category_order(&tx, &ids, Some(category)).await?;
    }
    let category: Category = tx
        .query_typed_opt(
            "UPDATE room_categories SET name = COALESCE($3, name)
         WHERE channel_id = $1 AND id = $2 RETURNING *",
            &[
                (&channel.0, Type::INT8),
                (&category.0, Type::INT8),
                (&new_name, Type::TEXT),
            ],
        )
        .await?
        .map(Category::from_row)
        .transpose()?
        .ok_or_else(not_found)?;
    category.emit(&tx, "ROOM_CATEGORY_UPDATE").await?;
    tx.commit().await?;
    reply(category.view(), 200)
}

async fn delete_category(
    client: &mut Client,
    actor: &Actor,
    (channel, category): (Id, Id),
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    require_category(&tx, channel, Some(category.0)).await?;
    let mut ungrouped = room_order(&tx, channel, None, Id(actor.user_id)).await?;
    let grouped = room_order(&tx, channel, Some(category.0), Id(actor.user_id)).await?;
    if !grouped.reserved.is_empty() {
        return Err(forbidden());
    }
    ungrouped.ids.extend(grouped.ids);
    write_room_order(&tx, None, &ungrouped).await?;
    tx.query_typed(
        "UPDATE rooms SET category_id = NULL WHERE channel_id = $1 AND category_id = $2",
        &[(&channel.0, Type::INT8), (&category.0, Type::INT8)],
    )
    .await?;
    tx.query_typed(
        "DELETE FROM room_categories WHERE channel_id = $1 AND id = $2",
        &[(&channel.0, Type::INT8), (&category.0, Type::INT8)],
    )
    .await?;
    events::emit(
        &tx,
        events::Scope::Channel(channel.0),
        "ROOM_CATEGORY_DELETE",
        json!({ "channel_id": channel, "category_id": category }),
    )
    .await?;
    let ids = category_order(&tx, channel).await?;
    write_category_order(&tx, &ids, None).await?;
    tx.commit().await?;
    empty()
}

async fn order_categories(
    client: &mut Client,
    actor: &Actor,
    channel: Id,
    body: OrderCategories,
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    let previous = category_order(&tx, channel).await?;
    let ids: Vec<_> = body.category_ids.iter().map(|id| id.0).collect();
    check_order(&previous, &ids)?;
    write_category_order(&tx, &ids, None).await?;
    tx.commit().await?;
    empty()
}

async fn require_category(
    conn: &Transaction<'_>,
    channel: Id,
    category: Option<i64>,
) -> Result<()> {
    if let Some(category) = category {
        let exists: bool = conn
            .query_typed_one(
                "SELECT EXISTS (SELECT 1 FROM room_categories WHERE channel_id = $1 AND id = $2)",
                &[(&channel.0, Type::INT8), (&category, Type::INT8)],
            )
            .await?
            .try_get(0)?;
        if !exists {
            return Err(not_found());
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
    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            channel_id: row.try_get("channel_id")?,
            category_id: row.try_get("category_id")?,
            name: row.try_get("name")?,
            kind: row.try_get("kind")?,
            private: row.try_get("private")?,
            position: row.try_get("position")?,
            last_sequence: row.try_get("last_sequence")?,
        })
    }
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

    async fn emit(&self, conn: &Transaction<'_>, kind: &str, viewers: Vec<Id>) -> Result<()> {
        if viewers.is_empty() {
            return Ok(());
        }
        events::emit(
            conn,
            events::Scope::RoomUsers(self.id, viewers.into_iter().map(|id| id.0).collect()),
            kind,
            self.view(),
        )
        .await
    }

    async fn revoke(&self, conn: &Transaction<'_>, users: Vec<Id>) -> Result<()> {
        if users.is_empty() {
            return Ok(());
        }
        events::emit(
            conn,
            events::Scope::Revocation(users.into_iter().map(|id| id.0).collect()),
            "ROOM_DELETE",
            json!({ "room_id": Id(self.id), "channel_id": Id(self.channel_id) }),
        )
        .await
    }

    async fn initialize_reads(&self, conn: &Transaction<'_>, users: &[Id]) -> Result<()> {
        if self.kind != "TEXT" || users.is_empty() {
            return Ok(());
        }
        let ids: Vec<_> = users.iter().map(|id| id.0).collect();
        conn.query_typed(
            "INSERT INTO message_reads (room_id, user_id, last_sequence)
             SELECT $1, user_id, $2 FROM unnest($3::bigint[]) AS users(user_id)
             ON CONFLICT (room_id, user_id) DO UPDATE
             SET last_sequence = GREATEST(message_reads.last_sequence, EXCLUDED.last_sequence),
                 read_at = CASE WHEN message_reads.last_sequence < EXCLUDED.last_sequence THEN now()
                           ELSE message_reads.read_at END",
            &[
                (&self.id, Type::INT8),
                (&self.last_sequence, Type::INT8),
                (&ids, Type::INT8_ARRAY),
            ],
        )
        .await?;
        Ok(())
    }
}

async fn change_role_visibility(
    conn: &Transaction<'_>,
    channel: Id,
    user: Id,
    before: &BTreeSet<i64>,
    removed_denials: &[i64],
) -> Result<()> {
    let after = visible_private_rooms(conn, channel, user).await?;
    if before == &after {
        return Ok(());
    }
    let changed: Vec<_> = before.symmetric_difference(&after).copied().collect();
    let rooms: Vec<Room> = conn
        .query_typed(
            "SELECT * FROM rooms WHERE id = ANY($1) ORDER BY id FOR UPDATE",
            &[(&changed, Type::INT8_ARRAY)],
        )
        .await?
        .into_iter()
        .map(Room::from_row)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for room in rooms {
        if after.contains(&room.id) {
            if !removed_denials.contains(&room.id) {
                room.initialize_reads(conn, &[user]).await?;
            }
            room.emit(conn, "ROOM_CREATE", vec![user]).await?;
        } else {
            room.revoke(conn, vec![user]).await?;
        }
    }
    Ok(())
}

async fn visible_private_rooms(
    conn: &Transaction<'_>,
    channel: Id,
    user: Id,
) -> Result<BTreeSet<i64>> {
    let ids: Vec<i64> = conn
        .query_typed(
            "SELECT r.id FROM rooms r JOIN channel_room_access access ON access.room_id = r.id
         WHERE r.channel_id = $1 AND r.private AND access.user_id = $2",
            &[(&channel.0, Type::INT8), (&user.0, Type::INT8)],
        )
        .await?
        .into_iter()
        .map(|row| row.try_get(0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().collect())
}

struct RoomOrder {
    ids: Vec<i64>,
    reserved: BTreeSet<i32>,
}

impl RoomOrder {
    fn positions(&self) -> impl Iterator<Item = i32> + '_ {
        (0..).filter(|position| !self.reserved.contains(position))
    }

    fn place(&mut self, id: i64, requested: Option<u32>) -> Result<i32> {
        let placement = {
            let mut slots = self.positions().take(self.ids.len() + 1).enumerate();
            match requested {
                Some(requested) => slots.find(|&(_, position)| position as u32 == requested),
                None => slots.last(),
            }
        };
        let (index, position) = placement.ok_or_else(|| bad("INVALID_POSITION"))?;
        self.ids.insert(index, id);
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_routes_match_the_rest_contract() {
        use ChannelRoute::*;
        use Method::{Delete as DeleteMethod, Get, Patch, Post, Put};
        let channel = Id(9_007_199_254_740_993);
        let target = Id(9_007_199_254_740_994);
        let cases = [
            (Post, "/channels", Create),
            (Patch, "/channels/9007199254740993", Update(channel)),
            (DeleteMethod, "/channels/9007199254740993", Delete(channel)),
            (
                Post,
                "/channels/9007199254740993/invitations",
                Invite(channel),
            ),
            (
                Patch,
                "/invitations/9007199254740994",
                ResolveInvitation(target),
            ),
            (Get, "/channel-links/abc", PreviewLink("abc".into())),
            (Post, "/channel-links/abc/join", JoinLink("abc".into())),
            (
                DeleteMethod,
                "/channels/9007199254740993/members/me",
                RemoveMember(channel, "me".into()),
            ),
            (
                DeleteMethod,
                "/channels/9007199254740993/members/9007199254740994",
                RemoveMember(channel, target.0.to_string()),
            ),
            (
                Patch,
                "/channels/9007199254740993/members/9007199254740994/role",
                UpdateRole(channel, target),
            ),
            (
                Post,
                "/channels/9007199254740993/transfer-ownership",
                TransferOwnership(channel),
            ),
            (
                Post,
                "/channels/9007199254740993/categories",
                CreateCategory(channel),
            ),
            (
                Patch,
                "/channels/9007199254740993/categories/9007199254740994",
                UpdateCategory(channel, target),
            ),
            (
                DeleteMethod,
                "/channels/9007199254740993/categories/9007199254740994",
                DeleteCategory(channel, target),
            ),
            (
                Put,
                "/channels/9007199254740993/categories/order",
                OrderCategories(channel),
            ),
            (
                Post,
                "/channels/9007199254740993/rooms",
                CreateRoom(channel),
            ),
            (
                Patch,
                "/channels/9007199254740993/rooms/9007199254740994",
                UpdateRoom(channel, target),
            ),
            (
                DeleteMethod,
                "/channels/9007199254740993/rooms/9007199254740994",
                DeleteRoom(channel, target),
            ),
            (
                Put,
                "/channels/9007199254740993/rooms/order",
                OrderRooms(channel),
            ),
        ];
        for (method, path, expected) in cases {
            assert_eq!(
                parse_route(method, &format!("/api/v1{path}")).unwrap(),
                Some(expected)
            );
        }
        assert!(parse_route(Patch, "/api/v1/channels/01").is_err());
        assert!(
            parse_route(Get, "/api/v1/channels/9007199254740993")
                .unwrap()
                .is_none()
        );
        assert!(serde_json::from_str::<Id>("9007199254740993").is_err());
        assert_eq!(
            serde_json::to_value(channel).unwrap(),
            json!("9007199254740993")
        );
    }

    #[test]
    fn channel_patch_distinguishes_explicit_null_and_missing_fields() {
        let missing: UpdateChannel = serde_json::from_value(json!({})).unwrap();
        assert_eq!(missing.description, None);
        assert_eq!(missing.icon_upload_id, None);
        let clear: UpdateChannel =
            serde_json::from_value(json!({"description": null, "icon_upload_id": null})).unwrap();
        assert_eq!(clear.description, Some(None));
        assert_eq!(clear.icon_upload_id, Some(None));
        assert!(serde_json::from_value::<UpdateChannel>(json!({"join_mode": null})).is_err());
    }

    #[test]
    fn room_position_includes_hidden_slots() {
        let mut order = RoomOrder {
            ids: vec![11],
            reserved: BTreeSet::from([0, 2]),
        };

        assert_eq!(order.place(22, Some(3)).ok(), Some(3));
        assert_eq!(order.ids, [11, 22]);
    }
}

async fn room_order(
    conn: &Transaction<'_>,
    channel: Id,
    category: Option<i64>,
    manager: Id,
) -> Result<RoomOrder> {
    let rows: Vec<(i64, i32, bool)> = conn
        .query_typed(
            "SELECT r.id, r.position, d.user_id IS NOT NULL
         FROM rooms r LEFT JOIN room_moderator_denials d ON d.room_id = r.id AND d.user_id = $3
         WHERE r.channel_id = $1 AND r.is_available
           AND r.category_id IS NOT DISTINCT FROM $2 ORDER BY r.position, r.id",
            &[
                (&channel.0, Type::INT8),
                (&category, Type::INT8),
                (&manager.0, Type::INT8),
            ],
        )
        .await?
        .into_iter()
        .map(|row| -> Result<_> { Ok((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?)) })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut order = RoomOrder {
        ids: Vec::new(),
        reserved: BTreeSet::new(),
    };
    for (id, position, denied) in rows {
        if denied {
            order.reserved.insert(position);
        } else {
            order.ids.push(id);
        }
    }
    Ok(order)
}

async fn write_room_order(
    conn: &Transaction<'_>,
    category: Option<i64>,
    order: &RoomOrder,
) -> Result<()> {
    let positions: Vec<_> = order.positions().take(order.ids.len()).collect();
    let updated: Vec<Room> = conn
        .query_typed(
            "UPDATE rooms r SET category_id = $1, position = sort.position
         FROM unnest($2::bigint[], $3::integer[]) AS sort(id, position)
         WHERE r.id = sort.id AND (r.category_id IS DISTINCT FROM $1 OR r.position <> sort.position)
         RETURNING r.*",
            &[
                (&category, Type::INT8),
                (&order.ids, Type::INT8_ARRAY),
                (&positions, Type::INT4_ARRAY),
            ],
        )
        .await?
        .into_iter()
        .map(Room::from_row)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for room in updated {
        events::emit(
            conn,
            events::Scope::Room(room.id),
            "ROOM_UPDATE",
            room.view(),
        )
        .await?;
    }
    Ok(())
}

async fn locked_room(conn: &Transaction<'_>, channel: Id, room: Id) -> Result<Room> {
    conn.query_typed_opt(
        "SELECT * FROM rooms WHERE channel_id = $1 AND id = $2 AND is_available FOR UPDATE",
        &[(&channel.0, Type::INT8), (&room.0, Type::INT8)],
    )
    .await?
    .map(Room::from_row)
    .transpose()?
    .ok_or_else(not_found)
}

type RoomMembers = BTreeMap<i64, bool>;

fn member_ids(ids: &[Id]) -> Result<BTreeSet<i64>> {
    let unique: BTreeSet<_> = ids.iter().map(|id| id.0).collect();
    if unique.len() != ids.len() {
        return Err(bad("DUPLICATE_MEMBER"));
    }
    Ok(unique)
}

async fn assigned_roles(
    conn: &Transaction<'_>,
    channel: Id,
    users: &BTreeSet<i64>,
) -> Result<BTreeMap<i64, i32>> {
    let ids: Vec<_> = users.iter().copied().collect();
    let roles: Vec<(i64, i32)> = conn.query_typed("SELECT user_id, role_id FROM channel_members WHERE channel_id = $1 AND user_id = ANY($2)", &[(&channel.0, Type::INT8), (&ids, Type::INT8_ARRAY)]).await?.into_iter().map(|row| -> Result<_> { Ok((row.try_get(0)?, row.try_get(1)?)) }).collect::<std::result::Result<Vec<_>, _>>()?;
    if roles.len() != users.len() {
        return Err(bad("INVALID_ROOM_MEMBER"));
    }
    Ok(roles.into_iter().collect())
}

async fn write_room_members(conn: &Transaction<'_>, room: Id, members: &RoomMembers) -> Result<()> {
    let (users, moderators): (Vec<_>, Vec<_>) =
        members.iter().map(|(&id, &role)| (id, role)).unzip();
    conn.query_typed(
        "DELETE FROM room_members WHERE room_id = $1 AND user_id <> ALL($2)",
        &[(&room.0, Type::INT8), (&users, Type::INT8_ARRAY)],
    )
    .await?;
    conn.query_typed("INSERT INTO room_members (room_id, user_id, is_room_mod)
         SELECT $1, user_id, is_room_mod FROM unnest($2::bigint[], $3::boolean[]) AS members(user_id, is_room_mod)
         ON CONFLICT (room_id, user_id) DO UPDATE SET is_room_mod = EXCLUDED.is_room_mod
         WHERE room_members.is_room_mod IS DISTINCT FROM EXCLUDED.is_room_mod", &[(&room.0, Type::INT8), (&users, Type::INT8_ARRAY), (&moderators, Type::BOOL_ARRAY)]).await?;
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
    client: &mut Client,
    actor: &Actor,
    channel: Id,
    body: CreateRoom,
) -> Result<Response> {
    let name = name(&body.room_name)?;
    let members = member_ids(&body.member_ids)?;
    if !body.is_private && !members.is_empty() {
        return Err(bad("PUBLIC_ROOM_MEMBERS"));
    }
    let category = body.category_id.map(|id| id.0);
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    require_category(&tx, channel, category).await?;
    assigned_roles(&tx, channel, &members).await?;
    let mut order = room_order(&tx, channel, category, Id(actor.user_id)).await?;
    let room_id = Id(db::snowflake(&tx).await?);
    let position = order.place(room_id.0, body.position)?;
    let room: Room = Room::from_row(
        tx.query_typed_one(
            "INSERT INTO rooms (id, channel_id, category_id, name, kind, private, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
            &[
                (&room_id.0, Type::INT8),
                (&channel.0, Type::INT8),
                (&category, Type::INT8),
                (&name, Type::TEXT),
                (&body.kind.as_str(), Type::TEXT),
                (&body.is_private, Type::BOOL),
                (&position, Type::INT4),
            ],
        )
        .await?,
    )?;
    if !members.is_empty() {
        let members = members.into_iter().map(|id| (id, false)).collect();
        write_room_members(&tx, room_id, &members).await?;
    }
    let viewers = room_viewers(&tx, room_id).await?;
    room.initialize_reads(&tx, &viewers).await?;
    room.emit(&tx, "ROOM_CREATE", viewers).await?;
    write_room_order(&tx, category, &order).await?;
    tx.commit().await?;
    reply(room.view(), 201)
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
    #[serde(default, deserialize_with = "supplied")]
    denied_moderator_ids: Option<Vec<Id>>,
}

async fn update_room_denials(
    conn: &Transaction<'_>,
    room: &Room,
    body: &UpdateRoom,
) -> Result<Vec<Id>> {
    let private = body.is_private.unwrap_or(room.private);
    if private && body.denied_moderator_ids.is_none() {
        return Ok(Vec::new());
    }
    let ids = member_ids(body.denied_moderator_ids.as_deref().unwrap_or_default())?;
    if !ids.is_empty() {
        if !private
            || assigned_roles(conn, Id(room.channel_id), &ids)
                .await?
                .values()
                .any(|role| *role != ChannelRole::Moderator as i32)
        {
            return Err(bad("INVALID_DENIED_MODERATOR"));
        }
    }
    let ids: Vec<_> = ids.into_iter().collect();
    let removed: Vec<i64> = conn
        .query_typed(
            "DELETE FROM room_moderator_denials WHERE room_id = $1 AND user_id <> ALL($2)
         RETURNING user_id",
            &[(&room.id, Type::INT8), (&ids, Type::INT8_ARRAY)],
        )
        .await?
        .into_iter()
        .map(|row| row.try_get(0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    conn.query_typed(
        "INSERT INTO room_moderator_denials (room_id, user_id)
         SELECT $1, user_id FROM unnest($2::bigint[]) AS ids(user_id) ON CONFLICT DO NOTHING",
        &[(&room.id, Type::INT8), (&ids, Type::INT8_ARRAY)],
    )
    .await?;
    Ok(removed.into_iter().map(Id).collect())
}

async fn update_room_members(
    conn: &Transaction<'_>,
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
            return Err(bad("PUBLIC_ROOM_MEMBERS"));
        }
        if room.private {
            write_room_members(conn, Id(room.id), &RoomMembers::new()).await?;
        }
        return Ok(());
    }
    if !room.private && body.member_ids.is_none() {
        return Err(bad("ROOM_MEMBERS_REQUIRED"));
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
            return Err(bad("INVALID_ROOM_MODERATOR"));
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
            if role != ChannelRole::Member as i32 {
                if previous.contains_key(&id) {
                    users.insert(id);
                } else {
                    return Err(forbidden());
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
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    (channel, id): (Id, Id),
    body: UpdateRoom,
) -> Result<Response> {
    let new_name = body.room_name.as_deref().map(name).transpose()?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let (_, role) = channel_member(&tx, channel, Id(actor.user_id)).await?;
    let room = locked_room(&tx, channel, id).await?;
    require_room_viewer(&tx, Id(actor.user_id), id).await?;
    if body.denied_moderator_ids.is_some() && role != ChannelRole::Creator {
        return Err(forbidden());
    }
    let members: Vec<(i64, bool)> = tx
        .query_typed(
            "SELECT user_id, is_room_mod FROM room_members WHERE room_id = $1",
            &[(&id.0, Type::INT8)],
        )
        .await?
        .into_iter()
        .map(|row| -> Result<_> { Ok((row.try_get(0)?, row.try_get(1)?)) })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let members: RoomMembers = members.into_iter().collect();
    let manager = role.manages_channel();
    if !manager {
        let assignment = members.get(&actor.user_id);
        if room.private && assignment.is_none() {
            return Err(not_found());
        }
        if !room.private
            || assignment != Some(&true)
            || body.is_private.is_some()
            || body.category_id.is_some()
            || body.position.is_some()
            || body.room_mod_ids.is_some()
        {
            return Err(forbidden());
        }
    }
    let before = viewers(&tx, id).await?;
    let category = body
        .category_id
        .map(|id| id.map(|id| id.0))
        .unwrap_or(room.category_id);
    require_category(&tx, channel, category).await?;
    update_room_members(&tx, &room, manager, &members, &body).await?;
    let removed_denials = update_room_denials(&tx, &room, &body).await?;
    let moving = category != room.category_id;
    let repositioning = body
        .position
        .is_some_and(|position| position != room.position as u32);
    let new_order = if moving || repositioning {
        let mut order = room_order(&tx, channel, category, Id(actor.user_id)).await?;
        order.ids.retain(|existing| *existing != id.0);
        let position = order.place(id.0, body.position)?;
        Some((order, position))
    } else {
        None
    };
    let updated: Room = Room::from_row(
        tx.query_typed_one(
            "UPDATE rooms SET name = COALESCE($2, name), private = COALESCE($3, private),
         category_id = $4, position = $5 WHERE id = $1 RETURNING *",
            &[
                (&id.0, Type::INT8),
                (&new_name, Type::TEXT),
                (&body.is_private, Type::BOOL),
                (&category, Type::INT8),
                (
                    &new_order.as_ref().map(|(_, p)| *p).unwrap_or(room.position),
                    Type::INT4,
                ),
            ],
        )
        .await?,
    )?;
    let after = viewers(&tx, id).await?;
    let gained: Vec<_> = after.difference(&before).copied().map(Id).collect();
    let retained: Vec<_> = after.intersection(&before).copied().map(Id).collect();
    let lost: Vec<_> = before.difference(&after).copied().map(Id).collect();
    let initialize_reads: Vec<_> = gained
        .iter()
        .copied()
        .filter(|user| !updated.private || !removed_denials.contains(user))
        .collect();
    updated.initialize_reads(&tx, &initialize_reads).await?;
    updated.emit(&tx, "ROOM_CREATE", gained).await?;
    updated.emit(&tx, "ROOM_UPDATE", retained).await?;
    let revoked = !lost.is_empty();
    if revoked {
        updated.revoke(&tx, lost).await?;
        voice::revoke_invalid(&tx).await?;
    }
    if let Some((order, _)) = new_order {
        write_room_order(&tx, category, &order).await?;
    }
    if moving {
        let old_order = room_order(&tx, channel, room.category_id, Id(actor.user_id)).await?;
        write_room_order(&tx, room.category_id, &old_order).await?;
    }
    tx.commit().await?;
    if revoked {
        let _ = voice::flush_after_commit(client, env).await;
    }
    reply(updated.view(), 200)
}

async fn viewers(conn: &Transaction<'_>, room: Id) -> Result<BTreeSet<i64>> {
    Ok(room_viewers(conn, room)
        .await?
        .into_iter()
        .map(|id| id.0)
        .collect())
}

async fn delete_room(
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    (channel, id): (Id, Id),
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    let room = locked_room(&tx, channel, id).await?;
    require_room_viewer(&tx, Id(actor.user_id), id).await?;
    let viewers = room_viewers(&tx, id).await?;
    tx.query_typed(
        "UPDATE rooms SET is_available = false WHERE id = $1",
        &[(&id.0, Type::INT8)],
    )
    .await?;
    tx.query_typed(
        "DELETE FROM room_moderator_denials WHERE room_id = $1",
        &[(&id.0, Type::INT8)],
    )
    .await?;
    let remaining = room_order(&tx, channel, room.category_id, Id(actor.user_id)).await?;
    write_room_order(&tx, room.category_id, &remaining).await?;
    room.revoke(&tx, viewers).await?;
    voice::revoke_invalid(&tx).await?;
    tx.commit().await?;
    let _ = voice::flush_after_commit(client, env).await;
    empty()
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
    client: &mut Client,
    actor: &Actor,
    channel: Id,
    body: OrderRooms,
) -> Result<Response> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    channel_manager(&tx, channel, Id(actor.user_id)).await?;
    let mut groups = BTreeSet::new();
    let mut supplied = 0;
    for group in body.groups {
        let category = group.category_id.map(|id| id.0);
        if !groups.insert(category) {
            return Err(bad("INVALID_ORDER"));
        }
        require_category(&tx, channel, category).await?;
        let mut order = room_order(&tx, channel, category, Id(actor.user_id)).await?;
        let ids: Vec<_> = group.room_ids.into_iter().map(|id| id.0).collect();
        check_order(&order.ids, &ids)?;
        supplied += ids.len();
        order.ids = ids;
        write_room_order(&tx, category, &order).await?;
    }
    let total: i64 = tx
        .query_typed_one(
            "SELECT count(*) FROM rooms r JOIN channel_room_access a ON a.room_id = r.id
         WHERE r.channel_id = $1 AND a.user_id = $2",
            &[(&channel.0, Type::INT8), (&actor.user_id, Type::INT8)],
        )
        .await?
        .try_get(0)?;
    if !groups.contains(&None) || supplied as i64 != total {
        return Err(bad("INVALID_ORDER"));
    }
    tx.commit().await?;
    empty()
}
