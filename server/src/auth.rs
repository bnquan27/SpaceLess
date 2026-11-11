use std::{env, net::SocketAddr, time::Duration};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Router,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use rand::{RngCore, rngs::OsRng};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use tokio::sync::Semaphore;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{Actor, ApiError, AppState, Body, Id, Result, db, files, sync, voice};

const SESSION_SECONDS: i64 = 7 * 24 * 60 * 60;
const FORGOT_WINDOW: i64 = 900;
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXktc2FsdC1zcGFjZWxlc3M$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
static PASSWORD_WORK: Semaphore = Semaphore::const_new(4);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password", post(reset_password))
        .route("/users/me", get(get_profile).patch(update_profile))
}

#[derive(Clone)]
pub struct MailConfig {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    sender: Mailbox,
    reset_url: Url,
}

impl MailConfig {
    pub fn from_env() -> Result<Option<Self>> {
        let host = match env::var("SMTP_HOST") {
            Ok(host) => host,
            Err(env::VarError::NotPresent) => return Ok(None),
            Err(_) => return Err(ApiError::bad("SMTP_HOST không hợp lệ.")),
        };
        let port: u16 = env::var("SMTP_PORT")
            .unwrap_or_else(|_| "465".into())
            .parse()
            .map_err(|_| ApiError::bad("SMTP_PORT không hợp lệ."))?;
        let sender = env::var("SMTP_FROM")
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| ApiError::bad("SMTP_FROM không hợp lệ."))?;
        let reset_url = env::var("PASSWORD_RESET_URL")
            .ok()
            .and_then(|value| Url::parse(&value).ok())
            .filter(|url| {
                matches!(url.scheme(), "https" | "spacelessapp")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && !url.query_pairs().any(|(key, _)| key == "token")
            })
            .ok_or_else(|| ApiError::bad("PASSWORD_RESET_URL không hợp lệ."))?;
        let mut transport = if port == 587 {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
        }
        .map_err(|_| ApiError::bad("SMTP_HOST không hợp lệ."))?
        .port(port)
        .timeout(Some(Duration::from_secs(15)));
        match (env::var("SMTP_USERNAME"), env::var("SMTP_PASSWORD")) {
            (Ok(username), Ok(password)) => {
                transport = transport.credentials(Credentials::new(username, password));
            }
            (Err(env::VarError::NotPresent), Err(env::VarError::NotPresent)) => {}
            _ => return Err(ApiError::bad("Cấu hình đăng nhập SMTP chưa đầy đủ.")),
        }
        Ok(Some(Self {
            transport: transport.build(),
            sender,
            reset_url,
        }))
    }

    async fn send_reset(self, email: Address, username: String, token: [u8; 32]) {
        let mut url = self.reset_url;
        url.query_pairs_mut()
            .append_pair("token", &hex::encode(token));
        let message = Message::builder()
            .from(self.sender)
            .to(Mailbox::new(None, email))
            .subject("Đặt lại mật khẩu SpaceLessApp")
            .body(format!(
                "Tên đăng nhập: {username}\n\nĐặt lại mật khẩu: {url}\n\nLiên kết có hiệu lực trong 15 phút."
            ));
        if let Ok(message) = message {
            if self.transport.send(message).await.is_ok() {
                return;
            }
        }
        tracing::warn!("Password reset email delivery failed");
    }
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Id,
    sid: Option<Uuid>,
    jti: Uuid,
    exp: i64,
    auth_version: i64,
}

impl FromRequestParts<AppState> for Actor {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token)
            .ok_or_else(ApiError::unauthorized)?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        let claims = jsonwebtoken::decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &validation,
        )
        .map_err(|_| ApiError::unauthorized())?
        .claims;
        let actor = Actor {
            user_id: claims.sub,
            sid: claims.sid.unwrap_or(claims.jti),
            jti: claims.jti,
            exp: claims.exp,
            auth_version: claims.auth_version,
        };
        session_valid(state, &actor).await?;
        Ok(actor)
    }
}

pub async fn session_valid(state: &AppState, actor: &Actor) -> Result<()> {
    db::validate_session(&state.pool, actor).await
}

fn new_expiry() -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(Utc::now().timestamp() + SESSION_SECONDS, 0)
        .ok_or_else(ApiError::internal)
}

fn token_response(
    state: &AppState,
    user_id: Id,
    sid: Uuid,
    auth_version: i64,
    expires_at: DateTime<Utc>,
) -> Result<axum::Json<Value>> {
    let token = jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub: user_id,
            sid: Some(sid),
            jti: Uuid::new_v4(),
            exp: expires_at.timestamp(),
            auth_version,
        },
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| ApiError::internal())?;
    Ok(axum::Json(
        json!({"access_token": token, "expires_at": expires_at}),
    ))
}

fn invalid_credentials() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "INVALID_CREDENTIALS",
        "Tài khoản hoặc mật khẩu không khớp. Vui lòng kiểm tra lại",
    )
}

fn invalid_reset_token() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "INVALID_RESET_TOKEN",
        "Liên kết đặt lại mật khẩu không hợp lệ hoặc đã hết hạn.",
    )
}

fn normalized(value: &str, max: usize, error: &str) -> Result<String> {
    let value: String = value.trim().nfc().collect();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(ApiError::bad(error));
    }
    Ok(value)
}

fn username(value: &str) -> Result<String> {
    let value = normalized(value, 50, "Tên đăng nhập không hợp lệ.")?;
    if !value
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(ApiError::bad("Tên đăng nhập không hợp lệ."));
    }
    Ok(value)
}

fn email(value: &str) -> Result<(String, Address)> {
    let value = normalized(value, 100, "Email không hợp lệ.")?;
    let address = value
        .parse()
        .map_err(|_| ApiError::bad("Email không hợp lệ."))?;
    Ok((value, address))
}

fn phone(value: &str) -> Result<String> {
    let value = normalized(value, 15, "Số điện thoại không hợp lệ.")?;
    let digits = value.strip_prefix('+').unwrap_or(&value);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::bad("Số điện thoại không hợp lệ."));
    }
    Ok(value)
}

fn birth_date(value: NaiveDate) -> Result<NaiveDate> {
    if value > Utc::now().date_naive() {
        return Err(ApiError::bad("Ngày sinh không được ở tương lai."));
    }
    Ok(value)
}

async fn hash_password(password: String) -> Result<String> {
    if !(12..=128).contains(&password.chars().count()) {
        return Err(ApiError::bad("Mật khẩu phải có từ 12 đến 128 ký tự."));
    }
    let permit = PASSWORD_WORK
        .try_acquire()
        .map_err(|_| ApiError::unavailable())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| ApiError::internal())
    })
    .await
    .map_err(|_| ApiError::internal())?
}

async fn verify_password(password: String, hash: String) -> Result<bool> {
    let permit = PASSWORD_WORK
        .try_acquire()
        .map_err(|_| ApiError::unavailable())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let hash = PasswordHash::new(&hash).map_err(|_| ApiError::internal())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok())
    })
    .await
    .map_err(|_| ApiError::internal())?
}

fn identity_write_error(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .is_some_and(|db| db.is_unique_violation())
    {
        return ApiError::conflict("Tên đăng nhập hoặc email đã được sử dụng.");
    }
    error.into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    username: String,
    password: String,
    full_name: String,
    date_of_birth: NaiveDate,
    email: String,
    phone: String,
}

async fn register(
    State(state): State<AppState>,
    Body(input): Body<Register>,
) -> Result<(StatusCode, axum::Json<Value>)> {
    let username = username(&input.username)?;
    let (email, _) = email(&input.email)?;
    let full_name = normalized(&input.full_name, 100, "Họ tên không hợp lệ.")?;
    let date_of_birth = birth_date(input.date_of_birth)?;
    let phone = phone(&input.phone)?;
    let password_hash = hash_password(input.password).await?;
    let id = state.id().await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO users (id, username, full_name, password_hash) VALUES ($1, $2, $3, $4)",
    )
    .bind(id.0)
    .bind(username)
    .bind(full_name)
    .bind(password_hash)
    .execute(&mut *tx)
    .await
    .map_err(identity_write_error)?;
    sqlx::query(
        "INSERT INTO user_details (user_id, email, date_of_birth, phone) VALUES ($1, $2, $3, $4)",
    )
    .bind(id.0)
    .bind(email)
    .bind(date_of_birth)
    .bind(phone)
    .execute(&mut *tx)
    .await
    .map_err(identity_write_error)?;
    let body = profile(&mut tx, id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, axum::Json(body)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Body(input): Body<Login>,
) -> Result<axum::Json<Value>> {
    let username = username(&input.username).map_err(|_| invalid_credentials())?;
    if !(12..=128).contains(&input.password.chars().count()) {
        return Err(invalid_credentials());
    }
    let candidate = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, password_hash FROM users WHERE identity_key(username) = identity_key($1)",
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?;
    let hash = candidate
        .as_ref()
        .map_or(DUMMY_HASH, |(_, hash)| hash.as_str());
    if !verify_password(input.password, hash.into()).await? {
        return Err(invalid_credentials());
    }
    let (user_id, password_hash) = candidate.ok_or_else(invalid_credentials)?;
    let mut tx = state.pool.begin().await?;
    let (available, auth_version) = sqlx::query_as::<_, (bool, i64)>(
        "SELECT is_available, auth_version FROM users WHERE id = $1 AND password_hash = $2 FOR SHARE",
    )
    .bind(user_id)
    .bind(password_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(invalid_credentials)?;
    if !available {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "ACCOUNT_LOCKED",
            "Tài khoản đã bị khóa.",
        ));
    }
    let sid = Uuid::new_v4();
    let expires_at = new_expiry()?;
    sqlx::query(
        "INSERT INTO auth_sessions (session_id, user_id, auth_version, expires_at) VALUES ($1, $2, $3, $4)",
    )
        .bind(sid)
        .bind(user_id)
        .bind(auth_version)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
    let response = token_response(&state, Id(user_id), sid, auth_version, expires_at)?;
    tx.commit().await?;
    Ok(response)
}

async fn lock_account(conn: &mut PgConnection, user: Id) -> Result<()> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.0)
        .fetch_optional(conn)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(())
}

async fn lock_mutable_session(conn: &mut PgConnection, actor: &Actor) -> Result<()> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE id = $1 FOR SHARE")
        .bind(actor.user_id.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT session_id FROM auth_sessions WHERE session_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(actor.sid)
    .bind(actor.user_id.0)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    db::validate_session(conn, actor).await
}

async fn revoke_token(conn: &mut PgConnection, actor: &Actor) -> Result<()> {
    let expiry = DateTime::from_timestamp(actor.exp, 0).ok_or_else(ApiError::unauthorized)?;
    // Legacy refreshes reused the session ID as the token ID across different expiry times.
    sqlx::query(
        "INSERT INTO revoked_tokens (token_jti, user_id, expires_at)
         SELECT $1, user_id, CASE WHEN session_id = $1 THEN expires_at ELSE $3 END
         FROM auth_sessions WHERE session_id = $2",
    )
    .bind(actor.jti)
    .bind(actor.sid)
    .bind(expiry)
    .execute(conn)
    .await?;
    Ok(())
}

async fn refresh(State(state): State<AppState>, actor: Actor) -> Result<axum::Json<Value>> {
    let mut tx = state.pool.begin().await?;
    lock_mutable_session(&mut tx, &actor).await?;
    let expires_at = new_expiry()?;
    revoke_token(&mut tx, &actor).await?;
    sqlx::query("UPDATE auth_sessions SET expires_at = $2 WHERE session_id = $1")
        .bind(actor.sid)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
    let response = token_response(
        &state,
        actor.user_id,
        actor.sid,
        actor.auth_version,
        expires_at,
    )?;
    tx.commit().await?;
    sync::disconnect_token(&state, actor.user_id, actor.jti).await;
    Ok(response)
}

async fn logout(State(state): State<AppState>, actor: Actor) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    lock_mutable_session(&mut tx, &actor).await?;
    revoke_token(&mut tx, &actor).await?;
    sqlx::query("UPDATE auth_sessions SET revoked_at = now() WHERE session_id = $1")
        .bind(actor.sid)
        .execute(&mut *tx)
        .await?;
    voice::revoke_invalid(&mut tx).await?;
    tx.commit().await?;
    sync::disconnect(&state, actor.user_id, Some(actor.sid)).await;
    voice::flush_after_commit(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForgotPassword {
    email: String,
}

fn recovery_key(secret: &str, kind: &str, value: &str) -> Result<String> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::unavailable())?;
    mac.update(b"spaceless-recovery-quota\0");
    mac.update(kind.as_bytes());
    mac.update(b"\0");
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

async fn check_forgot_limit(state: &AppState, email: &str, peer: SocketAddr) -> Result<()> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| ApiError::unavailable())?;
    let (normalized_email, now): (String, i64) = sqlx::query_as(
        "SELECT identity_key($1), floor(extract(epoch FROM clock_timestamp()))::bigint",
    )
    .bind(email)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::unavailable())?;
    let email_key = recovery_key(&state.jwt_secret, "EMAIL", &normalized_email)?;
    let ip_key = recovery_key(
        &state.jwt_secret,
        "IP",
        &peer.ip().to_canonical().to_string(),
    )?;
    let window_start = DateTime::from_timestamp(now - now.rem_euclid(FORGOT_WINDOW), 0)
        .ok_or_else(ApiError::unavailable)?;
    let result = sqlx::query(
        "INSERT INTO recovery_rate_buckets AS b (key_kind, key_hash, window_start, admitted_count)
         VALUES ('EMAIL', $1, $3, 1), ('IP', $2, $3, 1)
         ON CONFLICT (key_kind, key_hash, window_start) DO UPDATE SET admitted_count = b.admitted_count + 1
         WHERE b.admitted_count < CASE b.key_kind WHEN 'EMAIL' THEN 3 ELSE 20 END",
    )
    .bind(email_key)
    .bind(ip_key)
    .bind(window_start)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::unavailable())?;
    if result.rows_affected() != 2 {
        tx.rollback().await.map_err(|_| ApiError::unavailable())?;
        return Err(ApiError::rate_limited(
            (FORGOT_WINDOW - now.rem_euclid(FORGOT_WINDOW)) as u64,
        ));
    }
    tx.commit().await.map_err(|_| ApiError::unavailable())
}

async fn forgot_password(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Body(input): Body<ForgotPassword>,
) -> Result<axum::Json<Value>> {
    let (email, address) = email(&input.email)?;
    check_forgot_limit(&state, &email, peer).await?;
    let mail = state.mail.clone().ok_or_else(ApiError::unavailable)?;
    let mut tx = state.pool.begin().await?;
    let user = sqlx::query_as::<_, (i64, String)>(
        "SELECT u.id, u.username FROM users u JOIN user_details d ON d.user_id = u.id
         WHERE identity_key(d.email) = identity_key($1) AND u.is_available FOR UPDATE OF u",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((user_id, username)) = user {
        let mut token = [0; 32];
        OsRng.fill_bytes(&mut token);
        // The email could have changed while the lookup waited for the account lock.
        let inserted = sqlx::query(
            "INSERT INTO password_reset_tokens (token_hash, user_id, expires_at)
             SELECT $1, user_id, clock_timestamp() + interval '15 minutes' FROM user_details
             WHERE user_id = $2 AND identity_key(email) = identity_key($3)",
        )
        .bind(Sha256::digest(token).as_slice())
        .bind(user_id)
        .bind(email)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if inserted.rows_affected() == 1 {
            tokio::spawn(mail.send_reset(address, username, token));
        }
    } else {
        tx.commit().await?;
    }
    Ok(axum::Json(
        json!({"message": "Đã tiếp nhận yêu cầu, bạn sẽ nhận được email nếu đã đăng ký"}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetPassword {
    token: String,
    new_password: String,
}

async fn reset_password(
    State(state): State<AppState>,
    Body(input): Body<ResetPassword>,
) -> Result<StatusCode> {
    let mut token = [0; 32];
    hex::decode_to_slice(&input.token, &mut token).map_err(|_| invalid_reset_token())?;
    let hash = Sha256::digest(token);
    let user = sqlx::query_scalar::<_, i64>(
        "SELECT t.user_id FROM password_reset_tokens t JOIN users u ON u.id = t.user_id \
         WHERE t.token_hash = $1 AND NOT t.is_used AND t.expires_at > now() AND u.is_available",
    )
    .bind(hash.as_slice())
    .fetch_optional(&state.pool)
    .await?
    .map(Id)
    .ok_or_else(invalid_reset_token)?;
    let password_hash = hash_password(input.new_password).await?;
    let mut tx = state.pool.begin().await?;
    lock_account(&mut tx, user).await?;
    let changed = sqlx::query(
        "UPDATE users u SET password_hash = $2, auth_version = auth_version + 1 \
         WHERE id = $1 AND is_available AND EXISTS ( \
             SELECT 1 FROM password_reset_tokens t WHERE t.token_hash = $3 AND t.user_id = u.id \
             AND NOT t.is_used AND t.expires_at > clock_timestamp() FOR UPDATE)",
    )
    .bind(user.0)
    .bind(password_hash)
    .bind(hash.as_slice())
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() == 0 {
        return Err(invalid_reset_token());
    }
    sqlx::query(
        "UPDATE password_reset_tokens SET is_used = true WHERE user_id = $1 AND NOT is_used",
    )
    .bind(user.0)
    .execute(&mut *tx)
    .await?;
    let sessions = sqlx::query_scalar::<_, Uuid>(
        "UPDATE auth_sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL RETURNING session_id",
    )
    .bind(user.0)
    .fetch_all(&mut *tx)
    .await?;
    voice::revoke_invalid(&mut tx).await?;
    tx.commit().await?;
    for sid in sessions {
        sync::disconnect(&state, user, Some(sid)).await;
    }
    voice::flush_after_commit(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn profile(conn: &mut PgConnection, user: Id) -> Result<Value> {
    let (username, email, full_name, date_of_birth, phone, address, avatar_upload_id) =
        sqlx::query_as::<_, (String, String, String, NaiveDate, String, Option<String>, Option<i64>)>(
            "SELECT u.username, d.email, u.full_name, d.date_of_birth, d.phone, d.address, d.avatar_upload_id \
             FROM users u JOIN user_details d ON d.user_id = u.id WHERE u.id = $1 AND u.is_available",
        )
        .bind(user.0)
        .fetch_optional(conn)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(json!({
        "user_id": user, "username": username, "full_name": full_name,
        "date_of_birth": date_of_birth, "email": email, "phone": phone, "address": address,
        "avatar_url": avatar_upload_id.map(|id| files::image_url(Id(id))),
    }))
}

async fn get_profile(State(state): State<AppState>, actor: Actor) -> Result<axum::Json<Value>> {
    let mut conn = state.pool.acquire().await?;
    Ok(axum::Json(profile(&mut conn, actor.user_id).await?))
}

fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateProfile {
    #[serde(default, deserialize_with = "present")]
    full_name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    date_of_birth: Option<NaiveDate>,
    #[serde(default, deserialize_with = "present")]
    phone: Option<String>,
    #[serde(default, deserialize_with = "present")]
    email: Option<String>,
    #[serde(default, deserialize_with = "present")]
    address: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    avatar_upload_id: Option<Option<Id>>,
}

async fn update_profile(
    State(state): State<AppState>,
    actor: Actor,
    Body(input): Body<UpdateProfile>,
) -> Result<axum::Json<Value>> {
    let full_name = input
        .full_name
        .as_deref()
        .map(|v| normalized(v, 100, "Họ tên không hợp lệ."))
        .transpose()?;
    let date_of_birth = input.date_of_birth.map(birth_date).transpose()?;
    let phone = input.phone.as_deref().map(phone).transpose()?;
    let email = input
        .email
        .as_deref()
        .map(email)
        .transpose()?
        .map(|(value, _)| value);
    let address = input
        .address
        .as_ref()
        .and_then(|value| value.as_deref())
        .map(|value| normalized(value, 500, "Địa chỉ không hợp lệ."))
        .transpose()?;
    let mut tx = state.pool.begin().await?;
    lock_account(&mut tx, actor.user_id).await?;
    db::lock_session(&mut tx, &actor).await?;
    if let Some(full_name) = full_name {
        sqlx::query("UPDATE users SET full_name = $2 WHERE id = $1")
            .bind(actor.user_id.0)
            .bind(full_name)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(email) = email {
        let update =
            sqlx::query("UPDATE user_details SET email = $2 WHERE user_id = $1 AND email <> $2")
                .bind(actor.user_id.0)
                .bind(email)
                .execute(&mut *tx)
                .await
                .map_err(identity_write_error)?;
        if update.rows_affected() > 0 {
            sqlx::query("UPDATE password_reset_tokens SET is_used = true WHERE user_id = $1 AND NOT is_used")
                .bind(actor.user_id.0)
                .execute(&mut *tx)
                .await?;
        }
    }
    let mut replaced_image = None;
    if let Some(avatar) = input.avatar_upload_id {
        let previous = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT avatar_upload_id FROM user_details WHERE user_id = $1",
        )
        .bind(actor.user_id.0)
        .fetch_one(&mut *tx)
        .await?
        .map(Id);
        if previous != avatar {
            if let Some(upload) = avatar {
                files::attach_image(&state, &mut tx, actor.user_id, None, upload, "AVATAR").await?;
            }
            replaced_image = previous;
        }
    }
    sqlx::query(
        "UPDATE user_details SET date_of_birth = coalesce($2, date_of_birth), \
         phone = coalesce($3, phone), address = CASE WHEN $4 THEN $5 ELSE address END, \
         avatar_upload_id = CASE WHEN $6 THEN $7 ELSE avatar_upload_id END WHERE user_id = $1",
    )
    .bind(actor.user_id.0)
    .bind(date_of_birth)
    .bind(phone)
    .bind(input.address.is_some())
    .bind(address)
    .bind(input.avatar_upload_id.is_some())
    .bind(input.avatar_upload_id.flatten().map(|id| id.0))
    .execute(&mut *tx)
    .await?;
    if let Some(previous) = replaced_image {
        files::discard_image(&mut tx, previous).await?;
    }
    let response = profile(&mut tx, actor.user_id).await?;
    tx.commit().await?;
    Ok(axum::Json(response))
}
