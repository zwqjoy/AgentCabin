use crate::models::AgentSessionMode;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionModes {
    pub current_mode_id: String,
    pub available_modes: Vec<AgentSessionMode>,
}

/// Parse the optional ACP `modes` state returned by session/new or session/load.
pub(super) fn parse_session_modes(result: &Value) -> Option<SessionModes> {
    let modes = result.get("modes")?;
    let current_mode_id = modes
        .get("currentModeId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let available_modes = modes
        .get("availableModes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(normalize_mode)
        .collect();
    Some(SessionModes {
        current_mode_id,
        available_modes,
    })
}

/// Parse ACP `current_mode_update` notifications.
pub(super) fn parse_current_mode_update(update: &Value) -> Option<String> {
    update
        .get("currentModeId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Some ACP servers include the full mode catalog on a mode update. Accept it
/// when present so the UI remains driven by the live protocol instead of a
/// hardcoded default/plan pair.
pub(super) fn parse_available_modes_update(update: &Value) -> Vec<AgentSessionMode> {
    update
        .get("availableModes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(normalize_mode)
        .collect()
}

pub(super) fn supports_plan(modes: &[AgentSessionMode]) -> bool {
    modes.iter().any(|mode| mode.id == "plan")
}

fn normalize_mode(value: &Value) -> Option<AgentSessionMode> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(id);
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(AgentSessionMode {
        id: id.to_string(),
        name: name.to_string(),
        description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_session_modes_and_plan_capability() {
        let result = json!({
            "modes": {
                "currentModeId": "default",
                "availableModes": [
                    {"id": "default", "name": "Agent"},
                    {"id": "plan", "name": "Plan", "description": "Plan before editing"},
                    {"id": "ask", "name": "Ask"}
                ]
            }
        });
        let modes = parse_session_modes(&result).expect("modes");
        assert_eq!(modes.current_mode_id, "default");
        assert_eq!(modes.available_modes.len(), 3);
        assert!(supports_plan(&modes.available_modes));
        assert_eq!(
            modes.available_modes[1].description.as_deref(),
            Some("Plan before editing")
        );
    }

    #[test]
    fn rejects_incomplete_mode_state_conservatively() {
        assert!(parse_session_modes(&json!({})).is_none());
        assert!(parse_session_modes(&json!({"modes": {"availableModes": []}})).is_none());
    }

    #[test]
    fn parses_current_mode_update() {
        let update = json!({
            "sessionUpdate": "current_mode_update",
            "currentModeId": "plan"
        });
        assert_eq!(parse_current_mode_update(&update).as_deref(), Some("plan"));
    }

    #[test]
    fn parses_mode_catalog_from_update_when_present() {
        let update = json!({
            "currentModeId": "ask",
            "availableModes": [{"id": "default"}, {"id": "ask", "name": "Ask"}]
        });
        let modes = parse_available_modes_update(&update);
        assert_eq!(modes.len(), 2);
        assert_eq!(modes[1].name, "Ask");
    }

    #[test]
    fn plan_detection_uses_canonical_mode_id() {
        let modes = vec![AgentSessionMode {
            id: "PLAN".to_string(),
            name: "Plan".to_string(),
            description: None,
        }];
        assert!(!supports_plan(&modes));
    }
}
