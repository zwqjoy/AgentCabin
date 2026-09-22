//! Pi start-session bridge for the WebSocket dispatcher.
//!
//! Once started, Pi is stored in the shared `ActorSessionMap`, so the legacy web
//! dispatcher can use its normal send/control/stop paths. Only startup needs to
//! select the Pi process factory instead of the Claude/Codex one.

use crate::agent::session_actor::AttachmentData;
use crate::models::SessionMode;
use crate::storage;
use crate::web_server::state::AppState;
use serde_json::{json, Value};

fn string_param(params: &Value, snake: &str, camel: &str) -> Option<String> {
    params
        .get(snake)
        .or_else(|| params.get(camel))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub async fn dispatch_if_pi(
    method: &str,
    params: &Value,
    state: &AppState,
) -> Option<Result<Value, String>> {
    if method != "start_session" {
        return None;
    }

    let run_id = match string_param(params, "run_id", "runId") {
        Some(value) if !value.is_empty() => value,
        _ => return Some(Err("run_id is required".to_string())),
    };
    let run = match storage::runs::get_run(&run_id) {
        Some(run) => run,
        None => return Some(Err(format!("Run {} not found", run_id))),
    };
    if run.agent != "pi" {
        return None;
    }

    let mode: Option<SessionMode> = params
        .get("mode")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let session_id = string_param(params, "session_id", "sessionId");
    let initial_message = string_param(params, "initial_message", "initialMessage");
    let attachments: Option<Vec<AttachmentData>> = params
        .get("attachments")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let platform_id = string_param(params, "platform_id", "platformId");
    let permission_mode_override =
        string_param(params, "permission_mode_override", "permissionModeOverride");

    Some(
        super::session_dispatch::start_session_impl(
            &state.emitter,
            &state.sessions,
            &state.spawn_locks,
            &state.cancel_token,
            run_id,
            mode,
            session_id,
            initial_message,
            attachments,
            platform_id,
            permission_mode_override,
            None,
        )
        .await
        .map(|_| json!(true)),
    )
}
