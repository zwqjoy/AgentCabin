use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::Ordering;

use crate::web_server::state::{AppState, SessionEntry};

/// Session cookie TTL: 24 hours
const SESSION_TTL_SECS: i64 = 86400;

/// WS authentication subject — identifies how the client authenticated
pub enum WsAuthSubject {
    /// Cookie session (session_id for expiry checking)
    Session(String),
    /// One-time query token (no session_id)
    QueryToken,
}

/// Authenticate WS connection and return the auth subject.
/// Returns None if authentication fails.
pub async fn authenticate_ws(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    query_token: Option<&str>,
) -> Option<WsAuthSubject> {
    // 1. Try cookie auth
    if let Some(cookie_header) = headers.get("cookie").and_then(|v| v.to_str().ok()) {
        for part in cookie_header.split(';') {
            if let Some(session_id) = part.trim().strip_prefix("session=") {
                let sessions = state.http_sessions.lock().await;
                let tv = state.token_version.load(Ordering::Relaxed);
                if let Some(entry) = sessions.get(session_id) {
                    if chrono::Utc::now() < entry.expires_at && entry.token_version == tv {
                        log::debug!("[auth] WS auth: valid session cookie");
                        return Some(WsAuthSubject::Session(session_id.to_string()));
                    }
                }
            }
        }
    }
    // 2. Fallback to query token
    if let Some(tok) = query_token {
        if tok == state.token.read().await.as_str() {
            log::debug!("[auth] WS auth: valid query token");
            return Some(WsAuthSubject::QueryToken);
        }
    }
    log::debug!("[auth] WS auth: no valid credentials");
    None
}

/// POST /auth — exchange token for session cookie
#[derive(Deserialize)]
pub struct AuthRequest {
    token: String,
}

/// Create a session cookie and insert into the session store.
/// Returns (session_id, Set-Cookie header value).
async fn create_session_cookie(state: &AppState) -> (String, String) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires = now + chrono::Duration::seconds(SESSION_TTL_SECS);
    let tv = state.token_version.load(Ordering::Relaxed);
    state.http_sessions.lock().await.insert(
        session_id.clone(),
        SessionEntry {
            id: session_id.clone(),
            issued_at: now,
            expires_at: expires,
            token_version: tv,
        },
    );
    let cookie = format!(
        "session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        session_id, SESSION_TTL_SECS
    );
    log::debug!(
        "[auth] session created: expires={}, tv={}",
        expires.to_rfc3339(),
        tv
    );
    (session_id, cookie)
}

pub async fn auth_handler(
    State(state): State<AppState>,
    Json(body): Json<AuthRequest>,
) -> impl IntoResponse {
    // Validate token
    if body.token != *state.token.read().await {
        log::debug!("[auth] POST /auth: invalid token (masked)");
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("content-type", "application/json")
            .body(Body::from(json!({"error": "invalid token"}).to_string()))
            .unwrap();
    }

    let (_session_id, cookie) = create_session_cookie(&state).await;

    Response::builder()
        .status(StatusCode::OK)
        .header("set-cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(json!({"ok": true}).to_string()))
        .unwrap()
}

/// Query params for GET /login
#[derive(Deserialize, Default)]
pub struct LoginQuery {
    #[serde(default)]
    token: Option<String>,
}

/// GET /login — if `?token=xxx` is present, validate and redirect (server-side auto-login).
/// Otherwise show the manual login form.
pub async fn login_page(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<LoginQuery>,
) -> Response<Body> {
    // Server-side auto-login via query param — no JS fetch needed
    if let Some(ref token) = query.token {
        if token == state.token.read().await.as_str() {
            let (_session_id, cookie) = create_session_cookie(&state).await;
            log::debug!("[auth] GET /login: auto-login via query token");
            return Response::builder()
                .status(StatusCode::FOUND)
                .header("location", "/")
                .header("set-cookie", cookie)
                .body(Body::empty())
                .unwrap();
        }
        log::debug!("[auth] GET /login: invalid query token (masked)");
        // Fall through to show login form (token invalid)
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .body(Body::from(LOGIN_HTML))
        .unwrap()
}

/// Session cookie middleware for protected routes.
/// Validates cookie, redirects to /login on failure.
pub async fn session_cookie_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Extract session cookie
    let cookie_val = extract_cookie(&req, "session");

    if let Some(session_id) = cookie_val {
        let mut sessions = state.http_sessions.lock().await;
        let current_token_version = state.token_version.load(Ordering::Relaxed);

        if let Some(entry) = sessions.get(&session_id) {
            let now = Utc::now();

            // Check expiry
            if now >= entry.expires_at {
                log::debug!("[auth] cookie middleware: session expired");
                sessions.remove(&session_id);
                drop(sessions);
                return redirect_to_login_with_clear_cookie();
            }

            // Check token version (invalidate on token rotation)
            if entry.token_version != current_token_version {
                log::debug!(
                    "[auth] cookie middleware: token version mismatch (session={}, current={})",
                    entry.token_version,
                    current_token_version
                );
                sessions.remove(&session_id);
                drop(sessions);
                return redirect_to_login_with_clear_cookie();
            }

            // Valid session — proceed
            drop(sessions);
            return next.run(req).await;
        } else {
            log::debug!("[auth] cookie middleware: unknown session id");
            drop(sessions);
            return redirect_to_login_with_clear_cookie();
        }
    }

    // No cookie — redirect to login
    log::debug!("[auth] cookie middleware: no session cookie");
    redirect_to_login_with_clear_cookie()
}

/// Validate WS connection — check cookie first, then query token fallback.
/// Returns true if authenticated.
pub async fn validate_ws_auth(state: &AppState, req: &Request<Body>) -> bool {
    // 1. Try cookie auth
    if let Some(session_id) = extract_cookie(req, "session") {
        let sessions = state.http_sessions.lock().await;
        let current_token_version = state.token_version.load(Ordering::Relaxed);

        if let Some(entry) = sessions.get(&session_id) {
            let now = Utc::now();
            if now < entry.expires_at && entry.token_version == current_token_version {
                log::debug!("[auth] WS auth: valid session cookie");
                return true;
            }
            log::debug!("[auth] WS auth: expired/invalid session cookie");
        }
    }

    // 2. Fallback to query param token
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(token_val) = pair.strip_prefix("token=") {
                if token_val == state.token.read().await.as_str() {
                    log::debug!("[auth] WS auth: valid query token");
                    return true;
                }
                log::debug!("[auth] WS auth: invalid query token (masked)");
            }
        }
    }

    log::debug!("[auth] WS auth: no valid credentials");
    false
}

/// Validate WS connection using pre-extracted headers and query token.
/// Used by the WS handler where headers/query are extracted by axum before upgrade.
pub async fn validate_ws_auth_extracted(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    query_token: Option<&str>,
) -> bool {
    // 1. Try cookie auth from headers
    if let Some(cookie_header) = headers.get("cookie").and_then(|v| v.to_str().ok()) {
        let prefix = "session=";
        for part in cookie_header.split(';') {
            let trimmed = part.trim();
            if let Some(session_id) = trimmed.strip_prefix(prefix) {
                let sessions = state.http_sessions.lock().await;
                let current_token_version = state.token_version.load(Ordering::Relaxed);

                if let Some(entry) = sessions.get(session_id) {
                    let now = Utc::now();
                    if now < entry.expires_at && entry.token_version == current_token_version {
                        log::debug!("[auth] WS auth: valid session cookie");
                        return true;
                    }
                    log::debug!("[auth] WS auth: expired/invalid session cookie");
                }
            }
        }
    }

    // 2. Fallback to query param token
    if let Some(token_val) = query_token {
        if token_val == state.token.read().await.as_str() {
            log::debug!("[auth] WS auth: valid query token");
            return true;
        }
        log::debug!("[auth] WS auth: invalid query token (masked)");
    }

    log::debug!("[auth] WS auth: no valid credentials");
    false
}

/// Extract a named cookie from the request
fn extract_cookie(req: &Request<Body>, name: &str) -> Option<String> {
    let cookie_header = req.headers().get("cookie")?.to_str().ok()?;
    let prefix = format!("{}=", name);
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(val) = trimmed.strip_prefix(&prefix) {
            return Some(val.to_string());
        }
    }
    None
}

/// Redirect to /login with expired cookie (clear stale session)
fn redirect_to_login_with_clear_cookie() -> Response<Body> {
    Response::builder()
        .status(StatusCode::FOUND)
        .header("location", "/login")
        .header(
            "set-cookie",
            "session=; Max-Age=0; HttpOnly; SameSite=Lax; Path=/",
        )
        .body(Body::empty())
        .unwrap()
}

/// Minimal login page HTML
const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AgentCabin — Login</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: #0a0a0a; color: #e5e5e5;
      display: flex; justify-content: center; align-items: center;
      min-height: 100vh;
    }
    .card {
      background: #1a1a1a; border: 1px solid #333; border-radius: 12px;
      padding: 2rem; width: 100%; max-width: 400px;
    }
    h1 { font-size: 1.5rem; margin-bottom: 0.5rem; }
    p { color: #888; font-size: 0.875rem; margin-bottom: 1.5rem; }
    label { display: block; font-size: 0.875rem; margin-bottom: 0.5rem; color: #aaa; }
    input {
      width: 100%; padding: 0.75rem; border: 1px solid #333; border-radius: 8px;
      background: #111; color: #e5e5e5; font-size: 1rem;
      margin-bottom: 1rem;
    }
    input:focus { outline: none; border-color: #666; }
    button {
      width: 100%; padding: 0.75rem; border: none; border-radius: 8px;
      background: #3b82f6; color: white; font-size: 1rem; cursor: pointer;
      font-weight: 500;
    }
    button:hover { background: #2563eb; }
    button:disabled { background: #333; cursor: not-allowed; }
    .error { color: #ef4444; font-size: 0.875rem; margin-top: 0.5rem; display: none; }
  </style>
</head>
<body>
  <div class="card">
    <h1>AgentCabin</h1>
    <p>Enter your access token to continue. Find it in the desktop app under Settings.</p>
    <form id="loginForm">
      <label for="token">Access Token</label>
      <input type="password" id="token" placeholder="Paste token here..." autocomplete="off" autofocus>
      <button type="submit" id="submitBtn">Sign In</button>
      <div class="error" id="errorMsg"></div>
    </form>
  </div>
  <script>
    // Auto-auth via URL fragment: /login#token=xxx
    (function() {
      var hash = window.location.hash;
      if (hash && hash.indexOf('#token=') === 0) {
        var token = hash.substring(7);
        if (token) {
          history.replaceState(null, '', '/login');
          fetch('/auth', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ token: token }),
            credentials: 'same-origin',
          }).then(function(res) {
            if (res.ok) window.location.href = '/';
            else {
              document.getElementById('errorMsg').textContent = 'Invalid or expired token';
              document.getElementById('errorMsg').style.display = 'block';
            }
          }).catch(function() {
            document.getElementById('errorMsg').textContent = 'Connection error';
            document.getElementById('errorMsg').style.display = 'block';
          });
          return;
        }
      }
    })();

    const form = document.getElementById('loginForm');
    const tokenInput = document.getElementById('token');
    const submitBtn = document.getElementById('submitBtn');
    const errorMsg = document.getElementById('errorMsg');

    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const token = tokenInput.value.trim();
      if (!token) return;

      submitBtn.disabled = true;
      submitBtn.textContent = 'Signing in...';
      errorMsg.style.display = 'none';

      try {
        const res = await fetch('/auth', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token }),
          credentials: 'same-origin',
        });
        if (res.ok) {
          window.location.href = '/';
        } else {
          const data = await res.json().catch(() => ({}));
          errorMsg.textContent = data.error || 'Invalid token';
          errorMsg.style.display = 'block';
        }
      } catch (err) {
        errorMsg.textContent = 'Connection error. Is the server running?';
        errorMsg.style.display = 'block';
      } finally {
        submitBtn.disabled = false;
        submitBtn.textContent = 'Sign In';
      }
    });
  </script>
</body>
</html>"#;
