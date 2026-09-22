use crate::models::McpServerInfo;
use serde_json::Value;

pub(super) fn parse_servers(value: Option<&Value>) -> Vec<McpServerInfo> {
    let Some(value) = value else {
        return Vec::new();
    };
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .or_else(|| value.get("server"))
        .unwrap_or(value);
    let values: Vec<&Value> = match servers {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![servers],
        _ => Vec::new(),
    };
    values.into_iter().filter_map(normalize_server).collect()
}

fn normalize_server(value: &Value) -> Option<McpServerInfo> {
    let name = value
        .get("name")
        .or_else(|| value.get("serverName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())?
        .to_string();
    let raw_status = value
        .get("status")
        .or_else(|| value.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = match raw_status.to_ascii_lowercase().as_str() {
        "connected" | "ready" | "running" => "connected",
        "starting" | "pending" | "connecting" => "pending",
        "failed" | "error" | "disconnected" => "failed",
        _ => raw_status,
    }
    .to_string();
    Some(McpServerInfo {
        name,
        status,
        server_type: value
            .get("serverType")
            .or_else(|| value.get("type"))
            .and_then(Value::as_str)
            .map(str::to_string),
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_session_mcp_statuses() {
        let servers = parse_servers(Some(&json!({
            "mcpServers": [
                {"name": "docs", "status": "ready", "serverType": "http"},
                {"name": "broken", "status": "error", "error": "offline"}
            ]
        })));
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].status, "connected");
        assert_eq!(servers[0].server_type.as_deref(), Some("http"));
        assert_eq!(servers[1].status, "failed");
        assert_eq!(servers[1].error.as_deref(), Some("offline"));
    }

    #[test]
    fn ignores_malformed_server_entries() {
        let servers = parse_servers(Some(&json!({
            "servers": [{"status": "ready"}, {"name": "valid", "status": "pending"}]
        })));
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "valid");
    }
}
