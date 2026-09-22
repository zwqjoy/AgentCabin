//! Isolated loopback bridge for using OpenAI Chat Completions providers with Codex.
//!
//! Codex only speaks the Responses wire protocol. This module deliberately owns the entire
//! compatibility surface: callers hand it a persisted `wire_api = "chat"` credential and get
//! back a short-lived Responses credential pointing at a token-protected loopback listener.

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use futures_util::{stream, StreamExt};
use once_cell::sync::Lazy;
use rand::distributions::{Alphanumeric, DistString};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use va_ai_api_bridge::{
    DecodeState, EncodeState, OpenAiChatTranslator, OpenAiResponsesTranslator,
    ProviderBridgeAdapter, ProviderBridgeAdapterConfig, ProviderRequestSource, ToolChoice,
    UniversalEvent, WireEvent, WireProtocol, WireTranslator,
};

use crate::models::CodexProviderCredential;

const LOCAL_ENV_KEY: &str = "AGENTCABIN_CODEX_BRIDGE_TOKEN";

#[derive(Clone)]
struct BridgeEndpoint {
    base_url: String,
    token: String,
}

struct BridgeState {
    upstream_url: String,
    upstream_api_key: Option<String>,
    provider_id: String,
    supports_reasoning_effort: bool,
    local_token: String,
    client: reqwest::Client,
}

static BRIDGES: Lazy<Mutex<HashMap<String, BridgeEndpoint>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Prepare a provider for Codex without changing the persisted provider configuration.
pub async fn prepare_provider(
    provider: &CodexProviderCredential,
    app_cancel: &CancellationToken,
) -> Result<CodexProviderCredential, String> {
    if provider.wire_api != "chat" {
        return Ok(provider.clone());
    }

    let fingerprint = provider_fingerprint(provider);
    let endpoint = {
        let mut bridges = BRIDGES.lock().await;
        if let Some(endpoint) = bridges.get(&fingerprint) {
            endpoint.clone()
        } else {
            let endpoint = start_bridge(provider, app_cancel.clone()).await?;
            bridges.insert(fingerprint, endpoint.clone());
            endpoint
        }
    };

    let mut prepared = provider.clone();
    prepared.base_url = endpoint.base_url;
    prepared.env_key = LOCAL_ENV_KEY.to_string();
    prepared.api_key = Some(endpoint.token);
    prepared.wire_api = "responses".to_string();
    prepared.supports_websockets = Some(false);
    Ok(prepared)
}

fn provider_fingerprint(provider: &CodexProviderCredential) -> String {
    let mut hasher = Sha256::new();
    hasher.update(provider.id.as_bytes());
    hasher.update([0]);
    hasher.update(provider.name.as_bytes());
    hasher.update([0]);
    hasher.update(provider.base_url.as_bytes());
    hasher.update([0]);
    hasher.update([u8::from(
        provider.supports_reasoning_effort.unwrap_or(false),
    )]);
    hasher.update([0]);
    if let Some(key) = &provider.api_key {
        hasher.update(key.as_bytes());
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

async fn start_bridge(
    provider: &CodexProviderCredential,
    app_cancel: CancellationToken,
) -> Result<BridgeEndpoint, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("Cannot start Codex Chat bridge: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("Cannot read Codex Chat bridge address: {error}"))?;
    let token = Alphanumeric.sample_string(&mut rand::thread_rng(), 48);
    let upstream_url = chat_completions_url(&provider.base_url);
    let state = Arc::new(BridgeState {
        upstream_url,
        upstream_api_key: provider.api_key.clone(),
        provider_id: normalized_provider_id(provider),
        supports_reasoning_effort: provider.supports_reasoning_effort.unwrap_or(false),
        local_token: token.clone(),
        client: reqwest::Client::new(),
    });
    let router = Router::new()
        .route("/v1/responses", post(handle_responses))
        .route("/responses", post(handle_responses))
        .with_state(state);

    tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(app_cancel.cancelled_owned())
            .await;
        if let Err(error) = result {
            log::warn!("[codex-chat-bridge] loopback server stopped: {error}");
        }
    });

    log::info!(
        "[codex-chat-bridge] started provider={} address={}",
        provider.id,
        address
    );
    Ok(BridgeEndpoint {
        base_url: format!("http://{address}/v1"),
        token,
    })
}

fn normalized_provider_id(provider: &CodexProviderCredential) -> String {
    let haystack = format!(
        "{} {} {}",
        provider.id.to_lowercase(),
        provider.name.to_lowercase(),
        provider.base_url.to_lowercase()
    );
    for id in [
        "deepseek",
        "kimi",
        "mimo",
        "minimax",
        "dashscope",
        "qwen",
        "xai",
        "zai",
    ] {
        if haystack.contains(id) {
            return id.to_string();
        }
    }
    provider.id.clone()
}

fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

async fn handle_responses(
    State(state): State<Arc<BridgeState>>,
    headers: HeaderMap,
    Json(original): Json<Value>,
) -> Response {
    if !is_authorized(&headers, &state.local_token) {
        return bridge_error(StatusCode::UNAUTHORIZED, "Invalid local bridge token");
    }

    let plan = match translate_request(
        &original,
        &state.provider_id,
        state.supports_reasoning_effort,
    ) {
        Ok(plan) => plan,
        Err(error) => return bridge_error(StatusCode::BAD_REQUEST, &error),
    };
    let wants_stream = plan
        .body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut request = state.client.post(&state.upstream_url).json(&plan.body);
    if let Some(key) = state
        .upstream_api_key
        .as_deref()
        .filter(|key| !key.is_empty())
    {
        request = request.bearer_auth(key);
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return bridge_error(
                StatusCode::BAD_GATEWAY,
                &format!("Chat Completions upstream request failed: {error}"),
            )
        }
    };
    if !upstream.status().is_success() {
        let status = upstream.status();
        let detail = upstream
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(2000)
            .collect::<String>();
        return bridge_error(
            StatusCode::BAD_GATEWAY,
            &format!("Chat Completions upstream returned {status}: {detail}"),
        );
    }

    if wants_stream {
        streaming_response(upstream, plan.custom_tools, plan.adapter)
    } else {
        non_streaming_response(upstream, plan.custom_tools, plan.adapter).await
    }
}

fn is_authorized(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        == Some(expected)
}

struct RequestPlan {
    body: Value,
    custom_tools: HashSet<String>,
    adapter: ProviderBridgeAdapter,
}

fn translate_request(
    original: &Value,
    provider_id: &str,
    supports_reasoning_effort: bool,
) -> Result<RequestPlan, String> {
    let (normalized, custom_tools) = normalize_custom_tools(original.clone());
    let mut universal = OpenAiResponsesTranslator
        .decode_request(normalized)
        .map_err(|error| format!("Cannot decode Codex Responses request: {error}"))?;
    if !universal.server_tools.is_empty() {
        // Chat Completions has no representation for Responses-hosted tools such as web_search.
        // Codex can add one automatically from model metadata, so rejecting the whole request
        // would make even an ordinary "hello" fail. Drop only the hosted declarations; local
        // function/custom tools remain available through the normal Chat tools field.
        universal.server_tools.clear();
        if matches!(universal.tool_choice, Some(ToolChoice::ServerTool { .. }))
            || (universal.tools.is_empty()
                && matches!(universal.tool_choice, Some(ToolChoice::Required)))
        {
            universal.tool_choice = if universal.tools.is_empty() {
                None
            } else {
                Some(ToolChoice::Auto)
            };
        }
    }
    let mut body = OpenAiChatTranslator
        .encode_request(&universal)
        .map_err(|error| format!("Cannot encode Chat Completions request: {error}"))?;
    if universal.stream {
        body["stream_options"] = json!({ "include_usage": true });
    }
    if supports_reasoning_effort {
        if let Some(effort) = original
            .pointer("/reasoning/effort")
            .and_then(Value::as_str)
        {
            body["reasoning_effort"] = Value::String(effort.to_string());
        }
    }
    let mut adapter = ProviderBridgeAdapter::for_provider(
        provider_id,
        WireProtocol::OpenAiChat,
        ProviderBridgeAdapterConfig::default(),
    );
    adapter.prepare_chat_request(ProviderRequestSource::OpenAiResponses, original, &mut body);
    normalize_chat_system_messages(&mut body);
    Ok(RequestPlan {
        body,
        custom_tools,
        adapter,
    })
}

/// Responses histories can carry system/developer items inside `input` after user messages.
/// Qwen-compatible chat templates require a single system message at the beginning, so merge
/// all system/developer content into one leading Chat Completions message.
fn normalize_chat_system_messages(body: &mut Value) {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };

    let mut system_messages = Vec::new();
    let mut conversation_messages = Vec::with_capacity(messages.len());
    for message in std::mem::take(messages) {
        let is_system = matches!(
            message.get("role").and_then(Value::as_str),
            Some("system" | "developer")
        );
        if is_system {
            system_messages.push(message);
        } else {
            conversation_messages.push(message);
        }
    }
    let mut normalized_messages = Vec::with_capacity(messages.len());
    if !system_messages.is_empty() {
        normalized_messages.push(merge_chat_system_messages(system_messages));
    }
    normalized_messages.extend(conversation_messages);
    *messages = normalized_messages;
}

fn merge_chat_system_messages(system_messages: Vec<Value>) -> Value {
    let mut text_parts = Vec::new();
    let mut structured_parts = Vec::new();
    let mut has_structured_content = false;

    for message in system_messages {
        let Some(content) = message.get("content") else {
            continue;
        };
        match content {
            Value::String(text) => {
                text_parts.push(text.clone());
                structured_parts.push(json!({"type": "text", "text": text}));
            }
            Value::Array(parts) => {
                has_structured_content = true;
                structured_parts.extend(parts.iter().cloned());
            }
            Value::Null => {}
            other => {
                has_structured_content = true;
                structured_parts.push(other.clone());
            }
        }
    }

    let content = if has_structured_content {
        Value::Array(structured_parts)
    } else {
        Value::String(text_parts.join("\n\n"))
    };
    json!({"role": "system", "content": content})
}

fn normalize_custom_tools(mut request: Value) -> (Value, HashSet<String>) {
    let mut names = HashSet::new();
    if let Some(tools) = request.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if tool.get("type").and_then(Value::as_str) != Some("custom") {
                continue;
            }
            let Some(name) = tool.get("name").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            names.insert(name.clone());
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Freeform tool input");
            *tool = json!({
                "type": "function",
                "name": name,
                "description": description,
                "parameters": {
                    "type": "object",
                    "properties": { "input": { "type": "string" } },
                    "required": ["input"],
                    "additionalProperties": false
                },
                "strict": false
            });
        }
    }
    if let Some(input) = request.get_mut("input").and_then(Value::as_array_mut) {
        for item in input {
            match item.get("type").and_then(Value::as_str) {
                Some("custom_tool_call") => {
                    if let Some(object) = item.as_object_mut() {
                        let raw = object
                            .remove("input")
                            .unwrap_or(Value::String(String::new()));
                        object.insert("type".into(), Value::String("function_call".into()));
                        object.insert(
                            "arguments".into(),
                            Value::String(json!({ "input": raw }).to_string()),
                        );
                    }
                }
                Some("custom_tool_call_output") => {
                    item["type"] = Value::String("function_call_output".into());
                }
                _ => {}
            }
        }
    }
    (request, names)
}

async fn non_streaming_response(
    upstream: reqwest::Response,
    custom_tools: HashSet<String>,
    mut adapter: ProviderBridgeAdapter,
) -> Response {
    let mut raw = match upstream.json::<Value>().await {
        Ok(value) => value,
        Err(error) => {
            return bridge_error(
                StatusCode::BAD_GATEWAY,
                &format!("Invalid Chat Completions response: {error}"),
            )
        }
    };
    adapter.normalize_chat_response(&mut raw);
    let usage_details = normalize_chat_usage_aliases(&mut raw);
    let mut events = match OpenAiChatTranslator.decode_response(raw) {
        Ok(events) => events,
        Err(error) => {
            return bridge_error(
                StatusCode::BAD_GATEWAY,
                &format!("Cannot decode Chat Completions response: {error}"),
            )
        }
    };
    adapter.transform_upstream_events(&mut events);
    let mut encode_state = EncodeState::default();
    let mut wire = match OpenAiResponsesTranslator.encode_events(&events, &mut encode_state) {
        Ok(events) => events,
        Err(error) => return bridge_error(StatusCode::BAD_GATEWAY, &error.to_string()),
    };
    inject_responses_usage_details(&mut wire, usage_details);
    let mut custom_state = CustomToolState::new(custom_tools);
    let wire = custom_state.transform(wire);
    let response = wire
        .into_iter()
        .find(|event| event.data.get("type").and_then(Value::as_str) == Some("response.completed"))
        .and_then(|event| event.data.get("response").cloned());
    match response {
        Some(response) => Json(response).into_response(),
        None => bridge_error(
            StatusCode::BAD_GATEWAY,
            "Bridge produced no completed response",
        ),
    }
}

fn streaming_response(
    upstream: reqwest::Response,
    custom_tools: HashSet<String>,
    mut adapter: ProviderBridgeAdapter,
) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(32);
    tokio::spawn(async move {
        let mut chunks = upstream.bytes_stream();
        let mut sse = SseDecoder::default();
        let mut decode_state = DecodeState::default();
        let mut encode_state = EncodeState::default();
        let mut custom_state = CustomToolState::new(custom_tools);
        let mut completed = false;

        while let Some(chunk) = chunks.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = tx.send(Ok(failed_sse(&error.to_string()))).await;
                    return;
                }
            };
            for data in sse.push(&chunk) {
                if data == "[DONE]" {
                    continue;
                }
                let mut raw: Value = match serde_json::from_str(&data) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = tx
                            .send(Ok(failed_sse(&format!("Invalid upstream SSE: {error}"))))
                            .await;
                        return;
                    }
                };
                let usage_details = normalize_chat_usage_aliases(&mut raw);
                let mut events =
                    match OpenAiChatTranslator.decode_stream_chunk(raw, &mut decode_state) {
                        Ok(events) => events,
                        Err(error) => {
                            let _ = tx.send(Ok(failed_sse(&error.to_string()))).await;
                            return;
                        }
                    };
                adapter.transform_upstream_events(&mut events);
                defer_usage_less_response_done(&mut events, &mut decode_state);
                let wire = match OpenAiResponsesTranslator.encode_events(&events, &mut encode_state)
                {
                    Ok(mut events) => {
                        inject_responses_usage_details(&mut events, usage_details);
                        custom_state.transform(events)
                    }
                    Err(error) => {
                        let _ = tx.send(Ok(failed_sse(&error.to_string()))).await;
                        return;
                    }
                };
                for event in wire {
                    completed |= event.data.get("type").and_then(Value::as_str)
                        == Some("response.completed");
                    if tx.send(Ok(wire_sse(&event))).await.is_err() {
                        return;
                    }
                }
            }
        }
        if !completed {
            let done = UniversalEvent::ResponseDone {
                usage: None,
                extensions: Default::default(),
            };
            if let Ok(wire) = OpenAiResponsesTranslator.encode_events(&[done], &mut encode_state) {
                for event in custom_state.transform(wire) {
                    if tx.send(Ok(wire_sse(&event))).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    let body_stream = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(body_stream))
        .expect("valid bridge streaming response")
}

fn defer_usage_less_response_done(events: &mut Vec<UniversalEvent>, state: &mut DecodeState) {
    let should_defer = events
        .iter()
        .any(|event| matches!(event, UniversalEvent::ResponseDone { usage: None, .. }));
    if !should_defer {
        return;
    }

    events.retain(|event| !matches!(event, UniversalEvent::ResponseDone { usage: None, .. }));
    // OpenAI-compatible streams normally emit finish_reason first and a separate usage-only
    // chunk afterwards when stream_options.include_usage is enabled. The upstream decoder marks
    // the response complete on finish_reason, which would suppress that later usage chunk.
    state.extensions.remove("response_done");
}

#[derive(Clone, Copy, Default)]
struct ChatUsageDetails {
    cached_input_tokens: Option<u64>,
}

fn normalize_chat_usage_aliases(raw: &mut Value) -> ChatUsageDetails {
    let Some(usage) = raw.get_mut("usage").and_then(Value::as_object_mut) else {
        return ChatUsageDetails::default();
    };

    let cached_input_tokens = chat_cached_input_tokens(usage).filter(|tokens| *tokens > 0);
    let explicit_zero_input = usage.get("input_tokens").and_then(Value::as_u64) == Some(0);
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    // NewAPI's GLM adapter reports only the uncached portion in `prompt_tokens` and puts the
    // cached prefix in `prompt_tokens_details.cached_tokens`. Its non-standard `input_tokens: 0`
    // distinguishes this from OpenAI-compatible responses where `prompt_tokens` already includes
    // cached tokens. Reconstruct the standard Responses total for Codex in that case.
    if explicit_zero_input {
        let input_tokens = prompt_tokens.saturating_add(cached_input_tokens.unwrap_or(0));
        if input_tokens > 0 {
            usage.insert("input_tokens".into(), Value::Number(input_tokens.into()));
        }

        if let Some(cached) = cached_input_tokens {
            let completion_tokens = usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let uncached_total = prompt_tokens.saturating_add(completion_tokens);
            let reported_total = usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if reported_total <= uncached_total {
                usage.insert(
                    "total_tokens".into(),
                    Value::Number(reported_total.saturating_add(cached).into()),
                );
            }
        }
    } else {
        prefer_positive_usage_alias(usage, "input_tokens", "prompt_tokens");
    }
    prefer_positive_usage_alias(usage, "output_tokens", "completion_tokens");

    ChatUsageDetails {
        cached_input_tokens,
    }
}

fn chat_cached_input_tokens(usage: &serde_json::Map<String, Value>) -> Option<u64> {
    usage
        .get("input_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
                .and_then(Value::as_u64)
        })
        .or_else(|| usage.get("cached_input_tokens").and_then(Value::as_u64))
        .or_else(|| usage.get("cache_read_input_tokens").and_then(Value::as_u64))
        .or_else(|| usage.get("cache_read_tokens").and_then(Value::as_u64))
}

fn inject_responses_usage_details(events: &mut [WireEvent], details: ChatUsageDetails) {
    let Some(cached_tokens) = details.cached_input_tokens else {
        return;
    };
    for event in events {
        if event.data.get("type").and_then(Value::as_str) != Some("response.completed") {
            continue;
        }
        let Some(usage) = event
            .data
            .get_mut("response")
            .and_then(|response| response.get_mut("usage"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        usage.insert(
            "input_tokens_details".into(),
            json!({ "cached_tokens": cached_tokens }),
        );
    }
}

fn prefer_positive_usage_alias(
    usage: &mut serde_json::Map<String, Value>,
    primary: &str,
    fallback: &str,
) {
    if usage.get(primary).and_then(Value::as_u64).unwrap_or(0) > 0 {
        return;
    }
    let Some(tokens) = usage
        .get(fallback)
        .and_then(Value::as_u64)
        .filter(|tokens| *tokens > 0)
    else {
        return;
    };
    usage.insert(primary.to_string(), Value::Number(tokens.into()));
}

struct CustomToolState {
    names: HashSet<String>,
    item_names: HashMap<String, String>,
    arguments: HashMap<String, String>,
}

impl CustomToolState {
    fn new(names: HashSet<String>) -> Self {
        Self {
            names,
            item_names: HashMap::new(),
            arguments: HashMap::new(),
        }
    }

    fn transform(&mut self, events: Vec<WireEvent>) -> Vec<WireEvent> {
        let mut output = Vec::new();
        for mut event in events {
            let kind = event
                .data
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if kind == "response.output_item.added" {
                if let Some(item) = event.data.get_mut("item") {
                    if self.convert_item(item) {
                        if let (Some(id), Some(name)) = (
                            item.get("id").and_then(Value::as_str),
                            item.get("name").and_then(Value::as_str),
                        ) {
                            self.item_names.insert(id.to_string(), name.to_string());
                        }
                    }
                }
                output.push(event);
                continue;
            }
            if kind == "response.function_call_arguments.delta" {
                let id = event
                    .data
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if self.item_names.contains_key(id) {
                    let delta = event
                        .data
                        .get("delta")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    self.arguments
                        .entry(id.to_string())
                        .or_default()
                        .push_str(delta);
                    continue;
                }
            }
            if kind == "response.function_call_arguments.done" {
                let id = event
                    .data
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if self.item_names.contains_key(id) {
                    let arguments = event
                        .data
                        .get("arguments")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| self.arguments.remove(id))
                        .unwrap_or_default();
                    let input = unwrap_custom_input(&arguments);
                    let output_index = event.data.get("output_index").cloned().unwrap_or(json!(0));
                    output.push(WireEvent {
                        event: None,
                        data: json!({
                            "type": "response.custom_tool_call_input.delta",
                            "item_id": id,
                            "output_index": output_index,
                            "delta": input
                        }),
                    });
                    output.push(WireEvent {
                        event: None,
                        data: json!({
                            "type": "response.custom_tool_call_input.done",
                            "item_id": id,
                            "output_index": output_index,
                            "input": input
                        }),
                    });
                    continue;
                }
            }
            if kind == "response.output_item.done" {
                if let Some(item) = event.data.get_mut("item") {
                    self.convert_item(item);
                }
            } else if kind == "response.completed" {
                if let Some(items) = event
                    .data
                    .pointer_mut("/response/output")
                    .and_then(Value::as_array_mut)
                {
                    for item in items {
                        self.convert_item(item);
                    }
                }
            }
            output.push(event);
        }
        output
    }

    fn convert_item(&self, item: &mut Value) -> bool {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return false;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            return false;
        };
        if !self.names.contains(name) {
            return false;
        }
        let input = item
            .get("arguments")
            .and_then(Value::as_str)
            .map(unwrap_custom_input)
            .unwrap_or_default();
        if let Some(object) = item.as_object_mut() {
            object.remove("arguments");
            object.insert("type".into(), Value::String("custom_tool_call".into()));
            object.insert("input".into(), Value::String(input));
        }
        true
    }
}

fn unwrap_custom_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| value.get("input").cloned())
        .and_then(|value| match value {
            Value::String(value) => Some(value),
            other if !other.is_null() => Some(other.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| arguments.to_string())
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((index, delimiter_len)) = find_sse_delimiter(&self.buffer) {
            let block = self.buffer.drain(..index).collect::<Vec<_>>();
            self.buffer.drain(..delimiter_len);
            let text = String::from_utf8_lossy(&block).replace("\r\n", "\n");
            let data = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                frames.push(data);
            }
        }
        frames
    }
}

fn find_sse_delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    match (crlf, lf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(found), None) | (None, Some(found)) => Some(found),
        (None, None) => None,
    }
}

fn wire_sse(event: &WireEvent) -> Bytes {
    let kind = event
        .event
        .as_deref()
        .or_else(|| event.data.get("type").and_then(Value::as_str));
    let data = serde_json::to_string(&event.data).unwrap_or_else(|_| "{}".to_string());
    match kind {
        Some(kind) => Bytes::from(format!("event: {kind}\ndata: {data}\n\n")),
        None => Bytes::from(format!("data: {data}\n\n")),
    }
}

fn failed_sse(message: &str) -> Bytes {
    let event = WireEvent {
        event: None,
        data: json!({
            "type": "response.failed",
            "error": { "type": "bridge_error", "message": message }
        }),
    };
    wire_sse(&event)
}

fn bridge_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": { "type": "bridge_error", "message": message }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mock_chat_completions(
        State(received): State<Arc<Mutex<Option<Value>>>>,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        *received.lock().await = Some(body);
        let sse = concat!(
            "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
            "\"model\":\"glm-5.2\",\"choices\":[{\"index\":0,",
            "\"delta\":{\"role\":\"assistant\",\"content\":\"你好\"},",
            "\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
            "\"model\":\"glm-5.2\",\"choices\":[{\"index\":0,",
            "\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
            "\"model\":\"glm-5.2\",\"choices\":[],",
            "\"usage\":{\"prompt_tokens\":57,\"completion_tokens\":3,",
            "\"total_tokens\":60,\"input_tokens\":0,\"output_tokens\":0,",
            "\"prompt_tokens_details\":{\"cached_tokens\":4160}}}\n\n",
            "data: [DONE]\n\n"
        );
        ([(header::CONTENT_TYPE, "text/event-stream")], sse)
    }

    #[test]
    fn translates_responses_request_to_chat() {
        let original = json!({
            "model": "test-model",
            "instructions": "Be concise",
            "input": [{"role":"user", "content":"hello"}],
            "reasoning": {"effort":"high"},
            "stream": true,
            "tools": [{
                "type": "function",
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type":"object"}
            }]
        });
        let plan = translate_request(&original, "custom", false).unwrap();
        assert_eq!(plan.body["model"], "test-model");
        assert_eq!(plan.body["messages"][0]["role"], "system");
        assert_eq!(plan.body["messages"][1]["role"], "user");
        assert_eq!(plan.body["stream_options"]["include_usage"], true);
        assert_eq!(plan.body["tools"][0]["type"], "function");
        assert!(plan.body.get("reasoning_effort").is_none());
        let reasoning_plan = translate_request(&original, "custom", true).unwrap();
        assert_eq!(reasoning_plan.body["reasoning_effort"], "high");
    }

    #[test]
    fn merges_embedded_system_messages_into_one_leading_message() {
        let original = json!({
            "model": "test-model",
            "instructions": "Top-level instruction",
            "input": [
                {"role":"user", "content":"hello"},
                {"role":"system", "content":"Global instruction"},
                {"role":"developer", "content":"Developer instruction"}
            ]
        });

        let plan = translate_request(&original, "custom", false).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(
            messages[0]["content"],
            "Top-level instruction\n\nGlobal instruction\n\nDeveloper instruction"
        );
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn drops_hosted_tools_but_keeps_function_tools() {
        let original = json!({
            "model": "DeepSeek-V4-Flash",
            "input": "hello",
            "tools": [
                {"type":"web_search_preview"},
                {
                    "type":"function",
                    "name":"read_file",
                    "description":"Read a file",
                    "parameters":{"type":"object"}
                }
            ],
            "tool_choice": "auto"
        });

        let plan = translate_request(&original, "custom", false).unwrap();

        assert_eq!(plan.body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(plan.body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(plan.body["tool_choice"], "auto");
    }

    #[test]
    fn custom_tools_round_trip_as_custom_response_items() {
        let original = json!({
            "model": "test-model",
            "input": "patch it",
            "tools": [{"type":"custom", "name":"apply_patch", "description":"patch"}]
        });
        let plan = translate_request(&original, "custom", false).unwrap();
        assert_eq!(plan.body["tools"][0]["function"]["name"], "apply_patch");

        let mut state = CustomToolState::new(HashSet::from(["apply_patch".to_string()]));
        let events = state.transform(vec![WireEvent {
            event: None,
            data: json!({
                "type":"response.output_item.done",
                "item":{"type":"function_call", "id":"fc_1", "call_id":"call_1",
                    "name":"apply_patch", "arguments":"{\"input\":\"*** Begin Patch\"}"}
            }),
        }]);
        assert_eq!(events[0].data["item"]["type"], "custom_tool_call");
        assert_eq!(events[0].data["item"]["input"], "*** Begin Patch");
    }

    #[test]
    fn parses_split_sse_frames() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"data: {\"a\":").is_empty());
        assert_eq!(
            decoder.push(b"1}\n\ndata: [DONE]\r\n\r\n"),
            vec!["{\"a\":1}", "[DONE]"]
        );
    }

    #[test]
    fn completion_url_is_not_duplicated() {
        assert_eq!(
            chat_completions_url("https://example.com/v1"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn prepares_a_token_protected_loopback_responses_provider() {
        let provider = CodexProviderCredential {
            id: "test-chat".into(),
            name: "Test Chat".into(),
            base_url: "https://example.test/v1".into(),
            env_key: "UPSTREAM_API_KEY".into(),
            wire_api: "chat".into(),
            model: "test-model".into(),
            api_key: Some("upstream-secret".into()),
            supports_reasoning_effort: None,
            supports_websockets: None,
        };
        let cancel = CancellationToken::new();
        let prepared = prepare_provider(&provider, &cancel).await.unwrap();
        assert!(prepared.base_url.starts_with("http://127.0.0.1:"));
        assert!(prepared.base_url.ends_with("/v1"));
        assert_eq!(prepared.wire_api, "responses");
        assert_eq!(prepared.env_key, LOCAL_ENV_KEY);
        assert_ne!(prepared.api_key, provider.api_key);
        assert_eq!(prepared.supports_websockets, Some(false));
        cancel.cancel();
    }

    #[tokio::test]
    async fn streams_a_complete_response_through_the_loopback_bridge() {
        let received = Arc::new(Mutex::new(None));
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream_listener.local_addr().unwrap();
        let upstream_router = Router::new()
            .route("/v1/chat/completions", post(mock_chat_completions))
            .with_state(Arc::clone(&received));
        let upstream_task = tokio::spawn(async move {
            axum::serve(upstream_listener, upstream_router)
                .await
                .unwrap();
        });

        let provider = CodexProviderCredential {
            id: "mock-newapi-chat".into(),
            name: "Mock NewAPI".into(),
            base_url: format!("http://{upstream_address}/v1"),
            env_key: "UPSTREAM_API_KEY".into(),
            wire_api: "chat".into(),
            model: "glm-5.2".into(),
            api_key: Some("upstream-secret".into()),
            supports_reasoning_effort: Some(true),
            supports_websockets: None,
        };
        let cancel = CancellationToken::new();
        let prepared = prepare_provider(&provider, &cancel).await.unwrap();
        let local_token = prepared.api_key.as_deref().unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            reqwest::Client::new()
                .post(format!("{}/responses", prepared.base_url))
                .bearer_auth(local_token)
                .json(&json!({
                    "model": "glm-5.2",
                    "input": "你好",
                    "stream": true,
                    "tools": [{"type":"web_search_preview"}],
                    "tool_choice": {"type":"web_search_preview"}
                }))
                .send(),
        )
        .await
        .expect("bridge response timed out")
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response_text = response.text().await.unwrap();
        assert!(response_text.contains("response.output_text.delta"));
        assert!(response_text.contains("你好"));
        assert!(response_text.contains("response.completed"));
        assert!(response_text.contains("\"input_tokens\":4217"));
        assert!(response_text.contains("\"output_tokens\":3"));
        assert!(response_text.contains("\"total_tokens\":4220"));
        assert!(response_text.contains("\"input_tokens_details\":{\"cached_tokens\":4160}"));

        let upstream_request = received.lock().await.clone().unwrap();
        assert_eq!(upstream_request["model"], "glm-5.2");
        assert_eq!(upstream_request["stream"], true);
        assert_eq!(upstream_request["stream_options"]["include_usage"], true);
        assert!(upstream_request.get("tools").is_none());
        assert!(upstream_request.get("tool_choice").is_none());

        cancel.cancel();
        upstream_task.abort();
    }
}
