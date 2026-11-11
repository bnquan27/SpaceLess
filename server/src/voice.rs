use std::{collections::HashMap, str::FromStr};

use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{delete, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::{
    Actor, ApiError, AppState, Id, Path, Query, Result, db,
    sync::{self, Scope},
};

#[derive(Clone)]
pub struct Config {
    pub url: String,
    api_key: String,
    api_secret: String,
    cloud_revocation: bool,
}

impl Config {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(url) = std::env::var("LIVEKIT_URL") else {
            return Ok(None);
        };
        let api_key = std::env::var("LIVEKIT_API_KEY").map_err(|_| ApiError::unavailable())?;
        let api_secret =
            std::env::var("LIVEKIT_API_SECRET").map_err(|_| ApiError::unavailable())?;
        let cloud_revocation = std::env::var("LIVEKIT_CLOUD_REVOCATION").as_deref() == Ok("true");
        if !url.starts_with("wss://") && !url.starts_with("ws://") {
            return Err(ApiError::bad("INVALID_LIVEKIT_URL"));
        }
        Ok(Some(Self {
            url: url.trim_end_matches('/').into(),
            api_key,
            api_secret,
            cloud_revocation,
        }))
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/voice/join", post(join))
        .route("/rooms/{room_id}/voice/leave", delete(leave))
        .route("/webhooks/livekit", post(webhook))
}

fn room_name(room: Id) -> String {
    format!("room-{}", room.0)
}
fn identity(id: Id) -> String {
    format!("voice-{}", id.0)
}

fn token(
    config: &Config,
    subject: &str,
    grants: Value,
    metadata: Option<String>,
    ttl: i64,
) -> Result<String> {
    let now = Utc::now().timestamp();
    let claims = json!({"iss":config.api_key,"sub":subject,"iat":now,"nbf":now,"exp":now+ttl,"video":grants,"metadata":metadata.unwrap_or_default()});
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(config.api_secret.as_bytes()),
    )
    .map_err(|_| ApiError::internal())
}

async fn call(state: &AppState, method: &str, room: Option<Id>, input: Value) -> Result<Value> {
    let config = state.voice.as_ref().ok_or_else(ApiError::unavailable)?;
    let grants = match room {
        Some(room) => json!({"roomAdmin":true,"room":room_name(room)}),
        None => json!({"roomList":true}),
    };
    let bearer = token(config, "", grants, None, 60)?;
    let base = config
        .url
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1);
    let response = state
        .http
        .post(format!("{base}/twirp/livekit.RoomService/{method}"))
        .bearer_auth(bearer)
        .json(&input)
        .send()
        .await
        .map_err(|_| ApiError::unavailable())?;
    if !response.status().is_success() {
        return Err(ApiError::unavailable());
    }
    response.json().await.map_err(|_| ApiError::unavailable())
}

async fn check_join(conn: &mut PgConnection, actor: &Actor, room: Id) -> Result<()> {
    db::lock_session(conn, actor).await?;
    let access = db::room_access(conn, actor.user_id, room).await?;
    if let Some(channel) = access.channel_id {
        db::lock_channel(conn, channel).await?;
    }
    db::lock_room(conn, room).await?;
    let access = db::room_access(conn, actor.user_id, room).await?;
    if access.kind != "VOICE" {
        return Err(ApiError::bad("VOICE_ROOM_REQUIRED"));
    }
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(actor.user_id.0)
        .execute(conn)
        .await?;
    Ok(())
}

async fn join(
    State(state): State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
) -> Result<Json<Value>> {
    let config = state.voice.as_ref().ok_or_else(ApiError::unavailable)?;
    if !config.cloud_revocation {
        return Err(ApiError::unavailable());
    }
    let mut tx = state.pool.begin().await?;
    check_join(&mut tx, &actor, room).await?;
    let old = sqlx::query("SELECT * FROM voice_sessions WHERE user_id=$1 FOR UPDATE")
        .bind(actor.user_id.0)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(old) = old {
        let old_id = Id(old.get("id"));
        queue_revoke(&mut tx, old_id).await?;
        let old_sid: Uuid = old.get("session_id");
        if old_sid != actor.sid {
            sync::emit(&mut tx,Scope::Session(old_sid),"VOICE_STATE_UPDATE",json!({"room_id":Id(old.get("room_id")),"user_id":actor.user_id,"voice_session_id":old_id,"state":"REVOKING","device_switched":true})).await?;
        }
        tx.commit().await?;
        process_revocation(&state, old_id).await?;
        tx = state.pool.begin().await?;
        check_join(&mut tx, &actor, room).await?;
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM voice_sessions WHERE user_id=$1)")
            .bind(actor.user_id.0)
            .fetch_one(&mut *tx)
            .await?;
    if exists {
        return Err(ApiError::conflict("VOICE_SESSION_CHANGED"));
    }
    let voice_id = state.id().await?;
    let expires_at = Utc::now() + chrono::Duration::seconds(300);
    let identity = identity(voice_id);
    let metadata = json!({"voice_session_id":voice_id}).to_string();
    let jwt = token(
        config,
        &identity,
        json!({"roomJoin":true,"room":room_name(room),"canPublish":true,"canSubscribe":true,"canPublishData":false,"canUpdateOwnMetadata":false,"canPublishSources":["microphone","camera","screen_share","screen_share_audio"]}),
        Some(metadata),
        300,
    )?;
    sqlx::query("INSERT INTO voice_sessions(id,user_id,room_id,session_id,participant_identity,state,join_token_expires_at) VALUES($1,$2,$3,$4,$5,'ISSUED',$6)")
        .bind(voice_id.0).bind(actor.user_id.0).bind(room.0).bind(actor.sid).bind(identity).bind(expires_at).execute(&mut *tx).await?;
    sync::emit(&mut tx,Scope::Room(room),"VOICE_STATE_UPDATE",json!({"room_id":room,"user_id":actor.user_id,"voice_session_id":voice_id,"state":"PENDING"})).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"voice_session_id":voice_id,"livekit_url":config.url,"livekit_token":jwt,"expires_at":expires_at}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaveQuery {
    voice_session_id: Id,
}

async fn leave(
    State(state): State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
    Query(input): Query<LeaveQuery>,
) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let owned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM voice_sessions WHERE id=$1 AND user_id=$2 AND room_id=$3 AND session_id=$4)")
        .bind(input.voice_session_id.0).bind(actor.user_id.0).bind(room.0).bind(actor.sid).fetch_one(&mut *tx).await?;
    if owned {
        queue_revoke(&mut tx, input.voice_session_id).await?;
    }
    tx.commit().await?;
    if owned {
        process_revocation(&state, input.voice_session_id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn queue_revoke(conn: &mut PgConnection, id: Id) -> Result<()> {
    sqlx::query("WITH marked AS (UPDATE voice_sessions SET state='REVOKE_PENDING' WHERE id=$1 RETURNING id,room_id,participant_identity) INSERT INTO media_jobs(voice_session_id,room_id,identity) SELECT id,room_id,participant_identity FROM marked ON CONFLICT(voice_session_id) DO NOTHING")
        .bind(id.0).execute(conn).await?;
    Ok(())
}

pub async fn revoke_invalid(conn: &mut PgConnection) -> Result<()> {
    let invalid: Vec<i64> = sqlx::query_scalar(
        "SELECT v.id FROM voice_sessions v
         WHERE v.state <> 'REVOKE_PENDING'
           AND NOT EXISTS (SELECT 1 FROM authorized_voice_sessions a WHERE a.id=v.id)
         ORDER BY v.id",
    )
    .fetch_all(&mut *conn)
    .await?;
    for id in invalid {
        queue_revoke(conn, Id(id)).await?;
    }
    Ok(())
}

async fn emit_end(conn: &mut PgConnection, row: &sqlx::postgres::PgRow) -> Result<()> {
    let room = Id(row.get("room_id"));
    let data = json!({"room_id":room,"user_id":Id(row.get("user_id")),"voice_session_id":Id(row.get("id")),"state":"ENDED"});
    if let Some(sid) = row.get::<Option<String>, _>("screen_video_sid") {
        let mut stopped = data.clone();
        stopped["track_sid"] = Value::String(sid);
        sync::emit(conn, Scope::Room(room), "SCREEN_SHARE_STOP", stopped).await?;
    }
    sync::emit(conn, Scope::Room(room), "VOICE_STATE_UPDATE", data).await
}

async fn process_revocation(state: &AppState, id: Id) -> Result<()> {
    let job = sqlx::query("SELECT room_id,identity FROM media_jobs WHERE voice_session_id=$1")
        .bind(id.0)
        .fetch_optional(&state.pool)
        .await?;
    let Some(job) = job else { return Ok(()) };
    let room = Id(job.get("room_id"));
    let identity: String = job.get("identity");
    let cutoff = Utc::now().timestamp() + 1;
    if call(
        state,
        "RemoveParticipant",
        Some(room),
        json!({"room":room_name(room),"identity":identity,"revoke_token_ts":cutoff.to_string()}),
    )
    .await
    .is_err()
    {
        sqlx::query("UPDATE media_jobs SET attempts=attempts+1,next_attempt_at=now()+make_interval(secs => LEAST(60, power(2,LEAST(attempts,5))::integer) + (random()*3)::integer) WHERE voice_session_id=$1").bind(id.0).execute(&state.pool).await?;
        return Err(ApiError::unavailable());
    }
    let mut tx = state.pool.begin().await?;
    let old = sqlx::query(
        "DELETE FROM voice_sessions WHERE id=$1 AND state='REVOKE_PENDING' RETURNING *",
    )
    .bind(id.0)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(old) = old {
        emit_end(&mut tx, &old).await?;
    }
    sqlx::query("DELETE FROM media_jobs WHERE voice_session_id=$1")
        .bind(id.0)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct WebhookClaims {
    iss: String,
    exp: i64,
    sha256: String,
}

#[derive(Deserialize)]
struct LiveKitEvent {
    id: String,
    event: String,
    room: Option<LiveKitRoom>,
    participant: Option<Participant>,
    track: Option<Track>,
}

#[derive(Deserialize)]
struct LiveKitRoom {
    name: String,
}

#[derive(Deserialize, Clone)]
struct Participant {
    identity: String,
    #[serde(default)]
    sid: String,
    #[serde(default)]
    tracks: Vec<Track>,
}

#[derive(Deserialize, Clone)]
struct Track {
    sid: String,
    #[serde(default)]
    source: Value,
}

fn screen_kind(source: &Value) -> Option<bool> {
    match source {
        Value::String(s) if s == "SCREEN_SHARE" => Some(true),
        Value::String(s) if s == "SCREEN_SHARE_AUDIO" => Some(false),
        Value::Number(n) if n.as_u64() == Some(3) => Some(true),
        Value::Number(n) if n.as_u64() == Some(4) => Some(false),
        _ => None,
    }
}

fn verify_webhook(config: &Config, authorization: &str, body: &[u8]) -> Result<LiveKitEvent> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[&config.api_key]);
    validation.leeway = 0;
    let jwt = authorization
        .strip_prefix("Bearer ")
        .unwrap_or(authorization);
    let claims = decode::<WebhookClaims>(
        jwt,
        &DecodingKey::from_secret(config.api_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::unauthorized())?
    .claims;
    let expected = STANDARD
        .decode(claims.sha256)
        .map_err(|_| ApiError::unauthorized())?;
    if expected.as_slice() != Sha256::digest(body).as_slice() {
        return Err(ApiError::unauthorized());
    }
    let event: LiveKitEvent =
        serde_json::from_slice(body).map_err(|_| ApiError::bad("INVALID_WEBHOOK"))?;
    if event.id.is_empty() || event.id.len() > 256 {
        return Err(ApiError::bad("INVALID_WEBHOOK_ID"));
    }
    Ok(event)
}

async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode> {
    let config = state.voice.as_ref().ok_or_else(ApiError::unavailable)?;
    let authorization = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::unauthorized)?;
    let event = verify_webhook(config, authorization, &body)?;
    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO livekit_webhook_receipts(event_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(event.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Ok(StatusCode::NO_CONTENT);
    }
    let mut revoke = None;
    if let (Some(room), Some(participant)) = (event.room, event.participant) {
        let row =
            sqlx::query("SELECT * FROM voice_sessions WHERE participant_identity=$1 FOR UPDATE")
                .bind(&participant.identity)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(row) = row {
            let id = Id(row.get("id"));
            let actual_room = Id(row.get("room_id"));
            let sid: Option<String> = row.get("participant_sid");
            let valid_sid = sid.as_ref().is_none_or(|sid| sid == &participant.sid);
            let service_room: String = row.get("service_room");
            let session_state: String = row.get("state");
            if room.name == service_room && session_state == "REVOKE_PENDING" {
                revoke = Some(id);
            } else if room.name == service_room && valid_sid {
                let authorized: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM authorized_voice_sessions WHERE id=$1)",
                )
                .bind(id.0)
                .fetch_one(&mut *tx)
                .await?;
                if !authorized {
                    queue_revoke(&mut tx, id).await?;
                    revoke = Some(id);
                } else {
                    match event.event.as_str() {
                        "participant_joined" => {
                            if session_state == "ISSUED" {
                                sqlx::query("UPDATE voice_sessions SET state='CONNECTED',participant_sid=$2 WHERE id=$1").bind(id.0).bind(&participant.sid).execute(&mut *tx).await?;
                                sync::emit(&mut *tx,Scope::Room(actual_room),"VOICE_STATE_UPDATE",json!({"room_id":actual_room,"user_id":Id(row.get("user_id")),"voice_session_id":id,"state":"CONNECTED"})).await?;
                            }
                        }
                        "participant_left" => {
                            queue_revoke(&mut tx, id).await?;
                            revoke = Some(id);
                        }
                        "track_published" | "track_unpublished" => {
                            if let Some(track) = event.track {
                                if apply_track(
                                    &mut tx,
                                    &row,
                                    &track,
                                    event.event == "track_published",
                                )
                                .await?
                                {
                                    revoke = Some(id);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if event.event == "participant_joined" {
            if let (Some(raw_room), Some(raw_id)) = (
                room.name.strip_prefix("room-"),
                participant.identity.strip_prefix("voice-"),
            ) {
                if let (Ok(room), Ok(id)) = (Id::from_str(raw_room), Id::from_str(raw_id)) {
                    sqlx::query("INSERT INTO media_jobs(voice_session_id,room_id,identity) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(id.0).bind(room.0).bind(&participant.identity).execute(&mut *tx).await?;
                    revoke = Some(id);
                }
            }
        }
    }
    tx.commit().await?;
    if let Some(id) = revoke {
        process_revocation(&state, id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn apply_track(
    conn: &mut PgConnection,
    row: &sqlx::postgres::PgRow,
    track: &Track,
    published: bool,
) -> Result<bool> {
    let Some(video) = screen_kind(&track.source) else {
        return Ok(false);
    };
    let field = if video {
        "screen_video_sid"
    } else {
        "screen_audio_sid"
    };
    let current: Option<String> = row.get(field);
    let id = Id(row.get("id"));
    if published && current.as_ref().is_some_and(|sid| sid != &track.sid) {
        queue_revoke(conn, id).await?;
        return Ok(true);
    }
    let changed = if published {
        current.is_none()
    } else {
        current.as_ref() == Some(&track.sid)
    };
    if !changed {
        return Ok(false);
    }
    let sql = if video {
        "UPDATE voice_sessions SET screen_video_sid=$2 WHERE id=$1"
    } else {
        "UPDATE voice_sessions SET screen_audio_sid=$2 WHERE id=$1"
    };
    sqlx::query(sql)
        .bind(id.0)
        .bind(if published { Some(&track.sid) } else { None })
        .execute(&mut *conn)
        .await?;
    if video {
        let room = Id(row.get("room_id"));
        sync::emit(conn,Scope::Room(room),if published { "SCREEN_SHARE_START" } else { "SCREEN_SHARE_STOP" },json!({"room_id":room,"user_id":Id(row.get("user_id")),"voice_session_id":id,"track_sid":track.sid})).await?;
    }
    Ok(false)
}

pub async fn run_worker(state: &AppState) -> Result<()> {
    let Some(config) = state.voice.as_ref() else {
        return Ok(());
    };
    if !config.cloud_revocation {
        return Ok(());
    }
    let mut tx = state.pool.begin().await?;
    revoke_invalid(&mut tx).await?;
    tx.commit().await?;
    flush_revocations(state).await?;
    let rooms = call(state, "ListRooms", None, json!({})).await?;
    let mut room_ids: Vec<Id> =
        sqlx::query_scalar::<_, i64>("SELECT DISTINCT room_id FROM voice_sessions")
            .fetch_all(&state.pool)
            .await?
            .into_iter()
            .map(Id)
            .collect();
    for room in rooms
        .get("rooms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = room
            .get("name")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("room-"))
            .and_then(|s| s.parse::<Id>().ok())
        {
            room_ids.push(id);
        }
    }
    room_ids.sort();
    room_ids.dedup();
    let active: Vec<String> = rooms
        .get("rooms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|room| room.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect();
    for room in room_ids {
        let participants = if active.contains(&room_name(room)) {
            let response = call(
                state,
                "ListParticipants",
                Some(room),
                json!({"room":room_name(room)}),
            )
            .await?;
            serde_json::from_value::<Vec<Participant>>(
                response.get("participants").cloned().unwrap_or(json!([])),
            )
            .map_err(|_| ApiError::unavailable())?
        } else {
            Vec::new()
        };
        reconcile_room(state, room, participants).await?;
    }
    Ok(())
}

pub async fn flush_revocations(state: &AppState) -> Result<()> {
    let jobs: Vec<i64> = sqlx::query_scalar("SELECT voice_session_id FROM media_jobs WHERE next_attempt_at<=now() ORDER BY id LIMIT 100").fetch_all(&state.pool).await?;
    for id in jobs {
        if let Err(error) = process_revocation(state, Id(id)).await {
            tracing::warn!(voice_session_id=id,%error,"Media revocation will retry");
        }
    }
    Ok(())
}

pub async fn flush_after_commit(state: &AppState) {
    if let Err(error) = flush_revocations(state).await {
        tracing::warn!(%error, "Committed voice revocations will retry");
    }
}

async fn reconcile_room(state: &AppState, room: Id, participants: Vec<Participant>) -> Result<()> {
    let mut participants: HashMap<String, Participant> = participants
        .into_iter()
        .map(|p| (p.identity.clone(), p))
        .collect();
    let mut tx = state.pool.begin().await?;
    let sessions =
        sqlx::query("SELECT * FROM voice_sessions WHERE room_id=$1 ORDER BY id FOR UPDATE")
            .bind(room.0)
            .fetch_all(&mut *tx)
            .await?;
    let mut revoke = Vec::new();
    for row in sessions {
        let id = Id(row.get("id"));
        let identity: String = row.get("participant_identity");
        let participant = participants.remove(&identity);
        let state_name: String = row.get("state");
        if state_name == "REVOKE_PENDING" {
            revoke.push(id);
            continue;
        }
        if let Some(participant) = participant {
            let sid: Option<String> = row.get("participant_sid");
            if sid.as_ref().is_some_and(|sid| sid != &participant.sid) {
                queue_revoke(&mut tx, id).await?;
                revoke.push(id);
                continue;
            }
            if state_name == "ISSUED" {
                sqlx::query(
                    "UPDATE voice_sessions SET state='CONNECTED',participant_sid=$2 WHERE id=$1",
                )
                .bind(id.0)
                .bind(&participant.sid)
                .execute(&mut *tx)
                .await?;
                sync::emit(&mut tx,Scope::Room(room),"VOICE_STATE_UPDATE",json!({"room_id":room,"user_id":Id(row.get("user_id")),"voice_session_id":id,"state":"CONNECTED"})).await?;
            }
            for video in [true, false] {
                let tracks: Vec<_> = participant
                    .tracks
                    .iter()
                    .filter(|t| screen_kind(&t.source) == Some(video))
                    .collect();
                if tracks.len() > 1 {
                    queue_revoke(&mut tx, id).await?;
                    revoke.push(id);
                    break;
                }
                let field = if video {
                    "screen_video_sid"
                } else {
                    "screen_audio_sid"
                };
                let current: Option<String> = row.get(field);
                if let Some(current) = current {
                    if !tracks.iter().any(|t| t.sid == current) {
                        apply_track(
                            &mut tx,
                            &row,
                            &Track {
                                sid: current,
                                source: json!(if video { 3 } else { 4 }),
                            },
                            false,
                        )
                        .await?;
                    }
                }
                if let Some(track) = tracks.first() {
                    let fresh = sqlx::query("SELECT * FROM voice_sessions WHERE id=$1")
                        .bind(id.0)
                        .fetch_one(&mut *tx)
                        .await?;
                    if apply_track(&mut tx, &fresh, track, true).await? {
                        revoke.push(id);
                    }
                }
            }
        } else if state_name == "CONNECTED"
            || row.get::<DateTime<Utc>, _>("join_token_expires_at") <= Utc::now()
        {
            queue_revoke(&mut tx, id).await?;
            revoke.push(id);
        }
    }
    for participant in participants.into_values() {
        if let Some(id) = participant
            .identity
            .strip_prefix("voice-")
            .and_then(|v| v.parse::<Id>().ok())
        {
            sqlx::query("INSERT INTO media_jobs(voice_session_id,room_id,identity) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(id.0).bind(room.0).bind(participant.identity).execute(&mut *tx).await?;
            revoke.push(id);
        }
    }
    tx.commit().await?;
    for id in revoke {
        process_revocation(state, id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_authentication_covers_exact_body_and_issuer() {
        let config = Config {
            url: "wss://example.test".into(),
            api_key: "key".into(),
            api_secret: "a-secret-for-testing-only".into(),
            cloud_revocation: true,
        };
        let body = br#"{"id":"event-1","event":"room_started"}"#;
        let claims = WebhookClaims {
            iss: "key".into(),
            exp: Utc::now().timestamp() + 60,
            sha256: STANDARD.encode(Sha256::digest(body)),
        };
        let jwt = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(config.api_secret.as_bytes()),
        )
        .unwrap();
        assert!(verify_webhook(&config, &jwt, body).is_ok());
        assert!(verify_webhook(&config, &jwt, b"{}").is_err());
        let other = Config {
            api_key: "other".into(),
            ..config
        };
        assert!(verify_webhook(&other, &jwt, body).is_err());
    }

    #[test]
    fn screen_track_sources_accept_livekit_json_encodings() {
        assert_eq!(screen_kind(&json!("SCREEN_SHARE")), Some(true));
        assert_eq!(screen_kind(&json!(4)), Some(false));
        assert_eq!(screen_kind(&json!("CAMERA")), None);
    }
}
