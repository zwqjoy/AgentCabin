//! Shared, keyless OpenAI subscription bridge backed by an AgentCabin-managed ChatGPT login.
//!
//! The global Provider record intentionally contains no OAuth material.  When a managed
//! Codex, Grok, or Pi process selects the reserved subscription provider, this module starts
//! one loopback Responses endpoint and injects only a random local bearer into that process.
//! The endpoint reads AgentCabin's separate host secret on demand, refreshes the ChatGPT session
//! when it is close to expiry, and forwards the request to the ChatGPT Codex backend.

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use rand::distributions::{Alphanumeric, DistString};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;
use uuid::Uuid;

use crate::models::{
    CodexProviderCredential, GlobalProviderCredential, PiProviderCredential,
    SubscriptionRateLimitWindow, SubscriptionRateLimits,
};

// Keep this distinct from Pi's native `openai-codex` provider. This id is only for the
// AgentCabin-wide subscription profile backed by its own OAuth secret.
pub(crate) const CODEX_SUBSCRIPTION_PROVIDER_ID: &str = "openai-chatgpt-subscription";

const CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const AUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const AUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
// This is the public subscription client id used by OpenWorker/Codex's OAuth flow. It is not a
// bearer or a credential.
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CALLBACK_PORT: u16 = 1455;
const CALLBACK_PATH: &str = "/auth/callback";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const AUTH_SCOPE: &str = "openid profile email offline_access";
// Keep the upstream identity compatible with the Codex Responses backend. This is a wire
// protocol marker, not a dependency on the Codex app or on OpenWorker's process/state.
const CODEX_ORIGINATOR: &str = "codex_cli_rs";
const SUBSCRIPTION_SECRET_REF: &str = "openai-chatgpt-subscription";
const REFRESH_MARGIN_SECONDS: u64 = 300;
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_INSTRUCTIONS: &str = "You are a helpful assistant.";

const SIGNED_IN_PAGE: &str = r#"<!doctype html>
<meta charset="utf-8"><title>AgentCabin</title>
<body style="font-family: system-ui; margin: 4rem auto; max-width: 32rem; text-align: center;">
<h2>Signed in</h2><p>You can close this tab and return to AgentCabin.</p>
</body>"#;
const CALLBACK_ERROR_PAGE: &str = r#"<!doctype html>
<meta charset="utf-8"><title>AgentCabin</title>
<body style="font-family: system-ui; margin: 4rem auto; max-width: 32rem; text-align: center;">
<h2>Sign-in failed</h2><p>Return to AgentCabin and start the sign-in again.</p>
</body>"#;
const CALLBACK_NOT_FOUND_PAGE: &str = r#"<!doctype html>
<meta charset="utf-8"><title>AgentCabin</title>
<body style="font-family: system-ui; margin: 4rem auto; max-width: 32rem; text-align: center;">
<h2>Not found</h2>
</body>"#;

#[derive(Clone)]
struct BridgeEndpoint {
    base_url: String,
    token: String,
}

struct BridgeState {
    local_token: String,
    session_id: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
struct ManagedCodexAuth {
    access_token: String,
    refresh_token: Option<String>,
    account_id: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodexSubscriptionStatus {
    pub logged_in: bool,
    pub account: Option<String>,
}

static BRIDGE: Lazy<Mutex<Option<BridgeEndpoint>>> = Lazy::new(|| Mutex::new(None));
static AUTH_REFRESH: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static ACTIVE_AUTHORIZE_URL: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

pub(crate) fn is_codex_subscription_id(id: &str) -> bool {
    id == CODEX_SUBSCRIPTION_PROVIDER_ID
}

/// Return metadata for the AgentCabin-managed subscription login without exposing tokens.
pub(crate) fn subscription_status() -> CodexSubscriptionStatus {
    let Ok(Some(auth)) = read_managed_auth() else {
        return CodexSubscriptionStatus::default();
    };
    let account = read_managed_secret().and_then(|secret| {
        secret
            .get("account_email")
            .cloned()
            .or_else(|| secret.get("account_id").cloned())
    });
    CodexSubscriptionStatus {
        logged_in: true,
        account: account.or(Some(auth.account_id)),
    }
}

/// Run the OpenWorker-compatible ChatGPT subscription OAuth flow.
///
/// This deliberately does not invoke `codex login` and does not read Pi's auth file. The
/// resulting tokens are persisted in AgentCabin's own host-secret entry and are only used by
/// the local Responses bridge.
pub(crate) async fn sign_in() -> Result<bool, String> {
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                format!(
                    "OpenAI 登录回调端口 {} 已被占用，请先关闭 OpenWorker 或其他正在进行的 Codex 登录，然后重试。",
                    CALLBACK_PORT
                )
            } else {
                format!("无法启动 OpenAI 登录回调服务：{error}")
            }
        })?;

    let verifier = Alphanumeric.sample_string(&mut rand::thread_rng(), 64);
    let state = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);
    let challenge = URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()));
    let authorize_url = build_authorize_url(&state, &challenge)?;

    {
        let mut active_url = ACTIVE_AUTHORIZE_URL.lock().await;
        *active_url = Some(authorize_url.clone());
    }
    if let Err(error) = open_authorize_url(&authorize_url) {
        clear_active_authorize_url().await;
        return Err(error);
    }
    let code = match wait_for_oauth_code(listener, &state).await {
        Ok(code) => code,
        Err(error) => {
            clear_active_authorize_url().await;
            return Err(error);
        }
    };
    clear_active_authorize_url().await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("无法创建 OpenAI 登录网络客户端：{error}"))?;
    let response = client
        .post(AUTH_TOKEN_URL)
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", CODEX_CLIENT_ID),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("无法完成 ChatGPT 登录：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "ChatGPT 登录换取令牌失败（HTTP {}），请重新登录。",
            response.status()
        ));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("ChatGPT 登录响应无效：{error}"))?;
    save_managed_token_payload(&payload)?;
    Ok(true)
}

/// Reopen the current browser URL while the OAuth callback task is still waiting.
/// Closing the browser tab cannot be observed by a loopback listener, so this keeps the
/// flow recoverable without starting a second listener or asking the user to wait for timeout.
pub(crate) async fn reopen_sign_in() -> Result<bool, String> {
    let authorize_url = ACTIVE_AUTHORIZE_URL
        .lock()
        .await
        .clone()
        .ok_or_else(|| "当前没有正在等待的 ChatGPT 登录流程，请重新点击登录。".to_string())?;
    open_authorize_url(&authorize_url)?;
    Ok(true)
}

async fn clear_active_authorize_url() {
    let mut active_url = ACTIVE_AUTHORIZE_URL.lock().await;
    *active_url = None;
}

/// Remove only AgentCabin's global subscription credentials. Pi and native Codex credentials
/// remain untouched.
pub(crate) fn clear_subscription_auth() -> Result<bool, String> {
    if read_managed_secret().is_none() {
        return Ok(false);
    }
    crate::storage::profile_bindings::delete_host_secret(SUBSCRIPTION_SECRET_REF)?;
    Ok(true)
}

fn read_managed_secret() -> Option<HashMap<String, String>> {
    crate::storage::profile_bindings::get_host_secret(SUBSCRIPTION_SECRET_REF)
}

fn read_managed_auth() -> Result<Option<ManagedCodexAuth>, String> {
    let Some(secret) = read_managed_secret() else {
        return Ok(None);
    };
    auth_from_secret(&secret).map(Some)
}

fn auth_from_secret(secret: &HashMap<String, String>) -> Result<ManagedCodexAuth, String> {
    let access_token = secret
        .get("access_token")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| "AgentCabin 的 ChatGPT 登录凭证不完整，请重新登录。".to_string())?;
    let refresh_token = secret
        .get("refresh_token")
        .filter(|value| !value.is_empty())
        .cloned();
    let account_id = secret
        .get("account_id")
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| account_id_from_token(secret.get("id_token").map(String::as_str)))
        .or_else(|| account_id_from_token(Some(&access_token)))
        .ok_or_else(|| "ChatGPT 登录响应缺少 account id，请重新登录。".to_string())?;
    Ok(ManagedCodexAuth {
        access_token,
        refresh_token,
        account_id,
    })
}

fn save_managed_token_payload(payload: &Value) -> Result<ManagedCodexAuth, String> {
    let mut secret = read_managed_secret().unwrap_or_default();
    for key in ["access_token", "refresh_token", "id_token"] {
        if let Some(value) = payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            secret.insert(key.to_string(), value.to_string());
        }
    }
    if secret
        .get("access_token")
        .is_none_or(|value| value.is_empty())
    {
        return Err("ChatGPT 登录响应缺少 access token，请重试。".to_string());
    }

    let account_id = payload
        .get("account_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| account_id_from_token(secret.get("id_token").map(String::as_str)))
        .or_else(|| account_id_from_token(secret.get("access_token").map(String::as_str)))
        .or_else(|| secret.get("account_id").cloned())
        .ok_or_else(|| "ChatGPT 登录响应缺少 account id，请重试。".to_string())?;
    secret.insert("account_id".to_string(), account_id);

    let account_email = payload
        .get("email")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| email_from_token(secret.get("id_token").map(String::as_str)))
        .or_else(|| secret.get("account_email").cloned());
    if let Some(email) = account_email {
        secret.insert("account_email".to_string(), email);
    }
    secret.insert("tokens_issued_at".to_string(), now_epoch().to_string());

    crate::storage::profile_bindings::set_host_secret(SUBSCRIPTION_SECRET_REF, secret.clone())?;
    auth_from_secret(&secret)
}

fn sha256(value: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value);
    digest.into()
}

fn build_authorize_url(state: &str, challenge: &str) -> Result<String, String> {
    let mut url =
        Url::parse(AUTH_AUTHORIZE_URL).map_err(|error| format!("OpenAI 登录地址无效：{error}"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CODEX_CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", AUTH_SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", CODEX_ORIGINATOR);
    Ok(url.to_string())
}

fn open_authorize_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    return Err("当前平台不支持自动打开浏览器，请手动打开登录地址。".to_string());

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 ChatGPT 登录页面：{error}"))
}

async fn wait_for_oauth_code(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("ChatGPT 登录等待超时，请重新点击登录。".to_string());
        }
        let accepted = timeout(remaining, listener.accept())
            .await
            .map_err(|_| "ChatGPT 登录等待超时，请重新点击登录。".to_string())?
            .map_err(|error| format!("读取 OpenAI 登录回调失败：{error}"))?;
        match handle_oauth_callback(accepted.0, expected_state).await? {
            Some(code) => return Ok(code),
            None => continue,
        }
    }
}

async fn handle_oauth_callback(
    stream: tokio::net::TcpStream,
    expected_state: &str,
) -> Result<Option<String>, String> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    timeout(Duration::from_secs(10), reader.read_line(&mut request_line))
        .await
        .map_err(|_| "读取 OpenAI 登录回调超时。".to_string())?
        .map_err(|error| format!("读取 OpenAI 登录回调失败：{error}"))?;
    if request_line.trim().is_empty() {
        return Ok(None);
    }
    loop {
        let mut header_line = String::new();
        let read = timeout(Duration::from_secs(10), reader.read_line(&mut header_line))
            .await
            .map_err(|_| "读取 OpenAI 登录回调超时。".to_string())?
            .map_err(|error| format!("读取 OpenAI 登录回调失败：{error}"))?;
        if read == 0 || header_line == "\r\n" || header_line == "\n" {
            break;
        }
    }

    let mut stream = reader.into_inner();
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let callback_url = Url::parse(target)
        .or_else(|_| Url::parse(&format!("http://localhost{target}")))
        .map_err(|_| "OpenAI 登录回调地址无效。".to_string())?;
    if callback_url.path() != CALLBACK_PATH {
        write_callback_response(&mut stream, "404 Not Found", CALLBACK_NOT_FOUND_PAGE).await?;
        return Ok(None);
    }

    let params = callback_url
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    if params
        .get("error")
        .is_some_and(|value| !value.trim().is_empty())
    {
        write_callback_response(&mut stream, "400 Bad Request", CALLBACK_ERROR_PAGE).await?;
        return Err("ChatGPT 登录被取消或拒绝，请重新登录。".to_string());
    }
    let code = params.get("code").cloned().unwrap_or_default();
    let state = params.get("state").cloned().unwrap_or_default();
    if code.is_empty() || state != expected_state {
        write_callback_response(&mut stream, "400 Bad Request", CALLBACK_ERROR_PAGE).await?;
        return Ok(None);
    }
    write_callback_response(&mut stream, "200 OK", SIGNED_IN_PAGE).await?;
    Ok(Some(code))
}

async fn write_callback_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    body: &str,
) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("发送 OpenAI 登录回调响应失败：{error}"))
}

/// Prepare the provider consumed by the Codex app-server/CLI. The returned credentials are
/// process-local and contain only the random loopback bearer, never the native OAuth token.
pub(crate) async fn prepare_codex_provider(
    provider: &CodexProviderCredential,
) -> Result<CodexProviderCredential, String> {
    if !is_codex_subscription_id(&provider.id) {
        return Ok(provider.clone());
    }

    let endpoint = ensure_bridge().await?;
    let mut prepared = provider.clone();
    prepared.base_url = endpoint.base_url;
    prepared.env_key = "OPENAI_API_KEY".to_string();
    prepared.api_key = Some(endpoint.token);
    prepared.wire_api = "responses".to_string();
    prepared.supports_websockets = Some(false);
    Ok(prepared)
}

/// Prepare the provider registered in the managed Pi Provider Bridge.
pub(crate) async fn prepare_pi_provider(
    provider: &PiProviderCredential,
) -> Result<PiProviderCredential, String> {
    if !is_codex_subscription_id(&provider.id) {
        return Ok(provider.clone());
    }

    let endpoint = ensure_bridge().await?;
    let mut prepared = provider.clone();
    prepared.base_url = endpoint.base_url;
    prepared.api = "openai-responses".to_string();
    prepared.api_key = Some(endpoint.token);
    Ok(prepared)
}

/// Prepare the provider used by the Grok managed config. This keeps the persisted global
/// Provider keyless while giving Grok its normal OpenAI-compatible `OPENAI_API_KEY` shape.
pub(crate) async fn prepare_global_provider(
    provider: &GlobalProviderCredential,
) -> Result<GlobalProviderCredential, String> {
    if !is_codex_subscription_id(&provider.id) {
        return Ok(provider.clone());
    }

    let endpoint = ensure_bridge().await?;
    let mut prepared = provider.clone();
    prepared.base_url = endpoint.base_url;
    prepared.api_key = Some(endpoint.token);
    prepared.env_key = Some("OPENAI_API_KEY".to_string());
    prepared.keyless = Some(true);
    Ok(prepared)
}

async fn ensure_bridge() -> Result<BridgeEndpoint, String> {
    let mut bridge = BRIDGE.lock().await;
    if let Some(endpoint) = bridge.as_ref() {
        return Ok(endpoint.clone());
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("无法启动 OpenAI 订阅本地桥接：{error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("无法读取 OpenAI 订阅本地桥接地址：{error}"))?;
    let token = Alphanumeric.sample_string(&mut rand::thread_rng(), 48);
    let endpoint = BridgeEndpoint {
        base_url: format!("http://{address}/v1"),
        token: token.clone(),
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("无法创建 OpenAI 订阅网络客户端：{error}"))?;
    let state = Arc::new(BridgeState {
        local_token: token,
        session_id: Uuid::new_v4().to_string(),
        client,
    });
    let router = Router::new()
        .route("/v1/responses", post(handle_responses))
        .route("/responses", post(handle_responses))
        .with_state(state);

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::warn!("OpenAI subscription bridge stopped: {error}");
        }
    });

    *bridge = Some(endpoint.clone());
    Ok(endpoint)
}

async fn handle_responses(
    State(state): State<Arc<BridgeState>>,
    headers: HeaderMap,
    Json(original): Json<Value>,
) -> Response {
    if !is_authorized(&headers, &state.local_token) {
        return bridge_error(StatusCode::UNAUTHORIZED, "Invalid local bridge token");
    }

    let wants_stream = original
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut upstream_body = sanitize_request(original);
    // The ChatGPT Codex backend is a streaming endpoint. For a non-streaming caller we drain
    // the event stream below and return the completed Responses object.
    upstream_body["stream"] = Value::Bool(true);

    let mut auth = match read_managed_request_auth(&state.client, false).await {
        Ok(auth) => auth,
        Err(error) => return bridge_error(StatusCode::UNAUTHORIZED, &error),
    };
    let mut upstream = match send_upstream(&state, &auth, &upstream_body).await {
        Ok(response) => response,
        Err(error) => return bridge_error(StatusCode::BAD_GATEWAY, &error),
    };

    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        match read_managed_request_auth(&state.client, true).await {
            Ok(refreshed) => {
                auth = refreshed;
                upstream = match send_upstream(&state, &auth, &upstream_body).await {
                    Ok(response) => response,
                    Err(error) => return bridge_error(StatusCode::BAD_GATEWAY, &error),
                };
            }
            Err(error) => return bridge_error(StatusCode::UNAUTHORIZED, &error),
        }
    }

    if !upstream.status().is_success() {
        return relay_buffered_response(upstream).await;
    }

    if wants_stream {
        relay_streaming_response(upstream)
    } else {
        complete_streaming_response(upstream).await
    }
}

fn is_authorized(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        == Some(expected)
}

fn sanitize_request(mut body: Value) -> Value {
    let Some(object) = body.as_object_mut() else {
        return body;
    };

    for key in ["max_output_tokens", "temperature", "top_p"] {
        object.remove(key);
    }
    object.insert("store".to_string(), Value::Bool(false));
    let has_instructions = object
        .get("instructions")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !has_instructions {
        object.insert(
            "instructions".to_string(),
            Value::String(DEFAULT_INSTRUCTIONS.to_string()),
        );
    }
    body
}

async fn send_upstream(
    state: &BridgeState,
    auth: &ManagedCodexAuth,
    body: &Value,
) -> Result<reqwest::Response, String> {
    state
        .client
        .post(CODEX_RESPONSES_URL)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", auth.access_token),
        )
        .header("chatgpt-account-id", &auth.account_id)
        .header("originator", CODEX_ORIGINATOR)
        .header("OpenAI-Beta", "responses=experimental")
        .header("session-id", &state.session_id)
        .header(header::ACCEPT, "text/event-stream")
        .json(body)
        .send()
        .await
        .map_err(|error| format!("OpenAI 订阅请求失败：{error}"))
}

async fn relay_buffered_response(upstream: reqwest::Response) -> Response {
    let status = map_status(upstream.status());
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let body = match upstream.bytes().await {
        Ok(body) => body,
        Err(error) => Bytes::from(
            json!({
                "error": {
                    "type": "agentcabin_codex_subscription",
                    "message": format!("读取 OpenAI 订阅错误响应失败：{error}")
                }
            })
            .to_string(),
        ),
    };
    response_with_body(status, content_type, Body::from(body))
}

fn relay_streaming_response(upstream: reqwest::Response) -> Response {
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("text/event-stream"));
    let stream = upstream.bytes_stream().map(|chunk| {
        chunk.map_err(|error| std::io::Error::other(format!("OpenAI 订阅流读取失败：{error}")))
    });
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache");
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))
}

async fn complete_streaming_response(upstream: reqwest::Response) -> Response {
    let body = match upstream.text().await {
        Ok(body) => body,
        Err(error) => return bridge_error(StatusCode::BAD_GATEWAY, &error.to_string()),
    };
    match completed_response_from_sse(&body) {
        Ok(response) => Json(response).into_response(),
        Err(error) => bridge_error(StatusCode::BAD_GATEWAY, &error),
    }
}

fn completed_response_from_sse(body: &str) -> Result<Value, String> {
    for frame in body.split("\n\n") {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let event: Value = serde_json::from_str(&data)
            .map_err(|error| format!("OpenAI 订阅返回了无效 SSE：{error}"))?;
        if event.get("type").and_then(Value::as_str) == Some("response.completed") {
            if let Some(response) = event.get("response") {
                return Ok(response.clone());
            }
        }
        if event.get("response").is_some()
            && event.get("type").and_then(Value::as_str) == Some("response")
        {
            return Ok(event["response"].clone());
        }
    }
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if value.is_object() {
            return Ok(value);
        }
    }
    Err("OpenAI 订阅没有返回完成的 Responses 结果".to_string())
}

fn bridge_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "type": "agentcabin_codex_subscription",
                "message": message,
            }
        })),
    )
        .into_response()
}

fn response_with_body(
    status: StatusCode,
    content_type: Option<HeaderValue>,
    body: Body,
) -> Response {
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    builder
        .body(body)
        .unwrap_or_else(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))
}

fn map_status(status: reqwest::StatusCode) -> StatusCode {
    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY)
}

async fn read_managed_request_auth(
    client: &reqwest::Client,
    force_refresh: bool,
) -> Result<ManagedCodexAuth, String> {
    let auth = read_managed_auth()?.ok_or_else(|| {
        "AgentCabin 的 ChatGPT 订阅尚未登录，请先在模型配置中点击“使用 ChatGPT 登录”。".to_string()
    })?;
    let stale = jwt_expiration(&auth.access_token)
        .map(|expiration| expiration <= now_epoch().saturating_add(REFRESH_MARGIN_SECONDS))
        .unwrap_or(false);
    if !force_refresh && !stale {
        return Ok(auth);
    }

    let refresh_token = auth.refresh_token.clone().ok_or_else(|| {
        "AgentCabin 的 ChatGPT 登录已过期，请重新点击全局 Provider 的登录按钮。".to_string()
    })?;
    refresh_managed_auth(client, refresh_token).await
}

async fn refresh_managed_auth(
    client: &reqwest::Client,
    refresh_token: String,
) -> Result<ManagedCodexAuth, String> {
    let _refresh_guard = AUTH_REFRESH.lock().await;

    // Another request may have refreshed the AgentCabin secret while this request was waiting.
    let latest = read_managed_auth()?.ok_or_else(|| {
        "AgentCabin 的 ChatGPT 登录已退出，请重新点击全局 Provider 的登录按钮。".to_string()
    })?;
    let fresh = jwt_expiration(&latest.access_token)
        .map(|expiration| expiration > now_epoch().saturating_add(REFRESH_MARGIN_SECONDS))
        .unwrap_or(false);
    if fresh {
        return Ok(latest);
    }

    let response = client
        .post(AUTH_TOKEN_URL)
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", CODEX_CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|error| format!("无法刷新 AgentCabin 的 ChatGPT 登录：{error}"))?;
    if !response.status().is_success() {
        let _ = clear_subscription_auth();
        return Err(
            "AgentCabin 的 ChatGPT 登录已失效，请重新点击全局 Provider 的登录按钮。".to_string(),
        );
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("ChatGPT 登录刷新响应无效：{error}"))?;
    save_managed_token_payload(&payload)
}

fn account_id_from_token(token: Option<&str>) -> Option<String> {
    let claims = decode_jwt_claims(Some(token?))?;
    let auth = claims
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object);
    auth.and_then(|auth| {
        auth.get("chatgpt_account_id")
            .or_else(|| auth.get("account_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
    .or_else(|| {
        claims
            .get("chatgpt_account_id")
            .or_else(|| claims.get("account_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn email_from_token(token: Option<&str>) -> Option<String> {
    decode_jwt_claims(token)?
        .get("email")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn decode_jwt_claims(token: Option<&str>) -> Option<Value> {
    let payload = token?.split('.').nth(1)?;
    let mut padded = payload.to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let bytes = URL_SAFE.decode(padded.as_bytes()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn jwt_expiration(token: &str) -> Option<u64> {
    decode_jwt_claims(Some(token))?
        .get("exp")
        .and_then(Value::as_u64)
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Fetch real-time rate limits (5-hour primary and weekly secondary windows)
/// directly from OpenAI ChatGPT subscription endpoints using the stored access token.
pub(crate) async fn fetch_subscription_rate_limits(
) -> Result<Option<SubscriptionRateLimits>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("无法创建网络客户端：{e}"))?;

    let auth = match read_managed_request_auth(&client, false).await {
        Ok(auth) => auth,
        Err(_) => return Ok(None),
    };

    let endpoints = [
        "https://chatgpt.com/backend-api/wham/usage",
        "https://chatgpt.com/backend-api/api/codex/usage",
    ];

    for url in endpoints {
        let resp = client
            .get(url)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", auth.access_token),
            )
            .header("chatgpt-account-id", &auth.account_id)
            .header("originator", CODEX_ORIGINATOR)
            .header(header::ACCEPT, "application/json")
            .send()
            .await;

        if let Ok(response) = resp {
            let status = response.status();
            log::info!("[codex_subscription] rate limits endpoint {url} status: {status}");
            if status.is_success() {
                if let Ok(val) = response.json::<Value>().await {
                    log::info!("[codex_subscription] rate limits payload from {url}: {val}");
                    if let Some(parsed) = parse_rate_limits_payload(&val) {
                        return Ok(Some(parsed));
                    }
                }
            }
        }
    }

    Ok(None)
}

fn as_f64_lenient(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        let trimmed = s.trim().trim_end_matches('%').trim();
        return trimmed.parse::<f64>().ok();
    }
    None
}

fn as_u64_lenient(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(n) = v.as_f64() {
        return Some(n as u64);
    }
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        return trimmed.parse::<u64>().ok();
    }
    None
}

fn parse_reset_epoch(v: &Value) -> Option<f64> {
    if let Some(n) = as_f64_lenient(v) {
        if n > 10_000_000_000.0 {
            return Some(n / 1000.0);
        }
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp() as f64);
        }
    }
    None
}

fn parse_single_window(
    w_val: &Value,
    default_duration_mins: u64,
) -> Option<SubscriptionRateLimitWindow> {
    if !w_val.is_object() {
        return None;
    }

    let mut used_percent = w_val
        .get("used_percent")
        .or_else(|| w_val.get("usedPercent"))
        .or_else(|| w_val.get("used_percentage"))
        .or_else(|| w_val.get("usedPercentage"))
        .or_else(|| w_val.get("percentage"))
        .or_else(|| w_val.get("percent"))
        .or_else(|| w_val.get("utilization"))
        .and_then(as_f64_lenient);

    if used_percent.is_none() {
        if let Some(ratio) = w_val
            .get("used_ratio")
            .or_else(|| w_val.get("usedRatio"))
            .or_else(|| w_val.get("ratio"))
            .and_then(as_f64_lenient)
        {
            used_percent = Some(if ratio <= 1.0 && ratio > 0.0 {
                ratio * 100.0
            } else {
                ratio
            });
        }
    }

    if used_percent.is_none() {
        let used = w_val
            .get("used")
            .or_else(|| w_val.get("count"))
            .or_else(|| w_val.get("current"))
            .and_then(as_f64_lenient);
        let limit = w_val
            .get("limit")
            .or_else(|| w_val.get("max"))
            .or_else(|| w_val.get("total"))
            .and_then(as_f64_lenient);
        if let (Some(u), Some(l)) = (used, limit) {
            if l > 0.0 {
                used_percent = Some((u / l) * 100.0);
            }
        }
    }

    if used_percent.is_none() {
        if let Some(rem_pct) = w_val
            .get("remaining_percent")
            .or_else(|| w_val.get("remainingPercent"))
            .and_then(as_f64_lenient)
        {
            used_percent = Some((100.0 - rem_pct).max(0.0));
        }
    }

    let used_percent = used_percent.unwrap_or(0.0);

    let window_duration_mins = w_val
        .get("window_duration_mins")
        .or_else(|| w_val.get("windowDurationMins"))
        .or_else(|| w_val.get("duration_mins"))
        .or_else(|| w_val.get("durationMinutes"))
        .and_then(as_u64_lenient)
        .or_else(|| {
            w_val
                .get("window_duration_secs")
                .or_else(|| w_val.get("windowDurationSecs"))
                .or_else(|| w_val.get("duration_secs"))
                .or_else(|| w_val.get("durationSeconds"))
                .and_then(as_u64_lenient)
                .map(|secs| secs / 60)
        })
        .unwrap_or(default_duration_mins);

    let resets_at = w_val
        .get("resets_at")
        .or_else(|| w_val.get("resetsAt"))
        .or_else(|| w_val.get("reset_at"))
        .or_else(|| w_val.get("resetAt"))
        .or_else(|| w_val.get("reset_time"))
        .or_else(|| w_val.get("resetTime"))
        .and_then(parse_reset_epoch)
        .or_else(|| {
            w_val
                .get("resets_after")
                .or_else(|| w_val.get("resetsAfter"))
                .or_else(|| w_val.get("reset_after"))
                .and_then(as_f64_lenient)
                .map(|secs| now_epoch() as f64 + secs)
        });

    Some(SubscriptionRateLimitWindow {
        used_percent,
        window_duration_mins,
        resets_at,
    })
}

fn inspect_window_item<'a>(
    item: &'a Value,
    primary_val: &mut Option<&'a Value>,
    secondary_val: &mut Option<&'a Value>,
) {
    if !item.is_object() {
        return;
    }
    let name = item
        .get("limit_name")
        .or_else(|| item.get("name"))
        .or_else(|| item.get("type"))
        .or_else(|| item.get("window"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let duration_mins = item
        .get("window_duration_mins")
        .or_else(|| item.get("windowDurationMins"))
        .and_then(as_u64_lenient)
        .unwrap_or(0);

    if (name.eq_ignore_ascii_case("primary")
        || name.eq_ignore_ascii_case("primary_window")
        || name.eq_ignore_ascii_case("short_term")
        || duration_mins == 300)
        && primary_val.is_none()
    {
        *primary_val = Some(item);
    } else if (name.eq_ignore_ascii_case("secondary")
        || name.eq_ignore_ascii_case("secondary_window")
        || name.eq_ignore_ascii_case("long_term")
        || name.eq_ignore_ascii_case("weekly")
        || duration_mins >= 1440)
        && secondary_val.is_none()
    {
        *secondary_val = Some(item);
    }
}

fn find_windows_in_obj<'a>(
    obj: &'a Value,
    primary_val: &mut Option<&'a Value>,
    secondary_val: &mut Option<&'a Value>,
) {
    if !obj.is_object() {
        return;
    }
    if primary_val.is_none() {
        *primary_val = obj
            .get("primary_window")
            .or_else(|| obj.get("primaryWindow"))
            .or_else(|| obj.get("primary"))
            .or_else(|| obj.get("short_term"))
            .or_else(|| obj.get("shortTerm"))
            .or_else(|| obj.get("5h"))
            .or_else(|| obj.get("window_5h"));
    }
    if secondary_val.is_none() {
        *secondary_val = obj
            .get("secondary_window")
            .or_else(|| obj.get("secondaryWindow"))
            .or_else(|| obj.get("secondary"))
            .or_else(|| obj.get("long_term"))
            .or_else(|| obj.get("longTerm"))
            .or_else(|| obj.get("weekly"))
            .or_else(|| obj.get("window_weekly"))
            .or_else(|| obj.get("7d"));
    }

    if primary_val.is_none() || secondary_val.is_none() {
        if let Some(sub) = obj
            .get("rate_limit")
            .or_else(|| obj.get("rate_limits"))
            .or_else(|| obj.get("rateLimits"))
            .or_else(|| obj.get("rateLimit"))
            .or_else(|| obj.get("usage"))
            .or_else(|| obj.get("limits"))
            .or_else(|| obj.get("codex"))
            .or_else(|| obj.get("rate_limits_by_limit_id"))
        {
            if let Some(arr) = sub.as_array() {
                for item in arr {
                    inspect_window_item(item, primary_val, secondary_val);
                }
            } else if sub.is_object() {
                find_windows_in_obj(sub, primary_val, secondary_val);
            }
        }
    }
}

fn parse_rate_limits_payload(val: &Value) -> Option<SubscriptionRateLimits> {
    let mut primary_val: Option<&Value> = None;
    let mut secondary_val: Option<&Value> = None;

    if val.is_object() {
        find_windows_in_obj(val, &mut primary_val, &mut secondary_val);
    } else if let Some(arr) = val.as_array() {
        for item in arr {
            inspect_window_item(item, &mut primary_val, &mut secondary_val);
        }
    }

    let primary = primary_val.and_then(|v| parse_single_window(v, 300));
    let secondary = secondary_val.and_then(|v| parse_single_window(v, 10080));

    let plan_type = val
        .get("plan_type")
        .or_else(|| val.get("planType"))
        .or_else(|| {
            val.get("rate_limit")
                .or_else(|| val.get("rate_limits"))
                .and_then(|c| c.get("plan_type").or_else(|| c.get("planType")))
        })
        .and_then(Value::as_str)
        .map(str::to_string);

    let has_credits = val
        .get("credits")
        .or_else(|| {
            val.get("rate_limit")
                .or_else(|| val.get("rate_limits"))
                .and_then(|c| c.get("credits"))
        })
        .and_then(|c| c.get("has_credits").or_else(|| c.get("hasCredits")))
        .and_then(Value::as_bool);

    if primary.is_some() || secondary.is_some() || plan_type.is_some() {
        Some(SubscriptionRateLimits {
            primary,
            secondary,
            plan_type,
            has_credits,
            raw_json: Some(val.to_string()),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    #[test]
    fn recognizes_only_the_reserved_subscription_provider() {
        assert!(is_codex_subscription_id("openai-chatgpt-subscription"));
        assert!(!is_codex_subscription_id("openai-codex"));
    }

    #[test]
    fn builds_the_registered_codex_authorize_redirect() {
        let url = build_authorize_url("state-test", "challenge-test").unwrap();
        let parsed = Url::parse(&url).unwrap();
        let params = parsed.query_pairs().into_owned().collect::<HashMap<_, _>>();
        assert_eq!(parsed.as_str().split('?').next(), Some(AUTH_AUTHORIZE_URL));
        assert_eq!(
            params.get("client_id").map(String::as_str),
            Some(CODEX_CLIENT_ID)
        );
        assert_eq!(
            params.get("redirect_uri").map(String::as_str),
            Some(REDIRECT_URI)
        );
        assert_eq!(params.get("state").map(String::as_str), Some("state-test"));
        assert_eq!(
            params.get("code_challenge").map(String::as_str),
            Some("challenge-test")
        );
        assert_eq!(
            params.get("originator").map(String::as_str),
            Some("codex_cli_rs")
        );
    }

    #[test]
    fn removes_parameters_rejected_by_the_subscription_backend() {
        let sanitized = sanitize_request(json!({
            "model": "gpt-5.6-sol",
            "max_output_tokens": 1000,
            "temperature": 0.2,
            "top_p": 0.9,
            "input": []
        }));
        assert!(sanitized.get("max_output_tokens").is_none());
        assert!(sanitized.get("temperature").is_none());
        assert!(sanitized.get("top_p").is_none());
        assert_eq!(sanitized["store"], false);
        assert_eq!(sanitized["instructions"], DEFAULT_INSTRUCTIONS);
    }

    #[test]
    fn extracts_account_id_from_the_openai_auth_claim() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD
            .encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct-test"}}"#);
        let token = format!("{header}.{payload}.signature");
        assert_eq!(
            account_id_from_token(Some(&token)).as_deref(),
            Some("acct-test")
        );
    }

    #[test]
    fn returns_completed_response_from_sse() {
        let body = concat!(
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n",
            "\n",
        );
        assert_eq!(completed_response_from_sse(body).unwrap()["id"], "resp_1");
    }

    #[test]
    fn parses_rate_limits_payload_correctly() {
        let payload = json!({
            "rate_limits": {
                "primary": {
                    "used_percent": 18.5,
                    "window_duration_mins": 300,
                    "resets_at": 1711900000.0
                },
                "secondary": {
                    "used_percent": 42.0,
                    "window_duration_mins": 10080,
                    "resets_at": 1712400000.0
                },
                "plan_type": "plus"
            }
        });
        let parsed = parse_rate_limits_payload(&payload).expect("should parse");
        let primary = parsed.primary.expect("has primary");
        assert_eq!(primary.used_percent, 18.5);
        assert_eq!(primary.window_duration_mins, 300);
        assert_eq!(primary.resets_at, Some(1711900000.0));

        let secondary = parsed.secondary.expect("has secondary");
        assert_eq!(secondary.used_percent, 42.0);
        assert_eq!(secondary.window_duration_mins, 10080);
        assert_eq!(parsed.plan_type.as_deref(), Some("plus"));
    }

    #[test]
    fn parses_rate_limits_payload_wham_format() {
        let payload = json!({
            "plan_type": "team",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0.0,
                    "window_duration_mins": 300,
                    "resets_at": 1741829384.0
                },
                "secondary_window": {
                    "used_percent": 8.5,
                    "window_duration_mins": 10080,
                    "resets_at": 1742434184.0
                }
            }
        });
        let parsed = parse_rate_limits_payload(&payload).expect("should parse");
        assert_eq!(parsed.plan_type.as_deref(), Some("team"));
        let primary = parsed.primary.expect("has primary");
        assert_eq!(primary.used_percent, 0.0);
        assert_eq!(primary.window_duration_mins, 300);
        assert_eq!(primary.resets_at, Some(1741829384.0));

        let secondary = parsed.secondary.expect("has secondary");
        assert_eq!(secondary.used_percent, 8.5);
        assert_eq!(secondary.window_duration_mins, 10080);
        assert_eq!(secondary.resets_at, Some(1742434184.0));
    }

    #[test]
    fn parses_rate_limits_payload_array_format() {
        let payload = json!({
            "plan_type": "team",
            "rate_limits": [
                {
                    "limit_name": "primary",
                    "used_percent": 15.0,
                    "window_duration_mins": 300,
                    "resets_at": 1741829384.0
                },
                {
                    "limit_name": "secondary",
                    "used_percent": 25.0,
                    "window_duration_mins": 10080,
                    "resets_at": 1742434184.0
                }
            ]
        });
        let parsed = parse_rate_limits_payload(&payload).expect("should parse");
        assert_eq!(parsed.plan_type.as_deref(), Some("team"));
        let primary = parsed.primary.expect("has primary");
        assert_eq!(primary.used_percent, 15.0);
        let secondary = parsed.secondary.expect("has secondary");
        assert_eq!(secondary.used_percent, 25.0);
    }
}
