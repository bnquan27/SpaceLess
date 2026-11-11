mod auth;
mod channels;
mod chat;
mod db;
mod files;
mod social;
mod sync;
mod voice;

use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{FromRequest, FromRequestParts, Request},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use serde_json::json;
use sqlx::{Connection, PgConnection, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    id_pool: PgPool,
    worker_id: i32,
    jwt_secret: Arc<str>,
    http: reqwest::Client,
    mail: Option<auth::MailConfig>,
    files: Option<files::Config>,
    voice: Option<voice::Config>,
    realtime: sync::Realtime,
}

impl AppState {
    async fn id(&self) -> Result<Id> {
        const EPOCH_MS: i64 = 1_704_067_200_000;
        let started = tokio::time::Instant::now();
        let wall_start = Utc::now().timestamp_millis();
        loop {
            let now = (wall_start + started.elapsed().as_millis() as i64)
                .min(Utc::now().timestamp_millis());
            let elapsed = now - EPOCH_MS;
            if !(1..(1_i64 << 41)).contains(&elapsed) {
                return Err(ApiError::unavailable());
            }
            let next = sqlx::query_as::<_, (i64, i32)>(
                "INSERT INTO snowflake_workers AS w (worker_id,last_ms,sequence) VALUES ($1,$2,0)
                 ON CONFLICT (worker_id) DO UPDATE SET last_ms=EXCLUDED.last_ms,
                     sequence=CASE WHEN EXCLUDED.last_ms > w.last_ms THEN 0 ELSE w.sequence + 1 END
                 WHERE EXCLUDED.last_ms > w.last_ms OR (EXCLUDED.last_ms=w.last_ms AND w.sequence<4095)
                 RETURNING last_ms,sequence",
            )
            .bind(self.worker_id)
            .bind(elapsed)
            .fetch_optional(&self.id_pool)
            .await?;
            if let Some((time, sequence)) = next {
                return Ok(Id((time << 22)
                    | (i64::from(self.worker_id) << 12)
                    | i64::from(sequence)));
            }
            let last: i64 =
                sqlx::query_scalar("SELECT last_ms FROM snowflake_workers WHERE worker_id=$1")
                    .bind(self.worker_id)
                    .fetch_one(&self.id_pool)
                    .await?;
            if last - elapsed > 5_000 || started.elapsed() >= Duration::from_secs(5) {
                tracing::error!(
                    worker_id = self.worker_id,
                    "Snowflake clock could not advance"
                );
                return Err(ApiError::unavailable());
            }
            tokio::time::sleep(Duration::from_millis((last - elapsed + 1).max(1) as u64)).await;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, sqlx::Type)]
#[sqlx(transparent)]
struct Id(i64);

impl FromStr for Id {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self> {
        if value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ApiError::bad("INVALID_ID"));
        }
        let id = value
            .parse::<i64>()
            .map_err(|_| ApiError::bad("INVALID_ID"))?;
        if id <= 0 {
            return Err(ApiError::bad("INVALID_ID"));
        }
        Ok(Self(id))
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug)]
struct Actor {
    user_id: Id,
    sid: Uuid,
    jti: Uuid,
    exp: i64,
    auth_version: i64,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    fn new(status: StatusCode, code: &str, message: &str) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            retry_after: None,
        }
    }

    fn bad(code: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, "Yêu cầu không hợp lệ.")
    }
    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Phiên không hợp lệ.",
        )
    }
    fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "Bạn không có quyền thực hiện thao tác này.",
        )
    }
    fn not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Không tìm thấy tài nguyên.",
        )
    }
    fn conflict(code: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            code,
            "Trạng thái tài nguyên đã thay đổi.",
        )
    }
    fn unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "Dịch vụ tạm thời không khả dụng.",
        )
    }
    fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Đã xảy ra lỗi máy chủ.",
        )
    }
    fn rate_limited(seconds: u64) -> Self {
        let mut error = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            "Bạn đã gửi quá nhiều yêu cầu.",
        );
        error.retry_after = Some(seconds);
        error
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(json!({
                "error": {"code": self.code, "message": self.message}, "request_id": Uuid::new_v4(),
            })),
        )
            .into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::RowNotFound => return Self::not_found(),
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                return Self::conflict("ALREADY_EXISTS");
            }
            sqlx::Error::Database(db)
                if matches!(db.code().as_deref(), Some("40001" | "40P01")) =>
            {
                return Self::conflict("CONCURRENT_UPDATE");
            }
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
                return Self::unavailable();
            }
            _ => {}
        }
        tracing::error!(%error, "Database operation failed");
        Self::internal()
    }
}

struct Body<T>(T);
struct Path<T>(T);
struct Query<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self> {
        Json::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|_| ApiError::bad("INVALID_BODY"))
    }
}

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for Path<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        axum::extract::Path::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Self(value))
            .map_err(|_| ApiError::bad("INVALID_PATH"))
    }
}

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Query<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        axum::extract::Query::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|_| ApiError::bad("INVALID_QUERY"))
    }
}

fn api(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            auth::routes()
                .merge(social::routes())
                .merge(channels::routes())
                .merge(chat::routes())
                .merge(files::routes())
                .merge(voice::routes())
                .merge(sync::routes()),
        )
        .fallback(|| async { ApiError::not_found() })
        .with_state(state)
}

pub async fn serve() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db_url = std::env::var("DATABASE_URL")?;
    let jwt_secret = std::env::var("APP_JWT_SECRET")?;
    if jwt_secret.len() < 32 {
        return Err("APP_JWT_SECRET must be at least 32 bytes".into());
    }
    let mode = std::env::var("SERVER_MODE").unwrap_or_else(|_| "api".into());
    let gateway_only = match mode.as_str() {
        "api" => false,
        "gateway" => true,
        _ => return Err("SERVER_MODE must be api or gateway".into()),
    };
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let files = files::Config::from_env().await?;
    if gateway_only {
        if files.is_none() {
            return Err("Gateway mode requires S3 and file gateway configuration".into());
        }
        let pool = PgPoolOptions::new()
            .max_connections(12)
            .connect(&db_url)
            .await?;
        let state = AppState {
            id_pool: pool.clone(),
            pool,
            worker_id: 0,
            jwt_secret: jwt_secret.into(),
            http,
            mail: None,
            files,
            voice: None,
            realtime: sync::Realtime::default(),
        };
        let addr = std::env::var("FILE_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8081".into())
            .parse::<SocketAddr>()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(address=%listener.local_addr()?, "SpaceLess download gateway ready");
        axum::serve(listener, files::gateway_routes().with_state(state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        return Ok(());
    }
    let worker_id = std::env::var("SNOWFLAKE_WORKER_ID")
        .unwrap_or_else(|_| "0".into())
        .parse::<i32>()?;
    if !(0..=1023).contains(&worker_id) {
        return Err("SNOWFLAKE_WORKER_ID must be between 0 and 1023".into());
    }
    let pool = PgPoolOptions::new()
        .max_connections(24)
        .connect(&db_url)
        .await?;
    let mut dispatcher_lease = PgConnection::connect(&db_url).await?;
    if !sqlx::query_scalar::<_, bool>(
        "SELECT pg_try_advisory_lock(hashtextextended('spaceless:single-replica',0))",
    )
    .fetch_one(&mut dispatcher_lease)
    .await?
    {
        return Err("Another SpaceLessApp server already owns this database".into());
    }
    sqlx::migrate!().run(&pool).await?;
    let id_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await?;
    let state = AppState {
        pool,
        id_pool,
        worker_id,
        jwt_secret: jwt_secret.into(),
        http,
        mail: auth::MailConfig::from_env()?,
        files,
        voice: voice::Config::from_env()?,
        realtime: sync::Realtime::default(),
    };
    let addr = std::env::var("SERVER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse::<SocketAddr>()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let dispatcher = tokio::spawn(sync::dispatch(state.clone()));
    let workers = tokio::spawn(run_workers(state.clone()));
    let lease = Arc::new(tokio::sync::Mutex::new(dispatcher_lease));
    let watched_lease = lease.clone();
    tracing::info!(address=%listener.local_addr()?, "SpaceLess server ready");
    let result = axum::serve(listener, api(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            let mut lease_tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    _ = lease_tick.tick() => {
                        let mut conn = watched_lease.lock().await;
                        if !matches!(tokio::time::timeout(Duration::from_secs(5), conn.ping()).await, Ok(Ok(()))) {
                            tracing::error!("Database connection holding the replica lease was lost");
                            break;
                        }
                    }
                }
            }
        }).await;
    dispatcher.abort();
    workers.abort();
    let _ = dispatcher.await;
    let _ = workers.await;
    drop(lease);
    result?;
    Ok(())
}

async fn run_workers(state: AppState) {
    let mut maintenance = tokio::time::interval(Duration::from_secs(60));
    loop {
        maintenance.tick().await;
        let (files, voice, housekeeping) = tokio::join!(
            files::run_cleanup(&state),
            voice::run_worker(&state),
            async {
                sqlx::query("DELETE FROM revoked_tokens WHERE expires_at <= now()")
                    .execute(&state.pool)
                    .await?;
                sqlx::query("DELETE FROM recovery_rate_buckets WHERE window_start < now() - interval '30 minutes'")
                    .execute(&state.pool)
                    .await?;
                sqlx::query(
                    "DELETE FROM outbox_events WHERE dispatched_at < now()-interval '1 day'",
                )
                .execute(&state.pool)
                .await?;
                Ok::<_, sqlx::Error>(())
            }
        );
        if let Err(error) = files {
            tracing::warn!(%error, "File cleanup will retry");
        }
        if let Err(error) = voice {
            tracing::warn!(%error, "Voice reconciliation will retry");
        }
        if let Err(error) = housekeeping {
            tracing::warn!(%error, "Database cleanup will retry");
        }
    }
}
