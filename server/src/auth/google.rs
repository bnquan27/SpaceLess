use std::{env, time::Duration};

use axum::{Json, extract::State, http::StatusCode};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{ApiError, AppState, Body, Id, Result};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
static GOOGLE_WORK: Semaphore = Semaphore::const_new(4);

#[derive(Clone)]
pub(crate) struct Config {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

impl Config {
    pub(crate) fn from_env() -> Result<Option<Self>> {
        Self::from_values(
            env::var("GOOGLE_OAUTH_CLIENT_ID").ok(),
            env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok(),
        )
    }

    fn from_values(
        client_id: Option<String>,
        client_secret: Option<String>,
    ) -> Result<Option<Self>> {
        let (client_id, client_secret) = match (client_id, client_secret) {
            (None, None) => return Ok(None),
            (Some(id), Some(secret))
                if id.ends_with(".apps.googleusercontent.com")
                    && id.len() <= 255
                    && !id.chars().any(char::is_whitespace)
                    && !secret.is_empty()
                    && secret.len() <= 1024
                    && !secret.chars().any(char::is_whitespace) =>
            {
                (id, secret)
            }
            _ => {
                return Err(ApiError::bad(
                    "Cấu hình đăng nhập Google chưa đầy đủ hoặc không hợp lệ.",
                ));
            }
        };
        Ok(Some(Self {
            client_id,
            client_secret,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|_| ApiError::internal())?,
        }))
    }

    fn token_request(&self, input: &Exchange) -> reqwest::RequestBuilder {
        let redirect_uri = format!("http://127.0.0.1:{}", input.loopback_port);
        self.http.post(TOKEN_URL).form(&[
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("code", input.code.as_str()),
            ("code_verifier", input.code_verifier.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
    }

    async fn identity(&self, input: &Exchange) -> Result<Identity> {
        let response = self
            .token_request(input)
            .send()
            .await
            .map_err(|_| ApiError::unavailable())?;
        if matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED
        ) {
            return Err(invalid_google());
        }
        let token: TokenResponse = response
            .error_for_status()
            .map_err(|_| ApiError::unavailable())?
            .json()
            .await
            .map_err(|_| ApiError::unavailable())?;
        let keys: JwkSet = self
            .http
            .get(JWKS_URL)
            .send()
            .await
            .map_err(|_| ApiError::unavailable())?
            .error_for_status()
            .map_err(|_| ApiError::unavailable())?
            .json()
            .await
            .map_err(|_| ApiError::unavailable())?;
        verify_identity(&token.id_token, &keys, &self.client_id, &input.nonce)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Exchange {
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
            return Err(ApiError::bad("INVALID_GOOGLE_EXCHANGE"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    nonce: String,
    email: String,
    email_verified: bool,
    name: Option<String>,
    azp: Option<String>,
}

struct Identity {
    subject: String,
    email: String,
    full_name: String,
}

fn invalid_google() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "INVALID_GOOGLE_LOGIN",
        "Đăng nhập Google không hợp lệ hoặc đã hết hạn. Vui lòng thử lại.",
    )
}

fn verify_identity(token: &str, keys: &JwkSet, client_id: &str, nonce: &str) -> Result<Identity> {
    let header = jsonwebtoken::decode_header(token).map_err(|_| invalid_google())?;
    if header.alg != Algorithm::RS256 {
        return Err(invalid_google());
    }
    let key = header
        .kid
        .as_deref()
        .and_then(|kid| keys.find(kid))
        .ok_or_else(invalid_google)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.leeway = 0;
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_jwk(key).map_err(|_| invalid_google())?,
        &validation,
    )
    .map_err(|_| invalid_google())?
    .claims;
    if claims.nonce != nonce
        || !claims.email_verified
        || claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.sub.chars().any(char::is_control)
        || claims.azp.as_deref().is_some_and(|azp| azp != client_id)
    {
        return Err(invalid_google());
    }
    let (email, _) = super::email(&claims.email).map_err(|_| invalid_google())?;
    let full_name = claims
        .name
        .as_deref()
        .and_then(|name| super::normalized(name, 100, "").ok())
        .unwrap_or_else(|| "Người dùng Google".into());
    Ok(Identity {
        subject: claims.sub,
        email,
        full_name,
    })
}

pub(super) async fn exchange(
    State(state): State<AppState>,
    Body(input): Body<Exchange>,
) -> Result<Json<Value>> {
    input.validate()?;
    let config = state.google.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "GOOGLE_NOT_CONFIGURED",
            "Đăng nhập Google chưa được cấu hình.",
        )
    })?;
    let _permit = GOOGLE_WORK
        .try_acquire()
        .map_err(|_| ApiError::unavailable())?;
    let identity = config.identity(&input).await?;
    sign_in(&state, identity).await
}

async fn sign_in(state: &AppState, identity: Identity) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 717172))")
        .bind(&identity.subject)
        .execute(&mut *tx)
        .await?;
    let existing =
        sqlx::query_scalar::<_, i64>("SELECT user_id FROM google_identities WHERE subject = $1")
            .bind(&identity.subject)
            .fetch_optional(&mut *tx)
            .await?;
    let user_id = if let Some(user_id) = existing {
        Id(user_id)
    } else {
        let user_id = state.id().await?;
        let username = format!("g_{}", Uuid::new_v4().simple());
        sqlx::query("INSERT INTO users (id, username, full_name) VALUES ($1, $2, $3)")
            .bind(user_id.0)
            .bind(username)
            .bind(identity.full_name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO user_details (user_id, email) VALUES ($1, $2)")
            .bind(user_id.0)
            .bind(identity.email)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if error
                    .as_database_error()
                    .is_some_and(|db| db.is_unique_violation())
                {
                    ApiError::new(
                        StatusCode::CONFLICT,
                        "GOOGLE_EMAIL_CONFLICT",
                        "Email này đã được sử dụng. Hãy đăng nhập bằng tài khoản hiện có.",
                    )
                } else {
                    error.into()
                }
            })?;
        sqlx::query("INSERT INTO google_identities (subject, user_id) VALUES ($1, $2)")
            .bind(identity.subject)
            .bind(user_id.0)
            .execute(&mut *tx)
            .await?;
        user_id
    };
    let (available, auth_version): (bool, i64) =
        sqlx::query_as("SELECT is_available, auth_version FROM users WHERE id = $1 FOR SHARE")
            .bind(user_id.0)
            .fetch_one(&mut *tx)
            .await?;
    if !available {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "ACCOUNT_LOCKED",
            "Tài khoản đã bị khóa.",
        ));
    }
    let response = super::issue_session(state, &mut tx, user_id, auth_version).await?;
    tx.commit().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use jsonwebtoken::{EncodingKey, Header};
    use serde_json::json;

    const CLIENT_ID: &str = "fixture.apps.googleusercontent.com";
    const NONCE: &str = "fixture_nonce_012345678901234567890123456789";

    fn input() -> Exchange {
        Exchange {
            code: "one-time-code".into(),
            code_verifier: "01234567890123456789012345678901234567890123456789".into(),
            nonce: NONCE.into(),
            loopback_port: 49152,
        }
    }

    fn claims() -> Value {
        json!({
            "iss": "https://accounts.google.com", "aud": CLIENT_ID,
            "exp": Utc::now().timestamp() + 300, "iat": Utc::now().timestamp(),
            "sub": "google-fixture-subject", "nonce": NONCE,
            "email": "fixture@example.test", "email_verified": true, "name": "Người dùng thử"
        })
    }

    fn signed(claims: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("google-test-key".into());
        jsonwebtoken::encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(include_bytes!(
                "../../test/fixtures/google-private-key.pem"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    fn keys() -> JwkSet {
        serde_json::from_str(include_str!("../../test/fixtures/google-jwks.json")).unwrap()
    }

    #[test]
    fn google_claims_require_a_signed_current_verified_identity_for_this_client_and_nonce() {
        let identity = verify_identity(&signed(&claims()), &keys(), CLIENT_ID, NONCE).unwrap();
        assert_eq!(identity.subject, "google-fixture-subject");
        assert_eq!(identity.email, "fixture@example.test");
        assert_eq!(identity.full_name, "Người dùng thử");
        for (field, value) in [
            ("iss", json!("https://attacker.example")),
            ("aud", json!("another.apps.googleusercontent.com")),
            ("exp", json!(Utc::now().timestamp() - 1)),
            ("nonce", json!("another nonce")),
            ("email_verified", json!(false)),
            ("email", json!("not-an-email")),
            ("sub", json!("")),
            ("azp", json!("another.apps.googleusercontent.com")),
        ] {
            let mut invalid = claims();
            invalid[field] = value;
            assert_eq!(
                verify_identity(&signed(&invalid), &keys(), CLIENT_ID, NONCE)
                    .err()
                    .unwrap()
                    .code,
                "INVALID_GOOGLE_LOGIN",
                "{field}"
            );
        }
        let mut missing_expiry = claims();
        missing_expiry.as_object_mut().unwrap().remove("exp");
        assert!(verify_identity(&signed(&missing_expiry), &keys(), CLIENT_ID, NONCE).is_err());
        let forged = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &claims(),
            &EncodingKey::from_secret(b"attacker-key"),
        )
        .unwrap();
        assert!(verify_identity(&forged, &keys(), CLIENT_ID, NONCE).is_err());
        assert!(
            verify_identity(
                &signed(&claims()),
                &JwkSet { keys: vec![] },
                CLIENT_ID,
                NONCE
            )
            .is_err()
        );
        let mut changed_payload = signed(&claims());
        let index = changed_payload.find('.').unwrap() + 2;
        changed_payload.replace_range(index..index + 1, "A");
        assert!(verify_identity(&changed_payload, &keys(), CLIENT_ID, NONCE).is_err());
        let mut no_name = claims();
        no_name.as_object_mut().unwrap().remove("name");
        assert_eq!(
            verify_identity(&signed(&no_name), &keys(), CLIENT_ID, NONCE)
                .unwrap()
                .full_name,
            "Người dùng Google"
        );
    }

    #[test]
    fn google_exchange_validates_pkce_and_uses_only_the_official_token_endpoint() {
        let config = Config::from_values(Some(CLIENT_ID.into()), Some("fixture-secret".into()))
            .unwrap()
            .unwrap();
        let input = input();
        input.validate().unwrap();
        let request = config.token_request(&input).build().unwrap();
        assert_eq!(request.url().as_str(), TOKEN_URL);
        let form = std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap();
        let url = reqwest::Url::parse(&format!("http://fixture/?{form}")).unwrap();
        let fields = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields["redirect_uri"], "http://127.0.0.1:49152");
        assert_eq!(fields["grant_type"], "authorization_code");
        assert_eq!(fields["code_verifier"], input.code_verifier);
        assert_eq!(fields["client_id"], CLIENT_ID);
        assert_eq!(fields["client_secret"], "fixture-secret");
        assert!(Config::from_values(None, None).unwrap().is_none());
        assert!(Config::from_values(Some(CLIENT_ID.into()), None).is_err());
        assert!(Config::from_values(None, Some("secret".into())).is_err());
        for change in 0..5 {
            let mut invalid = self::input();
            match change {
                0 => invalid.code.clear(),
                1 => invalid.code_verifier = "short".into(),
                2 => invalid.code_verifier = "a".repeat(42) + "+",
                3 => invalid.nonce = "short".into(),
                _ => invalid.loopback_port = 0,
            }
            assert!(invalid.validate().is_err());
        }
    }

    fn fixture_identity(subject: &str, email: &str) -> Identity {
        Identity {
            subject: subject.into(),
            email: email.into(),
            full_name: "Google fixture".into(),
        }
    }

    #[tokio::test]
    async fn google_exchange_rejects_busy_provider_before_contacting_google() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://fixture:fixture@127.0.0.1/spaceless_test_unused")
            .unwrap();
        let state = AppState {
            id_pool: pool.clone(),
            pool,
            worker_id: 1023,
            jwt_secret: "google-test-session-key".into(),
            http: reqwest::Client::new(),
            mail: None,
            google: Config::from_values(Some(CLIENT_ID.into()), Some("fixture-secret".into()))
                .unwrap(),
            files: None,
            voice: None,
            realtime: crate::sync::Realtime::default(),
        };
        let _busy = GOOGLE_WORK.try_acquire_many(4).unwrap();
        let error = exchange(State(state), Body(input())).await.err().unwrap();
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code, "UNAVAILABLE");
    }

    fn actor(state: &AppState, response: &Json<Value>) -> crate::Actor {
        let token = response.0["access_token"].as_str().unwrap();
        let claims = jsonwebtoken::decode::<super::super::Claims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .unwrap()
        .claims;
        crate::Actor {
            user_id: claims.sub,
            sid: claims.sid.unwrap(),
            jti: claims.jti,
            exp: claims.exp,
            auth_version: claims.auth_version,
        }
    }

    #[tokio::test]
    #[ignore = "Requires GOOGLE_TEST_DATABASE_URL pointing to an isolated localhost spaceless_test_* PostgreSQL database"]
    async fn google_database_provisions_once_preserves_profiles_and_enforces_account_sessions() {
        let database = env::var("GOOGLE_TEST_DATABASE_URL").expect("Set GOOGLE_TEST_DATABASE_URL");
        let url = reqwest::Url::parse(&database).unwrap();
        assert!(matches!(url.host_str(), Some("127.0.0.1" | "localhost")));
        assert!(matches!(url.scheme(), "postgres" | "postgresql"));
        assert!(url.path().starts_with("/spaceless_test_"));
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(&database)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let mut state = AppState {
            id_pool: pool.clone(),
            pool,
            worker_id: 1023,
            jwt_secret: "google-test-session-key".into(),
            http: reqwest::Client::new(),
            mail: None,
            google: None,
            files: None,
            voice: None,
            realtime: crate::sync::Realtime::default(),
        };
        let unique = Uuid::new_v4().simple().to_string();
        let subject = format!("fixture-{unique}");
        let email = format!("{unique}@example.test");
        let (first, second) = tokio::join!(
            sign_in(&state, fixture_identity(&subject, &email)),
            sign_in(&state, fixture_identity(&subject, &email)),
        );
        let first = first.unwrap();
        let first_actor = actor(&state, &first);
        let second_actor = actor(&state, &second.unwrap());
        assert_eq!(first_actor.user_id, second_actor.user_id);
        assert_ne!(first_actor.sid, second_actor.sid);
        super::super::session_valid(&state, &first_actor)
            .await
            .unwrap();
        let mut conn = state.pool.acquire().await.unwrap();
        let profile = super::super::profile(&mut conn, first_actor.user_id)
            .await
            .unwrap();
        assert!(profile["user_id"].is_string());
        assert!(profile["date_of_birth"].is_null());
        assert!(profile["phone"].is_null());
        assert!(profile["avatar_url"].is_null());
        assert_eq!(profile["email"], email);
        let google_password = super::super::login(
            State(state.clone()),
            Body(super::super::Login {
                username: profile["username"].as_str().unwrap().into(),
                password: "a test password".into(),
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(google_password.code, "INVALID_CREDENTIALS");
        state.mail = Some(super::super::MailConfig {
            transport: lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
                "127.0.0.1",
            )
            .port(1)
            .build(),
            sender: "fixture@example.test".parse().unwrap(),
            reset_url: reqwest::Url::parse("https://example.test/reset").unwrap(),
        });
        let recovery = super::super::forgot_password(
            State(state.clone()),
            axum::extract::ConnectInfo("127.0.0.1:41000".parse().unwrap()),
            Body(super::super::ForgotPassword {
                email: email.clone(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            recovery.0["message"],
            "Đã tiếp nhận yêu cầu, bạn sẽ nhận được email nếu đã đăng ký"
        );
        let reset_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM password_reset_tokens WHERE user_id = $1")
                .bind(first_actor.user_id.0)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(reset_count, 0);
        let local_email = format!("local-{email}");
        let _ = super::super::register(
            State(state.clone()),
            Body(super::super::Register {
                username: format!("local_{unique}"),
                password: "local test password".into(),
                full_name: "Local fixture".into(),
                date_of_birth: "1990-01-01".parse().unwrap(),
                email: local_email.clone(),
                phone: "+84900000000".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sign_in(
                &state,
                fixture_identity(&format!("local-conflict-{subject}"), &local_email)
            )
            .await
            .err()
            .unwrap()
            .code,
            "GOOGLE_EMAIL_CONFLICT"
        );
        let changed_email = format!("edited-{email}");
        sqlx::query(
            "UPDATE user_details SET email = $2, phone = '+84900000000' WHERE user_id = $1",
        )
        .bind(first_actor.user_id.0)
        .bind(&changed_email)
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query("UPDATE users SET full_name = 'Edited profile' WHERE id = $1")
            .bind(first_actor.user_id.0)
            .execute(&state.pool)
            .await
            .unwrap();
        let again = sign_in(
            &state,
            fixture_identity(&subject, "new-provider-email@example.test"),
        )
        .await
        .unwrap();
        assert_eq!(actor(&state, &again).user_id, first_actor.user_id);
        let edited = super::super::profile(&mut conn, first_actor.user_id)
            .await
            .unwrap();
        assert_eq!(edited["full_name"], "Edited profile");
        assert_eq!(edited["email"], changed_email);
        assert_eq!(edited["phone"], "+84900000000");
        let count_before: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        let collision = sign_in(
            &state,
            fixture_identity(&format!("other-{subject}"), &changed_email),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(collision.code, "GOOGLE_EMAIL_CONFLICT");
        let count_after: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count_after, count_before);
        let reset: [u8; 32] = rand::random();
        use sha2::Digest;
        sqlx::query("INSERT INTO password_reset_tokens (token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '15 minutes')")
            .bind(sha2::Sha256::digest(reset).as_slice()).bind(first_actor.user_id.0)
            .execute(&state.pool).await.unwrap();
        let reset_error = super::super::reset_password(
            State(state.clone()),
            Body(super::super::ResetPassword {
                token: hex::encode(reset),
                new_password: "new test password".into(),
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(reset_error.code, "INVALID_RESET_TOKEN");
        let refreshed = super::super::refresh(State(state.clone()), first_actor.clone())
            .await
            .unwrap();
        assert!(
            super::super::session_valid(&state, &first_actor)
                .await
                .is_err()
        );
        let refreshed_actor = actor(&state, &refreshed);
        super::super::session_valid(&state, &refreshed_actor)
            .await
            .unwrap();
        super::super::logout(State(state.clone()), refreshed_actor.clone())
            .await
            .unwrap();
        assert!(
            super::super::session_valid(&state, &refreshed_actor)
                .await
                .is_err()
        );
        sqlx::query("UPDATE users SET is_available = false WHERE id = $1")
            .bind(first_actor.user_id.0)
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(
            super::super::session_valid(&state, &second_actor)
                .await
                .is_err()
        );
        assert_eq!(
            sign_in(&state, fixture_identity(&subject, &email))
                .await
                .err()
                .unwrap()
                .code,
            "ACCOUNT_LOCKED"
        );
        sqlx::query("UPDATE users SET is_available = true WHERE id = $1")
            .bind(first_actor.user_id.0)
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(
            super::super::session_valid(&state, &second_actor)
                .await
                .is_err()
        );
        let fresh = sign_in(&state, fixture_identity(&subject, &email))
            .await
            .unwrap();
        super::super::session_valid(&state, &actor(&state, &fresh))
            .await
            .unwrap();
    }
}
