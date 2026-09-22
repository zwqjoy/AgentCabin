use axum::extract::State;
use axum::http::{header, Method};
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use std::sync::atomic::Ordering;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::web_server::auth;
use crate::web_server::state::AppState;
use crate::web_server::ws;

/// Build the axum Router with all routes and middleware layers.
pub fn build_router(state: AppState) -> Router {
    let state_for_cors = state.clone();
    let state_port = state.effective_port.load(Ordering::Relaxed);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/auth", post(auth::auth_handler))
        .route("/login", get(auth::login_page));

    // WebSocket route (self-authenticating inside handler)
    let ws_routes = Router::new().route("/ws", get(ws::ws_handler));

    // Cookie-protected routes (SPA static files)
    let cookie_routes =
        Router::new()
            .fallback(get(serve_spa))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::session_cookie_middleware,
            ));

    // Assemble with CORS + Origin check
    let cors_layer = build_cors_layer(state_for_cors, state_port);

    Router::new()
        .merge(public_routes)
        .merge(ws_routes)
        .merge(cookie_routes)
        .layer(cors_layer)
        .with_state(state)
}

/// Health endpoint — public, minimal info only
async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

/// SPA fallback — serve index.html for all unmatched routes (client-side routing).
///
/// In release builds: serves from embedded build/ assets.
/// In debug builds: reverse-proxies HTTP requests to Vite dev server at :1420.
/// Note: Vite HMR WebSocket is NOT proxied — use :1420 directly for HMR dev.
async fn serve_spa(
    State(_state): State<AppState>,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{Response, StatusCode};

    let path = req.uri().path().trim_start_matches('/');

    // Try to serve the exact file from embedded assets
    if let Some(content) = get_embedded_file(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        return Response::builder()
            .status(StatusCode::OK)
            .header("content-type", mime)
            .header("cache-control", "public, max-age=3600")
            .body(Body::from(content))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            });
    }

    // Fallback to index.html for SPA routing
    if let Some(index) = get_embedded_file("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/html; charset=utf-8")
            .header("cache-control", "no-cache")
            .body(Body::from(index))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            });
    }

    // No embedded files — dev mode proxy to Vite dev server
    #[cfg(debug_assertions)]
    {
        let uri = req.uri().to_string();
        let vite_url = format!("http://localhost:1420{}", uri);
        log::trace!("[router] dev proxy: {} → {}", uri, vite_url);
        match dev_proxy(&vite_url).await {
            Ok(resp) => return resp,
            Err(e) => {
                log::debug!("[router] dev proxy failed: {}", e);
            }
        }
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from(
            "Not Found (no embedded assets, Vite dev server unreachable)",
        ))
        .unwrap()
}

/// Proxy an HTTP request to the Vite dev server (debug builds only).
#[cfg(debug_assertions)]
async fn dev_proxy(url: &str) -> Result<axum::response::Response, String> {
    use axum::body::Body;
    use axum::http::Response;

    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let mut builder = Response::builder().status(status);
    for (key, value) in resp.headers() {
        // Forward content-type and other relevant headers
        if let Ok(v) = value.to_str() {
            builder = builder.header(key.as_str(), v);
        }
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    builder.body(Body::from(bytes)).map_err(|e| e.to_string())
}

/// Get file content from the embedded release build.
#[cfg(not(debug_assertions))]
fn get_embedded_file(path: &str) -> Option<&'static [u8]> {
    use include_dir::{include_dir, Dir};

    static BUILD_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../build");
    BUILD_DIR.get_file(path).map(|file| file.contents())
}

/// Debug builds are served by Vite, so do not expand `include_dir!` at all.
/// The macro fails at compile time when the build directory does not exist.
#[cfg(debug_assertions)]
fn get_embedded_file(_path: &str) -> Option<&'static [u8]> {
    None
}

/// Build CORS layer with origin checking
fn build_cors_layer(state: AppState, port: u16) -> CorsLayer {
    let allowed_origins = state.allowed_origins.clone();

    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            let s = match origin.to_str() {
                Ok(s) => s,
                Err(_) => return false,
            };

            // No Origin header = non-browser request (curl, wscat) → allow
            // (Browser always sends Origin on cross-origin requests)
            if s.is_empty() {
                return true;
            }

            let Ok(o) = url::Url::parse(s) else {
                return false;
            };

            // 1. Check configured allowed origins (reverse proxy domains)
            if let Some(ref allowed) = allowed_origins {
                if allowed.iter().any(|a| origin_matches(s, a)) {
                    return true;
                }
            }

            // 2. Default: allow local origins with matching port
            let host = o.host_str().unwrap_or("");
            let host_ok = host == "localhost"
                || host == "127.0.0.1"
                || host == "::1"
                || host == "[::1]"
                || is_local_ip(host);
            let port_ok = o.port_or_known_default() == Some(port);
            host_ok && port_ok
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            header::COOKIE,
        ])
        .allow_credentials(true)
}

/// Compare two origins by (scheme, host, port) triple
fn origin_matches(origin: &str, allowed: &str) -> bool {
    let (Ok(o), Ok(a)) = (url::Url::parse(origin), url::Url::parse(allowed)) else {
        return false;
    };
    o.scheme() == a.scheme()
        && o.host() == a.host()
        && o.port_or_known_default() == a.port_or_known_default()
}

/// Check if a host string is a local IP address
fn is_local_ip(host: &str) -> bool {
    use std::net::IpAddr;
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
