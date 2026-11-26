use std::cell::Cell;

use tokio_postgres::{Client, types::Type};
use worker::*;

use crate::{
    db,
    events::{Event, Id},
    files, realtime, voice,
};

pub(crate) async fn wake(env: &Env) -> Result<()> {
    invoke(
        env,
        "staging-message-outbox",
        "https://outbox.internal/drain",
    )
    .await
}

pub(crate) async fn scheduled(env: &Env) -> Result<()> {
    let (drain, maintenance) = futures_util::future::join(
        wake(env),
        invoke(
            env,
            "staging-provider-maintenance",
            "https://outbox.internal/maintenance",
        ),
    )
    .await;
    drain.and(maintenance)
}

async fn invoke(env: &Env, name: &str, url: &str) -> Result<()> {
    let request = Request::new(url, Method::Post)?;
    let response = env
        .durable_object("MESSAGE_OUTBOX")?
        .get_by_name(name)?
        .fetch_with_request(request)
        .await?;
    if response.status_code() != 204 {
        return Err(Error::RustError("Message outbox will retry".into()));
    }
    Ok(())
}

#[durable_object(fetch)]
pub struct MessageOutbox {
    state: State,
    env: Env,
    draining: Cell<bool>,
    wake_requested: Cell<bool>,
}

impl DurableObject for MessageOutbox {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            draining: Cell::new(false),
            wake_requested: Cell::new(false),
        }
    }

    async fn fetch(&self, request: Request) -> Result<Response> {
        let path = request.path();
        if request.method() != Method::Post || !matches!(path.as_str(), "/drain" | "/maintenance") {
            return Response::error("Not found", 404);
        }
        if path == "/maintenance" {
            if self.draining.replace(true) {
                return Ok(Response::empty()?.with_status(204));
            }
            let result = self.maintain().await;
            self.draining.set(false);
            return result;
        }
        self.wake_requested.set(true);
        if self.draining.replace(true) {
            return Ok(Response::empty()?.with_status(204));
        }
        let result = self.drain().await;
        self.draining.set(false);
        if result.is_err() {
            return Response::error("Message outbox will retry", 503);
        }
        Ok(Response::empty()?.with_status(204))
    }
}

impl MessageOutbox {
    async fn maintain(&self) -> Result<Response> {
        let Ok(mut client) = db::connect(&self.env, &self.state).await else {
            return Response::error("Maintenance database unavailable", 503);
        };
        if voice::run_worker(&mut client, &self.env).await.is_err() {
            worker::console_warn!("LiveKit reconciliation will retry.");
        }
        if files::cleanup(&self.env, &mut client).await.is_err() {
            worker::console_warn!("R2 cleanup will retry.");
        }
        drop(client);
        wake(&self.env).await?;
        Ok(Response::empty()?.with_status(204))
    }

    async fn drain(&self) -> std::result::Result<(), ()> {
        let client = db::connect(&self.env, &self.state).await.map_err(|_| {
            worker::console_warn!("Outbox could not connect to PostgreSQL.");
        })?;
        for _ in 0..4 {
            self.wake_requested.set(false);
            let batch = pending(&client).await.map_err(|_| {
                worker::console_warn!("Outbox could not read pending events.");
            })?;
            let full = batch.len() == 32;
            for event in batch {
                let recipients = event.recipients(&client).await.map_err(|_| {
                    worker::console_warn!("Outbox could not resolve recipients.");
                })?;
                for user_id in recipients {
                    realtime::deliver(&self.env, user_id, &event)
                        .await
                        .map_err(|_| {
                            worker::console_warn!("Outbox delivery to a WebSocket object failed.");
                        })?;
                }
                client.query_typed(
                    "UPDATE outbox_events SET dispatched_at = now() WHERE id = $1 AND dispatched_at IS NULL",
                    &[(&event.id.0, Type::INT8)],
                ).await.map_err(|_| {
                    worker::console_warn!("Outbox could not acknowledge delivery.");
                })?;
            }
            if !full && !self.wake_requested.get() {
                break;
            }
        }
        Ok(())
    }
}

async fn pending(client: &Client) -> std::result::Result<Vec<Event>, ()> {
    let rows = client.query_typed(
        "SELECT id, stream, revision, kind, scope, COALESCE(data->'payload',data-'cloudflare_staging'), occurred_at
         FROM outbox_events WHERE dispatched_at IS NULL
           AND data @> '{\"cloudflare_staging\":true}'::jsonb
         ORDER BY id LIMIT 32",
        &[],
    ).await.map_err(|_| ())?;
    rows.into_iter()
        .map(|row| {
            Ok(Event {
                id: Id(row.try_get(0).map_err(|_| ())?),
                stream: row.try_get(1).map_err(|_| ())?,
                revision: Id(row.try_get(2).map_err(|_| ())?),
                kind: row.try_get(3).map_err(|_| ())?,
                scope: serde_json::from_value(row.try_get(4).map_err(|_| ())?).map_err(|_| ())?,
                data: row.try_get(5).map_err(|_| ())?,
                occurred_at: row.try_get(6).map_err(|_| ())?,
            })
        })
        .collect()
}
