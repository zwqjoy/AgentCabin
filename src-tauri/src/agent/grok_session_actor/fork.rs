use serde_json::{json, Value};

/// Grok Build 0.2.118 exposes fork as a structured ACP extension. The caller supplies the
/// source and target cwd; using the same cwd creates a normal branch without worktree isolation.
pub(super) fn request_frame(
    id: Value,
    source_session_id: &str,
    source_cwd: &str,
    new_cwd: &str,
) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "_x.ai/session/fork",
        "params": {
            "sourceSessionId": source_session_id,
            "sourceCwd": source_cwd,
            "newCwd": new_cwd
        }
    })
}

pub(crate) fn session_id(result: &Value) -> Result<String, String> {
    result
        .get("newSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Grok fork response did not include newSessionId".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_verified_extension_request() {
        let frame = request_frame(json!(7), "source-1", "/repo", "/repo");
        assert_eq!(frame["method"], "_x.ai/session/fork");
        assert_eq!(frame["params"]["sourceSessionId"], "source-1");
        assert_eq!(frame["params"]["sourceCwd"], "/repo");
        assert_eq!(frame["params"]["newCwd"], "/repo");
    }

    #[test]
    fn parses_distinct_child_identity() {
        let result = json!({
            "newSessionId": "child-2",
            "parentSessionId": "source-1",
            "chatMessagesCopied": 4,
            "updatesCopied": 9,
            "planStateCopied": true
        });
        assert_eq!(session_id(&result).unwrap(), "child-2");
    }

    #[test]
    fn rejects_missing_or_empty_child_identity() {
        assert!(session_id(&json!({})).is_err());
        assert!(session_id(&json!({"newSessionId": "  "})).is_err());
    }
}
