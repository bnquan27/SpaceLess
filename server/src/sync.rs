use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{Actor, ApiError, AppState, Id, Result, auth, chat, db};

#[derive(Clone, Deserialize, Serialize)]
pub enum Scope {
    Users(Vec<Id>),
    Channel(Id),
    Room(Id),
    RoomUsers(Id, Vec<Id>),
    Revocation(Vec<Id>),
    Session(Uuid),
}

#[derive(FromRow)]
struct Event {
    id: i64,
    stream: String,
    revision: i64,
    kind: String,
    scope: sqlx::types::Json<Scope>,
    data: Value,
    occurred_at: DateTime<Utc>,
}

#[derive(Clone)]
enum Signal {
    Event(Arc<Event>),
    Disconnect(Id, Option<Uuid>),
}

#[derive(Clone)]
pub struct Realtime(broadcast::Sender<Signal>);

impl Default for Realtime {
    fn default() -> Self {
        Self(broadcast::channel(512).0)
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/bootstrap", get(bootstrap))
        .route("/ws", get(upgrade))
}

pub async fn emit(conn: &mut PgConnection, scope: Scope, kind: &str, data: Value) -> Result<()> {
    match &scope {
        Scope::Users(users) | Scope::Revocation(users) => {
            let mut users = users.clone();
            users.sort_unstable();
            users.dedup();
            for user in users {
                let receiver = match scope {
                    Scope::Revocation(_) => Scope::Revocation(vec![user]),
                    _ => Scope::Users(vec![user]),
                };
                insert(conn, format!("user:{user}"), receiver, kind, &data).await?;
            }
        }
        _ => {
            let stream = match scope {
                Scope::Channel(channel) => format!("channel:{channel}"),
                Scope::Room(room) | Scope::RoomUsers(room, _) => format!("room:{room}"),
                Scope::Session(_) => format!(
                    "voice-session:{}",
                    data["voice_session_id"]
                        .as_str()
                        .ok_or_else(ApiError::internal)?
                ),
                _ => unreachable!(),
            };
            insert(conn, stream, scope, kind, &data).await?;
        }
    }
    Ok(())
}

async fn insert(
    conn: &mut PgConnection,
    stream: String,
    scope: Scope,
    kind: &str,
    data: &Value,
) -> Result<()> {
    sqlx::query("WITH next AS (
        INSERT INTO stream_revisions(stream,revision) VALUES($1,1)
        ON CONFLICT(stream) DO UPDATE SET revision=stream_revisions.revision+1 RETURNING revision
      ) INSERT INTO outbox_events(stream,revision,scope,kind,data) SELECT $1,revision,$2,$3,$4 FROM next")
        .bind(stream).bind(sqlx::types::Json(scope)).bind(kind).bind(data).execute(conn).await?;
    Ok(())
}

pub async fn revision(conn: &mut PgConnection, stream: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT revision FROM stream_revisions WHERE stream=$1")
            .bind(stream)
            .fetch_optional(conn)
            .await?
            .unwrap_or(0),
    )
}

pub async fn disconnect(state: &AppState, user: Id, session: Option<Uuid>) {
    let _ = state.realtime.0.send(Signal::Disconnect(user, session));
}

pub async fn dispatch(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        if let Err(error) = dispatch_batch(&state).await {
            tracing::warn!(%error, "WebSocket outbox dispatch will retry");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

async fn dispatch_batch(state: &AppState) -> Result<()> {
    let events = sqlx::query_as::<_, Event>(
        "SELECT id,stream,revision,kind,scope,data,occurred_at FROM outbox_events
        WHERE dispatched_at IS NULL ORDER BY id LIMIT 128",
    )
    .fetch_all(&state.pool)
    .await?;
    if events.is_empty() {
        return Ok(());
    }
    let ids: Vec<_> = events.iter().map(|event| event.id).collect();
    for event in events {
        let _ = state.realtime.0.send(Signal::Event(Arc::new(event)));
    }
    sqlx::query("UPDATE outbox_events SET dispatched_at=now() WHERE id=ANY($1)")
        .bind(ids)
        .execute(&state.pool)
        .await?;
    Ok(())
}

async fn upgrade(
    State(state): State<AppState>,
    actor: Actor,
    websocket: WebSocketUpgrade,
) -> Response {
    let rx = state.realtime.0.subscribe();
    websocket
        .max_message_size(4_096)
        .max_frame_size(4_096)
        .on_upgrade(move |socket| async move {
            let _ = socket_loop(&state, actor, socket, rx).await;
        })
}

async fn socket_loop(
    state: &AppState,
    actor: Actor,
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<Signal>,
) -> Result<()> {
    auth::session_valid(state, &actor).await?;
    let ping = Duration::from_secs(25);
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + ping, ping);
    let expiry = tokio::time::sleep(Duration::from_secs(
        actor.exp.saturating_sub(Utc::now().timestamp()).max(0) as u64,
    ));
    tokio::pin!(expiry);
    let mut awaiting_pong = false;
    loop {
        tokio::select! {
            _ = &mut expiry => break,
            _ = tick.tick() => {
                if awaiting_pong || auth::session_valid(state, &actor).await.is_err() { break; }
                awaiting_pong = true;
                if !send(&mut socket, Message::Ping(Vec::new().into())).await { break; }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Pong(_))) => awaiting_pong = false,
                Some(Ok(Message::Ping(_))) => {},
                _ => break,
            },
            signal = rx.recv() => match signal {
                Ok(Signal::Disconnect(user,jti)) if user == actor.user_id && jti.is_none_or(|jti| jti == actor.jti) => break,
                Ok(Signal::Disconnect(..)) => {},
                Ok(Signal::Event(event)) => {
                    if let Some(value) = delivery(state, &actor, &event).await? {
                        if !send(&mut socket, Message::Text(value.to_string().into())).await { break; }
                    }
                }
                Err(_) => break,
            },
        }
    }
    let _ = send(&mut socket, Message::Close(None)).await;
    Ok(())
}

async fn send(socket: &mut WebSocket, message: Message) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(5), socket.send(message)).await,
        Ok(Ok(()))
    )
}

async fn delivery(state: &AppState, actor: &Actor, event: &Event) -> Result<Option<Value>> {
    match &event.scope.0 {
        Scope::Users(ids) | Scope::Revocation(ids) | Scope::RoomUsers(_, ids)
            if !ids.contains(&actor.user_id) =>
        {
            return Ok(None);
        }
        Scope::Session(jti) if *jti != actor.jti => return Ok(None),
        _ => {}
    }
    auth::session_valid(state, actor).await?;
    let mut conn = state.pool.acquire().await?;
    match &event.scope.0 {
        Scope::Room(room) | Scope::RoomUsers(room, _) => {
            match db::room_access(&mut conn, actor.user_id, *room).await {
                Err(error) if error.status == axum::http::StatusCode::NOT_FOUND => return Ok(None),
                result => {
                    result?;
                }
            }
        }
        Scope::Channel(channel)
            if !db::channel_visible(&mut conn, actor.user_id, *channel).await? =>
        {
            return Ok(None);
        }
        _ => {}
    }
    let data = match event.kind.as_str() {
        "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_REACTION_UPDATE" => {
            let room: Id = serde_json::from_value(event.data["room_id"].clone())
                .map_err(|_| ApiError::internal())?;
            let message: Id = serde_json::from_value(event.data["message_id"].clone())
                .map_err(|_| ApiError::internal())?;
            let view = match chat::message_view(&mut conn, actor.user_id, room, message).await {
                Ok(view) => view,
                Err(error) if error.status == axum::http::StatusCode::NOT_FOUND => return Ok(None),
                Err(error) => return Err(error),
            };
            if event.kind == "MESSAGE_REACTION_UPDATE" {
                json!({"room_id":room,"message_id":message,"reaction_counts":view["reaction_counts"],"my_reaction":view["my_reaction"]})
            } else {
                view
            }
        }
        _ => event.data.clone(),
    };
    Ok(Some(
        json!({"event_id":event.id.to_string(),"type":event.kind,"stream":event.stream,
        "revision":event.revision.to_string(),"ordinal":0,"occurred_at":event.occurred_at,"data":data}),
    ))
}

async fn bootstrap(State(state): State<AppState>, actor: Actor) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let user = actor.user_id.0;
    let profile = auth::profile(&mut tx, actor.user_id).await?;
    let mut value: Value = sqlx::query_scalar(BOOTSTRAP)
        .bind(user)
        .fetch_one(&mut *tx)
        .await?;
    value["profile"] = profile;
    tx.commit().await?;
    Ok(Json(value))
}

const BOOTSTRAP: &str = "WITH my_channels AS (
  SELECT c.*,cm.role FROM channels c JOIN channel_members cm ON cm.channel_id=c.id WHERE c.is_available AND cm.user_id=$1
), visible AS (
  SELECT r.*,p.lo,p.hi FROM rooms r LEFT JOIN my_channels c ON c.id=r.channel_id LEFT JOIN dm_pairs p ON p.room_id=r.id
  WHERE r.is_available AND ((r.kind='DM' AND $1 IN (p.lo,p.hi)) OR
    (c.id IS NOT NULL AND (NOT r.private OR c.role IN ('CREATOR','MODERATOR') OR
      EXISTS(SELECT 1 FROM room_members rm WHERE rm.room_id=r.id AND rm.user_id=$1))))
), room_views AS (
  SELECT r.*, jsonb_build_object('room_id',r.id::text,'channel_id',r.channel_id::text,'category_id',r.category_id::text,
    'room_name',r.name,'type',r.kind,'is_private',r.private,'position',r.position,'last_message_sequence',r.last_sequence::text,
    'last_read_sequence',coalesce(mr.last_sequence,0)::text,
    'unread_count',(SELECT count(*) FROM (SELECT 1 FROM messages m WHERE m.room_id=r.id AND m.sequence>coalesce(mr.last_sequence,0) AND m.sender_id<>$1 LIMIT 100) unread),
    'is_read_only',r.kind='DM' AND NOT EXISTS(SELECT 1 FROM relations f WHERE f.lo=r.lo AND f.hi=r.hi AND f.status='FRIEND'),
    'member_ids',CASE WHEN r.kind='DM' THEN jsonb_build_array(r.lo::text,r.hi::text) END) AS view
  FROM visible r LEFT JOIN message_reads mr ON mr.room_id=r.id AND mr.user_id=$1
), pair_people AS (
  SELECT f.id,f.requester_id,f.status,f.created_at,CASE WHEN f.lo=$1 THEN f.hi ELSE f.lo END AS peer
  FROM relations f WHERE $1 IN(f.lo,f.hi)
), pair_views AS (
  SELECT p.*,jsonb_build_object('user_id',u.id::text,'username',u.username,'full_name',d.full_name,
    'avatar_url',CASE WHEN d.avatar_upload_id IS NOT NULL THEN '/api/v1/images/'||d.avatar_upload_id||'/url' END) AS person
  FROM pair_people p JOIN users u ON u.id=p.peer AND u.is_available JOIN user_details d ON d.user_id=u.id
), streams AS (
  SELECT 'user:'||$1::bigint::text AS key UNION ALL SELECT 'channel:'||id FROM my_channels UNION ALL SELECT 'room:'||id FROM visible
)
SELECT jsonb_build_object(
  'channels',coalesce((SELECT jsonb_agg(jsonb_build_object('channel_id',c.id::text,'channel_name',c.name,'description',c.description,
    'creator_id',c.creator_id::text,'role',c.role,'join_mode',c.join_mode,
    'invite_code',CASE WHEN c.role IN ('CREATOR','MODERATOR') AND c.join_mode='PUBLIC_LINK' THEN c.invite_code END,
    'icon_url',CASE WHEN c.icon_upload_id IS NOT NULL THEN '/api/v1/images/'||c.icon_upload_id||'/url' END,
    'background_url',CASE WHEN c.background_upload_id IS NOT NULL THEN '/api/v1/images/'||c.background_upload_id||'/url' END,
    'members',coalesce((SELECT jsonb_agg(jsonb_build_object('user_id',m.user_id::text,'role',m.role) ORDER BY m.user_id)
      FROM channel_members m JOIN users u ON u.id=m.user_id AND u.is_available WHERE m.channel_id=c.id),'[]')) ORDER BY c.id) FROM my_channels c),'[]'),
  'categories',coalesce((SELECT jsonb_agg(jsonb_build_object('category_id',g.id::text,'channel_id',g.channel_id::text,
    'category_name',g.name,'position',g.position) ORDER BY g.channel_id,g.position,g.id) FROM room_categories g JOIN my_channels c ON c.id=g.channel_id),'[]'),
  'rooms',coalesce((SELECT jsonb_agg(view ORDER BY channel_id,category_id,position,id) FROM room_views WHERE kind<>'DM'),'[]'),
  'dms',coalesce((SELECT jsonb_agg(view ORDER BY id) FROM room_views WHERE kind='DM'),'[]'),
  'friends',coalesce((SELECT jsonb_agg(person ORDER BY peer) FROM pair_views WHERE status='FRIEND'),'[]'),
  'friend_requests',coalesce((SELECT jsonb_agg(jsonb_build_object('request_id',id::text,'requester_id',requester_id::text,
    'created_at',created_at,'user',person) ORDER BY id) FROM pair_views WHERE status='PENDING'),'[]'),
  'blocks',coalesce((SELECT jsonb_agg(blocked_id::text ORDER BY blocked_id) FROM user_blocks WHERE blocker_id=$1),'[]'),
  'invitations',coalesce((SELECT jsonb_agg(jsonb_build_object('invitation_id',i.id::text,'channel_id',i.channel_id::text,
    'channel_name',c.name,'inviter_id',i.inviter_id::text,'invitee_id',i.invitee_id::text,'status',i.status,'created_at',i.created_at) ORDER BY i.id)
    FROM channel_invitations i JOIN channels c ON c.id=i.channel_id AND c.is_available WHERE $1 IN(i.inviter_id,i.invitee_id) AND i.status='PENDING'),'[]'),
  'voice_states',coalesce((SELECT jsonb_agg(jsonb_build_object('room_id',v.room_id::text,'user_id',v.user_id::text,
    'voice_session_id',v.id::text,'state',v.state,'screen_video_sid',v.screen_video_sid,'screen_audio_sid',v.screen_audio_sid) ORDER BY v.id)
    FROM voice_sessions v JOIN visible r ON r.id=v.room_id
    WHERE v.state='CONNECTED' OR (v.state='PENDING' AND v.expires_at>now())),'[]'),
  'revisions',coalesce((SELECT jsonb_object_agg(s.key,coalesce(v.revision,0)::text) FROM streams s LEFT JOIN stream_revisions v ON v.stream=s.key),'{}'))";
