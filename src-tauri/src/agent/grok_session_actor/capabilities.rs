use crate::models::{
    AgentCapabilities, AgentProtocolCapabilities, AgentRuntimeCapabilities, AgentUiCapabilities,
    CliModelInfo, GlobalProviderModel,
};
use serde_json::Value;

pub(super) const DEFAULT_GROK_EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh"];

/// Build AgentCabin's end-to-end Grok capability snapshot from the ACP initialize result.
///
/// The snapshot is deliberately conservative: an ACP server advertisement alone does not
/// enable a feature that AgentCabin has not wired through its runtime/UI yet.
pub(super) fn from_initialize(result: &Value) -> AgentCapabilities {
    let advertised = result.get("agentCapabilities").unwrap_or(&Value::Null);
    let session = advertised
        .get("sessionCapabilities")
        .unwrap_or(&Value::Null);
    let image_prompts = advertised
        .pointer("/promptCapabilities/image")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // The current Grok Build handshake uses this marker for its structured `_x.ai/*` ACP
    // extension family. We verified fork and interject against 0.2.118; without the marker the
    // adapter remains conservative and rejects those controls.
    let grok_shell_extensions = result
        .pointer("/_meta/grokShell")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    AgentCapabilities {
        protocol: AgentProtocolCapabilities {
            session_load: bool_capability(advertised, "loadSession"),
            // `models` is legacy ACP metadata used by adapters that expose
            // `session/set_model`. Accept explicit session capability aliases too,
            // but do not infer model switching from unrelated config options.
            session_set_model: capability_present(advertised, "models")
                || capability_present(session, "setModel")
                || capability_present(session, "set_model"),
            session_mode_control: false,
            // request_permission is an agent -> client ACP request. Grok's adapter
            // implements that reverse request end-to-end, so it is safe to expose.
            permission_request: true,
            // ACP permission requests are reverse requests; Grok does not currently expose
            // a host-driven permission-mode mutation endpoint.
            permission_mode_control: false,
            // Grok's plan policy is a safe host startup control. Native session
            // modes, when present, refine this capability after session/new.
            slash_commands: false,
            plan_mode: true,
            effort_control: false,
            goal_state: false,
            // Grok emits ACP `plan` snapshots. The parser still rejects malformed snapshots and
            // only mutates state after an actual structured update arrives.
            structured_task_state: grok_shell_extensions,
        },
        // Image content is safe to expose only when the live ACP server explicitly
        // advertises it. The adapter still validates individual attachment MIME types.
        runtime: AgentRuntimeCapabilities {
            attachments: image_prompts,
            fork: grok_shell_extensions,
            steer: grok_shell_extensions,
            // Follow-up is a separate delayed-turn semantic implemented by AgentCabin's queue;
            // it is not inferred from interject/steer.
            follow_up: grok_shell_extensions,
            ..AgentRuntimeCapabilities::default()
        },
        // These controls are host-backed: plan/permission apply at the next
        // process start, while Goal is the shared prompt-backed panel.
        ui: AgentUiCapabilities {
            plan_mode_toggle: true,
            goal_panel: true,
            permission_mode_switch: true,
            ..AgentUiCapabilities::default()
        },
    }
}

/// Overlay the capability contract with the model that will actually receive prompts.
/// Native Grok reports model capabilities through ACP; managed custom Providers report them
/// through the AgentCabin Provider model row.
pub(super) fn apply_model_capabilities(
    capabilities: &mut AgentCapabilities,
    model_options: &mut Vec<CliModelInfo>,
    current_model: Option<&str>,
    custom_provider_active: bool,
    custom_model: Option<&GlobalProviderModel>,
) {
    if custom_provider_active {
        capabilities.runtime.attachments = custom_model
            .and_then(|model| model.supports_images)
            .unwrap_or(false);

        let Some(custom_model) = custom_model else {
            capabilities.protocol.effort_control = false;
            capabilities.ui.effort_selector = false;
            return;
        };

        let info = cli_model_info(custom_model);
        let supports_effort = info.supports_effort == Some(true);
        capabilities.protocol.effort_control = supports_effort;
        capabilities.ui.effort_selector = supports_effort;
        upsert_model_option(model_options, info);
        return;
    }

    let model_info = current_model.and_then(|model| {
        model_options
            .iter()
            .find(|candidate| candidate.value == model)
    });
    let supports_effort = model_info.is_some_and(|model| model.supports_effort == Some(true));
    capabilities.protocol.effort_control = supports_effort;
    capabilities.ui.effort_selector = supports_effort;
}

pub(super) fn cli_model_info(model: &GlobalProviderModel) -> CliModelInfo {
    let supported_effort_levels = model
        .supported_effort_levels
        .clone()
        .filter(|levels| !levels.is_empty())
        .or_else(|| {
            model.supports_reasoning.unwrap_or(false).then(|| {
                DEFAULT_GROK_EFFORT_LEVELS
                    .iter()
                    .map(|level| (*level).to_string())
                    .collect()
            })
        });
    let supports_effort = model.supports_reasoning.or_else(|| {
        supported_effort_levels
            .as_ref()
            .map(|levels| !levels.is_empty())
    });

    CliModelInfo {
        value: model.id.clone(),
        display_name: model
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&model.id)
            .to_string(),
        description: String::new(),
        context_window: model.context_window,
        supports_effort,
        supported_effort_levels,
        supports_adaptive_thinking: None,
    }
}

fn upsert_model_option(model_options: &mut Vec<CliModelInfo>, model: CliModelInfo) {
    if let Some(existing) = model_options
        .iter_mut()
        .find(|candidate| candidate.value == model.value)
    {
        *existing = model;
    } else {
        model_options.push(model);
    }
}

fn bool_capability(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn capability_present(value: &Value, key: &str) -> bool {
    value.get(key).is_some_and(|capability| match capability {
        Value::Null => false,
        Value::Bool(enabled) => *enabled,
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_load_and_legacy_model_capabilities() {
        let result = json!({
            "agentCapabilities": {
                "loadSession": true,
                "models": [{"id": "grok-code"}]
            }
        });

        let capabilities = from_initialize(&result);
        assert!(capabilities.protocol.session_load);
        assert!(capabilities.protocol.session_set_model);
        assert!(capabilities.protocol.permission_request);
        assert!(!capabilities.protocol.permission_mode_control);
    }

    #[test]
    fn recognizes_explicit_session_set_model_capability() {
        let result = json!({
            "agentCapabilities": {
                "sessionCapabilities": {"setModel": {}}
            }
        });

        let capabilities = from_initialize(&result);
        assert!(!capabilities.protocol.session_load);
        assert!(capabilities.protocol.session_set_model);
    }

    #[test]
    fn keeps_unwired_features_disabled_when_server_advertises_them() {
        let result = json!({
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": {"image": true},
                "sessionCapabilities": {
                    "resume": {},
                    "list": {},
                    "close": {}
                }
            }
        });

        let capabilities = from_initialize(&result);
        assert!(capabilities.runtime.attachments);
        assert!(!capabilities.runtime.remote);
        assert!(!capabilities.runtime.fork);
        assert!(!capabilities.runtime.steer);
        assert!(!capabilities.protocol.slash_commands);
        assert!(capabilities.protocol.plan_mode);
        assert!(!capabilities.protocol.effort_control);
        assert!(!capabilities.protocol.session_mode_control);
        assert!(!capabilities.protocol.goal_state);
        assert!(!capabilities.protocol.structured_task_state);
        assert!(!capabilities.protocol.permission_mode_control);
        assert!(capabilities.ui.plan_mode_toggle);
        assert!(capabilities.ui.goal_panel);
        assert!(capabilities.ui.permission_mode_switch);
    }

    #[test]
    fn malformed_initialize_capabilities_degrade_conservatively() {
        let result = json!({
            "agentCapabilities": {
                "loadSession": "yes",
                "models": null,
                "sessionCapabilities": {"setModel": false}
            }
        });

        let capabilities = from_initialize(&result);
        assert!(!capabilities.protocol.session_load);
        assert!(!capabilities.protocol.session_set_model);
        assert!(capabilities.protocol.permission_request);
        assert!(!capabilities.protocol.permission_mode_control);
        assert!(!capabilities.runtime.attachments);
    }

    #[test]
    fn image_support_is_not_inferred_when_acp_does_not_advertise_it() {
        let result = json!({
            "agentCapabilities": {"promptCapabilities": {"image": false}}
        });
        assert!(!from_initialize(&result).runtime.attachments);
    }

    #[test]
    fn enables_only_verified_grok_shell_extension_paths() {
        let result = json!({
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": {"image": false}
            },
            "_meta": {"grokShell": true, "agentVersion": "0.2.118"}
        });
        let capabilities = from_initialize(&result);
        assert!(capabilities.runtime.fork);
        assert!(capabilities.runtime.steer);
        assert!(capabilities.runtime.follow_up);
        assert!(capabilities.protocol.structured_task_state);
        assert!(!capabilities.protocol.permission_mode_control);
        assert!(!capabilities.protocol.effort_control);
        assert!(!capabilities.protocol.goal_state);
    }

    #[test]
    fn enables_native_effort_only_for_the_selected_model() {
        let mut capabilities = from_initialize(&json!({
            "agentCapabilities": {"promptCapabilities": {"image": false}}
        }));
        let mut models = vec![CliModelInfo {
            value: "grok-4.5".into(),
            display_name: "Grok 4.5".into(),
            description: String::new(),
            context_window: None,
            supports_effort: Some(true),
            supported_effort_levels: Some(vec!["low".into(), "high".into()]),
            supports_adaptive_thinking: None,
        }];

        apply_model_capabilities(
            &mut capabilities,
            &mut models,
            Some("grok-4.5"),
            false,
            None,
        );

        assert!(capabilities.protocol.effort_control);
        assert!(capabilities.ui.effort_selector);
    }

    #[test]
    fn custom_provider_overrides_native_image_and_effort_capabilities() {
        let mut capabilities = from_initialize(&json!({
            "agentCapabilities": {"promptCapabilities": {"image": false}}
        }));
        let mut models = Vec::new();
        let custom = GlobalProviderModel {
            id: "glm-5.2".into(),
            name: Some("GLM 5.2".into()),
            context_window: Some(128_000),
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: Some(true),
            supported_effort_levels: Some(vec![
                "low".into(),
                "medium".into(),
                "high".into(),
                "xhigh".into(),
            ]),
            supports_images: Some(true),
        };

        apply_model_capabilities(
            &mut capabilities,
            &mut models,
            Some("glm-5.2"),
            true,
            Some(&custom),
        );

        assert!(capabilities.runtime.attachments);
        assert!(capabilities.protocol.effort_control);
        assert_eq!(models[0].value, "glm-5.2");
        assert_eq!(
            models[0].supported_effort_levels.as_deref(),
            Some(
                ["low", "medium", "high", "xhigh"]
                    .map(String::from)
                    .as_slice()
            )
        );
    }
}
