use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use tokio_postgres::{Client, GenericClient, IsolationLevel, types::Type};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
use worker::{Env, Method, Request, Response};

const SESSION_SECONDS: i64 = 7 * 24 * 60 * 60;
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXktc2FsdC1zcGFjZWxlc3M$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[derive(Clone, Copy, Debug)]
pub enum AuthRoute {
    Login,
    Refresh,
    Logout,
    Me,
    UpdateProfile,
}

impl AuthRoute {
    pub fn route(method: &Method, path: &str) -> Option<Self> {
        match (method, path) {
            (Method::Post, "/api/v1/auth/login") => Some(Self::Login),
            (Method::Post, "/api/v1/auth/refresh") => Some(Self::Refresh),
            (Method::Post, "/api/v1/auth/logout") => Some(Self::Logout),
            (Method::Get, "/api/v1/users/me") => Some(Self::Me),
            (Method::Patch, "/api/v1/users/me") => Some(Self::UpdateProfile),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: u16,
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    pub(crate) fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    fn bad_body() -> Self {
        Self::new(400, "INVALID_BODY", "Yêu cầu không hợp lệ.")
    }

    pub(crate) fn unauthorized() -> Self {
        Self::new(401, "UNAUTHORIZED", "Phiên không hợp lệ.")
    }

    fn invalid_credentials() -> Self {
        Self::new(
            401,
            "INVALID_CREDENTIALS",
            "Tài khoản hoặc mật khẩu không khớp. Vui lòng kiểm tra lại",
        )
    }

    pub(crate) fn unavailable() -> Self {
        Self::new(503, "UNAVAILABLE", "Dịch vụ tạm thời không khả dụng.")
    }

    pub(crate) fn internal() -> Self {
        Self::new(500, "INTERNAL", "Đã xảy ra lỗi máy chủ.")
    }

    pub(crate) fn response(&self) -> worker::Result<Response> {
        Response::from_json(&json!({
            "error": {"code": self.code, "message": self.message},
            "request_id": Uuid::new_v4(),
        }))
        .map(|response| response.with_status(self.status))
    }
}

impl From<tokio_postgres::Error> for ApiError {
    fn from(error: tokio_postgres::Error) -> Self {
        if let Some(db) = error.as_db_error() {
            return match db.code().code() {
                "23505" => Self::new(409, "ALREADY_EXISTS", "Trạng thái tài nguyên đã thay đổi."),
                "40001" | "40P01" => Self::new(
                    409,
                    "CONCURRENT_UPDATE",
                    "Trạng thái tài nguyên đã thay đổi.",
                ),
                _ => Self::internal(),
            };
        }
        Self::unavailable()
    }
}

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    sid: Option<Uuid>,
    jti: Uuid,
    exp: i64,
    auth_version: i64,
}

pub(crate) struct Actor {
    pub(crate) user_id: i64,
    pub(crate) sid: Uuid,
    pub(crate) jti: Uuid,
    pub(crate) exp: i64,
    pub(crate) auth_version: i64,
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    crit: Option<Value>,
    b64: Option<bool>,
}

fn decode_token(token: &str, secret: &[u8], now: i64) -> Result<Actor> {
    if token.len() > 16_384 {
        return Err(ApiError::unauthorized());
    }
    let (signed, signature) = token.rsplit_once('.').ok_or_else(ApiError::unauthorized)?;
    let (header, payload) = signed.split_once('.').ok_or_else(ApiError::unauthorized)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| ApiError::unauthorized())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| ApiError::internal())?;
    mac.update(signed.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| ApiError::unauthorized())?;
    let header: JwtHeader = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(header)
            .map_err(|_| ApiError::unauthorized())?,
    )
    .map_err(|_| ApiError::unauthorized())?;
    if header.alg != "HS256" || header.crit.is_some() || header.b64 == Some(false) {
        return Err(ApiError::unauthorized());
    }
    let claims: Claims = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| ApiError::unauthorized())?,
    )
    .map_err(|_| ApiError::unauthorized())?;
    let user_id = claims
        .sub
        .parse::<i64>()
        .map_err(|_| ApiError::unauthorized())?;
    if user_id <= 0
        || user_id.to_string() != claims.sub
        || claims.exp <= now
        || DateTime::from_timestamp(claims.exp, 0).is_none()
    {
        return Err(ApiError::unauthorized());
    }
    Ok(Actor {
        user_id,
        sid: claims.sid.unwrap_or(claims.jti),
        jti: claims.jti,
        exp: claims.exp,
        auth_version: claims.auth_version,
    })
}

pub(crate) fn actor(req: &Request, secret: &[u8]) -> Result<Actor> {
    let authorization = req
        .headers()
        .get("Authorization")
        .map_err(|_| ApiError::unauthorized())?
        .ok_or_else(ApiError::unauthorized)?;
    let token = authorization
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
        .ok_or_else(ApiError::unauthorized)?;
    decode_token(token, secret, Utc::now().timestamp())
}

pub(crate) fn token_response(
    secret: &[u8],
    user_id: i64,
    sid: Uuid,
    auth_version: i64,
    expires: DateTime<Utc>,
) -> Result<Response> {
    let claims = Claims {
        sub: user_id.to_string(),
        sid: Some(sid),
        jti: Uuid::new_v4(),
        exp: expires.timestamp(),
        auth_version,
    };
    let payload = serde_json::to_vec(&claims).map_err(|_| ApiError::internal())?;
    let signed = format!(
        "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.{}",
        URL_SAFE_NO_PAD.encode(payload)
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| ApiError::internal())?;
    mac.update(signed.as_bytes());
    let token = format!(
        "{signed}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    );
    Response::from_json(&json!({"access_token": token, "expires_at": expires}))
        .map_err(|_| ApiError::internal())
}

fn username(value: &str) -> Result<String> {
    let value: String = value.trim().nfc().collect();
    if value.is_empty()
        || value.chars().count() > 50
        || !value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(ApiError::invalid_credentials());
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}

pub(crate) async fn json_body<T: serde::de::DeserializeOwned>(mut req: Request) -> Result<T> {
    let content_type = req
        .headers()
        .get("Content-Type")
        .map_err(|_| ApiError::bad_body())?
        .ok_or_else(ApiError::bad_body)?;
    let mime = content_type.split(';').next().unwrap_or_default().trim();
    if !mime.eq_ignore_ascii_case("application/json")
        && !(mime.to_ascii_lowercase().starts_with("application/")
            && mime.to_ascii_lowercase().ends_with("+json"))
    {
        return Err(ApiError::bad_body());
    }
    let mut bytes = Vec::new();
    let mut stream = req.stream().map_err(|_| ApiError::bad_body())?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::bad_body())?;
        if chunk.len() > 2 * 1024 * 1024 - bytes.len() {
            return Err(ApiError::bad_body());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ApiError::bad_body())
}

async fn login(req: Request, client: &mut Client, secret: &[u8]) -> Result<Response> {
    let input: Login = json_body(req).await?;
    let username = username(&input.username)?;
    if !(12..=128).contains(&input.password.chars().count()) {
        return Err(ApiError::invalid_credentials());
    }
    let candidate = client
        .query_typed(
            "SELECT id, password_hash FROM users WHERE identity_key(username) = identity_key($1)",
            &[(&username, Type::TEXT)],
        )
        .await?
        .pop();
    let password_hash: Option<String> = candidate.as_ref().and_then(|row| row.get(1));
    let hash = PasswordHash::new(password_hash.as_deref().unwrap_or(DUMMY_HASH))
        .map_err(|_| ApiError::internal())?;
    if Argon2::default()
        .verify_password(input.password.as_bytes(), &hash)
        .is_err()
    {
        return Err(ApiError::invalid_credentials());
    }
    let user_id: i64 = candidate.ok_or_else(ApiError::invalid_credentials)?.get(0);
    let password_hash = password_hash.ok_or_else(ApiError::invalid_credentials)?;
    let tx = client
        .build_transaction()
        .read_only(false)
        .isolation_level(IsolationLevel::ReadCommitted)
        .start()
        .await?;
    let account = tx.query_typed(
        "SELECT is_available, auth_version FROM users WHERE id = $1 AND password_hash = $2 FOR SHARE",
        &[(&user_id, Type::INT8), (&password_hash, Type::TEXT)],
    ).await?.pop().ok_or_else(ApiError::invalid_credentials)?;
    if !account.get::<_, bool>(0) {
        return Err(ApiError::new(
            403,
            "ACCOUNT_LOCKED",
            "Tài khoản đã bị khóa.",
        ));
    }
    let auth_version: i64 = account.get(1);
    let response = issue_session(&tx, secret, user_id, auth_version).await?;
    tx.commit().await?;
    Ok(response)
}

pub(crate) async fn issue_session<C: GenericClient + Sync>(
    client: &C,
    secret: &[u8],
    user_id: i64,
    auth_version: i64,
) -> Result<Response> {
    let sid = Uuid::new_v4();
    let expires = DateTime::from_timestamp(Utc::now().timestamp() + SESSION_SECONDS, 0)
        .ok_or_else(ApiError::internal)?;
    client.query_typed(
        "INSERT INTO auth_sessions (session_id, user_id, auth_version, expires_at) VALUES ($1, $2, $3, $4)",
        &[(&sid, Type::UUID), (&user_id, Type::INT8), (&auth_version, Type::INT8), (&expires, Type::TIMESTAMPTZ)],
    ).await?;
    token_response(secret, user_id, sid, auth_version, expires)
}

pub(crate) async fn lock_session<C: GenericClient + Sync>(
    client: &C,
    actor: &Actor,
    mutable: bool,
) -> Result<()> {
    if client
        .query_typed(
            "SELECT id FROM users WHERE id = $1 FOR SHARE",
            &[(&actor.user_id, Type::INT8)],
        )
        .await?
        .is_empty()
    {
        return Err(ApiError::unauthorized());
    }
    let sql = if mutable {
        "SELECT session_id FROM auth_sessions WHERE session_id = $1 AND user_id = $2 FOR UPDATE"
    } else {
        "SELECT session_id FROM auth_sessions WHERE session_id = $1 AND user_id = $2 FOR SHARE"
    };
    if client
        .query_typed(
            sql,
            &[(&actor.sid, Type::UUID), (&actor.user_id, Type::INT8)],
        )
        .await?
        .is_empty()
    {
        return Err(ApiError::unauthorized());
    }
    validate_session(client, actor).await
}

pub(crate) async fn validate_session<C: GenericClient + Sync>(
    client: &C,
    actor: &Actor,
) -> Result<()> {
    let expires = DateTime::from_timestamp(actor.exp, 0).ok_or_else(ApiError::unauthorized)?;
    let valid = client
        .query_typed_one(
            include_str!("../../src/sql/validate_session.sql"),
            &[
                (&actor.sid, Type::UUID),
                (&actor.user_id, Type::INT8),
                (&actor.auth_version, Type::INT8),
                (&expires, Type::TIMESTAMPTZ),
                (&actor.jti, Type::UUID),
            ],
        )
        .await?;
    if !valid.try_get::<_, bool>(0)? {
        return Err(ApiError::unauthorized());
    }
    Ok(())
}

pub(crate) async fn profile<C: GenericClient + Sync>(client: &C, user_id: i64) -> Result<Value> {
    let row = client
        .query_typed_opt(
            "SELECT u.username, d.email, u.full_name, d.date_of_birth, d.phone, d.address, d.avatar_upload_id
             FROM users u JOIN user_details d ON d.user_id = u.id WHERE u.id = $1 AND u.is_available",
            &[(&user_id, Type::INT8)],
        )
        .await?
        .ok_or_else(|| ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên."))?;
    let avatar: Option<i64> = row.try_get(6)?;
    Ok(json!({
        "user_id": user_id.to_string(),
        "username": row.try_get::<_, String>(0)?,
        "email": row.try_get::<_, String>(1)?,
        "full_name": row.try_get::<_, String>(2)?,
        "date_of_birth": row.try_get::<_, Option<NaiveDate>>(3)?,
        "phone": row.try_get::<_, Option<String>>(4)?,
        "address": row.try_get::<_, Option<String>>(5)?,
        "avatar_url": avatar.map(|id| format!("/api/v1/images/{id}/url")),
    }))
}

async fn revoke_token<C: GenericClient + Sync>(client: &C, actor: &Actor) -> Result<()> {
    let expires = DateTime::from_timestamp(actor.exp, 0).ok_or_else(ApiError::unauthorized)?;
    // Legacy refreshes reused the session ID as the token ID across different expiry times.
    client
        .query_typed(
            "INSERT INTO revoked_tokens (token_jti, user_id, expires_at)
         SELECT $1, user_id, CASE WHEN session_id = $1 THEN expires_at ELSE $3 END
         FROM auth_sessions WHERE session_id = $2",
            &[
                (&actor.jti, Type::UUID),
                (&actor.sid, Type::UUID),
                (&expires, Type::TIMESTAMPTZ),
            ],
        )
        .await?;
    Ok(())
}

async fn authenticated(
    route: AuthRoute,
    req: Request,
    client: &mut Client,
    secret: &[u8],
    env: &Env,
) -> Result<Response> {
    let actor = actor(&req, secret)?;
    let tx = client
        .build_transaction()
        .read_only(false)
        .isolation_level(IsolationLevel::ReadCommitted)
        .start()
        .await?;
    lock_session(&tx, &actor, !matches!(route, AuthRoute::Me)).await?;
    let response = match route {
        AuthRoute::Refresh => {
            let expires = DateTime::from_timestamp(Utc::now().timestamp() + SESSION_SECONDS, 0)
                .ok_or_else(ApiError::internal)?;
            revoke_token(&tx, &actor).await?;
            tx.query_typed(
                "UPDATE auth_sessions SET expires_at = $2 WHERE session_id = $1",
                &[(&actor.sid, Type::UUID), (&expires, Type::TIMESTAMPTZ)],
            )
            .await?;
            token_response(
                secret,
                actor.user_id,
                actor.sid,
                actor.auth_version,
                expires,
            )?
        }
        AuthRoute::Logout => {
            revoke_token(&tx, &actor).await?;
            tx.query_typed(
                "UPDATE auth_sessions SET revoked_at = now() WHERE session_id = $1",
                &[(&actor.sid, Type::UUID)],
            )
            .await?;
            let invalid = tx.query_typed(
                "SELECT v.id FROM voice_sessions v WHERE v.state <> 'REVOKE_PENDING'
                 AND NOT EXISTS (SELECT 1 FROM authorized_voice_sessions a WHERE a.id=v.id) ORDER BY v.id",
                &[],
            ).await?;
            for row in invalid {
                let id: i64 = row.get(0);
                tx.query_typed(
                    "WITH marked AS (UPDATE voice_sessions SET state='REVOKE_PENDING' WHERE id=$1
                     RETURNING id,room_id,participant_identity)
                     INSERT INTO media_jobs(voice_session_id,room_id,identity)
                     SELECT id,room_id,participant_identity FROM marked
                     ON CONFLICT(voice_session_id) DO NOTHING",
                    &[(&id, Type::INT8)],
                )
                .await?;
            }
            Response::empty()
                .map_err(|_| ApiError::internal())?
                .with_status(204)
        }
        AuthRoute::Me => Response::from_json(&profile(&tx, actor.user_id).await?)
            .map_err(|_| ApiError::internal())?,
        AuthRoute::Login | AuthRoute::UpdateProfile => return Err(ApiError::internal()),
    };
    tx.commit().await?;
    let revocation = match route {
        AuthRoute::Refresh => Some(crate::realtime::RevokedSocket::Token(actor.jti)),
        AuthRoute::Logout => Some(crate::realtime::RevokedSocket::Session(actor.sid)),
        _ => None,
    };
    if let Some(revocation) = revocation {
        if crate::realtime::disconnect(env, actor.user_id, revocation)
            .await
            .is_err()
        {
            worker::console_warn!(
                "Could not notify the Durable Object of a committed session revocation."
            );
        }
    }
    Ok(response)
}

pub async fn handle(
    route: AuthRoute,
    req: Request,
    client: &mut Client,
    jwt_secret: &[u8],
    env: &Env,
) -> Result<Response> {
    if jwt_secret.len() < 32 {
        return Err(ApiError::unavailable());
    }
    match route {
        AuthRoute::Login => login(req, client, jwt_secret).await,
        AuthRoute::UpdateProfile => update_profile(req, client, jwt_secret, env).await,
        _ => authenticated(route, req, client, jwt_secret, env).await,
    }
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    input: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(input).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileUpdate {
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
    avatar_upload_id: Option<Option<crate::events::Id>>,
}

async fn update_profile(
    req: Request,
    client: &mut Client,
    key: &[u8],
    env: &Env,
) -> Result<Response> {
    let actor = actor(&req, key)?;
    let input: ProfileUpdate = json_body(req).await?;
    let invalid = || ApiError::new(400, "BAD_REQUEST", "Thông tin hồ sơ không hợp lệ.");
    let normalize = |value: Option<String>, max| -> Result<Option<String>> {
        value
            .map(|value| {
                let value: String = value.trim().nfc().collect();
                if value.is_empty()
                    || value.chars().count() > max
                    || value.chars().any(char::is_control)
                {
                    return Err(invalid());
                }
                Ok(value)
            })
            .transpose()
    };
    let name = normalize(input.full_name, 100)?;
    let phone = normalize(input.phone, 15)?;
    let email = normalize(input.email, 100)?;
    let set_address = input.address.is_some();
    let address = normalize(input.address.flatten(), 500)?;
    if input
        .date_of_birth
        .is_some_and(|date| date > Utc::now().date_naive())
        || email
            .as_ref()
            .is_some_and(|value| !email_address::EmailAddress::is_valid(value))
        || phone.as_ref().is_some_and(|value| {
            let digits = value.strip_prefix('+').unwrap_or(value);
            digits.is_empty() || !digits.bytes().all(|digit| digit.is_ascii_digit())
        })
    {
        return Err(invalid());
    }
    let user = actor.user_id;
    let tx = client.transaction().await?;
    tx.query_typed_opt(
        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
        &[(&user, Type::INT8)],
    )
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    lock_session(&tx, &actor, false).await?;
    if let Some(name) = name {
        tx.query_typed(
            "UPDATE users SET full_name=$2 WHERE id=$1",
            &[(&user, Type::INT8), (&name, Type::TEXT)],
        )
        .await?;
    }
    if let Some(email) = email {
        let changed = tx
            .query_typed_opt(
                "UPDATE user_details SET email=$2 WHERE user_id=$1 AND email<>$2 RETURNING user_id",
                &[(&user, Type::INT8), (&email, Type::TEXT)],
            )
            .await
            .map_err(|error| {
                if error.code() == Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) {
                    ApiError::new(409, "CONFLICT", "Tên đăng nhập hoặc email đã được sử dụng.")
                } else {
                    error.into()
                }
            })?;
        if changed.is_some() {
            tx.query_typed(
                "UPDATE password_reset_tokens SET is_used=true WHERE user_id=$1 AND NOT is_used",
                &[(&user, Type::INT8)],
            )
            .await?;
        }
    }
    let mut replaced = None;
    let set_avatar = input.avatar_upload_id.is_some();
    let avatar = input.avatar_upload_id.flatten().map(|id| id.0);
    if set_avatar {
        let old: Option<i64> = tx
            .query_typed_one(
                "SELECT avatar_upload_id FROM user_details WHERE user_id=$1",
                &[(&user, Type::INT8)],
            )
            .await?
            .try_get(0)?;
        if old != avatar {
            if let Some(upload) = avatar {
                crate::files::attach_image(env, &tx, user, None, upload, "AVATAR").await?;
            }
            replaced = old;
        }
    }
    tx.query_typed(
        "UPDATE user_details SET date_of_birth=COALESCE($2,date_of_birth), phone=COALESCE($3,phone),
         address=CASE WHEN $4 THEN $5 ELSE address END, avatar_upload_id=CASE WHEN $6 THEN $7 ELSE avatar_upload_id END WHERE user_id=$1",
        &[(&user,Type::INT8),(&input.date_of_birth,Type::DATE),(&phone,Type::TEXT),(&set_address,Type::BOOL),
          (&address,Type::TEXT),(&set_avatar,Type::BOOL),(&avatar,Type::INT8)],
    ).await?;
    if let Some(upload) = replaced {
        crate::files::discard_image(&tx, upload).await?;
    }
    let response =
        Response::from_json(&profile(&tx, user).await?).map_err(|_| ApiError::internal())?;
    tx.commit().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const LEGACY_TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI5MjIzMzcyMDM2ODU0Nzc1ODA3IiwianRpIjoiN2M5ZTY2NzktNzQyNS00MGRlLTk0NGItZTA3ZmMxZjkwYWU3IiwiZXhwIjoyMDAwMDAwMDAwLCJhdXRoX3ZlcnNpb24iOjN9.1iIKWVhHzyq5iRs7nxP-6KJUqgl315Yn7z3U3KtbuuA";

    fn signed(header: Value, payload: Value) -> String {
        let signed = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(payload.to_string())
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
        mac.update(signed.as_bytes());
        format!(
            "{signed}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn independently_signed_legacy_token_preserves_snowflake_and_session() {
        let actor = decode_token(LEGACY_TOKEN, SECRET, 1_999_999_999).unwrap();
        assert_eq!(actor.user_id, i64::MAX);
        assert_eq!(actor.sid, actor.jti);
        assert_eq!(actor.auth_version, 3);
        assert!(decode_token(LEGACY_TOKEN, SECRET, 2_000_000_000).is_err());
        assert!(
            decode_token(
                LEGACY_TOKEN,
                b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1_999_999_999
            )
            .is_err()
        );
    }

    #[test]
    fn signed_tokens_require_hs256_and_decimal_string_identity() {
        let payload = json!({
            "sub": "9007199254740993", "sid": "e3667835-3a73-40ad-85e1-613176fc66e3",
            "jti": "7c9e6679-7425-40de-944b-e07fc1f90ae7", "exp": 2_000_000_000,
            "auth_version": 3,
        });
        let token = signed(json!({"alg": "HS256"}), payload.clone());
        let actor = decode_token(&token, SECRET, 1_999_999_999).unwrap();
        assert_eq!(actor.user_id, 9_007_199_254_740_993);
        assert_ne!(actor.sid, actor.jti);
        for header in [
            json!({"alg": "none"}),
            json!({"alg": "HS384"}),
            json!({"alg": "HS256", "crit": ["x"]}),
        ] {
            assert!(decode_token(&signed(header, payload.clone()), SECRET, 1_999_999_999).is_err());
        }
        for invalid in [
            json!(9007199254740993_i64),
            json!("09007199254740993"),
            json!("-1"),
            json!("0"),
        ] {
            let mut invalid_payload = payload.clone();
            invalid_payload["sub"] = invalid;
            assert!(
                decode_token(
                    &signed(json!({"alg": "HS256"}), invalid_payload),
                    SECRET,
                    1_999_999_999
                )
                .is_err()
            );
        }
    }

    #[test]
    fn login_input_matches_native_normalization_and_rejects_extra_fields() {
        assert_eq!(username("  qua\u{0302}n  ").unwrap(), "quân");
        assert!(username("user@example.com").is_err());
        assert!(username("user\nname").is_err());
        assert!(
            serde_json::from_value::<Login>(
                json!({"username": "quan", "password": "password1234", "user_id": "1"})
            )
            .is_err()
        );
    }
}
