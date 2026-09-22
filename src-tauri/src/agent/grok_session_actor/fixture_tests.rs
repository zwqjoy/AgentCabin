use super::actor_loop::initialize_params;
use super::capabilities::{apply_model_capabilities, from_initialize};
use super::models::parse_model_state;
use serde_json::{json, Value};

const GROK_BUILD_1_0_INITIALIZE: &str = include_str!("fixtures/grok_build_1_0_initialize.json");
const GROK_BUILD_1_0_WIRE_INITIALIZE: &str =
    include_str!("fixtures/grok_build_1_0_initialize.wire.json");

fn grok_build_1_0_initialize() -> Value {
    serde_json::from_str(GROK_BUILD_1_0_INITIALIZE)
        .expect("Grok Build 1.0 initialize fixture must remain valid JSON")
}

fn grok_build_1_0_wire_initialize() -> Value {
    serde_json::from_str(GROK_BUILD_1_0_WIRE_INITIALIZE)
        .expect("Grok Build 1.0 wire initialize fixture must remain valid JSON")
}

#[test]
fn grok_build_1_0_source_fixture_tracks_stable_initialize_shape() {
    let result = grok_build_1_0_initialize();

    assert_eq!(result["protocolVersion"], json!(1));
    assert_eq!(result["_meta"]["agentVersion"], "1.0.0");
    assert_eq!(result["_meta"]["grokShell"], json!(true));
    assert_eq!(result["_meta"]["x.ai/mcp/sdk"], json!(true));
    assert_eq!(result["_meta"]["x.ai/pluginDirs"], json!(true));

    assert_eq!(result["agentCapabilities"]["loadSession"], json!(true));
    assert_eq!(
        result["agentCapabilities"]["promptCapabilities"]["embeddedContext"],
        json!(true)
    );
    assert_eq!(
        result["agentCapabilities"]["mcpCapabilities"]["http"],
        json!(true)
    );
    assert_eq!(
        result["agentCapabilities"]["mcpCapabilities"]["sse"],
        json!(true)
    );
    assert_eq!(
        result["agentCapabilities"]["_meta"]["x.ai/fs_notify"],
        json!(true)
    );

    for capability in ["close", "list", "resume"] {
        assert!(
            result["agentCapabilities"]["sessionCapabilities"][capability].is_object(),
            "Grok Build 1.0 should advertise session capability {capability}"
        );
    }

    // The published 1.0.0 initialize construction advertises embedded context here,
    // not an ACP image prompt capability. Do not infer image content support from the
    // native Grok file/tool surface.
    assert!(result["agentCapabilities"]["promptCapabilities"]
        .get("image")
        .is_none());
}

#[test]
fn grok_build_1_0_wire_fixture_tracks_live_initialize_shape() {
    let result = grok_build_1_0_wire_initialize();

    assert_eq!(result["protocolVersion"], json!(1));
    assert_eq!(result["_meta"]["agentVersion"], "1.0.0");
    assert_eq!(result["_meta"]["grokShell"], json!(true));
    assert_eq!(result["_meta"]["x.ai/mcp/sdk"], json!(true));
    assert_eq!(result["_meta"]["x.ai/pluginDirs"], json!(true));
    assert_eq!(
        result["_meta"]["currentWorkingDirectory"],
        "/fixture/workspace"
    );
    assert_eq!(result["_meta"]["agentId"], "fixture-agent-id");
    assert_eq!(
        result["_meta"]["agentInstanceId"],
        "fixture-agent-instance-id"
    );
    assert_eq!(result["_meta"]["hostname"], "fixture-host");

    assert_eq!(
        result["agentCapabilities"]["promptCapabilities"]["image"],
        json!(false)
    );
    assert_eq!(
        result["agentCapabilities"]["promptCapabilities"]["audio"],
        json!(false)
    );
    assert_eq!(
        result["agentCapabilities"]["_meta"]["x.ai/hooks"]["blockingEvents"],
        json!(["pre_tool_use", "stop", "subagent_stop"])
    );
    assert_eq!(result["_meta"]["cancelRewind"], json!(true));
    assert_eq!(result["_meta"]["voiceMode"], json!(true));
    assert_eq!(
        result["_meta"]["availableCommands"]
            .as_array()
            .map(Vec::len),
        Some(7)
    );
    assert_eq!(result["_meta"]["availableCommands"][6]["name"], "goal");
    assert_eq!(
        result["_meta"]["modelState"]["availableModels"][0]["_meta"]["supportsReasoningEffort"],
        json!(true)
    );
}

#[test]
fn grok_build_1_0_fixture_maps_to_only_agentcabin_wired_capabilities() {
    let capabilities = from_initialize(&grok_build_1_0_initialize());

    assert!(capabilities.protocol.session_load);
    assert!(!capabilities.protocol.session_set_model);
    assert!(capabilities.protocol.permission_request);
    assert!(!capabilities.protocol.permission_mode_control);
    assert!(!capabilities.protocol.effort_control);
    assert!(!capabilities.protocol.goal_state);
    assert!(capabilities.protocol.structured_task_state);

    assert!(!capabilities.runtime.attachments);
    assert!(!capabilities.runtime.remote);
    assert!(capabilities.runtime.fork);
    assert!(capabilities.runtime.steer);
    assert!(capabilities.runtime.follow_up);
}

#[test]
fn grok_build_1_0_selected_model_enables_effort_from_model_metadata() {
    let result = grok_build_1_0_wire_initialize();
    let (current_model, mut model_options) = parse_model_state(&result);
    let mut capabilities = from_initialize(&result);

    apply_model_capabilities(
        &mut capabilities,
        &mut model_options,
        current_model.as_deref(),
        false,
        None,
    );

    assert_eq!(current_model.as_deref(), Some("grok-4.5"));
    assert!(capabilities.protocol.effort_control);
    assert!(capabilities.ui.effort_selector);
    assert_eq!(
        model_options[0].supported_effort_levels.as_deref(),
        Some(["high".to_string(), "medium".to_string(), "low".to_string()].as_slice())
    );
}

#[test]
fn grok_build_1_0_wire_fixture_keeps_live_capability_mapping_conservative() {
    let capabilities = from_initialize(&grok_build_1_0_wire_initialize());

    assert!(capabilities.protocol.session_load);
    assert!(!capabilities.protocol.session_set_model);
    assert!(capabilities.protocol.permission_request);
    assert!(!capabilities.protocol.permission_mode_control);
    assert!(!capabilities.protocol.effort_control);
    assert!(!capabilities.protocol.goal_state);
    assert!(capabilities.protocol.structured_task_state);

    assert!(!capabilities.runtime.attachments);
    assert!(capabilities.runtime.fork);
    assert!(capabilities.runtime.steer);
    assert!(capabilities.runtime.follow_up);
}

#[test]
fn grok_build_1_0_fixture_does_not_prematurely_enable_host_fs_or_terminal() {
    let request = initialize_params();

    assert_eq!(
        request.pointer("/clientCapabilities/fs/readTextFile"),
        Some(&json!(false))
    );
    assert_eq!(
        request.pointer("/clientCapabilities/fs/writeTextFile"),
        Some(&json!(false))
    );
    assert_eq!(
        request.pointer("/clientCapabilities/terminal"),
        Some(&json!(false))
    );
}
