use crate::models::CliModelInfo;
use serde_json::Value;
use std::collections::HashSet;

/// Read Grok's live model catalog from the ACP `_meta.modelState` extension.
///
/// The catalog is deliberately separate from `sessionSetModel`: model metadata
/// tells the UI what can be displayed, while the session capability decides
/// whether `session/set_model` may be sent.
pub(super) fn parse_model_state(value: &Value) -> (Option<String>, Vec<CliModelInfo>) {
    let state = value
        .pointer("/_meta/modelState")
        .or_else(|| value.get("modelState"))
        .unwrap_or(&Value::Null);
    let current = state
        .get("currentModelId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut seen = HashSet::new();
    let models = state
        .get("availableModels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| normalize_model(model, &mut seen))
        .collect();
    (current, models)
}

fn normalize_model(value: &Value, seen: &mut HashSet<String>) -> Option<CliModelInfo> {
    let model_id = value
        .get("modelId")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    if !seen.insert(model_id.clone()) {
        return None;
    }
    let meta = value.get("_meta").unwrap_or(&Value::Null);
    let effort_levels = meta
        .get("reasoningEfforts")
        .and_then(Value::as_array)
        .map(|levels| {
            levels
                .iter()
                .filter_map(|level| {
                    level
                        .get("id")
                        .or_else(|| level.get("value"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .filter(|levels| !levels.is_empty());
    let supports_effort = meta
        .get("supportsReasoningEffort")
        .and_then(Value::as_bool)
        .or_else(|| effort_levels.as_ref().map(|levels| !levels.is_empty()));
    let context_window = meta
        .get("totalContextTokens")
        .or_else(|| meta.get("contextWindow"))
        .or_else(|| value.get("contextWindow"))
        .and_then(Value::as_u64)
        .filter(|window| *window > 0);
    Some(CliModelInfo {
        value: model_id.clone(),
        display_name: value
            .get("name")
            .or_else(|| value.get("displayName"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&model_id)
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        context_window,
        supports_effort,
        supported_effort_levels: effort_levels,
        supports_adaptive_thinking: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_live_model_catalog_without_implying_switch_support() {
        let (current, models) = parse_model_state(&json!({
            "_meta": {"modelState": {
                "currentModelId": "grok-4.5",
                "availableModels": [{
                    "modelId": "grok-4.5",
                    "name": "Grok 4.5",
                    "description": "frontier",
                    "_meta": {
                        "totalContextTokens": 500000,
                        "supportsReasoningEffort": true,
                        "reasoningEfforts": [{"id": "low"}, {"value": "high"}]
                    }
                }]
            }}
        }));
        assert_eq!(current.as_deref(), Some("grok-4.5"));
        assert_eq!(models.len(), 1);
        assert_eq!(
            serde_json::to_value(&models[0]).unwrap()["contextWindow"],
            json!(500000)
        );
        assert_eq!(
            models[0].supported_effort_levels.as_deref(),
            Some(["low".to_string(), "high".to_string()].as_slice())
        );
    }

    #[test]
    fn ignores_duplicate_or_malformed_models() {
        let (_, models) = parse_model_state(&json!({
            "modelState": {"availableModels": [
                {"modelId": "grok-4.5", "name": "one"},
                {"modelId": "grok-4.5", "name": "duplicate"},
                {"name": "missing id"}
            ]}
        }));
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].display_name, "one");
    }
}
