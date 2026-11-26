use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio_postgres::types::{ToSql, Type};
use uuid::Uuid;
use worker::{postgres_tls::PassthroughTls, *};

const PASSWORD: &[u8] = b"spaceless-synthetic-probe-password";
const SALT: &[u8] = b"spaceless-probe01";
const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3BhY2VsZXNzLXByb2JlMDE$L3ZCFOg6S67cmSOr1c3yw7D4tE012kgHtGm7rY2vQ0g";

type CheckResult = std::result::Result<(), &'static str>;

#[derive(Serialize)]
struct Outcome {
    check: &'static str,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'static str>,
}

fn reply(check: &'static str, result: CheckResult, status: u16) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::from_json(&Outcome {
        check,
        success: result.is_ok(),
        stage: result.err(),
    })?
    .with_status(status)
    .with_headers({
        headers.set("Content-Type", "application/json")?;
        headers
    }))
}

fn authorized(req: &Request, env: &Env) -> bool {
    let Ok(secret) = env.secret("PROBE_TOKEN") else {
        return false;
    };
    let secret = secret.to_string();
    if secret.len() < 32 {
        return false;
    }
    let Ok(Some(header)) = req.headers().get("Authorization") else {
        return false;
    };
    let Some(token) = header.strip_prefix("Bearer ") else {
        return false;
    };
    if token.len() > 512 {
        return false;
    }
    bool::from(Sha256::digest(secret.as_bytes()).ct_eq(&Sha256::digest(token.as_bytes())))
}

fn hash_check() -> CheckResult {
    let salt = SaltString::encode_b64(SALT).map_err(|_| "salt")?;
    // Identical defaults to server/src/auth.rs: Argon2id v19, m=19456,t=2,p=1.
    let hash = Argon2::default()
        .hash_password(PASSWORD, &salt)
        .map_err(|_| "hash")?;
    if hash.to_string() != PHC {
        return Err("hash_result");
    }
    Ok(())
}

fn verify_check() -> CheckResult {
    let hash = PasswordHash::new(PHC).map_err(|_| "phc")?;
    Argon2::default()
        .verify_password(PASSWORD, &hash)
        .map_err(|_| "verify")
}

async fn postgres_client(
    env: &Env,
    state: &State,
) -> std::result::Result<tokio_postgres::Client, &'static str> {
    let hyperdrive = env.hyperdrive("DB").map_err(|_| "binding")?;
    let socket = Socket::builder()
        .secure_transport(SecureTransport::StartTls)
        .connect(hyperdrive.host(), hyperdrive.port())
        .map_err(|_| "socket")?;
    let config = hyperdrive
        .connection_string()
        .parse::<tokio_postgres::Config>()
        .map_err(|_| "config")?;
    let (client, connection) = config
        .connect_raw(socket, PassthroughTls)
        .await
        .map_err(|_| "connect")?;
    state.wait_until(async move {
        // Never log driver errors: they can contain database details.
        let _ = connection.await;
    });
    Ok(client)
}

async fn postgres_check(env: &Env, state: &State) -> CheckResult {
    let mut client = postgres_client(env, state).await?;
    let transaction = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(|_| "transaction_begin")?;
    let id: i64 = 9_007_199_254_740_993;
    let text = "Lời nhắn của O'Brien; chào SpaceLess";
    // An unnamed typed query works with cache-disabled Hyperdrive. Only metadata is read.
    let query = transaction
        .query_typed(
            "SELECT $1::bigint, $2::text, current_setting('transaction_read_only') = 'on', \
             EXISTS(SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname = 'public')",
            &[(&id, Type::INT8), (&text, Type::TEXT)],
        )
        .await;
    let result = match query {
        Ok(rows) => match rows.as_slice() {
            [row] => {
                let bound_id = row.try_get::<_, i64>(0).map_err(|_| "bigint_decode");
                let bound_text = row.try_get::<_, String>(1).map_err(|_| "text_decode");
                let read_only = row.try_get::<_, bool>(2).map_err(|_| "transaction_decode");
                let schema = row.try_get::<_, bool>(3).map_err(|_| "schema_decode");
                (|| {
                    if bound_id? != id {
                        return Err("bigint_result");
                    }
                    if bound_text? != text {
                        return Err("text_result");
                    }
                    if !read_only? {
                        return Err("transaction_read_only");
                    }
                    if !schema? {
                        return Err("schema_result");
                    }
                    Ok(())
                })()
            }
            _ => Err("row_count"),
        },
        Err(_) => Err("query"),
    };
    transaction
        .rollback()
        .await
        .map_err(|_| "transaction_rollback")?;
    result
}

async fn session_revocation_check(env: &Env, state: &State) -> CheckResult {
    let mut client = postgres_client(env, state).await?;
    let transaction = client
        .build_transaction()
        .read_only(false)
        .start()
        .await
        .map_err(|_| "fixture_transaction_begin")?;
    let result = async {
        transaction
            .batch_execute(
                "SET LOCAL search_path TO pg_temp, pg_catalog;
                 CREATE TEMP TABLE users (
                    id bigint PRIMARY KEY, auth_version bigint NOT NULL, is_available boolean NOT NULL
                 ) ON COMMIT DROP;
                 CREATE TEMP TABLE auth_sessions (
                    session_id uuid PRIMARY KEY, user_id bigint NOT NULL, auth_version bigint NOT NULL,
                    revoked_at timestamptz, expires_at timestamptz NOT NULL
                 ) ON COMMIT DROP;
                 CREATE TEMP TABLE revoked_tokens (
                    token_jti uuid PRIMARY KEY, user_id bigint NOT NULL, expires_at timestamptz NOT NULL
                 ) ON COMMIT DROP;",
            )
            .await
            .map_err(|_| "fixture_tables")?;
        let expires: DateTime<Utc> = transaction
            .query_typed_one("SELECT clock_timestamp() + interval '1 hour'", &[])
            .await
            .map_err(|_| "fixture_clock")?
            .try_get(0)
            .map_err(|_| "fixture_timestamp_decode")?;
        let session_id = Uuid::from_u128(1);
        let user_id: i64 = 9_007_199_254_740_993;
        let auth_version: i64 = 7;
        let token_jti = Uuid::from_u128(2);
        transaction
            .query_typed(
                "INSERT INTO pg_temp.users (id, auth_version, is_available) VALUES ($1, $2, true)",
                &[(&user_id, Type::INT8), (&auth_version, Type::INT8)],
            )
            .await
            .map_err(|_| "fixture_user")?;
        transaction
            .query_typed(
                "INSERT INTO pg_temp.auth_sessions
                 (session_id, user_id, auth_version, expires_at) VALUES ($1, $2, $3, $4)",
                &[
                    (&session_id, Type::UUID),
                    (&user_id, Type::INT8),
                    (&auth_version, Type::INT8),
                    (&expires, Type::TIMESTAMPTZ),
                ],
            )
            .await
            .map_err(|_| "fixture_session")?;
        let parameters: &[(&(dyn ToSql + Sync), Type)] = &[
            (&session_id, Type::UUID),
            (&user_id, Type::INT8),
            (&auth_version, Type::INT8),
            (&expires, Type::TIMESTAMPTZ),
            (&token_jti, Type::UUID),
        ];
        let sql = include_str!("../../src/sql/validate_session.sql");
        let valid: bool = transaction
            .query_typed_one(sql, parameters)
            .await
            .map_err(|_| "fixture_validate_query")?
            .try_get(0)
            .map_err(|_| "fixture_validate_decode")?;
        if !valid {
            return Err("fixture_session_rejected");
        }
        transaction
            .query_typed(
                "INSERT INTO pg_temp.revoked_tokens (token_jti, user_id, expires_at) VALUES ($1, $2, $3)",
                &[
                    (&token_jti, Type::UUID),
                    (&user_id, Type::INT8),
                    (&expires, Type::TIMESTAMPTZ),
                ],
            )
            .await
            .map_err(|_| "fixture_revocation")?;
        let valid: bool = transaction
            .query_typed_one(sql, parameters)
            .await
            .map_err(|_| "fixture_revoked_query")?
            .try_get(0)
            .map_err(|_| "fixture_revoked_decode")?;
        if valid {
            return Err("fixture_revoked_session_accepted");
        }
        Ok(())
    }
    .await;
    transaction
        .rollback()
        .await
        .map_err(|_| "fixture_transaction_rollback")?;
    result
}

#[durable_object]
pub struct DeploymentCheck {
    state: State,
    env: Env,
}

impl DurableObject for DeploymentCheck {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if !authorized(&req, &self.env) {
            return reply("request", Err("unauthorized"), 401);
        }
        if req.method() != Method::Post {
            return reply("request", Err("method"), 405);
        }
        let (check, result) = match req.path().as_str() {
            "/check/postgres" => ("postgres", postgres_check(&self.env, &self.state).await),
            "/check/session-revocation" => (
                "session_revocation_fixture",
                session_revocation_check(&self.env, &self.state).await,
            ),
            "/check/argon2/hash" => ("argon2_hash", hash_check()),
            "/check/argon2/verify" => ("argon2_verify", verify_check()),
            _ => return reply("request", Err("path"), 404),
        };
        let status = if result.is_ok() { 200 } else { 503 };
        reply(check, result, status)
    }
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if !authorized(&req, &env) {
        return reply("request", Err("unauthorized"), 401);
    }
    if req.method() != Method::Post {
        return reply("request", Err("method"), 405);
    }
    if !matches!(
        req.path().as_str(),
        "/check/postgres"
            | "/check/session-revocation"
            | "/check/argon2/hash"
            | "/check/argon2/verify"
    ) {
        return reply("request", Err("path"), 404);
    }
    env.durable_object("CHECKS")?
        .get_by_name("deployment-check")?
        .fetch_with_request(req)
        .await
}
