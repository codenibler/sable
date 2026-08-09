//! Read-only HTTP API and PWA host for the mobile client.
//!
//! The desktop app stays the single source of truth: this server shares the same `AppState`,
//! the same SQLite connection and the same aggregation code as the Tauri commands, so the
//! phone can never disagree with the window. Only reads are exposed -- there is deliberately
//! no route that mutates anything, so a leaked token cannot alter the ledger.
//!
//! The listener is loopback-only (enforced in `config::mobile_api_bind`). Reaching it from a
//! phone is Tailscale's job: `tailscale serve --bg 8787` publishes it inside the tailnet over
//! real HTTPS, so nothing is ever exposed to the public internet.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Request, State as AxumState},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use subtle::ConstantTimeEq;
use tauri::AppHandle;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::commands::{self, AppState};

pub fn spawn(app: AppHandle, sable: Arc<AppState>) {
    if !sable.config.mobile_api_enabled {
        return;
    }
    let address = sable.config.mobile_api_bind;
    let router = api_router(sable).merge(asset_router(app));
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => {
                eprintln!("Sable mobile API listening on http://{address}");
                if let Err(error) = axum::serve(listener, router).await {
                    eprintln!("Sable mobile API stopped: {error}");
                }
            }
            // A busy port is not a reason to refuse to open the desktop window.
            Err(error) => eprintln!("Sable mobile API could not bind {address}: {error}"),
        }
    });
}

fn api_router(sable: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/net-worth", get(net_worth))
        .route_layer(middleware::from_fn_with_state(sable.clone(), authorize));

    Router::new()
        .route("/api/health", get(health))
        .merge(protected)
        // The payload is a complete financial position; no intermediary should retain it.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(sable)
}

fn asset_router(app: AppHandle) -> Router {
    Router::new()
        .fallback(assets)
        // The desktop webview gets its CSP from tauri.conf.json. Over HTTP nothing sends one,
        // so mirror it here -- this is the copy of the app reachable from the internet.
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:",
            ),
        ))
        .with_state(app)
}

/// Unauthenticated so a tunnel can health-check the origin. Reveals only that Sable is up.
async fn health() -> Response {
    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn authorize(
    AxumState(sable): AxumState<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    // No configured token means no way to authenticate, so nothing is reachable.
    let Some(expected) = sable.config.mobile_api_token.as_deref() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    // Comparing secrets with `==` returns early on the first differing byte, which lets a
    // caller recover the token one character at a time by timing the responses.
    let authorized: bool = presented.as_bytes().ct_eq(expected.as_bytes()).into();
    if !authorized {
        // No body: a rejected caller learns nothing beyond "wrong".
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn dashboard(AxumState(sable): AxumState<Arc<AppState>>) -> Response {
    // Never forces a rebuild: the phone rides the same cached snapshot as the desktop rather
    // than spending the shared Trading 212 rate-limit budget.
    match commands::dashboard(&sable, false).await {
        Ok(dashboard) => Json(dashboard).into_response(),
        Err(message) => (StatusCode::BAD_GATEWAY, message).into_response(),
    }
}

async fn net_worth(AxumState(sable): AxumState<Arc<AppState>>) -> Response {
    // Reading the ledger takes the blocking SQLite mutex, which `build_dashboard` can hold for
    // the length of a full upstream fan-out. Doing that on an async worker would stall every
    // other task sharing the thread, so it runs where blocking is allowed.
    let entries = tokio::task::spawn_blocking(move || commands::net_worth_entries(&sable)).await;
    match entries {
        Ok(Ok(entries)) => Json(entries).into_response(),
        Ok(Err(message)) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Net worth lookup failed").into_response(),
    }
}

/// Serves the same frontend bundle Tauri embeds for the desktop webview, so the PWA is
/// always the build that shipped with this binary and there is no `dist/` to deploy.
/// Unknown paths fall back to `index.html` because the client routes on its own.
async fn assets(AxumState(app): AxumState<AppHandle>, uri: Uri) -> Response {
    let path = uri.path().to_string();
    // A mistyped API route would otherwise fall through to index.html and answer 200 text/html,
    // which the client only discovers when `response.json()` throws a bare SyntaxError.
    if path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let resolver = app.asset_resolver();
    let asset = resolver
        .get(path.clone())
        .or_else(|| resolver.get("/index.html".to_string()));
    match asset {
        Some(asset) => (
            [(header::CONTENT_TYPE, content_type(&path, &asset.mime_type))],
            asset.bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// Tauri's resolver falls back to `text/html` for extensions it does not know, which breaks
/// the web manifest: a browser handed HTML there silently ignores it, and the Home Screen app
/// loses its name, icon and standalone display.
fn content_type(path: &str, resolved: &str) -> HeaderValue {
    if path.ends_with(".webmanifest") {
        return HeaderValue::from_static("application/manifest+json");
    }
    HeaderValue::from_str(resolved)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::SystemTime,
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    use super::api_router;
    use crate::{commands::AppState, config::Config, db};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn state(token: Option<&str>) -> Arc<AppState> {
        let suffix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sable-server-test-{suffix}.sqlite3"));
        let database = db::open(&path).expect("test database");
        let mut config = Config::for_tests();
        config.mobile_api_token = token.map(str::to_string);
        Arc::new(AppState {
            database: Mutex::new(database),
            client: reqwest::Client::new(),
            config,
            history_sync: tokio::sync::Mutex::new(()),
            dashboard_cache: tokio::sync::Mutex::new(None),
        })
    }

    async fn get(token: Option<&str>, path: &str, header_value: Option<&str>) -> (StatusCode, Vec<u8>, Option<String>) {
        let mut request = Request::builder().uri(path);
        if let Some(value) = header_value {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = api_router(state(token))
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let cache_control = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        (status, body, cache_control)
    }

    #[tokio::test]
    async fn rejects_requests_without_a_usable_bearer_token() {
        for (token, presented) in [
            (Some(TOKEN), None),
            (Some(TOKEN), Some("Bearer wrong")),
            (Some(TOKEN), Some(TOKEN)),                     // missing the Bearer prefix
            (Some(TOKEN), Some("Bearer ")),
            (Some(TOKEN), Some("Basic 0123456789abcdef0123456789abcdef")),
            // A token that is a prefix of the real one must not pass.
            (Some(TOKEN), Some("Bearer 0123456789abcdef0123456789abcde")),
            // No configured token means nothing is reachable, even with a plausible header.
            (None, Some("Bearer ")),
        ] {
            let (status, body, _) = get(token, "/api/net-worth", presented).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "expected 401 for header {presented:?}"
            );
            assert!(body.is_empty(), "401 must not describe why it failed");
        }
    }

    #[tokio::test]
    async fn serves_net_worth_to_a_correctly_authenticated_caller() {
        let (status, body, cache_control) =
            get(Some(TOKEN), "/api/net-worth", Some(&format!("Bearer {TOKEN}"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"[]", "a fresh database has no net worth entries");
        assert_eq!(cache_control.as_deref(), Some("no-store"));
    }

    #[tokio::test]
    async fn health_is_reachable_without_a_token_and_reveals_nothing() {
        let (status, body, cache_control) = get(Some(TOKEN), "/api/health", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, br#"{"ok":true}"#);
        assert_eq!(cache_control.as_deref(), Some("no-store"));
    }

    #[tokio::test]
    async fn exposes_no_write_routes() {
        for (method, path) in [
            ("POST", "/api/net-worth"),
            ("PUT", "/api/net-worth"),
            ("DELETE", "/api/net-worth"),
            ("POST", "/api/dashboard"),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap();
            let response = api_router(state(Some(TOKEN))).oneshot(request).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path} must not be routed"
            );
        }
    }

    #[test]
    fn overrides_the_manifest_content_type_the_resolver_gets_wrong() {
        // Tauri hands back text/html for unknown extensions; browsers ignore a manifest
        // served that way, which silently downgrades the Home Screen app.
        assert_eq!(
            super::content_type("/manifest.webmanifest", "text/html"),
            "application/manifest+json"
        );
        assert_eq!(super::content_type("/sw.js", "text/javascript"), "text/javascript");
        assert_eq!(super::content_type("/icons/icon-192.png", "image/png"), "image/png");
    }
}
