mod auth;
mod bootstrap;
mod channels;
mod db;
mod events;
mod files;
mod google;
mod messages;
mod outbox;
mod realtime;
mod social;
mod voice;

use sha2::{Digest, Sha256};
use std::net::IpAddr;
use subtle::ConstantTimeEq;
use worker::*;

const STAGING_HEADER: &str = "X-SpaceLess-Staging-Token";
const AUTH_SHARDS: u8 = 4;

fn staging_authorized(req: &Request, env: &Env) -> bool {
    let Ok(secret) = env.secret("STAGING_GATE_TOKEN") else {
        return false;
    };
    let secret = secret.to_string();
    if secret.len() < 32 {
        return false;
    }
    let Ok(Some(token)) = req.headers().get(STAGING_HEADER) else {
        return false;
    };
    if token.len() > 512 {
        return false;
    }
    bool::from(Sha256::digest(secret.as_bytes()).ct_eq(&Sha256::digest(token.as_bytes())))
}

fn auth_shard(req: &Request) -> String {
    let ip = req.cf().and_then(|_| {
        req.headers()
            .get("CF-Connecting-IP")
            .ok()
            .flatten()?
            .parse::<IpAddr>()
            .ok()
    });
    let shard = ip.map_or(0, |ip| {
        Sha256::digest(ip.to_string().as_bytes())[0] % AUTH_SHARDS
    });
    format!("staging-auth-{shard}")
}

fn error_response(status: u16, code: &str, message: &str) -> Result<Response> {
    Response::from_json(&serde_json::json!({
        "error": { "code": code, "message": message },
        "request_id": uuid::Uuid::new_v4(),
    }))
    .and_then(|response| no_store(response.with_status(status)))
}

fn unavailable() -> Result<Response> {
    auth::ApiError::unavailable().response().and_then(no_store)
}

fn no_store(mut response: Response) -> Result<Response> {
    response.headers_mut().set("Cache-Control", "no-store")?;
    response
        .headers_mut()
        .set("X-Content-Type-Options", "nosniff")?;
    Ok(response)
}

#[durable_object(fetch)]
pub struct AuthApi {
    state: State,
    env: Env,
}

impl DurableObject for AuthApi {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let response = self.handle(req).await.or_else(|_| unavailable())?;
        no_store(response)
    }
}

enum ApiRoute {
    Auth(auth::AuthRoute),
    Bootstrap,
    Google,
    Channels(channels::ChannelRoute),
    Social(social::Route),
    Files(files::Route),
    Voice(voice::VoiceRoute),
    Messages(messages::MessagesRoute),
}

fn route(req: &Request) -> std::result::Result<Option<ApiRoute>, auth::ApiError> {
    if let Some(route) = auth::AuthRoute::route(&req.method(), &req.path()) {
        return Ok(Some(ApiRoute::Auth(route)));
    }
    if req.method() == Method::Get && req.path() == "/api/v1/bootstrap" {
        return Ok(Some(ApiRoute::Bootstrap));
    }
    if req.method() == Method::Post && req.path() == "/api/v1/auth/google/exchange" {
        return Ok(Some(ApiRoute::Google));
    }
    if let Some(route) = channels::route(req)? {
        return Ok(Some(ApiRoute::Channels(route)));
    }
    if let Some(route) = social::route(req)? {
        return Ok(Some(ApiRoute::Social(route)));
    }
    if let Some(route) = files::route(req)? {
        return Ok(Some(ApiRoute::Files(route)));
    }
    if let Some(route) = voice::route(req)? {
        return Ok(Some(ApiRoute::Voice(route)));
    }
    messages::route(req).map(|route| route.map(ApiRoute::Messages))
}

impl AuthApi {
    async fn handle(&self, req: Request) -> Result<Response> {
        let route = match route(&req) {
            Ok(Some(route)) => route,
            Ok(None) => return error_response(404, "NOT_FOUND", "Không tìm thấy đường dẫn."),
            Err(error) => return error.response(),
        };
        if matches!(route, ApiRoute::Voice(voice::VoiceRoute::Webhook)) {
            return self.wake_if_mutated(
                voice::handle_webhook(req, &self.env, &self.state).await,
                true,
            );
        }
        if !staging_authorized(&req, &self.env) {
            return error_response(403, "FORBIDDEN", "Yêu cầu không được phép.");
        }
        let Ok(secret) = self.env.secret("APP_JWT_SECRET") else {
            return unavailable();
        };
        let secret = secret.to_string();
        if secret.len() < 32 {
            return unavailable();
        }
        let Ok(mut client) = db::connect(&self.env, &self.state).await else {
            return unavailable();
        };
        let mutated = req.method() != Method::Get;
        match route {
            ApiRoute::Files(route) => {
                files::handle(route, req, &mut client, secret.as_bytes(), &self.env)
                    .await
                    .or_else(|error| error.response())
            }
            ApiRoute::Voice(route) => {
                let result =
                    voice::handle(route, req, &mut client, secret.as_bytes(), &self.env).await;
                self.wake_if_mutated(result, true)
            }
            ApiRoute::Google => google::handle(req, &mut client, secret.as_bytes(), &self.env)
                .await
                .or_else(|error| error.response()),
            ApiRoute::Channels(route) => {
                let result =
                    channels::handle(route, req, &mut client, secret.as_bytes(), &self.env).await;
                self.wake_if_mutated(result, mutated)
            }
            ApiRoute::Social(route) => {
                let result = social::handle(route, req, &mut client, secret.as_bytes()).await;
                self.wake_if_mutated(result, mutated)
            }
            ApiRoute::Auth(route) => {
                match auth::handle(route, req, &mut client, secret.as_bytes(), &self.env).await {
                    Ok(response) if matches!(route, auth::AuthRoute::Logout) => {
                        if voice::flush_after_commit(&mut client, &self.env)
                            .await
                            .is_err()
                        {
                            worker::console_warn!("Committed media revocation will retry.");
                        }
                        self.wake_if_mutated(Ok(response), true)
                    }
                    Ok(response) => Ok(response),
                    Err(error) => error.response(),
                }
            }
            ApiRoute::Bootstrap => {
                match bootstrap::handle(req, &mut client, secret.as_bytes()).await {
                    Ok(response) => Ok(response),
                    Err(error) => error.response(),
                }
            }
            ApiRoute::Messages(route) => {
                match messages::handle(route, req, &mut client, secret.as_bytes(), &self.env).await
                {
                    Ok(outcome) => self.wake_if_mutated(Ok(outcome.response), outcome.wake),
                    Err(error) => error.response(),
                }
            }
        }
    }

    fn wake_if_mutated(
        &self,
        result: std::result::Result<Response, auth::ApiError>,
        mutated: bool,
    ) -> Result<Response> {
        let response = match result {
            Ok(response) => response,
            Err(error) => return error.response(),
        };
        if mutated {
            let env = self.env.clone();
            self.state.wait_until(async move {
                if outbox::wake(&env).await.is_err() {
                    worker::console_warn!("Committed outbox events are pending delivery.");
                }
            });
        }
        Ok(response)
    }
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    dispatch(req, env).await.or_else(|_| unavailable())
}

async fn dispatch(req: Request, env: Env) -> Result<Response> {
    let websocket = req.method() == Method::Get && req.path() == "/api/v1/ws";
    let (shard, webhook) = if websocket {
        (String::new(), false)
    } else {
        match route(&req) {
            Ok(Some(ApiRoute::Google)) => ("staging-google".to_string(), false),
            Ok(Some(ApiRoute::Voice(voice::VoiceRoute::Webhook))) => {
                ("staging-livekit-webhook".to_string(), true)
            }
            Ok(Some(_)) => (auth_shard(&req), false),
            Ok(None) => return error_response(404, "NOT_FOUND", "Không tìm thấy đường dẫn."),
            Err(error) => return error.response().and_then(no_store),
        }
    };
    if webhook {
        if !req
            .headers()
            .get("Authorization")?
            .is_some_and(|value| (32..=4096).contains(&value.len()))
        {
            return auth::ApiError::unauthorized().response().and_then(no_store);
        }
    } else if !staging_authorized(&req, &env) {
        return error_response(403, "FORBIDDEN", "Yêu cầu không được phép.");
    }
    if websocket {
        return match realtime::upgrade(req, &env).await {
            Ok(response) => Ok(response),
            Err(error) => error.response().and_then(no_store),
        };
    }
    env.durable_object("AUTH_API")?
        .get_by_name(&shard)?
        .fetch_with_request(req)
        .await
}

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    ctx.wait_until(async move {
        if outbox::scheduled(&env).await.is_err() {
            worker::console_warn!("Scheduled message outbox will retry.");
        }
    });
}
