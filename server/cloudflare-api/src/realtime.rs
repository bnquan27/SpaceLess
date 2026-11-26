use crate::{auth, db, events};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use worker::*;

type ApiResult<T> = std::result::Result<T, auth::ApiError>;

#[derive(Clone, Copy)]
pub(crate) enum RevokedSocket {
    Token(Uuid),
    Session(Uuid),
}

#[derive(Serialize, Deserialize)]
struct SocketAuth {
    sid: Uuid,
    jti: Uuid,
    exp: i64,
    auth_version: String,
    closing: bool,
}

fn user_name(user_id: i64) -> String {
    format!("user-{user_id}")
}

fn jwt_secret(env: &Env) -> ApiResult<String> {
    let secret = env
        .secret("APP_JWT_SECRET")
        .map_err(|_| auth::ApiError::unavailable())?
        .to_string();
    if secret.len() < 32 {
        return Err(auth::ApiError::unavailable());
    }
    Ok(secret)
}

fn upgrade_requested(req: &Request) -> bool {
    req.method() == Method::Get
        && req.path() == "/api/v1/ws"
        && req
            .headers()
            .get("Upgrade")
            .ok()
            .flatten()
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

pub(crate) async fn upgrade(req: Request, env: &Env) -> ApiResult<Response> {
    if !upgrade_requested(&req) {
        return Err(auth::ApiError::unauthorized());
    }
    let secret = jwt_secret(env)?;
    let actor = auth::actor(&req, secret.as_bytes())?;
    env.durable_object("USER_SOCKETS")
        .map_err(|_| auth::ApiError::unavailable())?
        .get_by_name(&user_name(actor.user_id))
        .map_err(|_| auth::ApiError::unavailable())?
        .fetch_with_request(req)
        .await
        .map_err(|_| auth::ApiError::unavailable())
}

pub(crate) async fn disconnect(env: &Env, user_id: i64, revoked: RevokedSocket) -> Result<()> {
    let unavailable = || Error::RustError("Socket revocation unavailable".into());
    if user_id <= 0 {
        return Err(unavailable());
    }
    let gate = env
        .secret("STAGING_GATE_TOKEN")
        .map_err(|_| unavailable())?
        .to_string();
    if gate.len() < 32 {
        return Err(unavailable());
    }
    let (kind, id) = match revoked {
        RevokedSocket::Token(id) => ("token", id),
        RevokedSocket::Session(id) => ("session", id),
    };
    let headers = Headers::new();
    headers
        .set("X-SpaceLess-Staging-Token", &gate)
        .map_err(|_| unavailable())?;
    let req = Request::new_with_init(
        &format!("https://spaceless-sockets.internal/_internal/users/{user_id}/{kind}/{id}"),
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers),
    )
    .map_err(|_| unavailable())?;
    let response = env
        .durable_object("USER_SOCKETS")
        .map_err(|_| unavailable())?
        .get_by_name(&user_name(user_id))
        .map_err(|_| unavailable())?
        .fetch_with_request(req)
        .await
        .map_err(|_| unavailable())?;
    if response.status_code() != 204 {
        return Err(unavailable());
    }
    Ok(())
}

pub(crate) async fn deliver(env: &Env, user_id: i64, notice: &events::Event) -> Result<()> {
    let unavailable = || Error::RustError("Socket delivery unavailable".into());
    if user_id <= 0 {
        return Err(unavailable());
    }
    let gate = env
        .secret("STAGING_GATE_TOKEN")
        .map_err(|_| unavailable())?
        .to_string();
    if gate.len() < 32 {
        return Err(unavailable());
    }
    let body = serde_json::to_string(notice).map_err(|_| unavailable())?;
    let headers = Headers::new();
    headers
        .set("X-SpaceLess-Staging-Token", &gate)
        .map_err(|_| unavailable())?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| unavailable())?;
    let req = Request::new_with_init(
        &format!("https://spaceless-sockets.internal/_internal/users/{user_id}/message"),
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(worker::wasm_bindgen::JsValue::from_str(&body))),
    )
    .map_err(|_| unavailable())?;
    let response = env
        .durable_object("USER_SOCKETS")
        .map_err(|_| unavailable())?
        .get_by_name(&user_name(user_id))
        .map_err(|_| unavailable())?
        .fetch_with_request(req)
        .await
        .map_err(|_| unavailable())?;
    if response.status_code() != 204 {
        return Err(unavailable());
    }
    Ok(())
}

#[durable_object]
pub struct UserSockets {
    state: State,
    env: Env,
}

impl UserSockets {
    fn belongs_to(&self, user_id: i64) -> bool {
        self.env
            .durable_object("USER_SOCKETS")
            .and_then(|namespace| {
                namespace
                    .id_from_name(&user_name(user_id))
                    .map(|id| id.to_string())
            })
            .is_ok_and(|id| id == self.state.id().to_string())
    }

    async fn accept(&self, req: Request) -> ApiResult<Response> {
        let secret = jwt_secret(&self.env)?;
        let actor = auth::actor(&req, secret.as_bytes())?;
        if !self.belongs_to(actor.user_id) {
            return Err(auth::ApiError::unauthorized());
        }
        let mut client = db::connect(&self.env, &self.state)
            .await
            .map_err(|_| auth::ApiError::unavailable())?;
        let tx = client.build_transaction().read_only(false).start().await?;
        auth::lock_session(&tx, &actor, false).await?;
        let pair = WebSocketPair::new().map_err(|_| auth::ApiError::internal())?;
        let response = crate::no_store(
            Response::from_websocket(pair.client).map_err(|_| auth::ApiError::internal())?,
        )
        .map_err(|_| auth::ApiError::internal())?;
        // Refresh and logout cannot commit until this accepted socket is visible to revocation.
        self.state.accept_web_socket(&pair.server);
        if pair
            .server
            .serialize_attachment(SocketAuth {
                sid: actor.sid,
                jti: actor.jti,
                exp: actor.exp,
                auth_version: actor.auth_version.to_string(),
                closing: false,
            })
            .is_err()
            || self.schedule_expiry().await.is_err()
        {
            close_socket(&pair.server, 1011);
            return Err(auth::ApiError::unavailable());
        }
        if actor.exp <= Utc::now().timestamp() {
            close_socket(&pair.server, 1008);
            return Err(auth::ApiError::unauthorized());
        }
        if let Err(error) = tx.commit().await {
            close_socket(&pair.server, 1011);
            return Err(error.into());
        }
        Ok(response)
    }

    fn internal_revocation(&self, req: &Request) -> Option<RevokedSocket> {
        if req.method() != Method::Post {
            return None;
        }
        let path = req.path();
        let parts: Vec<_> = path.split('/').collect();
        let ["", "_internal", "users", user, kind, id] = parts.as_slice() else {
            return None;
        };
        let user_id = user.parse::<i64>().ok()?;
        if user_id <= 0 || user_id.to_string() != *user || !self.belongs_to(user_id) {
            return None;
        }
        let parsed = Uuid::parse_str(id).ok()?;
        if parsed.to_string() != *id {
            return None;
        }
        match *kind {
            "token" => Some(RevokedSocket::Token(parsed)),
            "session" => Some(RevokedSocket::Session(parsed)),
            _ => None,
        }
    }

    fn internal_message_user(&self, req: &Request) -> Option<i64> {
        if req.method() != Method::Post {
            return None;
        }
        let path = req.path();
        let parts: Vec<_> = path.split('/').collect();
        let ["", "_internal", "users", user, "message"] = parts.as_slice() else {
            return None;
        };
        let user_id = user.parse::<i64>().ok()?;
        (user_id > 0 && user_id.to_string() == *user && self.belongs_to(user_id)).then_some(user_id)
    }

    async fn send_message(&self, mut req: Request, user_id: i64) -> ApiResult<Response> {
        let mut sockets = Vec::new();
        for ws in self.state.get_websockets() {
            let Ok(Some(auth)) = ws.deserialize_attachment::<SocketAuth>() else {
                close_socket(&ws, 1008);
                continue;
            };
            if auth.closing {
                continue;
            }
            let version = auth
                .auth_version
                .parse::<i64>()
                .ok()
                .filter(|value| *value >= 0 && value.to_string() == auth.auth_version);
            let Some(auth_version) = version.filter(|_| auth.exp > Utc::now().timestamp()) else {
                close_socket(&ws, 1008);
                continue;
            };
            sockets.push((
                ws,
                auth::Actor {
                    user_id,
                    sid: auth.sid,
                    jti: auth.jti,
                    exp: auth.exp,
                    auth_version,
                },
            ));
        }
        if sockets.is_empty() {
            return Response::empty()
                .map(|response| response.with_status(204))
                .map_err(|_| auth::ApiError::internal());
        }
        let mut body = Vec::new();
        let mut stream = req.stream().map_err(|_| auth::ApiError::internal())?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| auth::ApiError::internal())?;
            if chunk.len() > 2 * 1024 * 1024 - body.len() {
                return Err(auth::ApiError::new(
                    400,
                    "INVALID_BODY",
                    "Yêu cầu không hợp lệ.",
                ));
            }
            body.extend(chunk);
        }
        let notice: events::Event = serde_json::from_slice(&body)
            .map_err(|_| auth::ApiError::new(400, "INVALID_BODY", "Yêu cầu không hợp lệ."))?;
        let mut client = db::connect(&self.env, &self.state)
            .await
            .map_err(|_| auth::ApiError::unavailable())?;
        for (ws, actor) in sockets {
            let tx = client.build_transaction().read_only(false).start().await?;
            if let Err(error) = auth::lock_session(&tx, &actor, false).await {
                if error
                    .response()
                    .map(|response| response.status_code() == 401)
                    .unwrap_or(false)
                {
                    close_socket(&ws, 1008);
                    tx.rollback().await?;
                    continue;
                }
                return Err(auth::ApiError::unavailable());
            }
            let Some(payload) = notice.delivery(&tx, &actor).await? else {
                tx.rollback().await?;
                continue;
            };
            let payload =
                serde_json::to_string(&payload).map_err(|_| auth::ApiError::internal())?;
            let active = ws
                .deserialize_attachment::<SocketAuth>()
                .ok()
                .flatten()
                .is_some_and(|attachment| !attachment.closing);
            if active && actor.exp > Utc::now().timestamp() {
                if ws.send_with_str(&payload).is_err() {
                    close_socket(&ws, 1011);
                }
            } else if actor.exp <= Utc::now().timestamp() {
                close_socket(&ws, 1008);
            }
            tx.commit().await?;
        }
        Response::empty()
            .map(|response| response.with_status(204))
            .map_err(|_| auth::ApiError::internal())
    }

    async fn schedule_expiry(&self) -> Result<()> {
        let storage = self.state.storage();
        let current = storage.get_alarm().await?;
        let now = Utc::now().timestamp();
        let mut earliest = None;
        for ws in self.state.get_websockets() {
            match ws.deserialize_attachment::<SocketAuth>() {
                Ok(Some(auth)) if !auth.closing && auth.exp > now => {
                    earliest = Some(earliest.map_or(auth.exp, |exp: i64| exp.min(auth.exp)));
                }
                Ok(Some(auth)) if auth.closing => {}
                _ => close_socket(&ws, 1008),
            }
        }
        if let Some(exp) = earliest {
            let expires = DateTime::from_timestamp(exp, 0)
                .ok_or_else(|| Error::RustError("Invalid socket expiry".into()))?;
            if current.is_none_or(|scheduled| scheduled > expires.timestamp_millis()) {
                storage.set_alarm(expires).await?;
            }
        }
        Ok(())
    }
}

fn close_socket(ws: &WebSocket, code: u16) {
    if let Ok(Some(mut auth)) = ws.deserialize_attachment::<SocketAuth>() {
        auth.closing = true;
        let _ = ws.serialize_attachment(auth);
    }
    let _ = ws.close(Some(code), Some("Connection closed"));
}

impl DurableObject for UserSockets {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if !crate::staging_authorized(&req, &self.env) {
            return crate::no_store(auth::ApiError::unauthorized().response()?);
        }
        if upgrade_requested(&req) {
            return match self.accept(req).await {
                Ok(response) => Ok(response),
                Err(error) => crate::no_store(error.response()?),
            };
        }
        if let Some(user_id) = self.internal_message_user(&req) {
            return match self.send_message(req, user_id).await {
                Ok(response) => crate::no_store(response),
                Err(error) => crate::no_store(error.response()?),
            };
        }
        if let Some(revoked) = self.internal_revocation(&req) {
            for ws in self.state.get_websockets() {
                let matches = match ws.deserialize_attachment::<SocketAuth>() {
                    Ok(Some(auth)) => match revoked {
                        RevokedSocket::Token(id) => auth.jti == id,
                        RevokedSocket::Session(id) => auth.sid == id,
                    },
                    _ => true,
                };
                if matches {
                    close_socket(&ws, 1008);
                }
            }
            return crate::no_store(Response::empty()?.with_status(204));
        }
        crate::no_store(Response::error("Không tìm thấy đường dẫn.", 404)?)
    }

    async fn alarm(&self) -> Result<Response> {
        self.schedule_expiry().await?;
        Response::empty()
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        _message: WebSocketIncomingMessage,
    ) -> Result<()> {
        close_socket(&ws, 1008);
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        if code != 1006 {
            let _ = ws.close::<&str>(None, None);
        }
        Ok(())
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        close_socket(&ws, 1011);
        Ok(())
    }
}
