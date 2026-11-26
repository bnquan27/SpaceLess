use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use email_address::EmailAddress;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{cell::Cell, net::IpAddr};
use tokio_postgres::{Client, types::Type};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
use worker::{
    AbortSignal, Env, Fetch, Headers, Method, Request, RequestInit, RequestRedirect, Response,
    js_sys::{self, Array, Object, Reflect},
    url::{Host, form_urlencoded},
    wasm_bindgen::{JsCast, JsValue},
    wasm_bindgen_futures::JsFuture,
    web_sys::{self, CryptoKey, WorkerGlobalScope},
};

use crate::{
    auth::{self, ApiError},
    db,
};

type Result<T> = std::result::Result<T, ApiError>;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const RSA: &str = "RSASSA-PKCS1-v1_5";
const GOOGLE_WORK_LIMIT: u8 = 4;

thread_local! {
    static ACTIVE_EXCHANGES: Cell<u8> = const { Cell::new(0) };
}

struct ExchangePermit;

impl ExchangePermit {
    fn acquire() -> Result<Self> {
        ACTIVE_EXCHANGES.with(|active| {
            if active.get() >= GOOGLE_WORK_LIMIT {
                return Err(ApiError::unavailable());
            }
            active.set(active.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for ExchangePermit {
    fn drop(&mut self) {
        ACTIVE_EXCHANGES.with(|active| active.set(active.get() - 1));
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    code: String,
    code_verifier: String,
    nonce: String,
    loopback_port: u16,
}

impl Exchange {
    fn validate(&self) -> Result<()> {
        let unreserved = |byte: u8| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte);
        if self.code.is_empty()
            || self.code.len() > 4096
            || self.code.chars().any(char::is_control)
            || !(43..=128).contains(&self.code_verifier.len())
            || !self.code_verifier.bytes().all(unreserved)
            || !(32..=128).contains(&self.nonce.len())
            || !self.nonce.bytes().all(unreserved)
            || self.loopback_port == 0
        {
            return Err(ApiError::new(
                400,
                "INVALID_GOOGLE_EXCHANGE",
                "Yêu cầu không hợp lệ.",
            ));
        }
        Ok(())
    }
}

struct Config {
    client_id: String,
    client_secret: String,
}

impl Config {
    fn from_env(env: &Env) -> Result<Self> {
        let client_id = env
            .secret("GOOGLE_OAUTH_CLIENT_ID")
            .ok()
            .map(|value| value.to_string());
        let client_secret = env
            .secret("GOOGLE_OAUTH_CLIENT_SECRET")
            .ok()
            .map(|value| value.to_string());
        let (Some(client_id), Some(client_secret)) = (client_id, client_secret) else {
            return Err(ApiError::new(
                503,
                "GOOGLE_NOT_CONFIGURED",
                "Đăng nhập Google chưa được cấu hình.",
            ));
        };
        if !client_id.ends_with(".apps.googleusercontent.com")
            || client_id.len() > 255
            || client_id.chars().any(char::is_whitespace)
            || client_secret.is_empty()
            || client_secret.len() > 1024
            || client_secret.chars().any(char::is_whitespace)
        {
            return Err(ApiError::unavailable());
        }
        Ok(Self {
            client_id,
            client_secret,
        })
    }

    fn form(&self, exchange: &Exchange) -> String {
        let redirect_uri = format!("http://127.0.0.1:{}", exchange.loopback_port);
        form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", &self.client_id)
            .append_pair("client_secret", &self.client_secret)
            .append_pair("code", &exchange.code)
            .append_pair("code_verifier", &exchange.code_verifier)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("grant_type", "authorization_code")
            .finish()
    }

    async fn identity(&self, exchange: &Exchange) -> Result<Identity> {
        let headers = Headers::new();
        headers
            .set("Content-Type", "application/x-www-form-urlencoded")
            .map_err(|_| ApiError::internal())?;
        let request = Request::new_with_init(
            TOKEN_URL,
            RequestInit::new()
                .with_method(Method::Post)
                .with_headers(headers)
                .with_redirect(RequestRedirect::Manual)
                .with_body(Some(JsValue::from_str(&self.form(exchange)))),
        )
        .map_err(|_| ApiError::internal())?;
        let response = send(request).await?;
        if matches!(response.status_code(), 400 | 401) {
            return Err(invalid_google());
        }
        let token: TokenResponse = provider_json(response, 32 * 1024).await?;
        let token = SignedToken::parse(&token.id_token)?;
        let request = Request::new_with_init(
            JWKS_URL,
            RequestInit::new().with_redirect(RequestRedirect::Manual),
        )
        .map_err(|_| ApiError::internal())?;
        let keys: JwkSet = provider_json(send(request).await?, 64 * 1024).await?;
        let key = keys.for_token(&token.header.kid)?;
        if !verify_signature(key, &token).await? {
            return Err(invalid_google());
        }
        token.identity(&self.client_id, &exchange.nonce, Utc::now().timestamp())
    }
}

async fn send(request: Request) -> Result<Response> {
    let timeout = AbortSignal::from(web_sys::AbortSignal::timeout_with_u32(10_000));
    Fetch::Request(request)
        .send_with_signal(&timeout)
        .await
        .map_err(|_| ApiError::unavailable())
}

async fn provider_json<T: DeserializeOwned>(mut response: Response, limit: usize) -> Result<T> {
    if !(200..300).contains(&response.status_code()) {
        return Err(ApiError::unavailable());
    }
    let mut stream = response.stream().map_err(|_| ApiError::unavailable())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::unavailable())?;
        if chunk.len() > limit - bytes.len() {
            return Err(ApiError::unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ApiError::unavailable())
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Deserialize)]
struct JwkSet {
    keys: Vec<Jwk>,
}

impl JwkSet {
    fn for_token(&self, kid: &str) -> Result<&Jwk> {
        let mut matching = self
            .keys
            .iter()
            .filter(|key| key.kid.as_deref() == Some(kid));
        let key = matching.next().ok_or_else(invalid_google)?;
        if matching.next().is_some()
            || key.kty != "RSA"
            || key.alg.as_deref().is_some_and(|alg| alg != "RS256")
            || key
                .purpose
                .as_deref()
                .is_some_and(|purpose| purpose != "sig")
            || key
                .n
                .as_deref()
                .is_none_or(|n| n.is_empty() || n.len() > 1024)
            || key
                .e
                .as_deref()
                .is_none_or(|e| e.is_empty() || e.len() > 16)
        {
            return Err(invalid_google());
        }
        Ok(key)
    }
}

#[derive(Deserialize, Serialize)]
struct Jwk {
    kty: String,
    kid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alg: Option<String>,
    #[serde(rename = "use", skip_serializing_if = "Option::is_none")]
    purpose: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: String,
    crit: Option<Vec<String>>,
    b64: Option<bool>,
}

struct SignedToken<'a> {
    header: Header,
    signed: &'a str,
    payload: &'a str,
    signature: Vec<u8>,
}

impl<'a> SignedToken<'a> {
    fn parse(raw: &'a str) -> Result<Self> {
        if raw.len() > 16 * 1024 {
            return Err(invalid_google());
        }
        let (signed, signature) = raw.rsplit_once('.').ok_or_else(invalid_google)?;
        let (header, payload) = signed.split_once('.').ok_or_else(invalid_google)?;
        let header: Header = decode_json(header)?;
        if header.alg != "RS256"
            || header.kid.is_empty()
            || header.kid.len() > 255
            || header.crit.is_some()
            || header.b64 == Some(false)
        {
            return Err(invalid_google());
        }
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| invalid_google())?;
        if signature.is_empty() || payload.is_empty() {
            return Err(invalid_google());
        }
        Ok(Self {
            header,
            signed,
            payload,
            signature,
        })
    }

    fn identity(&self, client_id: &str, nonce: &str, now: i64) -> Result<Identity> {
        let claims: Claims = decode_json(self.payload)?;
        claims.identity(client_id, nonce, now)
    }
}

fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| invalid_google())?;
    serde_json::from_slice(&bytes).map_err(|_| invalid_google())
}

async fn verify_signature(jwk: &Jwk, token: &SignedToken<'_>) -> Result<bool> {
    let global: WorkerGlobalScope = js_sys::global().unchecked_into();
    let crypto = global
        .crypto()
        .map_err(|_| ApiError::unavailable())?
        .subtle();
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &RSA.into()).map_err(|_| ApiError::internal())?;
    Reflect::set(&algorithm, &"hash".into(), &"SHA-256".into())
        .map_err(|_| ApiError::internal())?;
    let raw = serde_json::to_string(jwk).map_err(|_| ApiError::internal())?;
    let jwk: Object = js_sys::JSON::parse(&raw)
        .map_err(|_| ApiError::internal())?
        .unchecked_into();
    let usages = Array::of1(&"verify".into());
    let key: CryptoKey = JsFuture::from(
        crypto
            .import_key_with_object("jwk", &jwk, &algorithm, false, &usages)
            .map_err(|_| invalid_google())?,
    )
    .await
    .map_err(|_| invalid_google())?
    .unchecked_into();
    let result = crypto
        .verify_with_str_and_u8_array_and_u8_array(
            RSA,
            &key,
            &token.signature,
            token.signed.as_bytes(),
        )
        .map_err(|_| invalid_google())?;
    JsFuture::from(result)
        .await
        .map_err(|_| invalid_google())?
        .as_bool()
        .ok_or_else(invalid_google)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn contains(&self, client_id: &str) -> bool {
        match self {
            Self::One(value) => value == client_id,
            Self::Many(values) => values.iter().any(|value| value == client_id),
        }
    }
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    exp: i64,
    sub: String,
    nonce: String,
    email: String,
    email_verified: bool,
    name: Option<String>,
    azp: Option<String>,
}

#[derive(Debug)]
struct Identity {
    subject: String,
    email: String,
    full_name: String,
}

impl Claims {
    fn identity(self, client_id: &str, nonce: &str, now: i64) -> Result<Identity> {
        if !matches!(
            self.iss.as_str(),
            "https://accounts.google.com" | "accounts.google.com"
        ) || !self.aud.contains(client_id)
            || self.exp <= now
            || self.nonce != nonce
            || !self.email_verified
            || self.sub.is_empty()
            || self.sub.len() > 255
            || self.sub.chars().any(char::is_control)
            || self.azp.as_deref().is_some_and(|azp| azp != client_id)
        {
            return Err(invalid_google());
        }
        let email = normalized(&self.email)
            .filter(|email| valid_email(email))
            .ok_or_else(invalid_google)?;
        let full_name = self
            .name
            .as_deref()
            .and_then(normalized)
            .unwrap_or_else(|| "Người dùng Google".into());
        Ok(Identity {
            subject: self.sub,
            email,
            full_name,
        })
    }
}

fn normalized(value: &str) -> Option<String> {
    let value: String = value.trim().nfc().collect();
    (!value.is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control))
        .then_some(value)
}

fn valid_email(value: &str) -> bool {
    let Some((user, domain)) = value.rsplit_once('@') else {
        return false;
    };
    let bracketed_ip = domain
        .strip_prefix('[')
        .and_then(|domain| domain.strip_suffix(']'));
    EmailAddress::is_valid_local_part(user)
        && (EmailAddress::is_valid_domain(domain)
            || bracketed_ip.unwrap_or(domain).parse::<IpAddr>().is_ok()
            || (!domain.is_ascii()
                && !domain.contains('%')
                && matches!(Host::parse(domain), Ok(Host::Domain(domain)) if EmailAddress::is_valid_domain(&domain))))
}

fn invalid_google() -> ApiError {
    ApiError::new(
        401,
        "INVALID_GOOGLE_LOGIN",
        "Đăng nhập Google không hợp lệ hoặc đã hết hạn. Vui lòng thử lại.",
    )
}

pub(crate) async fn handle(
    req: Request,
    client: &mut Client,
    jwt_secret: &[u8],
    env: &Env,
) -> Result<Response> {
    let exchange: Exchange = auth::json_body(req).await?;
    exchange.validate()?;
    let config = Config::from_env(env)?;
    let _permit = ExchangePermit::acquire()?;
    let identity = config.identity(&exchange).await?;
    sign_in(client, jwt_secret, identity).await
}

async fn sign_in(client: &mut Client, jwt_secret: &[u8], identity: Identity) -> Result<Response> {
    let tx = client.transaction().await?;
    tx.query_typed(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 717172))",
        &[(&identity.subject, Type::TEXT)],
    )
    .await?;
    let existing = tx
        .query_typed_opt(
            "SELECT user_id FROM google_identities WHERE subject = $1",
            &[(&identity.subject, Type::TEXT)],
        )
        .await?;
    let user_id: i64 = if let Some(row) = existing {
        row.try_get(0)?
    } else {
        let user_id = db::snowflake(&tx).await?;
        let username = format!("g_{}", Uuid::new_v4().simple());
        tx.query_typed(
            "INSERT INTO users (id, username, full_name) VALUES ($1, $2, $3)",
            &[
                (&user_id, Type::INT8),
                (&username, Type::TEXT),
                (&identity.full_name, Type::TEXT),
            ],
        )
        .await?;
        tx.query_typed(
            "INSERT INTO user_details (user_id, email) VALUES ($1, $2)",
            &[(&user_id, Type::INT8), (&identity.email, Type::TEXT)],
        )
        .await
        .map_err(|error| {
            if error.code() == Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) {
                ApiError::new(
                    409,
                    "GOOGLE_EMAIL_CONFLICT",
                    "Email này đã được sử dụng. Hãy đăng nhập bằng tài khoản hiện có.",
                )
            } else {
                error.into()
            }
        })?;
        tx.query_typed(
            "INSERT INTO google_identities (subject, user_id) VALUES ($1, $2)",
            &[(&identity.subject, Type::TEXT), (&user_id, Type::INT8)],
        )
        .await?;
        user_id
    };
    let account = tx
        .query_typed_one(
            "SELECT is_available, auth_version FROM users WHERE id = $1 FOR SHARE",
            &[(&user_id, Type::INT8)],
        )
        .await?;
    if !account.try_get::<_, bool>(0)? {
        return Err(ApiError::new(
            403,
            "ACCOUNT_LOCKED",
            "Tài khoản đã bị khóa.",
        ));
    }
    let response = auth::issue_session(&tx, jwt_secret, user_id, account.try_get(1)?).await?;
    tx.commit().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CLIENT_ID: &str = "fixture.apps.googleusercontent.com";
    const NONCE: &str = "fixture_nonce_012345678901234567890123456789";

    #[test]
    fn oauth_form_preserves_code_and_pkce_and_constructs_loopback_redirect() {
        let config = Config {
            client_id: CLIENT_ID.into(),
            client_secret: "test_secret".into(),
        };
        let exchange = Exchange {
            code: "one+two&three=/".into(),
            code_verifier: "x".repeat(43),
            nonce: NONCE.into(),
            loopback_port: 49152,
        };
        exchange.validate().unwrap();
        let fields = form_urlencoded::parse(config.form(&exchange).as_bytes())
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields["code"], exchange.code);
        assert_eq!(fields["code_verifier"], exchange.code_verifier);
        assert_eq!(fields["redirect_uri"], "http://127.0.0.1:49152");
        assert_eq!(fields["grant_type"], "authorization_code");
    }

    #[test]
    fn claims_must_match_google_client_and_exchange() {
        let claims = json!({
            "iss": "https://accounts.google.com", "aud": [CLIENT_ID],
            "exp": 501, "sub": "google-subject", "nonce": NONCE,
            "email": "fixture@example.test", "email_verified": true, "name": "  Người dùng  ",
        });
        let verified = serde_json::from_value::<Claims>(claims.clone())
            .unwrap()
            .identity(CLIENT_ID, NONCE, 500)
            .unwrap();
        assert_eq!(verified.subject, "google-subject");
        assert_eq!(verified.email, "fixture@example.test");
        assert_eq!(verified.full_name, "Người dùng");
        for (key, value) in [
            ("iss", json!("https://other.example")),
            ("aud", json!("some-other-client")),
            ("azp", json!("some-other-client")),
            ("exp", json!(500)),
            ("sub", json!("")),
            ("nonce", json!("different")),
            ("email_verified", json!(false)),
            ("email", json!("not-an-email")),
        ] {
            let mut broken = claims.clone();
            broken[key] = value;
            assert!(
                serde_json::from_value::<Claims>(broken)
                    .unwrap()
                    .identity(CLIENT_ID, NONCE, 500)
                    .is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn busy_exchange_leaves_no_extra_slot_and_releases_permits() {
        let permits = (0..GOOGLE_WORK_LIMIT)
            .map(|_| ExchangePermit::acquire().unwrap())
            .collect::<Vec<_>>();
        assert!(ExchangePermit::acquire().is_err());
        drop(permits);
        assert!(ExchangePermit::acquire().is_ok());
    }
}
