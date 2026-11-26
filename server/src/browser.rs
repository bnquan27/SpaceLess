//! Browser transport policy; JWT verification remains in the shared Actor extractor.
use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub(crate) fn origin_allowed(origin: &str) -> bool {
    let configured = std::env::var("APP_ALLOWED_ORIGINS").unwrap_or_else(|_| {
        "http://127.0.0.1:1420,http://localhost:1420,http://127.0.0.1:1421,http://localhost:1421,http://127.0.0.1:4173,http://localhost:4173,http://tauri.localhost,tauri://localhost".into()
    });
    configured.split(',').any(|value| value.trim() == origin)
}

pub(crate) fn socket_token(headers: &HeaderMap) -> Option<&str> {
    let origin = headers.get(header::ORIGIN)?.to_str().ok()?;
    if !origin_allowed(origin) { return None; }
    let protocols = headers.get(header::SEC_WEBSOCKET_PROTOCOL)?.to_str().ok()?;
    if !protocols.split(',').any(|value| value.trim() == "spaceless") { return None; }
    protocols.split(',').find_map(|value| value.trim().strip_prefix("bearer."))
}

pub(crate) async fn cors(request: Request, next: Next) -> Response {
    let origin = request.headers().get(header::ORIGIN).cloned();
    if origin.as_ref().is_some_and(|value| !value.to_str().is_ok_and(origin_allowed)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    if let Some(origin) = origin {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.append(header::VARY, HeaderValue::from_static("Origin"));
        headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE,OPTIONS"));
        headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Authorization,Content-Type,Range"));
        headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, HeaderValue::from_static("Retry-After,Content-Disposition"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_socket_requires_known_origin_and_protocol() {
        let mut headers = HeaderMap::new();
        headers.insert(header::SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("spaceless, bearer.fixture"));
        assert_eq!(socket_token(&headers), None);
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:4173"));
        assert_eq!(socket_token(&headers), Some("fixture"));
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://untrusted.example"));
        assert_eq!(socket_token(&headers), None);
    }
}
