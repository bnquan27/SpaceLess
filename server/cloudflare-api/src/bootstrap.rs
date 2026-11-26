use serde_json::Value;
use tokio_postgres::{Client, IsolationLevel, types::Type};
use worker::{Request, Response};

use crate::auth::{self, ApiError};

pub(crate) async fn handle(
    req: Request,
    client: &mut Client,
    secret: &[u8],
) -> Result<Response, ApiError> {
    let actor = auth::actor(&req, secret)?;
    let tx = client
        .build_transaction()
        .read_only(true)
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;
    auth::validate_session(&tx, &actor).await?;
    let profile = auth::profile(&tx, actor.user_id).await?;
    let row = tx
        .query_typed_one(
            include_str!("../../src/sql/bootstrap.sql"),
            &[(&actor.user_id, Type::INT8)],
        )
        .await?;
    let mut snapshot: Value = row.try_get(0)?;
    snapshot["profile"] = profile;
    tx.commit().await?;
    Response::from_json(&snapshot).map_err(|_| ApiError::internal())
}
