use std::time::Duration;

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::Utc;
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use worker::{
    AbortController, Delay, Env, Fetch, Headers, Method, Request, RequestInit, RequestRedirect, Url,
};

use crate::auth::ApiError;

const JWT_HEADER: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
const MAX_WEBHOOK_BYTES: usize = 512 * 1024;
const MAX_PROVIDER_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub(super) const JOIN_SECONDS: i64 = 300;

type Result<T> = std::result::Result<T, ApiError>;

pub(super) struct Provider {
    pub(super) url: String,
    base: String,
    key: String,
    secret: String,
}

impl Provider {
    pub(super) fn configured(env: &Env) -> Result<Option<Self>> {
        if !env
            .var("LIVEKIT_CLOUD_REVOCATION")
            .or_else(|_| env.secret("LIVEKIT_CLOUD_REVOCATION"))
            .is_ok_and(|enabled| enabled.to_string() == "true")
        {
            return Ok(None);
        }
        let url = env
            .var("LIVEKIT_URL")
            .or_else(|_| env.secret("LIVEKIT_URL"))
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        let key = env
            .secret("LIVEKIT_API_KEY")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        let secret = env
            .secret("LIVEKIT_API_SECRET")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        Self::new(&url, key, secret).map(Some)
    }

    fn new(url: &str, key: String, secret: String) -> Result<Self> {
        let parsed = Url::parse(url).map_err(|_| ApiError::unavailable())?;
        let host = parsed.host_str().ok_or_else(ApiError::unavailable)?;
        let scheme = match parsed.scheme() {
            "wss" => "https",
            "ws" if matches!(host, "localhost" | "127.0.0.1" | "[::1]") => "http",
            _ => return Err(ApiError::unavailable()),
        };
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || key.is_empty()
            || secret.is_empty()
        {
            return Err(ApiError::unavailable());
        }
        let url = parsed.as_str().trim_end_matches('/').to_owned();
        let base = format!("{scheme}{}", &url[parsed.scheme().len()..]);
        Ok(Self {
            url,
            base,
            key,
            secret,
        })
    }

    pub(super) fn issue(&self, room: i64, id: i64, issued_at: i64) -> Result<String> {
        self.token(
            &format!("voice-{id}"),
            json!({"roomJoin": true, "room": room_name(room), "canPublish": true, "canSubscribe": true,
                "canPublishData": false, "canUpdateOwnMetadata": false,
                "canPublishSources": ["microphone", "camera", "screen_share", "screen_share_audio"]}),
            json!({"voice_session_id": id.to_string()}).to_string(),
            issued_at,
            JOIN_SECONDS,
        )
    }

    fn token(
        &self,
        subject: &str,
        video: Value,
        metadata: String,
        issued_at: i64,
        ttl: i64,
    ) -> Result<String> {
        let claims = json!({"iss": self.key, "sub": subject, "iat": issued_at, "nbf": issued_at,
            "exp": issued_at + ttl, "video": video, "metadata": metadata});
        let signed = format!(
            "{JWT_HEADER}.{}",
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(|_| ApiError::internal())?;
        mac.update(signed.as_bytes());
        Ok(format!(
            "{signed}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        ))
    }

    async fn call(&self, method: &str, room: Option<i64>, body: Value) -> Result<Value> {
        let access = room.map_or_else(
            || json!({"roomList": true}),
            |room| json!({"roomAdmin": true, "room": room_name(room)}),
        );
        let bearer = self.token("", access, String::new(), Utc::now().timestamp(), 60)?;
        let headers = Headers::new();
        headers
            .set("Authorization", &format!("Bearer {bearer}"))
            .map_err(|_| ApiError::internal())?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| ApiError::internal())?;
        let request = Request::new_with_init(
            &format!("{}/twirp/livekit.RoomService/{method}", self.base),
            RequestInit::new()
                .with_method(Method::Post)
                .with_headers(headers)
                .with_body(Some(body.to_string().into()))
                .with_redirect(RequestRedirect::Manual),
        )
        .map_err(|_| ApiError::internal())?;
        let controller = AbortController::default();
        let signal = controller.signal();
        let execute = async {
            let mut response = Fetch::Request(request)
                .send_with_signal(&signal)
                .await
                .map_err(|_| ApiError::unavailable())?;
            if !(200..300).contains(&response.status_code()) {
                return Err(ApiError::unavailable());
            }
            let mut stream = response.stream().map_err(|_| ApiError::unavailable())?;
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| ApiError::unavailable())?;
                if chunk.len() > MAX_PROVIDER_RESPONSE_BYTES - bytes.len() {
                    return Err(ApiError::unavailable());
                }
                bytes.extend(chunk);
            }
            serde_json::from_slice(&bytes).map_err(|_| ApiError::unavailable())
        };
        match select(Box::pin(execute), Delay::from(Duration::from_secs(10))).await {
            Either::Left((result, _)) => result,
            Either::Right(_) => {
                controller.abort();
                Err(ApiError::unavailable())
            }
        }
    }

    pub(super) async fn remove(&self, room: i64, identity: &str) -> Result<()> {
        let cutoff = Utc::now().timestamp() + 1;
        self.call(
            "RemoveParticipant",
            Some(room),
            json!({"room": room_name(room),
            "identity": identity, "revoke_token_ts": cutoff.to_string()}),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn rooms(&self) -> Result<Vec<LiveKitRoom>> {
        let response = self.call("ListRooms", None, json!({})).await?;
        serde_json::from_value(response.get("rooms").cloned().unwrap_or(json!([])))
            .map_err(|_| ApiError::unavailable())
    }

    pub(super) async fn participants(&self, room: i64) -> Result<Vec<Participant>> {
        let response = self
            .call(
                "ListParticipants",
                Some(room),
                json!({"room": room_name(room)}),
            )
            .await?;
        serde_json::from_value(response.get("participants").cloned().unwrap_or(json!([])))
            .map_err(|_| ApiError::unavailable())
    }

    pub(super) async fn webhook(&self, mut request: Request) -> Result<LiveKitEvent> {
        let authorization = request
            .headers()
            .get("Authorization")
            .ok()
            .flatten()
            .ok_or_else(ApiError::unauthorized)?;
        let mut stream = request.stream().map_err(|_| invalid_webhook())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| invalid_webhook())?;
            if chunk.len() > MAX_WEBHOOK_BYTES - bytes.len() {
                return Err(ApiError::new(413, "BODY_TOO_LARGE", "Yêu cầu quá lớn."));
            }
            bytes.extend(chunk);
        }
        self.verify_webhook(&authorization, &bytes, Utc::now().timestamp())
    }

    fn verify_webhook(&self, header: &str, body: &[u8], now: i64) -> Result<LiveKitEvent> {
        let jwt = header.strip_prefix("Bearer ").unwrap_or(header);
        let denied = ApiError::unauthorized;
        if jwt.len() > 8192 {
            return Err(denied());
        }
        let (signed, signature) = jwt.rsplit_once('.').ok_or_else(denied)?;
        let (header, claims) = signed.split_once('.').ok_or_else(denied)?;
        let signature = URL_SAFE_NO_PAD.decode(signature).map_err(|_| denied())?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret.as_bytes()).map_err(|_| denied())?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&signature).map_err(|_| denied())?;
        let header: JwtHeader =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).map_err(|_| denied())?)
                .map_err(|_| denied())?;
        let claims: WebhookClaims =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).map_err(|_| denied())?)
                .map_err(|_| denied())?;
        if header.alg != "HS256"
            || header.crit.is_some()
            || header.b64 == Some(false)
            || claims.iss != self.key
            || claims.exp <= now
            || claims.nbf.is_some_and(|nbf| nbf > now)
        {
            return Err(denied());
        }
        let expected = STANDARD.decode(claims.sha256).map_err(|_| denied())?;
        if !bool::from(expected.as_slice().ct_eq(Sha256::digest(body).as_slice())) {
            return Err(denied());
        }
        let event: LiveKitEvent = serde_json::from_slice(body).map_err(|_| invalid_webhook())?;
        if event.id.is_empty() || event.id.len() > 256 {
            return Err(ApiError::new(
                400,
                "INVALID_WEBHOOK_ID",
                "Mã sự kiện không hợp lệ.",
            ));
        }
        Ok(event)
    }
}

fn invalid_webhook() -> ApiError {
    ApiError::new(400, "INVALID_WEBHOOK", "Sự kiện LiveKit không hợp lệ.")
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    crit: Option<Value>,
    b64: Option<bool>,
}

#[derive(Deserialize)]
struct WebhookClaims {
    iss: String,
    exp: i64,
    nbf: Option<i64>,
    sha256: String,
}

#[derive(Deserialize)]
pub(super) struct LiveKitEvent {
    pub(super) id: String,
    pub(super) event: String,
    pub(super) room: Option<LiveKitRoom>,
    pub(super) participant: Option<Participant>,
    pub(super) track: Option<Track>,
}

#[derive(Deserialize)]
pub(super) struct LiveKitRoom {
    pub(super) name: String,
}

#[derive(Deserialize)]
pub(super) struct Participant {
    pub(super) identity: String,
    #[serde(default)]
    pub(super) sid: String,
    #[serde(default)]
    pub(super) tracks: Vec<Track>,
}

#[derive(Deserialize)]
pub(super) struct Track {
    pub(super) sid: String,
    #[serde(default)]
    pub(super) source: Value,
}

pub(super) fn room_name(room: i64) -> String {
    format!("room-{room}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIME: i64 = 1_900_000_000;
    const SECRET: &str = "fixture-key-for-livekit";

    fn provider() -> Provider {
        Provider::new("wss://fixture.example.test", "issuer".into(), SECRET.into()).unwrap()
    }

    fn signed(alg: &str, claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(json!({"alg": alg}).to_string());
        let unsigned = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(unsigned.as_bytes());
        format!(
            "{unsigned}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn join_token_authorizes_one_room_and_supported_media_sources_only() {
        let jwt = provider()
            .issue(9_007_199_254_740_993, 9_007_199_254_740_995, TIME)
            .unwrap();
        let (unsigned, signature) = jwt.rsplit_once('.').unwrap();
        let (_, claims) = unsigned.split_once('.').unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(unsigned.as_bytes());
        mac.verify_slice(&URL_SAFE_NO_PAD.decode(signature).unwrap())
            .unwrap();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap();
        assert_eq!(claims["iss"], "issuer");
        assert_eq!(claims["sub"], "voice-9007199254740995");
        assert_eq!(claims["exp"], TIME + 300);
        assert_eq!(claims["video"]["room"], "room-9007199254740993");
        assert_eq!(claims["video"]["roomJoin"], true);
        assert_eq!(
            claims["video"]["canPublishSources"],
            json!(["microphone", "camera", "screen_share", "screen_share_audio"])
        );
        assert_eq!(claims["video"]["canPublishData"], false);
        assert!(claims["video"].get("roomAdmin").is_none());
    }

    #[test]
    fn webhook_requires_exact_body_signing_key_issuer_and_expiry() {
        let body = br#"{"id":"test-1","event":"participant_joined"}"#;
        let sha256 = STANDARD.encode(Sha256::digest(body));
        let claims = json!({"iss":"issuer","exp":TIME+60,"sha256":sha256});
        let token = signed("HS256", claims.clone());
        let provider = provider();
        assert_eq!(
            provider.verify_webhook(&token, body, TIME).unwrap().id,
            "test-1"
        );
        assert!(
            provider
                .verify_webhook(&format!("Bearer {token}"), body, TIME)
                .is_ok()
        );
        assert!(provider.verify_webhook(&token, b"{}", TIME).is_err());
        assert!(provider.verify_webhook(&token, body, TIME + 60).is_err());
        assert!(
            provider
                .verify_webhook(&signed("none", claims.clone()), body, TIME)
                .is_err()
        );
        let other = Provider {
            secret: "different".into(),
            ..provider
        };
        assert!(other.verify_webhook(&token, body, TIME).is_err());
        let wrong_issuer = signed(
            "HS256",
            json!({"iss":"other","exp":TIME+60,"sha256":sha256}),
        );
        assert!(other.verify_webhook(&wrong_issuer, body, TIME).is_err());
    }
}
