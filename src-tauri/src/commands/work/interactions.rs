use std::sync::Arc;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::conversations::{parse_work_recovery_action, recover_work_run_locked};
use super::tasks::retry_failed_work_run;
use super::{ensure_work_enabled, resume_work_session_after_approval, INBOX_DELIVERY_LOCK};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::apps::models::ConnectionStatus;
use crate::work::models::{
    InboxItem, InboxItemPayload, InboxItemStatus, InboxItemType, ToolRiskClass, WorkPolicy,
    WorkRecoveryAction, WorkRunStatus, WorkTaskState,
};
use crate::work::tasks::TaskManager;
use crate::work::{
    apps, connector_package, connector_package_manager, inbox::InboxManager, paths::WorkPaths,
    policy::PolicyEvaluator,
};

const AGENT_WORK_RESUME_CONTEXT_MARKER: &str = "[[AGENTCABIN_WORK_RESUME]]";

#[tauri::command]
pub async fn work_resolve_pending_approval(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    run_id: String,
    decision: String,
) -> Result<WorkTaskState, String> {
    resolve_pending_approval_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        run_id,
        decision,
    )
    .await
}

/// Headless-capable inner impl of [`work_resolve_pending_approval`] — takes
/// concrete state values instead of `tauri::State` extracts.
pub(crate) async fn resolve_pending_approval_impl(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    run_id: String,
    decision: String,
) -> Result<WorkTaskState, String> {
    ensure_work_enabled()?;
    let _state_lock = crate::work::task_state::lock(&run_id).await;
    let mut state = crate::work::task_state::load(&run_id)?
        .ok_or_else(|| format!("Work Run '{run_id}' has no task state"))?;
    let previous_state = state.clone();
    let pending = state
        .pending_approval
        .take()
        .ok_or_else(|| "当前 Work Run 没有等待确认的计划".to_string())?;

    match decision.as_str() {
        "approve" => state.plan = pending.steps,
        "revise" | "cancel" => {}
        _ => return Err(format!("不支持的计划确认结果：{decision}")),
    }
    state.revision += 1;
    state.updated_at = crate::models::now_iso();
    crate::work::task_state::save(&run_id, &state)?;
    for event in crate::work::task_state::bus_events(&run_id, &state) {
        emitter.persist_and_emit(&run_id, &event);
    }

    let paths = WorkPaths::app();
    let task_manager = TaskManager::new(paths.clone());
    let work_run = task_manager.find_run_by_session_id(&run_id)?;
    let next_status = if decision == "cancel" {
        WorkRunStatus::Cancelled
    } else {
        WorkRunStatus::Running
    };
    let controller = crate::work::lifecycle::WorkHarnessController::new(paths);
    if let Some(work_run) = work_run.as_ref() {
        controller.transition_run_status(&work_run.task_id, &work_run.id, next_status)?;
    }

    if decision != "cancel" {
        let continue_message = if decision == "approve" {
            "计划已确认，请继续执行，不要再次请求计划确认。"
        } else {
            "请根据用户反馈修改当前计划；修改完成后直接继续执行。"
        };
        if let Err(error) = resume_work_session_after_approval(
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            &run_id,
            continue_message,
        )
        .await
        {
            // A failed resume must not leave a silently cleared approval. Restore
            // the durable wait so the user can retry after the runtime recovers.
            crate::work::task_state::save(&run_id, &previous_state)?;
            for event in crate::work::task_state::bus_events(&run_id, &previous_state) {
                emitter.persist_and_emit(&run_id, &event);
            }
            if let Some(work_run) = work_run.as_ref() {
                if let Err(state_error) = controller.transition_run_status(
                    &work_run.task_id,
                    &work_run.id,
                    WorkRunStatus::WaitingApproval,
                ) {
                    return Err(format!(
                        "确认已保存，但 Work 会话恢复失败：{error}；等待状态恢复失败：{state_error}"
                    ));
                }
            }
            return Err(format!("确认已保存，但 Work 会话恢复失败：{error}"));
        }
    }

    Ok(state)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn work_create_inbox_item(
    app: tauri::AppHandle,
    task_id: String,
    run_id: String,
    workspace_id: String,
    item_type: InboxItemType,
    title: String,
    description: String,
    payload: InboxItemPayload,
) -> Result<InboxItem, String> {
    let item = create_inbox_item_impl(
        task_id,
        run_id,
        workspace_id,
        item_type,
        title,
        description,
        payload,
    )?;
    crate::agent::notify::notify_if_background(
        &app,
        "Work 待确认事项",
        &format!("「{}」等待您的处理", item.title),
    );
    Ok(item)
}

/// Headless-capable inner impl of [`work_create_inbox_item`] — no desktop
/// notification (that part requires a Tauri `AppHandle`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_inbox_item_impl(
    task_id: String,
    run_id: String,
    workspace_id: String,
    item_type: InboxItemType,
    title: String,
    description: String,
    payload: InboxItemPayload,
) -> Result<InboxItem, String> {
    ensure_work_enabled()?;
    let manager = InboxManager::new(WorkPaths::app());
    manager.create_item(
        &task_id,
        &run_id,
        &workspace_id,
        item_type,
        &title,
        &description,
        payload,
    )
}

#[tauri::command]
pub fn work_get_inbox_item(id: String) -> Result<InboxItem, String> {
    ensure_work_enabled()?;
    let manager = InboxManager::new(WorkPaths::app());
    manager.get_item(&id)
}

/// Resolve the durable WorkRun referenced by an Inbox item to the underlying
/// session run that the chat surface can reopen. Inbox items intentionally keep
/// the product-level WorkRun id; the UI must not guess that it is also the
/// session id.
#[tauri::command]
pub fn work_get_inbox_session_run(id: String) -> Result<Option<String>, String> {
    ensure_work_enabled()?;
    resolve_inbox_session_run(&WorkPaths::app(), &id)
}

pub(crate) fn resolve_inbox_session_run(
    paths: &WorkPaths,
    id: &str,
) -> Result<Option<String>, String> {
    let inbox_manager = InboxManager::new(paths.clone());
    let item = inbox_manager.get_item(id)?;
    let task_manager = TaskManager::new(paths.clone());
    Ok(task_manager
        .get_run(&item.task_id, &item.run_id)
        .ok()
        .and_then(|run| run.session_id)
        .or_else(|| {
            if crate::storage::runs::get_run(&item.run_id).is_some() {
                Some(item.run_id.clone())
            } else {
                None
            }
        }))
}

async fn ensure_connector_auth_ready(paths: &WorkPaths, item: &InboxItem) -> Result<(), String> {
    let connector_id = item
        .payload
        .connector_id
        .as_deref()
        .or(item.payload.app_id.as_deref())
        .ok_or_else(|| "连接器认证请求缺少 connectorId".to_string())?;
    let package = connector_package_manager::get_with_paths(paths, connector_id)?;

    match package.manifest.auth.kind {
        connector_package::ConnectorAuthKind::OAuth2 => {
            let status = apps::check_status_with_paths(paths, connector_id, None).await?;
            connector_package_manager::sync_auth_status_from_app_with_paths(
                paths,
                connector_id,
                status,
            )?;
            if status != ConnectionStatus::Connected {
                return Err(format!(
                    "连接器 '{}' 尚未完成授权（当前状态：{:?}）",
                    package.manifest.display_name, status
                ));
            }
        }
        connector_package::ConnectorAuthKind::ApiKey
        | connector_package::ConnectorAuthKind::Cli => {
            if package.state.auth_status != connector_package::ConnectorAuthStatus::Authenticated {
                return Err(format!(
                    "连接器 '{}' 尚未完成认证，请先完成 Host 配置或 CLI auth 操作",
                    package.manifest.display_name
                ));
            }
        }
        connector_package::ConnectorAuthKind::None => {}
    }
    Ok(())
}

#[tauri::command]
pub fn work_list_inbox_items(
    only_pending: Option<bool>,
    task_id: Option<String>,
) -> Result<Vec<InboxItem>, String> {
    ensure_work_enabled()?;
    let manager = InboxManager::new(WorkPaths::app());
    manager.list_items(only_pending.unwrap_or(false), task_id.as_deref())
}

#[tauri::command]
pub async fn work_resolve_inbox_item(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: tauri::State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    id: String,
    status: InboxItemStatus,
    response: Option<serde_json::Value>,
) -> Result<InboxItem, String> {
    ensure_work_enabled()?;
    resolve_inbox_item_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        id,
        status,
        response,
    )
    .await
}

pub(crate) async fn resolve_inbox_item_impl(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    id: String,
    status: InboxItemStatus,
    response: Option<serde_json::Value>,
) -> Result<InboxItem, String> {
    let _delivery_guard = INBOX_DELIVERY_LOCK.lock().await;
    let paths = WorkPaths::app();
    let inbox_manager = InboxManager::new(paths.clone());
    let task_manager = TaskManager::new(paths.clone());

    let existing_item = inbox_manager.get_item(&id)?;

    if existing_item.status != InboxItemStatus::Pending {
        return Ok(existing_item);
    }

    // Recovery Inbox items have explicit semantics that are different from a
    // normal permission grant. Route them through the same recovery controller
    // used by the Work recovery card; this prevents an "approve" click from
    // issuing a replay grant for an externally-uncertain call.
    if existing_item.payload.recovery_key.is_some() {
        let is_run_failure = existing_item
            .payload
            .recovery_key
            .as_deref()
            .is_some_and(|key| key.starts_with("run-failure:"));
        if is_run_failure {
            let requested_action = response
                .as_ref()
                .and_then(|value| value.get("recoveryAction").or_else(|| value.get("action")))
                .and_then(|value| value.as_str())
                .map(parse_work_recovery_action)
                .transpose()?
                .unwrap_or(match status {
                    InboxItemStatus::Rejected => WorkRecoveryAction::Retry,
                    InboxItemStatus::Cancelled | InboxItemStatus::Expired => {
                        WorkRecoveryAction::Cancel
                    }
                    _ => WorkRecoveryAction::Continue,
                });
            let interaction_manager =
                crate::work::interaction::InteractionManager::new(paths.clone());
            match requested_action {
                WorkRecoveryAction::Retry => {
                    let new_run = retry_failed_work_run(
                        &paths,
                        emitter,
                        sessions,
                        spawn_locks,
                        cancel_token,
                        &existing_item.task_id,
                        &existing_item.run_id,
                    )
                    .await?;
                    interaction_manager.commit_resolution_with_response(
                        &id,
                        crate::work::models::PendingInteractionState::Cancelled,
                        Some(InboxItemStatus::Rejected),
                        Some(serde_json::json!({
                            "recoveryDecision": "retry",
                            "newRunId": new_run.id,
                        })),
                    )?;
                }
                WorkRecoveryAction::Cancel => {
                    interaction_manager.commit_resolution_with_response(
                        &id,
                        crate::work::models::PendingInteractionState::Cancelled,
                        Some(InboxItemStatus::Cancelled),
                        Some(serde_json::json!({
                            "recoveryDecision": "cancel",
                        })),
                    )?;
                }
                _ => return Err("失败的自动化只能选择“重试”或“取消 Run”".to_string()),
            }
            return inbox_manager.get_item(&id);
        }

        let requested_action = response
            .as_ref()
            .and_then(|value| value.get("recoveryAction").or_else(|| value.get("action")))
            .and_then(|value| value.as_str())
            .map(parse_work_recovery_action)
            .transpose()?
            .unwrap_or(match status {
                InboxItemStatus::Approved | InboxItemStatus::Answered => {
                    WorkRecoveryAction::Continue
                }
                InboxItemStatus::Rejected => WorkRecoveryAction::Retry,
                InboxItemStatus::Cancelled | InboxItemStatus::Expired => WorkRecoveryAction::Cancel,
                InboxItemStatus::Pending => WorkRecoveryAction::Continue,
            });
        recover_work_run_locked(
            &paths,
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            &existing_item.workspace_id,
            &existing_item.task_id,
            &existing_item.run_id,
            requested_action,
            None,
        )
        .await?;
        return inbox_manager.get_item(&id);
    }

    let interaction_mgr = crate::work::interaction::InteractionManager::new(paths.clone());
    let interaction = interaction_mgr.get_interaction(&id).ok();

    let run = task_manager
        .get_run(&existing_item.task_id, &existing_item.run_id)
        .ok();
    let session_id = run
        .as_ref()
        .and_then(|r| r.session_id.as_deref())
        .unwrap_or(&existing_item.run_id);
    let actor_is_alive = {
        let map = sessions.lock().await;
        map.contains_key(session_id)
    };

    let kind =
        interaction
            .as_ref()
            .map(|i| i.kind.clone())
            .unwrap_or(match existing_item.item_type {
                InboxItemType::QuestionElicitation => {
                    crate::work::models::PendingInteractionKind::UserInput
                }
                InboxItemType::PlanApproval => {
                    crate::work::models::PendingInteractionKind::PlanApproval
                }
                InboxItemType::ArtifactValidation => {
                    crate::work::models::PendingInteractionKind::ArtifactValidation
                }
                InboxItemType::AccessRootRequest => {
                    crate::work::models::PendingInteractionKind::AccessRootRequest
                }
                InboxItemType::AppConnectionRequest => {
                    crate::work::models::PendingInteractionKind::AppConnectionRequest
                }
                InboxItemType::ConnectorAuthRequest => {
                    crate::work::models::PendingInteractionKind::ConnectorAuthRequest
                }
                InboxItemType::PermissionRequest => {
                    crate::work::models::PendingInteractionKind::Permission
                }
            });

    let runtime_request_id = interaction
        .as_ref()
        .and_then(|i| i.runtime_request_id.as_deref())
        .or(existing_item.payload.request_id.as_deref());

    let target_state = match status {
        InboxItemStatus::Approved | InboxItemStatus::Answered => {
            crate::work::models::PendingInteractionState::Resolved
        }
        InboxItemStatus::Rejected => crate::work::models::PendingInteractionState::Cancelled,
        InboxItemStatus::Cancelled => crate::work::models::PendingInteractionState::Cancelled,
        InboxItemStatus::Expired => crate::work::models::PendingInteractionState::Cancelled,
        InboxItemStatus::Pending => return Ok(existing_item),
    };

    if matches!(
        status,
        InboxItemStatus::Approved | InboxItemStatus::Answered
    ) && existing_item.item_type == InboxItemType::ConnectorAuthRequest
    {
        ensure_connector_auth_ready(&paths, &existing_item).await?;
    }

    // 1. Prepare grant and mark Delivering BEFORE waking any actor or dispatching protocol response
    let (delivering_interaction, _issued_grant, _is_first) = interaction_mgr
        .prepare_resolution_and_issue_grant(&id, target_state, Some(status), response.clone())?;

    let locked_target_state = delivering_interaction
        .prepared_target_state
        .unwrap_or(target_state);
    let locked_inbox_status = delivering_interaction
        .prepared_inbox_status
        .unwrap_or(status);
    let is_approved = locked_target_state == crate::work::models::PendingInteractionState::Resolved;

    // 2. Dispatch live response or resume dead actor
    if let Some(request_id) = runtime_request_id {
        if actor_is_alive {
            let dispatch_res = match kind {
                crate::work::models::PendingInteractionKind::UserInput => {
                    let answers = response
                        .as_ref()
                        .and_then(|value| value.get("answers").cloned())
                        .or(response.clone())
                        .unwrap_or_else(|| serde_json::json!({}));
                    crate::commands::session::respond_user_input_impl(
                        sessions,
                        session_id.to_string(),
                        request_id.to_string(),
                        answers,
                    )
                    .await
                }
                crate::work::models::PendingInteractionKind::Permission => {
                    let behavior = if is_approved { "allow" } else { "deny" };
                    crate::commands::session::respond_permission_impl(
                        sessions,
                        session_id.to_string(),
                        request_id.to_string(),
                        behavior.to_string(),
                        None,
                        if is_approved { response.clone() } else { None },
                        if is_approved {
                            None
                        } else {
                            Some("User denied action in Inbox".to_string())
                        },
                        Some(!is_approved),
                    )
                    .await
                }
                _ => Ok(()),
            };

            if let Err(dispatch_err) = dispatch_res {
                log::warn!(
                    "[work/inbox] Failed to dispatch actor response for item {id}: {dispatch_err}"
                );
                let waiting_status = match kind {
                    crate::work::models::PendingInteractionKind::UserInput
                    | crate::work::models::PendingInteractionKind::AppConnectionRequest
                    | crate::work::models::PendingInteractionKind::ConnectorAuthRequest => {
                        WorkRunStatus::WaitingInput
                    }
                    _ => WorkRunStatus::WaitingApproval,
                };
                if run.is_some() {
                    let controller =
                        crate::work::lifecycle::WorkHarnessController::new(paths.clone());
                    if let Err(state_error) = controller.transition_run_status(
                        &existing_item.task_id,
                        &existing_item.run_id,
                        waiting_status,
                    ) {
                        return Err(format!(
                            "Failed to deliver Inbox response: {dispatch_err}; additionally failed to restore WorkRun state: {state_error}"
                        ));
                    }
                }
                return Err(format!("Failed to deliver Inbox response: {dispatch_err}"));
            }
        }
    }

    let is_polling_tool_approval = matches!(
        kind,
        crate::work::models::PendingInteractionKind::Permission
            | crate::work::models::PendingInteractionKind::AccessRootRequest
            | crate::work::models::PendingInteractionKind::ConnectorAuthRequest
            | crate::work::models::PendingInteractionKind::UserInput
    ) && runtime_request_id.is_none();

    let needs_runtime_resume = if !actor_is_alive {
        true
    } else if is_polling_tool_approval {
        // Polling Work tools (including ask_questions) are actively waiting inside the Pi
        // extension's waitForWorkInboxResolution loop. They unblock automatically when the
        // inbox status is updated and must NOT receive an extra user resume message, which
        // would cause duplicate tool execution.
        false
    } else {
        runtime_request_id.is_none()
            || matches!(
                kind,
                crate::work::models::PendingInteractionKind::PlanApproval
                    | crate::work::models::PendingInteractionKind::ArtifactValidation
                    | crate::work::models::PendingInteractionKind::AppConnectionRequest
                    | crate::work::models::PendingInteractionKind::ConnectorAuthRequest
            )
    };

    if needs_runtime_resume {
        let continue_message = if is_approved {
            if let Some(app_id) = delivering_interaction
                .payload
                .get("appId")
                .and_then(|v| v.as_str())
            {
                format!("应用 '{app_id}' 已成功连接并完成授权，请继续执行刚才的任务。")
            } else if let Some(connector_id) = delivering_interaction
                .payload
                .get("connectorId")
                .and_then(|v| v.as_str())
            {
                format!("连接器 '{connector_id}' 已完成认证，请继续执行刚才的任务。")
            } else if let Some(tool_name) = delivering_interaction
                .payload
                .get("toolName")
                .and_then(|v| v.as_str())
            {
                if tool_name == "ask_questions" {
                    let answer_context = response
                        .as_ref()
                        .map(|value| {
                            serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
                        })
                        .unwrap_or_else(|| "{}".to_string());
                    format!(
                        "{AGENT_WORK_RESUME_CONTEXT_MARKER}已收到用户对当前问题的回答：{answer_context}。请将其中的 answers 作为当前任务的用户输入，继续完成刚才的任务。"
                    )
                } else {
                    format!("用户已批准执行操作 '{tool_name}'。请立即继续执行该操作。")
                }
            } else {
                "用户已确认，请继续完成刚才暂停的 Work 操作。".to_string()
            }
        } else {
            "用户已拒绝刚才的 Work 操作，请停止该操作并继续给出结果。".to_string()
        };

        if let Err(error) = resume_work_session_after_approval(
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            session_id,
            &continue_message,
        )
        .await
        {
            log::error!(
                "[work/inbox] Approval grant persisted for {id}, but Pi session resume failed: {error}"
            );
            if run.is_some() {
                let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
                if let Err(state_error) = controller.transition_run_status(
                    &existing_item.task_id,
                    &existing_item.run_id,
                    WorkRunStatus::WaitingApproval,
                ) {
                    return Err(format!(
                        "审批 Grant 已持久化生成，但会话恢复失败：{error}；状态恢复写回失败：{state_error}"
                    ));
                }
            }
            return Err(format!(
                "审批 Grant 已持久化生成，但会话恢复失败：{error}。状态已保留，请重试会话连接。"
            ));
        }
    }

    // 3. Commit final resolution state and transition WorkRun -> Running (or Cancelled if explicit cancel)
    interaction_mgr.commit_resolution(&id, locked_target_state, Some(locked_inbox_status))?;
    let item = inbox_manager.get_item(&id)?;

    let next_status = match locked_inbox_status {
        InboxItemStatus::Cancelled => crate::work::models::WorkRunStatus::Cancelled,
        _ => crate::work::models::WorkRunStatus::Running,
    };
    if run.is_some() {
        let controller = crate::work::lifecycle::WorkHarnessController::new(paths);
        controller.transition_run_status(&item.task_id, &item.run_id, next_status)?;
    }

    Ok(item)
}

#[tauri::command]
pub async fn work_delete_inbox_item(id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let inbox_manager = InboxManager::new(paths.clone());
    let item = inbox_manager.get_item(&id)?;
    if item.status == InboxItemStatus::Pending {
        return Err("无法删除待处理事项，请先处理或取消该事项".to_string());
    }
    let interaction_mgr = crate::work::interaction::InteractionManager::new(paths);
    let _ = interaction_mgr.delete_interaction(&id);
    inbox_manager.delete_item(&id)
}

#[tauri::command]
pub async fn work_clear_inbox_items(
    workspace_id: Option<String>,
    only_resolved: Option<bool>,
) -> Result<usize, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let inbox_manager = InboxManager::new(paths.clone());
    let interaction_mgr = crate::work::interaction::InteractionManager::new(paths);
    let items = inbox_manager.list_items(false, None)?;
    let mut count = 0;
    // Pending items are durable recovery anchors. Never remove them through a
    // bulk cleanup operation; resolve/cancel them through the interaction flow.
    let _only_resolved_requested = only_resolved.unwrap_or(true);
    for item in items {
        if let Some(ws_id) = workspace_id.as_deref() {
            if !ws_id.trim().is_empty() && item.workspace_id != ws_id {
                continue;
            }
        }
        if item.status == InboxItemStatus::Pending {
            continue;
        }
        interaction_mgr.delete_interaction(&item.id)?;
        inbox_manager.delete_item(&item.id)?;
        count += 1;
    }
    Ok(count)
}

#[tauri::command]
pub fn work_evaluate_tool_risk(tool_name: String) -> Result<ToolRiskClass, String> {
    ensure_work_enabled()?;
    Ok(PolicyEvaluator::classify_tool_risk(&tool_name))
}

#[tauri::command]
pub fn work_check_confirmation_required(
    policy: WorkPolicy,
    tool_name: String,
    target: String,
) -> Result<bool, String> {
    ensure_work_enabled()?;
    Ok(PolicyEvaluator::requires_confirmation(
        &policy, &tool_name, &target,
    ))
}
