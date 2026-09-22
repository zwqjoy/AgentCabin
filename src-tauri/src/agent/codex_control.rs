//! Live Codex model catalog via `codex app-server`.
//!
//! Codex's `exec` path has no control protocol (unlike Claude's stream-json), so we
//! cannot enumerate models the way `control::get_cli_info` does. Instead we drive the
//! experimental `codex app-server` JSON-RPC over stdio: one `initialize` request, one
//! `model/list` request, read the response, then kill the process. Same spawn→write→
//! read→kill→TTL-cache shape as `control::get_cli_info`.

use crate::agent::claude_stream::{augmented_path, which_binary};
use crate::models::{CliInfoError, CliModelInfo, CodexModelList};
use crate::process_ext::HideConsole;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::time::timeout;

const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);

// Codex currently exposes a 272K model window and uses 95% of it as the
// effective conversation budget in the first-party client (258.4K). The
// app-server `model/list` schema does not include this metadata, so preserve
// the known current GPT-5.6 subscription values when the catalog omits it.
const CODEX_MODEL_CONTEXT_WINDOW: u64 = 272_000;
const CODEX_EFFECTIVE_CONTEXT_PERCENT: u64 = 95;
const CODEX_EFFECTIVE_CONTEXT_WINDOW: u64 =
    CODEX_MODEL_CONTEXT_WINDOW * CODEX_EFFECTIVE_CONTEXT_PERCENT / 100;

pub(crate) fn known_effective_context_window(model: &str) -> Option<u64> {
    let model = model
        .strip_prefix("openai-codex/")
        .unwrap_or(model)
        .trim()
        .to_ascii_lowercase();
    matches!(
        model.as_str(),
        "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna"
    )
    .then_some(CODEX_EFFECTIVE_CONTEXT_WINDOW)
}

/// Cached Codex model catalog with TTL. Mirrors `control::CliInfoCache`.
#[derive(Clone, Default)]
pub struct CodexInfoCache {
    inner: Arc<RwLock<Option<(CodexModelList, Instant)>>>,
}

impl CodexInfoCache {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Curated fallback when app-server is unavailable (old Codex without `model/list`, not
/// installed, or timeout). These are the same subscription models exposed by the ChatGPT
/// OAuth provider, so a global subscription Provider still gets a useful model list even when
/// native Codex CLI discovery is unavailable.
pub fn fallback_models() -> CodexModelList {
    let mk = |value: &str, name: &str, desc: &str| CliModelInfo {
        value: value.to_string(),
        display_name: name.to_string(),
        description: desc.to_string(),
        context_window: known_effective_context_window(value),
        supports_effort: None,
        supported_effort_levels: None,
        supports_adaptive_thinking: None,
    };
    CodexModelList {
        models: vec![
            mk("gpt-5.6-sol", "GPT-5.6 Sol", "Frontier coding model"),
            mk("gpt-5.6-terra", "GPT-5.6 Terra", "Balanced coding model"),
            mk("gpt-5.6-luna", "GPT-5.6 Luna", "Fast coding model"),
            mk("gpt-5.2-codex", "GPT-5.2 Codex", "Coding model"),
            mk("gpt-5.2", "GPT-5.2", "General model"),
            mk("gpt-5.1-codex", "GPT-5.1 Codex", "Coding model"),
            mk(
                "gpt-5.1-codex-mini",
                "GPT-5.1 Codex Mini",
                "Compact coding model",
            ),
        ],
        default_model: Some("gpt-5.6-sol".to_string()),
    }
}

/// Get the Codex model catalog, using cache if fresh. On any failure the caller
/// (Tauri command) substitutes `fallback_models()` — errors are returned, not swallowed.
pub async fn get_codex_models(
    cache: &CodexInfoCache,
    force: bool,
) -> Result<CodexModelList, CliInfoError> {
    if !force {
        let guard = cache.inner.read().await;
        if let Some((ref list, ref instant)) = *guard {
            if instant.elapsed() < CACHE_TTL {
                log::debug!(
                    "[codex_control] returning cached models ({})",
                    list.models.len()
                );
                return Ok(list.clone());
            }
        }
    }

    if which_binary("codex").is_none() {
        return Err(CliInfoError {
            code: "cli_not_found".to_string(),
            message: "Codex CLI binary not found".to_string(),
        });
    }

    let path_env = augmented_path();
    let mut cmd = tokio::process::Command::new("codex");
    cmd.arg("app-server")
        .env("PATH", &path_env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .hide_console()
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| {
        log::error!("[codex_control] failed to spawn codex app-server: {}", e);
        CliInfoError {
            code: "cli_not_found".to_string(),
            message: format!("Failed to spawn codex app-server: {}", e),
        }
    })?;

    log::debug!(
        "[codex_control] spawned codex app-server pid={:?}",
        child.id()
    );

    let mut stdin = child.stdin.take().ok_or_else(|| CliInfoError {
        code: "protocol_error".to_string(),
        message: "Failed to capture stdin".to_string(),
    })?;

    // initialize (id 1) then model/list (id 2). app-server emits interleaved
    // notifications (e.g. remoteControl/status/changed) — the reader filters by id.
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "agentcabin",
                "version": env!("CARGO_PKG_VERSION"),
                "title": "AgentCabin"
            }
        }
    });
    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "model/list",
        "params": {}
    });

    let mut payload = serde_json::to_string(&init).unwrap_or_default();
    payload.push('\n');
    payload.push_str(&serde_json::to_string(&list_req).unwrap_or_default());
    payload.push('\n');

    stdin
        .write_all(payload.as_bytes())
        .await
        .map_err(|e| CliInfoError {
            code: "protocol_error".to_string(),
            message: format!("Failed to write to stdin: {}", e),
        })?;
    if let Err(e) = stdin.flush().await {
        log::warn!("[codex_control] stdin flush failed: {}", e);
    }
    // Keep stdin OPEN until after reading: closing it (EOF) makes app-server shut down
    // right after the initialize reply, before it answers model/list. We kill the process
    // explicitly once we have the response instead.
    let _stdin = stdin;

    let stdout = child.stdout.take().ok_or_else(|| CliInfoError {
        code: "protocol_error".to_string(),
        message: "Failed to capture stdout".to_string(),
    })?;

    let result = timeout(PROCESS_TIMEOUT, read_model_list(stdout)).await;

    let _ = child.kill().await;
    let _ = child.wait().await;

    let list = match result {
        Ok(Ok(list)) => list,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return Err(CliInfoError {
                code: "timeout".to_string(),
                message: format!(
                    "Timed out after {}s waiting for codex app-server",
                    PROCESS_TIMEOUT.as_secs()
                ),
            });
        }
    };

    log::debug!(
        "[codex_control] got {} models, default={:?}",
        list.models.len(),
        list.default_model
    );

    let mut guard = cache.inner.write().await;
    *guard = Some((list.clone(), Instant::now()));

    Ok(list)
}

/// Read app-server stdout line by line, returning when the `id:2` (model/list) response
/// arrives. JSON-RPC errors and EOF-without-response are surfaced as protocol errors.
async fn read_model_list(
    stdout: tokio::process::ChildStdout,
) -> Result<CodexModelList, CliInfoError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut reader = BufReader::new(stdout).lines();
    let mut line_count = 0u32;

    while let Ok(Some(text)) = reader.next_line().await {
        line_count += 1;
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        log::trace!(
            "[codex_control] stdout #{}: {}",
            line_count,
            &text[..text.len().min(200)]
        );

        let parsed: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only the model/list reply (our id 2); skip notifications + the initialize reply.
        if parsed.get("id").and_then(|v| v.as_u64()) != Some(2) {
            continue;
        }

        if let Some(err) = parsed.get("error") {
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown JSON-RPC error");
            // Unknown method => old Codex without model/list. Treat as protocol error;
            // caller falls back.
            return Err(CliInfoError {
                code: "protocol_error".to_string(),
                message: format!("model/list error: {}", msg),
            });
        }

        let data = parsed
            .get("result")
            .and_then(|r| r.get("data"))
            .and_then(|d| d.as_array())
            .ok_or_else(|| CliInfoError {
                code: "protocol_error".to_string(),
                message: "model/list response missing result.data".to_string(),
            })?;

        return Ok(map_models(data));
    }

    Err(CliInfoError {
        code: "protocol_error".to_string(),
        message: format!("EOF after {} lines without model/list response", line_count),
    })
}

/// Map Codex `Model` objects to `CliModelInfo`, filtering hidden models and capturing the
/// `isDefault` one. Lenient on missing fields — app-server is experimental.
fn map_models(data: &[Value]) -> CodexModelList {
    let mut models = Vec::new();
    let mut default_model = None;

    for m in data {
        if m.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        // `model` is the value passed to `--model`; fall back to `id` if absent.
        let value = m
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| m.get("id").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        if value.is_empty() {
            continue;
        }

        let display_name = m
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&value)
            .to_string();
        let description = m
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let effort_levels: Vec<String> = m
            .get("supportedReasoningEfforts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.get("reasoningEffort").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if m.get("isDefault")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            default_model = Some(value.clone());
        }

        let context_window = m
            .get("contextWindow")
            .or_else(|| m.get("context_window"))
            .and_then(|value| value.as_u64())
            .or_else(|| known_effective_context_window(&value));

        models.push(CliModelInfo {
            value,
            display_name,
            description,
            context_window,
            supports_effort: Some(!effort_levels.is_empty()),
            supported_effort_levels: if effort_levels.is_empty() {
                None
            } else {
                Some(effort_levels)
            },
            supports_adaptive_thinking: None,
        });
    }

    CodexModelList {
        models,
        default_model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_models_filters_hidden_and_captures_default() {
        let data = serde_json::json!([
            {
                "model": "gpt-5.5",
                "displayName": "GPT-5.5",
                "description": "Frontier model",
                "hidden": false,
                "supportedReasoningEfforts": [
                    {"reasoningEffort": "low"},
                    {"reasoningEffort": "medium"},
                    {"reasoningEffort": "high"},
                    {"reasoningEffort": "xhigh"}
                ],
                "isDefault": true
            },
            {
                "model": "gpt-5.4",
                "displayName": "GPT-5.4",
                "description": "Everyday",
                "hidden": false,
                "supportedReasoningEfforts": [],
                "isDefault": false
            },
            {
                "model": "internal-secret",
                "displayName": "Hidden",
                "hidden": true
            }
        ]);
        let arr = data.as_array().unwrap();
        let out = map_models(arr);

        assert_eq!(out.models.len(), 2, "hidden model must be filtered out");
        assert_eq!(out.default_model.as_deref(), Some("gpt-5.5"));

        let first = &out.models[0];
        assert_eq!(first.value, "gpt-5.5");
        assert_eq!(first.supports_effort, Some(true));
        assert_eq!(
            first.supported_effort_levels.as_ref().unwrap(),
            &vec!["low", "medium", "high", "xhigh"]
        );

        // No reasoning efforts → supports_effort false, levels None.
        let second = &out.models[1];
        assert_eq!(second.supports_effort, Some(false));
        assert!(second.supported_effort_levels.is_none());
    }

    #[test]
    fn falls_back_to_id_when_model_field_absent() {
        let data = serde_json::json!([
            {"id": "gpt-x", "displayName": "X", "hidden": false}
        ]);
        let out = map_models(data.as_array().unwrap());
        assert_eq!(out.models.len(), 1);
        assert_eq!(out.models[0].value, "gpt-x");
    }

    #[test]
    fn fallback_is_non_empty() {
        let fb = fallback_models();
        assert!(!fb.models.is_empty());
        assert_eq!(fb.default_model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(fb.models[0].context_window, Some(258_400));
    }

    #[test]
    fn known_subscription_model_gets_effective_context_when_app_server_omits_it() {
        let data = serde_json::json!([
            {"model": "gpt-5.6-sol", "displayName": "GPT-5.6 Sol", "hidden": false}
        ]);
        let out = map_models(data.as_array().unwrap());
        assert_eq!(out.models[0].context_window, Some(258_400));
    }

    #[test]
    fn explicit_context_metadata_remains_authoritative() {
        let data = serde_json::json!([
            {
                "model": "gpt-5.6-sol",
                "displayName": "GPT-5.6 Sol",
                "contextWindow": 123456,
                "hidden": false
            }
        ]);
        let out = map_models(data.as_array().unwrap());
        assert_eq!(out.models[0].context_window, Some(123456));
    }
}
