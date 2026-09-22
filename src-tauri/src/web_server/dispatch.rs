//! Protocol-aware WebSocket dispatcher.

use crate::web_server::state::AppState;
use serde_json::Value;

#[path = "dispatch_legacy.rs"]
mod legacy;

pub async fn dispatch_command(
    method: &str,
    params: Value,
    state: &AppState,
) -> Result<Value, String> {
    if let Some(result) =
        crate::commands::pi_web_dispatch::dispatch_if_pi(method, &params, state).await
    {
        return result;
    }
    legacy::dispatch_command(method, params, state).await
}
