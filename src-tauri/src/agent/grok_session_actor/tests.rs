use super::actor_loop::{
    authentication_method, compose_continuation_prompt, grok_question_events, grok_question_input,
    grok_question_response, grok_turn_usage_event, initialize_params, is_session_update_method,
    is_user_question_method, last_persisted_turn_usage, permission_option, permission_session_meta,
    session_plugin_meta, tool_name,
};
use serde_json::json;

#[test]
fn grok_initialize_does_not_declare_a_non_interactive_client() {
    let params = initialize_params();
    assert_ne!(
        params.pointer("/_meta/startupHints/nonInteractive"),
        Some(&json!(true))
    );
}

#[test]
fn grok_tool_name_prefers_the_canonical_tool_label() {
    let update = json!({
        "title": "Grok Tool",
        "kind": "other",
        "_meta": {"x.ai/tool": {"label": "Edit"}}
    });
    assert_eq!(tool_name(&update), "Edit");
}

#[test]
fn injected_api_key_skips_redundant_grok_authentication() {
    let methods = vec![json!({"id": "xai.api_key"}), json!({"id": "grok.com"})];
    assert_eq!(authentication_method(&methods, true), None);
}

#[test]
fn grok_authentication_is_still_selected_without_an_injected_key() {
    let methods = vec![json!({"id": "cached_token"})];
    assert_eq!(authentication_method(&methods, false), Some("cached_token"));
}

#[test]
fn ask_mode_explicitly_disables_acp_yolo_mode() {
    assert_eq!(
        permission_session_meta("default", Some("deepseek/deepseek-v4-flash")),
        json!({
            "modelId": "deepseek/deepseek-v4-flash",
            "yoloMode": false
        })
    );
}

#[test]
fn permission_session_meta_maps_grok_modes() {
    assert_eq!(
        permission_session_meta("acceptEdits", None)["yoloMode"],
        json!(false)
    );
    assert_eq!(
        permission_session_meta("bypassPermissions", None)["yoloMode"],
        json!(true)
    );
}

#[test]
fn session_plugin_meta_keeps_permission_and_model_fields() {
    let plugin_dirs = vec![
        "/Users/example/.claude/plugins/cache/demo/1.0.0".to_string(),
        "/Users/example/project/.claude/plugins/local".to_string(),
    ];
    assert_eq!(
        session_plugin_meta("auto", Some("grok-4.5"), &plugin_dirs),
        json!({
            "autoMode": true,
            "modelId": "grok-4.5",
            "yoloMode": false,
            "pluginDirs": plugin_dirs,
        })
    );
}

#[test]
fn session_plugin_meta_omits_empty_plugin_selection() {
    assert_eq!(
        session_plugin_meta("default", None, &[]),
        json!({"yoloMode": false})
    );
}

#[test]
fn permission_option_selects_explicit_allow() {
    let options = vec![
        json!({"optionId": "deny", "kind": "reject_once", "name": "Reject"}),
        json!({"optionId": "allow", "kind": "allow_once", "name": "Allow once"}),
    ];
    assert_eq!(permission_option(&options, true), Some("allow"));
}

#[test]
fn permission_option_selects_explicit_reject() {
    let options = vec![
        json!({"optionId": "allow", "kind": "allow_once", "name": "Allow once"}),
        json!({"optionId": "deny", "kind": "reject_once", "name": "Reject"}),
    ];
    assert_eq!(permission_option(&options, false), Some("deny"));
}

#[test]
fn permission_option_never_treats_first_reject_as_allow_fallback() {
    let options = vec![json!({
        "optionId": "deny",
        "kind": "reject_once",
        "name": "Reject"
    })];
    assert_eq!(permission_option(&options, true), None);
}

#[test]
fn permission_option_supports_legacy_id_field() {
    let options = vec![json!({
        "id": "allow-legacy",
        "kind": "custom",
        "name": "Allow this operation"
    })];
    assert_eq!(permission_option(&options, true), Some("allow-legacy"));
}

#[test]
fn continuation_context_is_attached_to_the_first_real_request() {
    let prompt = compose_continuation_prompt(Some("historical context"), "latest request");
    assert!(prompt.starts_with("historical context"));
    assert!(prompt.contains("<current-request>\nlatest request\n</current-request>"));
}

#[test]
fn ordinary_grok_requests_are_not_wrapped() {
    assert_eq!(
        compose_continuation_prompt(None, "latest request"),
        "latest request"
    );
}

#[test]
fn recognizes_grok_question_method_aliases() {
    assert!(is_user_question_method("x.ai/ask_user_question"));
    assert!(is_user_question_method("_x.ai/ask_user_question"));
    assert!(!is_user_question_method("session/request_permission"));
}

#[test]
fn recognizes_standard_and_grok_session_update_methods() {
    assert!(is_session_update_method("session/update"));
    assert!(is_session_update_method("x.ai/session/update"));
    assert!(is_session_update_method("_x.ai/session/update"));
    assert!(!is_session_update_method("x.ai/models/updated"));
}

#[test]
fn normalizes_grok_questions_for_the_shared_ask_card() {
    let questions = vec![json!({
        "question": "Which word?",
        "multiSelect": false,
        "options": [
            {"label": "FOO", "description": "Select FOO.", "preview": "foo"},
            {"label": "BAR", "description": "Select BAR."}
        ]
    })];
    let input = grok_question_input(&questions);
    assert_eq!(input["questions"][0]["id"], "Which word?");
    assert_eq!(input["questions"][0]["options"][0]["label"], "FOO");
    assert_eq!(input["questions"][0]["options"][0]["preview"], "foo");
}

#[test]
fn maps_shared_question_answers_to_grok_response() {
    let questions = vec![json!({
        "question": "Which word?",
        "options": [{"label": "FOO"}]
    })];
    let response =
        grok_question_response(&questions, &json!({"answers": {"Which word?": ["FOO"]}}));
    assert_eq!(response["outcome"], "accepted");
    assert_eq!(response["answers"]["Which word?"], "FOO");
    assert_eq!(response["annotations"], json!({}));
}

#[test]
fn emits_grok_intro_before_the_interactive_question() {
    let events = grok_question_events(
        "run-1",
        "question-1",
        json!({"questions": [{"id": "q1", "question": "Which word?"}]}),
        Some("好的，我先问你几个问题。".to_string()),
        Some("grok-test".to_string()),
    );

    assert!(matches!(
        events.first(),
        Some(crate::models::BusEvent::MessageComplete { text, .. })
            if text == "好的，我先问你几个问题。"
    ));
    assert!(matches!(
        events.get(1),
        Some(crate::models::BusEvent::ToolStart {
            tool_name,
            tool_use_id,
            ..
        }) if tool_name == "AskUserQuestion" && tool_use_id == "question-1"
    ));
    assert!(matches!(
        events.get(2),
        Some(crate::models::BusEvent::ToolEnd {
            tool_name,
            status,
            ..
        }) if tool_name == "AskUserQuestion" && status == "error"
    ));
}

#[test]
fn maps_grok_turn_completed_usage_without_double_counting_cached_input() {
    let event = grok_turn_usage_event(
        "run-1",
        &json!({
            "sessionUpdate": "turn_completed",
            "stop_reason": "end_turn",
            "usage": {
                "inputTokens": 14075,
                "outputTokens": 75,
                "cachedReadTokens": 8192,
                "cacheCreationTokens": 0,
                "apiDurationMs": 7028,
                "numTurns": 1
            }
        }),
    )
    .expect("turn usage should be recognized");

    assert!(matches!(
        event,
        crate::models::BusEvent::UsageUpdate {
            input_tokens: 5883,
            output_tokens: 75,
            cache_read_tokens: Some(8192),
            cache_write_tokens: Some(0),
            duration_api_ms: Some(7028),
            num_turns: Some(1),
            stop_reason: Some(reason),
            ..
        } if reason == "end_turn"
    ));
}

#[test]
fn restores_usage_from_groks_persisted_turn_when_acp_omits_the_update() {
    let root = std::env::temp_dir().join(format!("agentcabin-grok-usage-{}", uuid::Uuid::new_v4()));
    let cwd = "/Users/example/个人/Apps/AgentCabin";
    let session_dir = root
        .join("sessions")
        .join(urlencoding::encode(cwd).as_ref())
        .join("session-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("updates.jsonl"),
        concat!(
            "{\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"}}}\n",
            "{\"method\":\"_x.ai/session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"turn_completed\",\"stop_reason\":\"end_turn\",\"usage\":{\"inputTokens\":14242,\"outputTokens\":198,\"cachedReadTokens\":8192,\"cacheCreationTokens\":0,\"apiDurationMs\":16237,\"numTurns\":1}}}}\n"
        ),
    )
    .unwrap();

    let event = last_persisted_turn_usage(&root, cwd, "session-1", "run-1")
        .expect("persisted Grok turn usage should be restored");
    assert!(matches!(
        event,
        crate::models::BusEvent::UsageUpdate {
            input_tokens: 6050,
            output_tokens: 198,
            cache_read_tokens: Some(8192),
            duration_api_ms: Some(16237),
            ..
        }
    ));

    let _ = std::fs::remove_dir_all(root);
}
