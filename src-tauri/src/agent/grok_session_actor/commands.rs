use serde_json::{json, Value};

/// Normalize ACP `available_commands_update.availableCommands` into the
/// command shape already consumed by AgentCabin's slash-command UI.
///
/// ACP command names are sent without a leading slash. We still trim one
/// defensively because adapters in the wild may include it. `input.hint`
/// is mapped to AgentCabin's existing `argumentHint` field.
pub(super) fn parse_available_commands(update: &Value) -> Vec<Value> {
    update
        .get("availableCommands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(normalize_command)
        .collect()
}

fn normalize_command(command: &Value) -> Option<Value> {
    let name = command
        .get("name")
        .and_then(Value::as_str)?
        .trim()
        .trim_start_matches('/')
        .trim();
    if name.is_empty() {
        return None;
    }

    let description = command
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let argument_hint = command
        .pointer("/input/hint")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    Some(json!({
        "name": name,
        "description": description,
        "aliases": [],
        "argumentHint": argument_hint
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_acp_commands_to_agentcabin_shape() {
        let update = json!({
            "sessionUpdate": "available_commands_update",
            "availableCommands": [
                {
                    "name": "plan",
                    "description": "Create a plan",
                    "input": {"hint": "[topic]"}
                },
                {
                    "name": "/review",
                    "description": " Review changes "
                }
            ]
        });

        let commands = parse_available_commands(&update);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0]["name"], "plan");
        assert_eq!(commands[0]["description"], "Create a plan");
        assert_eq!(commands[0]["argumentHint"], "[topic]");
        assert_eq!(commands[1]["name"], "review");
        assert_eq!(commands[1]["description"], "Review changes");
    }

    #[test]
    fn skips_malformed_commands_and_treats_missing_list_as_empty() {
        let update = json!({
            "sessionUpdate": "available_commands_update",
            "availableCommands": [
                {"description": "missing name"},
                {"name": "   "},
                {"name": "compact", "description": null}
            ]
        });
        let commands = parse_available_commands(&update);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["name"], "compact");
        assert_eq!(commands[0]["description"], "");

        assert!(parse_available_commands(&json!({})).is_empty());
    }
}
