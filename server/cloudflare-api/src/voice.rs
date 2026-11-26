mod provider;

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio_postgres::{Client, GenericClient, Row, types::Type};
use uuid::Uuid;
use worker::{Env, Method, Request, Response, State};

use crate::{
    auth::{self, Actor, ApiError},
    db,
    events::{self, Scope},
};
use provider::{JOIN_SECONDS, LiveKitEvent, Participant, Provider, Track, room_name};

type Result<T> = std::result::Result<T, ApiError>;

const SESSION_FIELDS: &str = "id,user_id,room_id,session_id,participant_identity,participant_sid,state,screen_video_sid,screen_audio_sid,join_token_expires_at";
const RETRY_BATCH: i64 = 8;
const RECONCILE_BATCH: usize = 8;

#[derive(Clone, Copy)]
pub(crate) enum VoiceRoute {
    Join(i64),
    Leave(i64),
    Webhook,
}

pub(crate) fn route(request: &Request) -> Result<Option<VoiceRoute>> {
    let path = request.path();
    if request.method() == Method::Post && path == "/api/v1/webhooks/livekit" {
        return Ok(Some(VoiceRoute::Webhook));
    }
    let Some(tail) = path.strip_prefix("/api/v1/rooms/") else {
        return Ok(None);
    };
    let Some((room, endpoint)) = tail.split_once('/') else {
        return Ok(None);
    };
    let route = match (request.method(), endpoint) {
        (Method::Post, "voice/join") => VoiceRoute::Join,
        (Method::Delete, "voice/leave") => VoiceRoute::Leave,
        _ => return Ok(None),
    };
    Ok(Some(route(positive_id(room).ok_or_else(invalid_path)?)))
}

pub(crate) async fn handle(
    route: VoiceRoute,
    request: Request,
    client: &mut Client,
    jwt_secret: &[u8],
    env: &Env,
) -> Result<Response> {
    let actor = auth::actor(&request, jwt_secret)?;
    match route {
        VoiceRoute::Join(room) => {
            let provider = Provider::configured(env)?.ok_or_else(ApiError::unavailable)?;
            join(client, &provider, &actor, room).await
        }
        VoiceRoute::Leave(room) => {
            let id = parse_leave_id(&request)?;
            leave(client, env, &actor, room, id).await?;
            no_content()
        }
        VoiceRoute::Webhook => Err(ApiError::internal()),
    }
}

pub(crate) async fn handle_webhook(request: Request, env: &Env, state: &State) -> Result<Response> {
    let provider = Provider::configured(env)?.ok_or_else(ApiError::unavailable)?;
    let event = provider.webhook(request).await?;
    let mut client = db::connect(env, state)
        .await
        .map_err(|_| ApiError::unavailable())?;
    webhook(&mut client, &provider, event).await?;
    no_content()
}

fn positive_id(value: &str) -> Option<i64> {
    let id = value.parse::<i64>().ok()?;
    (id > 0 && id.to_string() == value).then_some(id)
}

fn invalid_path() -> ApiError {
    ApiError::new(400, "INVALID_PATH", "Đường dẫn không hợp lệ.")
}

fn parse_leave_id(request: &Request) -> Result<i64> {
    let invalid = || ApiError::new(400, "INVALID_QUERY", "Truy vấn không hợp lệ.");
    let url = request.url().map_err(|_| invalid())?;
    let mut params = url.query_pairs();
    let Some((key, value)) = params.next() else {
        return Err(invalid());
    };
    if key != "voice_session_id" || params.next().is_some() {
        return Err(invalid());
    }
    positive_id(&value).ok_or_else(invalid)
}

fn not_found() -> ApiError {
    ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên.")
}

fn no_content() -> Result<Response> {
    Response::empty()
        .map(|response| response.with_status(204))
        .map_err(|_| ApiError::internal())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Issued,
    Connected,
    Revoking,
}

struct VoiceSession {
    id: i64,
    user: i64,
    room: i64,
    session: Uuid,
    identity: String,
    participant_sid: Option<String>,
    state: SessionState,
    screen_video: Option<String>,
    screen_audio: Option<String>,
    join_expires: DateTime<Utc>,
}

impl VoiceSession {
    fn from_row(row: Row) -> Result<Self> {
        let state = match row.try_get::<_, &str>("state")? {
            "ISSUED" => SessionState::Issued,
            "CONNECTED" => SessionState::Connected,
            "REVOKE_PENDING" => SessionState::Revoking,
            _ => return Err(ApiError::internal()),
        };
        Ok(Self {
            id: row.try_get("id")?,
            user: row.try_get("user_id")?,
            room: row.try_get("room_id")?,
            session: row.try_get("session_id")?,
            identity: row.try_get("participant_identity")?,
            participant_sid: row.try_get("participant_sid")?,
            state,
            screen_video: row.try_get("screen_video_sid")?,
            screen_audio: row.try_get("screen_audio_sid")?,
            join_expires: row.try_get("join_token_expires_at")?,
        })
    }

    fn data(&self) -> Value {
        json!({"room_id": self.room.to_string(), "user_id": self.user.to_string(),
            "voice_session_id": self.id.to_string()})
    }

    fn accepts_participant(&self, sid: &str) -> bool {
        !sid.is_empty()
            && self
                .participant_sid
                .as_deref()
                .is_none_or(|previous| sid == previous)
    }

    async fn emit<C: GenericClient + Sync>(&self, tx: &C, state: &str) -> Result<()> {
        let mut data = self.data();
        data["state"] = json!(state);
        events::emit(tx, Scope::Room(self.room), "VOICE_STATE_UPDATE", data).await
    }

    async fn connect<C: GenericClient + Sync>(
        &mut self,
        tx: &C,
        participant_sid: &str,
    ) -> Result<()> {
        if self.state == SessionState::Issued {
            tx.query_typed(
                "UPDATE voice_sessions SET state='CONNECTED',participant_sid=$2 WHERE id=$1",
                &[(&self.id, Type::INT8), (&participant_sid, Type::TEXT)],
            )
            .await?;
            self.participant_sid = Some(participant_sid.to_owned());
            self.state = SessionState::Connected;
            self.emit(tx, "CONNECTED").await?;
        }
        Ok(())
    }
}

async fn room_access<C: GenericClient + Sync>(tx: &C, user: i64, room: i64) -> Result<Row> {
    Ok(tx.query_typed_opt(
        "SELECT r.kind,r.channel_id FROM rooms r
         WHERE r.id=$1 AND r.is_available AND (
           EXISTS (SELECT 1 FROM channel_room_access a WHERE a.room_id=r.id AND a.user_id=$2)
           OR (r.kind='DM' AND EXISTS (SELECT 1 FROM dm_pairs d WHERE d.room_id=r.id AND $2 IN (d.lo,d.hi)))
         )",
        &[(&room, Type::INT8), (&user, Type::INT8)],
    ).await?.ok_or_else(not_found)?)
}

async fn lock_join<C: GenericClient + Sync>(tx: &C, actor: &Actor, room: i64) -> Result<()> {
    auth::lock_session(tx, actor, false).await?;
    if let Some(channel) = room_access(tx, actor.user_id, room)
        .await?
        .try_get::<_, Option<i64>>(1)?
    {
        tx.query_typed_opt(
            "SELECT id FROM channels WHERE id=$1 AND is_available FOR UPDATE",
            &[(&channel, Type::INT8)],
        )
        .await?
        .ok_or_else(not_found)?;
    }
    tx.query_typed_opt(
        "SELECT id FROM rooms WHERE id=$1 AND is_available FOR UPDATE",
        &[(&room, Type::INT8)],
    )
    .await?
    .ok_or_else(not_found)?;
    let row = room_access(tx, actor.user_id, room).await?;
    if row.try_get::<_, &str>(0)? != "VOICE" {
        return Err(ApiError::new(
            400,
            "VOICE_ROOM_REQUIRED",
            "Cần một phòng thoại.",
        ));
    }
    tx.query_typed_one(
        "SELECT pg_advisory_xact_lock($1)",
        &[(&actor.user_id, Type::INT8)],
    )
    .await?;
    Ok(())
}

async fn join(
    client: &mut Client,
    provider: &Provider,
    actor: &Actor,
    room: i64,
) -> Result<Response> {
    let mut tx = client.transaction().await?;
    lock_join(&tx, actor, room).await?;
    let previous = tx
        .query_typed_opt(
            &format!("SELECT {SESSION_FIELDS} FROM voice_sessions WHERE user_id=$1 FOR UPDATE"),
            &[(&actor.user_id, Type::INT8)],
        )
        .await?
        .map(VoiceSession::from_row)
        .transpose()?;
    if let Some(previous) = previous {
        queue_revoke(&tx, previous.id).await?;
        if previous.session != actor.sid && previous.state != SessionState::Revoking {
            let mut data = previous.data();
            data["state"] = json!("REVOKING");
            data["device_switched"] = json!(true);
            events::emit(
                &tx,
                Scope::Session(previous.session),
                "VOICE_STATE_UPDATE",
                data,
            )
            .await?;
        }
        tx.commit().await?;
        process_revocation(client, provider, previous.id).await?;
        tx = client.transaction().await?;
        lock_join(&tx, actor, room).await?;
        if tx
            .query_typed_opt(
                "SELECT id FROM voice_sessions WHERE user_id=$1",
                &[(&actor.user_id, Type::INT8)],
            )
            .await?
            .is_some()
        {
            return Err(ApiError::new(
                409,
                "VOICE_SESSION_CHANGED",
                "Phiên thoại đã thay đổi.",
            ));
        }
    }
    let id = db::snowflake(&tx).await?;
    let issued = Utc::now().timestamp();
    let expires =
        DateTime::<Utc>::from_timestamp(issued + JOIN_SECONDS, 0).ok_or_else(ApiError::internal)?;
    let token = provider.issue(room, id, issued)?;
    let identity = format!("voice-{id}");
    tx.query_typed(
        "INSERT INTO voice_sessions(id,user_id,room_id,session_id,participant_identity,state,join_token_expires_at)
         VALUES($1,$2,$3,$4,$5,'ISSUED',$6)",
        &[(&id, Type::INT8), (&actor.user_id, Type::INT8), (&room, Type::INT8), (&actor.sid, Type::UUID),
          (&identity, Type::TEXT), (&expires, Type::TIMESTAMPTZ)],
    ).await?;
    events::emit(&tx, Scope::Room(room), "VOICE_STATE_UPDATE", json!({"room_id": room.to_string(),
        "user_id": actor.user_id.to_string(), "voice_session_id": id.to_string(), "state": "PENDING"})).await?;
    tx.commit().await?;
    Response::from_json(
        &json!({"voice_session_id": id.to_string(), "livekit_url": provider.url,
        "livekit_token": token, "expires_at": expires}),
    )
    .map_err(|_| ApiError::internal())
}

async fn leave(client: &mut Client, env: &Env, actor: &Actor, room: i64, id: i64) -> Result<()> {
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let owned = tx.query_typed_opt(
        "SELECT id FROM voice_sessions WHERE id=$1 AND user_id=$2 AND room_id=$3 AND session_id=$4 FOR UPDATE",
        &[(&id, Type::INT8), (&actor.user_id, Type::INT8), (&room, Type::INT8), (&actor.sid, Type::UUID)],
    ).await?.is_some();
    if owned {
        queue_revoke(&tx, id).await?;
    }
    tx.commit().await?;
    if owned {
        let provider = Provider::configured(env)?.ok_or_else(ApiError::unavailable)?;
        process_revocation(client, &provider, id).await?;
    }
    Ok(())
}

async fn queue_revoke<C: GenericClient + Sync>(tx: &C, id: i64) -> Result<()> {
    tx.query_typed(
        "WITH marked AS (
           UPDATE voice_sessions SET state='REVOKE_PENDING' WHERE id=$1 RETURNING id,room_id,participant_identity
         ) INSERT INTO media_jobs(voice_session_id,room_id,identity)
           SELECT id,room_id,participant_identity FROM marked ON CONFLICT(voice_session_id) DO NOTHING",
        &[(&id, Type::INT8)],
    ).await?;
    Ok(())
}

pub(crate) async fn revoke_invalid<C: GenericClient + Sync>(tx: &C) -> Result<()> {
    tx.query_typed(
        "WITH invalid AS (
           SELECT v.id FROM voice_sessions v WHERE v.state <> 'REVOKE_PENDING'
             AND NOT EXISTS (SELECT 1 FROM authorized_voice_sessions a WHERE a.id=v.id)
           ORDER BY v.id FOR UPDATE OF v
         ), marked AS (
           UPDATE voice_sessions v SET state='REVOKE_PENDING' FROM invalid
           WHERE v.id=invalid.id RETURNING v.id,v.room_id,v.participant_identity
         ) INSERT INTO media_jobs(voice_session_id,room_id,identity)
           SELECT id,room_id,participant_identity FROM marked ON CONFLICT(voice_session_id) DO NOTHING",
        &[],
    ).await?;
    Ok(())
}

async fn queue_orphan<C: GenericClient + Sync>(
    tx: &C,
    room: i64,
    identity: &str,
) -> Result<Option<i64>> {
    let Some(id) = identity.strip_prefix("voice-").and_then(positive_id) else {
        return Ok(None);
    };
    tx.query_typed(
        "INSERT INTO media_jobs(voice_session_id,room_id,identity) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        &[(&id, Type::INT8), (&room, Type::INT8), (&identity, Type::TEXT)],
    ).await?;
    Ok(Some(id))
}

async fn process_revocation(client: &mut Client, provider: &Provider, id: i64) -> Result<()> {
    let job = client
        .query_typed_opt(
            "SELECT room_id,identity FROM media_jobs WHERE voice_session_id=$1",
            &[(&id, Type::INT8)],
        )
        .await?;
    let Some(job) = job else { return Ok(()) };
    let room = job.try_get(0)?;
    let identity: String = job.try_get(1)?;
    if provider.remove(room, &identity).await.is_err() {
        client.query_typed(
            "UPDATE media_jobs SET attempts=attempts+1,next_attempt_at=now()+
               make_interval(secs => LEAST(60,power(2,LEAST(attempts,5))::integer)+(random()*3)::integer)
             WHERE voice_session_id=$1", &[(&id, Type::INT8)],
        ).await?;
        return Err(ApiError::unavailable());
    }
    let tx = client.transaction().await?;
    let previous = tx.query_typed_opt(
        &format!("DELETE FROM voice_sessions WHERE id=$1 AND state='REVOKE_PENDING' RETURNING {SESSION_FIELDS}"),
        &[(&id, Type::INT8)],
    ).await?.map(VoiceSession::from_row).transpose()?;
    if let Some(previous) = previous {
        if let Some(track_sid) = &previous.screen_video {
            emit_track(&tx, &previous, track_sid, false).await?;
        }
        previous.emit(&tx, "ENDED").await?;
    }
    tx.query_typed(
        "DELETE FROM media_jobs WHERE voice_session_id=$1",
        &[(&id, Type::INT8)],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn flush(client: &mut Client, provider: &Provider) -> Result<()> {
    let due = client.query_typed(
        "SELECT voice_session_id FROM media_jobs WHERE next_attempt_at<=now() ORDER BY id LIMIT $1",
        &[(&RETRY_BATCH, Type::INT8)],
    ).await?;
    for row in due {
        if process_revocation(client, provider, row.try_get(0)?)
            .await
            .is_err()
        {
            worker::console_warn!("LiveKit revocation will retry.");
        }
    }
    Ok(())
}

pub(crate) async fn flush_after_commit(client: &mut Client, env: &Env) -> Result<()> {
    if let Some(provider) = Provider::configured(env)? {
        flush(client, &provider).await?;
    }
    Ok(())
}

async fn authorized<C: GenericClient + Sync>(tx: &C, id: i64) -> Result<bool> {
    Ok(tx
        .query_typed_opt(
            "SELECT id FROM authorized_voice_sessions WHERE id=$1",
            &[(&id, Type::INT8)],
        )
        .await?
        .is_some())
}

async fn webhook(client: &mut Client, provider: &Provider, event: LiveKitEvent) -> Result<()> {
    let tx = client.transaction().await?;
    let receipt = tx.query_typed_opt(
        "INSERT INTO livekit_webhook_receipts(event_id) VALUES($1) ON CONFLICT DO NOTHING RETURNING event_id",
        &[(&event.id, Type::TEXT)],
    ).await?;
    if receipt.is_none() {
        tx.commit().await?;
        return Ok(());
    }
    let mut revoke = None;
    if let (Some(room), Some(participant)) = (event.room, event.participant) {
        let session = tx.query_typed_opt(
            &format!("SELECT {SESSION_FIELDS} FROM voice_sessions WHERE participant_identity=$1 FOR UPDATE"),
            &[(&participant.identity, Type::TEXT)],
        ).await?.map(VoiceSession::from_row).transpose()?;
        if let Some(mut session) = session {
            if room.name == room_name(session.room) {
                revoke = apply_event(
                    &tx,
                    &mut session,
                    &participant,
                    &event.event,
                    event.track.as_ref(),
                )
                .await?;
            }
        } else if event.event == "participant_joined" {
            if let Some(room) = room.name.strip_prefix("room-").and_then(positive_id) {
                revoke = queue_orphan(&tx, room, &participant.identity).await?;
            }
        }
    }
    tx.commit().await?;
    if let Some(id) = revoke {
        process_revocation(client, provider, id).await?;
    }
    Ok(())
}

async fn apply_event<C: GenericClient + Sync>(
    tx: &C,
    session: &mut VoiceSession,
    participant: &Participant,
    event: &str,
    track: Option<&Track>,
) -> Result<Option<i64>> {
    if session.state == SessionState::Revoking {
        return Ok(Some(session.id));
    }
    if !session.accepts_participant(&participant.sid) {
        return Ok(None);
    }
    if !authorized(tx, session.id).await? || event == "participant_left" {
        queue_revoke(tx, session.id).await?;
        return Ok(Some(session.id));
    }
    match event {
        "participant_joined" => session.connect(tx, &participant.sid).await?,
        "track_published" | "track_unpublished" => {
            if let Some(track) = track {
                if apply_track(tx, session, track, event == "track_published").await? {
                    return Ok(Some(session.id));
                }
            }
        }
        _ => {}
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScreenTrack {
    Video,
    Audio,
}

impl ScreenTrack {
    fn kind(source: &Value) -> Option<Self> {
        match source {
            Value::String(value) if value == "SCREEN_SHARE" => Some(Self::Video),
            Value::String(value) if value == "SCREEN_SHARE_AUDIO" => Some(Self::Audio),
            Value::Number(value) if value.as_u64() == Some(3) => Some(Self::Video),
            Value::Number(value) if value.as_u64() == Some(4) => Some(Self::Audio),
            _ => None,
        }
    }

    fn slot(self, session: &mut VoiceSession) -> &mut Option<String> {
        match self {
            Self::Video => &mut session.screen_video,
            Self::Audio => &mut session.screen_audio,
        }
    }

    fn source(self) -> Value {
        json!(match self {
            Self::Video => 3,
            Self::Audio => 4,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TrackTransition<'a> {
    Ignore,
    Replace(Option<&'a str>),
    Revoke,
}

fn track_transition<'a>(
    current: Option<&str>,
    sid: &'a str,
    published: bool,
) -> TrackTransition<'a> {
    match (published, current) {
        (_, _) if sid.is_empty() => TrackTransition::Ignore,
        (true, Some(previous)) if previous != sid => TrackTransition::Revoke,
        (true, None) => TrackTransition::Replace(Some(sid)),
        (false, Some(previous)) if previous == sid => TrackTransition::Replace(None),
        _ => TrackTransition::Ignore,
    }
}

async fn emit_track<C: GenericClient + Sync>(
    tx: &C,
    session: &VoiceSession,
    sid: &str,
    published: bool,
) -> Result<()> {
    let mut data = session.data();
    data["track_sid"] = json!(sid);
    events::emit(
        tx,
        Scope::Room(session.room),
        if published {
            "SCREEN_SHARE_START"
        } else {
            "SCREEN_SHARE_STOP"
        },
        data,
    )
    .await
}

async fn apply_track<C: GenericClient + Sync>(
    tx: &C,
    session: &mut VoiceSession,
    track: &Track,
    published: bool,
) -> Result<bool> {
    let Some(kind) = ScreenTrack::kind(&track.source) else {
        return Ok(false);
    };
    match track_transition(kind.slot(session).as_deref(), &track.sid, published) {
        TrackTransition::Ignore => Ok(false),
        TrackTransition::Revoke => {
            queue_revoke(tx, session.id).await?;
            Ok(true)
        }
        TrackTransition::Replace(replacement) => {
            let sql = match kind {
                ScreenTrack::Video => "UPDATE voice_sessions SET screen_video_sid=$2 WHERE id=$1",
                ScreenTrack::Audio => "UPDATE voice_sessions SET screen_audio_sid=$2 WHERE id=$1",
            };
            tx.query_typed(
                sql,
                &[(&session.id, Type::INT8), (&replacement, Type::TEXT)],
            )
            .await?;
            *kind.slot(session) = replacement.map(str::to_owned);
            if kind == ScreenTrack::Video {
                emit_track(tx, session, &track.sid, published).await?;
            }
            Ok(false)
        }
    }
}

pub(crate) async fn run_worker(client: &mut Client, env: &Env) -> Result<()> {
    let Some(provider) = Provider::configured(env)? else {
        return Ok(());
    };
    revoke_invalid(client).await?;
    flush(client, &provider).await?;
    let mut observed: HashMap<i64, HashMap<i64, ProviderState>> = HashMap::new();
    for row in client.query_typed(
        "SELECT id,room_id,participant_sid,screen_video_sid,screen_audio_sid FROM voice_sessions", &[],
    ).await? {
        observed.entry(row.try_get("room_id")?).or_default()
            .insert(row.try_get("id")?, ProviderState {
                participant: row.try_get("participant_sid")?,
                screen_video: row.try_get("screen_video_sid")?,
                screen_audio: row.try_get("screen_audio_sid")?,
            });
    }
    let active: BTreeSet<i64> = provider
        .rooms()
        .await?
        .into_iter()
        .filter_map(|room| room.name.strip_prefix("room-").and_then(positive_id))
        .collect();
    let mut rooms = active.clone();
    rooms.extend(observed.keys().copied());
    if rooms.is_empty() {
        return Ok(());
    }
    let windows = rooms.len().div_ceil(RECONCILE_BATCH);
    let offset =
        (Utc::now().timestamp().div_euclid(60).unsigned_abs() as usize % windows) * RECONCILE_BATCH;
    for room in rooms.into_iter().skip(offset).take(RECONCILE_BATCH) {
        let previous = observed.remove(&room).unwrap_or_default();
        let participants = if active.contains(&room) {
            provider.participants(room).await?
        } else {
            Vec::new()
        };
        reconcile_room(client, room, previous, participants).await?;
    }
    flush(client, &provider).await
}

struct ProviderState {
    participant: Option<String>,
    screen_video: Option<String>,
    screen_audio: Option<String>,
}

impl ProviderState {
    fn still_current(&self, session: &VoiceSession) -> bool {
        self.participant == session.participant_sid
            && self.screen_video == session.screen_video
            && self.screen_audio == session.screen_audio
    }
}

async fn reconcile_room(
    client: &mut Client,
    room: i64,
    observed: HashMap<i64, ProviderState>,
    participants: Vec<Participant>,
) -> Result<()> {
    let mut participants: HashMap<_, _> = participants
        .into_iter()
        .map(|participant| (participant.identity.clone(), participant))
        .collect();
    let tx = client.transaction().await?;
    let rows = tx.query_typed(
        &format!("SELECT {SESSION_FIELDS} FROM voice_sessions WHERE room_id=$1 ORDER BY id FOR UPDATE"),
        &[(&room, Type::INT8)],
    ).await?;
    for row in rows {
        let mut session = VoiceSession::from_row(row)?;
        let participant = participants.remove(&session.identity);
        // A provider response describes only the generations observed before its request.
        if session.state == SessionState::Revoking
            || !observed
                .get(&session.id)
                .is_some_and(|before| before.still_current(&session))
        {
            continue;
        }
        if !authorized(&tx, session.id).await? {
            queue_revoke(&tx, session.id).await?;
        } else if let Some(participant) = participant {
            reconcile_participant(&tx, &mut session, participant).await?;
        } else if session.state == SessionState::Connected || session.join_expires <= Utc::now() {
            queue_revoke(&tx, session.id).await?;
        }
    }
    for participant in participants.into_values() {
        queue_orphan(&tx, room, &participant.identity).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn reconcile_participant<C: GenericClient + Sync>(
    tx: &C,
    session: &mut VoiceSession,
    participant: Participant,
) -> Result<()> {
    if !session.accepts_participant(&participant.sid) {
        return queue_revoke(tx, session.id).await;
    }
    session.connect(tx, &participant.sid).await?;
    for kind in [ScreenTrack::Video, ScreenTrack::Audio] {
        let mut tracks = participant
            .tracks
            .iter()
            .filter(|track| ScreenTrack::kind(&track.source) == Some(kind));
        let found = tracks.next();
        if tracks.next().is_some() {
            return queue_revoke(tx, session.id).await;
        }
        let current = kind.slot(session).clone();
        if let Some(sid) = current {
            if found.is_none_or(|track| track.sid != sid) {
                apply_track(
                    tx,
                    session,
                    &Track {
                        sid,
                        source: kind.source(),
                    },
                    false,
                )
                .await?;
            }
        }
        if let Some(track) = found {
            if apply_track(tx, session, track, true).await? {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_preserve_decimal_strings_and_reject_noncanonical_values() {
        assert_eq!(positive_id("9007199254740993"), Some(9_007_199_254_740_993));
        for value in ["0", "-1", "001", "+1", "1e6", "9223372036854775808"] {
            assert_eq!(positive_id(value), None);
        }
    }

    #[test]
    fn screen_track_retry_stale_unpublish_and_second_track_have_distinct_effects() {
        assert_eq!(
            ScreenTrack::kind(&json!("SCREEN_SHARE")),
            Some(ScreenTrack::Video)
        );
        assert_eq!(ScreenTrack::kind(&json!(4)), Some(ScreenTrack::Audio));
        assert_eq!(ScreenTrack::kind(&json!("CAMERA")), None);
        assert_eq!(
            track_transition(None, "TR_current", true),
            TrackTransition::Replace(Some("TR_current"))
        );
        assert_eq!(
            track_transition(Some("TR_current"), "TR_current", true),
            TrackTransition::Ignore
        );
        assert_eq!(
            track_transition(Some("TR_current"), "TR_old", false),
            TrackTransition::Ignore
        );
        assert_eq!(
            track_transition(Some("TR_current"), "TR_current", false),
            TrackTransition::Replace(None)
        );
        assert_eq!(
            track_transition(Some("TR_current"), "TR_extra", true),
            TrackTransition::Revoke
        );
    }
}
