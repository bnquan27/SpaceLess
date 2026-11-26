use tokio_postgres::{GenericClient, types::Type};
use worker::{Env, SecureTransport, Socket, State, postgres_tls::PassthroughTls};

use crate::auth::ApiError;

pub async fn connect(env: &Env, state: &State) -> Result<tokio_postgres::Client, ()> {
    let hyperdrive = env.hyperdrive("DB").map_err(|_| ())?;
    let socket = Socket::builder()
        .secure_transport(SecureTransport::StartTls)
        .connect(hyperdrive.host(), hyperdrive.port())
        .map_err(|_| ())?;
    let config = hyperdrive
        .connection_string()
        .parse::<tokio_postgres::Config>()
        .map_err(|_| ())?;
    let (client, connection) = config
        .connect_raw(socket, PassthroughTls)
        .await
        .map_err(|_| ())?;
    state.wait_until(async move {
        // PostgreSQL driver errors may contain credentials or query values.
        let _ = connection.await;
    });
    Ok(client)
}

pub(crate) async fn snowflake<C: GenericClient + Sync>(client: &C) -> Result<i64, ApiError> {
    // ID 1023 is reserved for the Worker connected to the staging database.
    let row = client.query_typed_opt(
        "WITH clock AS (SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint - 1704067200000 AS ms)
         INSERT INTO snowflake_workers AS w (worker_id,last_ms,sequence)
         SELECT 1023,ms,0 FROM clock WHERE ms BETWEEN 1 AND 2199023255551
         ON CONFLICT (worker_id) DO UPDATE SET last_ms=EXCLUDED.last_ms,
           sequence=CASE WHEN EXCLUDED.last_ms > w.last_ms THEN 0 ELSE w.sequence + 1 END
         WHERE EXCLUDED.last_ms > w.last_ms OR (EXCLUDED.last_ms=w.last_ms AND w.sequence<4095)
         RETURNING last_ms,sequence", &[],
    ).await?.ok_or_else(ApiError::unavailable)?;
    let millis: i64 = row.try_get(0)?;
    let sequence: i32 = row.try_get(1)?;
    Ok((millis << 22) | (1023_i64 << 12) | i64::from(sequence))
}

pub(crate) async fn lock_pair<C: GenericClient + Sync>(
    client: &C,
    first: i64,
    second: i64,
) -> Result<(), ApiError> {
    let (lo, hi) = (first.min(second), first.max(second));
    client
        .query_typed_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[(&format!("social:{lo}:{hi}"), Type::TEXT)],
        )
        .await?;
    Ok(())
}
