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
    DisconnectToken(Id, Uuid),
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

pub async fn disconnect_token(state: &AppState, user: Id, jti: Uuid) {
    let _ = state.realtime.0.send(Signal::DisconnectToken(user, jti));
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
        WHERE dispatched_at IS NULL AND NOT (data @> '{\"cloudflare_staging\":true}'::jsonb)
        ORDER BY id LIMIT 128",
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
                Ok(Signal::Disconnect(user,sid)) if user == actor.user_id && sid.is_none_or(|sid| sid == actor.sid) => break,
                Ok(Signal::DisconnectToken(user,jti)) if user == actor.user_id && jti == actor.jti => break,
                Ok(Signal::Disconnect(..) | Signal::DisconnectToken(..)) => {},
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
        Scope::Session(sid) if *sid != actor.sid => return Ok(None),
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

const BOOTSTRAP: &str = include_str!("sql/bootstrap.sql");
