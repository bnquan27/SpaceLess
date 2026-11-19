use axum::{
    extract::State,
    http::{header, Method, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri_plugin_opener::OpenerExt;
use tokio::{net::TcpListener, sync::oneshot};
use tokio_util::sync::CancellationToken;
use url::Url;

const CANCELLED: &str = "Đã hủy đăng nhập. Bạn có thể thử lại.";
const NETWORK_ERROR: &str = "Không thể kết nối máy chủ. Kiểm tra kết nối rồi thử lại.";
const INVALID_RESPONSE: &str = "Máy chủ trả về dữ liệu không hợp lệ. Vui lòng thử lại.";
const CALLBACK_HTML: &str = "<!doctype html><html lang=vi><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>SpaceLessApp</title><p>Đã nhận phản hồi đăng nhập. Bạn có thể đóng cửa sổ này và quay lại SpaceLessApp.</p></html>";

#[derive(Default)]
pub struct AuthState(Mutex<AuthInner>);

#[derive(Default)]
struct AuthInner {
    generation: u64,
    pending: Option<(u64, CancellationToken)>,
    session: Option<Session>,
}

struct Session {
    token: String,
    api: Url,
}

struct Attempt<'a> {
    state: &'a AuthState,
    generation: u64,
    cancel: CancellationToken,
}

impl AuthState {
    fn begin(&self) -> Result<Attempt<'_>, String> {
        let mut inner = self.0.lock().unwrap();
        if inner.pending.is_some() || inner.session.is_some() {
            return Err("Một phiên đăng nhập đang hoạt động. Vui lòng chờ hoặc hủy.".into());
        }
        inner.generation += 1;
        let generation = inner.generation;
        let cancel = CancellationToken::new();
        inner.pending = Some((generation, cancel.clone()));
        Ok(Attempt {
            state: self,
            generation,
            cancel,
        })
    }
}

impl Attempt<'_> {
    fn commit(&self, session: Session) -> Result<(), String> {
        let mut inner = self.state.0.lock().unwrap();
        if self.cancel.is_cancelled() || inner.generation != self.generation {
            return Err(CANCELLED.into());
        }
        inner.session = Some(session);
        Ok(())
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        let mut inner = self.state.0.lock().unwrap();
        if inner
            .pending
            .as_ref()
            .is_some_and(|(id, _)| *id == self.generation)
        {
            inner.pending = None;
        }
    }
}

#[derive(Serialize)]
pub struct Availability {
    password: bool,
    google: bool,
    message: String,
}

fn api_url(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|_| "Địa chỉ API chưa hợp lệ.".to_string())?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().trim_end_matches('/') != "/api/v1"
    {
        return Err("API cần dùng HTTPS hoặc địa chỉ loopback và đường dẫn /api/v1.".into());
    }
    url.set_path("/api/v1/");
    Ok(url)
}

fn config() -> Result<Url, String> {
    api_url(option_env!("VITE_SPACELESS_API_URL").unwrap_or_default())
}

fn google_client_id() -> Result<&'static str, String> {
    option_env!("VITE_GOOGLE_CLIENT_ID")
        .filter(|value| {
            value.ends_with(".apps.googleusercontent.com") && !value.contains(char::is_whitespace)
        })
        .ok_or_else(|| "Chưa cấu hình Google Client ID cho ứng dụng desktop.".into())
}

#[tauri::command]
pub fn auth_availability() -> Availability {
    let password = config().is_ok();
    let google = password && google_client_id().is_ok();
    Availability {
        password,
        google,
        message: if !password {
            "Chưa cấu hình máy chủ. Đây là bản xem thử, chưa thể đăng nhập.".into()
        } else if !google {
            "Đăng nhập Google chưa được cấu hình trên bản desktop này.".into()
        } else {
            "Google mở trong trình duyệt mặc định. Lần đầu đăng nhập sẽ tạo tài khoản SpaceLessApp."
                .into()
        },
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct Profile {
    username: String,
    full_name: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| NETWORK_ERROR.into())
}

async fn token(response: reqwest::Response) -> Result<TokenResponse, String> {
    match response.status().as_u16() {
        200..=299 => {
            let token: TokenResponse = response.json().await.map_err(|_| INVALID_RESPONSE.to_string())?;
            if token.access_token.is_empty() || token.access_token.len() > 16384 {
                return Err(INVALID_RESPONSE.into());
            }
            Ok(token)
        }
        401 => Err("Không thể xác thực tài khoản. Kiểm tra thông tin hoặc thử lại với Google.".into()),
        403 => Err("Không thể đăng nhập tài khoản này. Tài khoản có thể đã bị khóa hoặc cần dùng phương thức đăng nhập cũ.".into()),
        409 => Err("Email này đã gắn với một tài khoản. Hãy dùng phương thức đăng nhập trước đây.".into()),
        429 => Err("Bạn đã thử quá nhiều lần. Vui lòng đợi rồi thử lại.".into()),
        _ => Err("Máy chủ chưa thể hoàn tất đăng nhập. Vui lòng thử lại sau.".into()),
    }
}

async fn establish(
    api: Url,
    response: reqwest::Response,
    attempt: &Attempt<'_>,
) -> Result<Profile, String> {
    let token = token(response).await?;
    let result = async {
        if attempt.cancel.is_cancelled() {
            return Err(CANCELLED.into());
        }
        let response = client()?
            .get(api.join("users/me").unwrap())
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|_| NETWORK_ERROR.to_string())?;
        if !response.status().is_success() {
            return Err("Không thể xác minh hồ sơ tài khoản. Vui lòng đăng nhập lại.".into());
        }
        let profile: Profile = response
            .json()
            .await
            .map_err(|_| INVALID_RESPONSE.to_string())?;
        if profile.username.trim().is_empty()
            || profile.username.chars().count() > 50
            || profile.full_name.trim().is_empty()
            || profile.full_name.chars().count() > 100
        {
            return Err(INVALID_RESPONSE.into());
        }
        attempt.commit(Session {
            token: token.access_token.clone(),
            api: api.clone(),
        })?;
        Ok(profile)
    }
    .await;
    if result.is_err() {
        if let Ok(client) = client() {
            let _ = client
                .post(api.join("auth/logout").unwrap())
                .timeout(Duration::from_secs(5))
                .bearer_auth(&token.access_token)
                .send()
                .await;
        }
    }
    result
}

#[derive(Serialize)]
struct PasswordLogin {
    username: String,
    password: String,
}

#[tauri::command]
pub async fn auth_password_login(
    state: tauri::State<'_, AuthState>,
    username: String,
    password: String,
) -> Result<Profile, String> {
    let api = config()?;
    let attempt = state.begin()?;
    let response = client()?
        .post(api.join("auth/login").unwrap())
        .json(&PasswordLogin { username, password })
        .send()
        .await
        .map_err(|_| NETWORK_ERROR.to_string())?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Tài khoản hoặc mật khẩu không khớp. Vui lòng kiểm tra lại".into());
    }
    establish(api, response, &attempt).await
}

struct OAuthRequest {
    state: String,
    verifier: String,
    nonce: String,
}

fn random_secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không thể tạo mã bảo vệ đăng nhập. Vui lòng thử lại.".to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

impl OAuthRequest {
    fn new() -> Result<Self, String> {
        Ok(Self {
            state: random_secret()?,
            verifier: random_secret()?,
            nonce: random_secret()?,
        })
    }

    fn authorization_url(&self, client_id: &str, port: u16) -> Url {
        let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &format!("http://127.0.0.1:{port}"))
            .append_pair("response_type", "code")
            .append_pair("scope", "openid email profile")
            .append_pair("state", &self.state)
            .append_pair("nonce", &self.nonce)
            .append_pair(
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes())),
            )
            .append_pair("code_challenge_method", "S256");
        url
    }
}

fn callback_result(uri: &Uri, expected: &str) -> Option<Result<String, String>> {
    if uri.path() != "/" || uri.query().is_some_and(|query| query.len() > 16384) {
        return None;
    }
    let pairs: Vec<_> =
        url::form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes()).collect();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.as_ref())
            .collect::<Vec<_>>()
    };
    let states = values("state");
    if states.len() != 1 || states[0] != expected {
        return None;
    }
    let errors = values("error");
    let codes = values("code");
    if errors.len() == 1 && codes.is_empty() {
        return Some(Err(
            "Google chưa cấp quyền đăng nhập. Bạn có thể thử lại.".into()
        ));
    }
    if !errors.is_empty() || codes.len() != 1 || codes[0].is_empty() {
        return None;
    }
    Some(Ok(codes[0].to_owned()))
}

struct CallbackState {
    expected: String,
    sender: Mutex<Option<oneshot::Sender<Result<String, String>>>>,
}

fn callback_page(status: StatusCode) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; frame-ancestors 'none'",
            ),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(CALLBACK_HTML),
    )
        .into_response()
}

async fn receive_callback(
    State(state): State<Arc<CallbackState>>,
    method: Method,
    uri: Uri,
) -> Response {
    if method != Method::GET {
        return callback_page(StatusCode::METHOD_NOT_ALLOWED);
    }
    if let Some(result) = callback_result(&uri, &state.expected) {
        if let Some(sender) = state.sender.lock().unwrap().take() {
            let _ = sender.send(result);
            return callback_page(StatusCode::OK);
        }
    }
    callback_page(StatusCode::BAD_REQUEST)
}

#[derive(Serialize)]
struct GoogleExchange {
    code: String,
    code_verifier: String,
    nonce: String,
    loopback_port: u16,
}

#[tauri::command]
pub async fn auth_google_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, AuthState>,
) -> Result<Profile, String> {
    let api = config()?;
    let client_id = google_client_id()?;
    let attempt = state.begin()?;
    let oauth = OAuthRequest::new()?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "Không thể mở cổng đăng nhập cục bộ. Vui lòng thử lại.".to_string())?;
    let port = listener
        .local_addr()
        .map_err(|_| NETWORK_ERROR.to_string())?
        .port();
    let (sender, receiver) = oneshot::channel();
    let callback = Arc::new(CallbackState {
        expected: oauth.state.clone(),
        sender: Mutex::new(Some(sender)),
    });
    let router = Router::new()
        .route("/", get(receive_callback))
        .fallback(|| async { callback_page(StatusCode::NOT_FOUND) })
        .with_state(callback);
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    let mut server = tauri::async_runtime::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
    });
    let callback_result = async {
        app.opener()
            .open_url(
                oauth.authorization_url(client_id, port).to_string(),
                None::<&str>,
            )
            .map_err(|_| {
                "Không thể mở trình duyệt mặc định. Kiểm tra cài đặt trình duyệt rồi thử lại."
                    .to_string()
            })?;
        tokio::select! {
            biased;
            _ = attempt.cancel.cancelled() => Err(CANCELLED.to_string()),
            _ = tokio::time::sleep(Duration::from_secs(180)) => Err("Đã hết thời gian đăng nhập Google. Hãy thử lại và hoàn tất trong 3 phút.".into()),
            result = receiver => result.map_err(|_| "Cổng đăng nhập đã đóng. Vui lòng thử lại.".to_string())?,
        }
    }.await;
    shutdown.cancel();
    if tokio::time::timeout(Duration::from_secs(1), &mut server)
        .await
        .is_err()
    {
        server.abort();
        let _ = server.await;
    }
    let code = callback_result?;
    if attempt.cancel.is_cancelled() {
        return Err(CANCELLED.into());
    }
    let response = client()?
        .post(api.join("auth/google/exchange").unwrap())
        .json(&GoogleExchange {
            code,
            code_verifier: oauth.verifier,
            nonce: oauth.nonce,
            loopback_port: port,
        })
        .send()
        .await
        .map_err(|_| NETWORK_ERROR.to_string())?;
    establish(api, response, &attempt).await
}

#[tauri::command]
pub fn auth_cancel(state: tauri::State<'_, AuthState>) {
    if let Some((_, cancel)) = &state.0.lock().unwrap().pending {
        cancel.cancel();
    }
}

#[tauri::command]
pub async fn auth_logout(state: tauri::State<'_, AuthState>) -> Result<(), String> {
    let session = state.0.lock().unwrap().session.take();
    if let Some(session) = session {
        let response = client()?
            .post(session.api.join("auth/logout").unwrap())
            .bearer_auth(session.token)
            .send()
            .await
            .map_err(|_| {
                "Đã xóa phiên trên thiết bị. Chưa thể thu hồi phiên ở máy chủ do mất kết nối."
                    .to_string()
            })?;
        if !(response.status().is_success()
            || response.status() == reqwest::StatusCode::UNAUTHORIZED)
        {
            return Err("Đã xóa phiên trên thiết bị. Máy chủ chưa xác nhận thu hồi phiên.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn authorization_uses_desktop_loopback_and_pkce() {
        let request = OAuthRequest {
            state: "state".into(),
            verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
            nonce: "nonce".into(),
        };
        let url = request.authorization_url("desktop.apps.googleusercontent.com", 54321);
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["redirect_uri"], "http://127.0.0.1:54321");
        assert_eq!(
            query["code_challenge"],
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["state"], "state");
        assert_eq!(query["nonce"], "nonce");
        assert!(!query.contains_key("client_secret"));
    }

    #[test]
    fn callback_requires_one_matching_state_and_one_code() {
        for value in [
            "/?code=code",
            "/?state=wrong&code=code",
            "/?state=expected&state=wrong&code=code",
            "/?state=expected&code=",
            "/?state=expected&code=a&code=b",
            "/callback?state=expected&code=code",
            "/?state=expected&code=code&error=denied",
        ] {
            assert!(callback_result(&value.parse().unwrap(), "expected").is_none());
        }
        assert_eq!(
            callback_result(
                &"/?state=expected&code=encoded%2Bcode".parse().unwrap(),
                "expected"
            ),
            Some(Ok("encoded+code".into()))
        );
        assert!(callback_result(
            &"/?state=expected&error=access_denied".parse().unwrap(),
            "expected"
        )
        .unwrap()
        .is_err());
    }

    #[test]
    fn api_configuration_rejects_insecure_remote_and_ambiguous_urls() {
        for value in [
            "http://example.com/api/v1",
            "https://example.com",
            "https://user@example.com/api/v1",
            "https://example.com/api/v1?key=value",
        ] {
            assert!(api_url(value).is_err());
        }
        assert!(api_url("http://127.0.0.1:3000/api/v1").is_ok());
        assert!(api_url("https://example.com/api/v1/").is_ok());
    }

    #[test]
    fn cancel_prevents_a_pending_login_from_creating_a_session() {
        let state = AuthState::default();
        let attempt = state.begin().unwrap();
        assert!(state.begin().is_err());
        attempt.cancel.cancel();
        assert!(attempt
            .commit(Session {
                token: "test".into(),
                api: Url::parse("https://example.com/api/v1/").unwrap()
            })
            .is_err());
        drop(attempt);
        assert!(state.begin().is_ok());
        assert!(state.0.lock().unwrap().session.is_none());
    }

    #[tokio::test]
    async fn callback_listener_ignores_head_and_bad_state_then_accepts_get() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, mut receiver) = oneshot::channel();
        let callback = Arc::new(CallbackState {
            expected: "expected".into(),
            sender: Mutex::new(Some(sender)),
        });
        let router = Router::new()
            .route("/", get(receive_callback))
            .with_state(callback);
        let shutdown = CancellationToken::new();
        let stop = shutdown.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
                .unwrap()
        });
        let client = client().unwrap();
        let base = format!("http://{address}");
        let head = client
            .head(format!("{base}/?state=expected&code=secret"))
            .send()
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(matches!(
            receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        let rejected = client
            .get(format!("{base}/?state=wrong&code=secret"))
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        assert!(!rejected.text().await.unwrap().contains("secret"));
        assert!(matches!(
            receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        let accepted = client
            .get(format!("{base}/?state=expected&code=secret"))
            .send()
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        assert_eq!(accepted.headers()[header::CACHE_CONTROL], "no-store");
        assert!(!accepted.text().await.unwrap().contains("secret"));
        assert_eq!(receiver.await.unwrap(), Ok("secret".into()));
        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }

    #[tokio::test]
    async fn received_tokens_are_revoked_unless_a_verified_session_is_committed() {
        for (body, status, cancel_before, cancel_during, successful) in [
            (
                r#"{"username":"verified","full_name":"Verified User"}"#,
                StatusCode::OK,
                false,
                false,
                true,
            ),
            (
                r#"{"username":"verified","full_name":"Verified User"}"#,
                StatusCode::OK,
                true,
                false,
                false,
            ),
            (
                r#"{"username":"verified","full_name":"Verified User"}"#,
                StatusCode::OK,
                false,
                true,
                false,
            ),
            ("{}", StatusCode::OK, false, false, false),
            ("{}", StatusCode::UNAUTHORIZED, false, false, false),
        ] {
            let state = AuthState::default();
            let attempt = state.begin().unwrap();
            let cancellation = attempt.cancel.clone();
            let revocations = Arc::new(AtomicUsize::new(0));
            let observed = revocations.clone();
            let router = Router::new()
                .route(
                    "/token",
                    get(|| async {
                        (
                            [(header::CONTENT_TYPE, "application/json")],
                            r#"{"access_token":"fixture-session"}"#,
                        )
                    }),
                )
                .route(
                    "/api/v1/users/me",
                    get(move || {
                        let cancellation = cancellation.clone();
                        async move {
                            if cancel_during {
                                cancellation.cancel();
                            }
                            (status, [(header::CONTENT_TYPE, "application/json")], body)
                        }
                    }),
                )
                .route(
                    "/api/v1/auth/logout",
                    axum::routing::post(move |headers: axum::http::HeaderMap| {
                        let observed = observed.clone();
                        async move {
                            assert_eq!(headers[header::AUTHORIZATION], "Bearer fixture-session");
                            observed.fetch_add(1, Ordering::SeqCst);
                            StatusCode::NO_CONTENT
                        }
                    }),
                );
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let shutdown = CancellationToken::new();
            let stop = shutdown.clone();
            let server = tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(stop.cancelled_owned())
                    .await
                    .unwrap();
            });
            if cancel_before {
                attempt.cancel.cancel();
            }
            let response = client()
                .unwrap()
                .get(format!("http://{address}/token"))
                .send()
                .await
                .unwrap();
            let result = establish(
                Url::parse(&format!("http://{address}/api/v1/")).unwrap(),
                response,
                &attempt,
            )
            .await;
            assert_eq!(result.is_ok(), successful);
            assert_eq!(state.0.lock().unwrap().session.is_some(), successful);
            assert_eq!(revocations.load(Ordering::SeqCst), usize::from(!successful));
            shutdown.cancel();
            tokio::time::timeout(Duration::from_secs(1), server)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
