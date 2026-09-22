//! Durable PendingInteraction primitive.
//!
//! Unifies permission requests, user input, plan approvals, and reviews under a
//! single durable interaction identity. Attended inline UI and Unattended Inbox
//! are two presentation surfaces of the same interaction.

use chrono::Utc;
use once_cell::sync::Lazy;
use std::fs;
use std::sync::Mutex;
use uuid::Uuid;

use crate::work::models::{
    ApprovalGrant, ApprovalOutcome, InboxItem, InboxItemPayload, InboxItemStatus, InboxItemType,
    PendingInteraction, PendingInteractionKind, PendingInteractionState,
};
use crate::work::paths::{validate_workspace_id, WorkPaths};

static INTERACTION_RESOLVE_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone)]
pub struct InteractionManager {
    paths: WorkPaths,
}

impl InteractionManager {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    /// Create a durable PendingInteraction. Also synchronizes to the Inbox store.
    #[allow(clippy::too_many_arguments)]
    pub fn create_interaction(
        &self,
        task_id: &str,
        work_run_id: &str,
        workspace_id: &str,
        session_id: Option<&str>,
        runtime_request_id: Option<&str>,
        tool_call_id: Option<&str>,
        kind: PendingInteractionKind,
        title: &str,
        description: &str,
        payload: serde_json::Value,
    ) -> Result<PendingInteraction, String> {
        self.paths.ensure_layout()?;
        validate_workspace_id(task_id)?;
        if !workspace_id.trim().is_empty() {
            validate_workspace_id(workspace_id)?;
        }

        let id = format!("interaction-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let interaction = PendingInteraction {
            interaction_id: id.clone(),
            task_id: task_id.to_string(),
            work_run_id: work_run_id.to_string(),
            session_id: session_id.map(String::from),
            runtime_request_id: runtime_request_id.map(String::from),
            tool_call_id: tool_call_id.map(String::from),
            kind: kind.clone(),
            state: PendingInteractionState::Pending,
            title: title.to_string(),
            description: description.to_string(),
            payload: payload.clone(),
            prepared_target_state: None,
            prepared_inbox_status: None,
            resolution: None,
            created_at: now.clone(),
            resolved_at: None,
        };

        self.save_interaction(&interaction)?;

        // Map to InboxItem so existing Inbox consumers and APIs continue to work seamlessly
        let inbox_type = match kind {
            PendingInteractionKind::Permission => InboxItemType::PermissionRequest,
            PendingInteractionKind::UserInput => InboxItemType::QuestionElicitation,
            PendingInteractionKind::PlanApproval => InboxItemType::PlanApproval,
            PendingInteractionKind::ArtifactValidation => InboxItemType::ArtifactValidation,
            PendingInteractionKind::AccessRootRequest => InboxItemType::AccessRootRequest,
            PendingInteractionKind::AppConnectionRequest => InboxItemType::AppConnectionRequest,
            PendingInteractionKind::ConnectorAuthRequest => InboxItemType::ConnectorAuthRequest,
            PendingInteractionKind::Custom(_) => InboxItemType::PermissionRequest,
        };

        let inbox_payload = InboxItemPayload {
            request_id: runtime_request_id.map(String::from),
            interaction_kind: serde_json::to_value(&kind)
                .ok()
                .and_then(|value| value.as_str().map(String::from)),
            tool_use_id: tool_call_id.map(String::from),
            tool_name: payload
                .get("toolName")
                .and_then(|v| v.as_str())
                .map(String::from),
            parameters: payload.get("parameters").cloned(),
            question: payload
                .get("question")
                .and_then(|v| v.as_str())
                .map(String::from),
            proposed_plan: payload
                .get("proposedPlan")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            artifact_id: payload
                .get("artifactId")
                .and_then(|v| v.as_str())
                .map(String::from),
            standing_rule_proposal: payload
                .get("standingRuleProposal")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            app_id: payload
                .get("appId")
                .and_then(|v| v.as_str())
                .map(String::from),
            connector_id: payload
                .get("connectorId")
                .and_then(|v| v.as_str())
                .map(String::from),
            runtime_kind: payload
                .get("runtimeKind")
                .and_then(|v| v.as_str())
                .map(String::from),
            provider: payload
                .get("provider")
                .and_then(|v| v.as_str())
                .map(String::from),
            account_id: payload
                .get("accountId")
                .and_then(|v| v.as_str())
                .map(String::from),
            requested_scopes: payload
                .get("requestedScopes")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            // Never persist authUrl in a new Inbox item. The UI must obtain a
            // fresh, provider-validated URL when the user starts auth.
            auth_url: None,
            recovery_key: payload
                .get("recoveryKey")
                .and_then(|v| v.as_str())
                .map(String::from),
            recovery_action: payload
                .get("recoveryAction")
                .and_then(|v| v.as_str())
                .map(String::from),
            original_tool_call_id: payload
                .get("originalToolCallId")
                .and_then(|v| v.as_str())
                .map(String::from),
            side_effect_class: payload
                .get("sideEffectClass")
                .and_then(|v| v.as_str())
                .map(String::from),
            expected_outputs: payload
                .get("expectedOutputs")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            failure_kind: payload
                .get("failureKind")
                .and_then(|v| v.as_str())
                .map(String::from),
            failure_reason: payload
                .get("failureReason")
                .and_then(|v| v.as_str())
                .map(String::from),
            available_actions: payload
                .get("availableActions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        };

        let inbox_item = InboxItem {
            id: id.clone(),
            task_id: task_id.to_string(),
            run_id: work_run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            item_type: inbox_type,
            status: InboxItemStatus::Pending,
            title: title.to_string(),
            description: description.to_string(),
            payload: inbox_payload,
            response: None,
            created_at: now,
            resolved_at: None,
        };

        let inbox_path = self.paths.inbox_item_path(&id)?;
        let bytes = serde_json::to_vec_pretty(&inbox_item)
            .map_err(|e| format!("Failed to serialize inbox item: {e}"))?;
        fs::write(&inbox_path, bytes).map_err(|e| e.to_string())?;

        Ok(interaction)
    }

    /// Create or reuse the single durable Inbox prompt for a recovered
    /// invocation. The recovery key is stable across startup reconciliation,
    /// so repeated scans cannot create duplicate human decisions.
    #[allow(clippy::too_many_arguments)]
    pub fn create_recovery_interaction(
        &self,
        task_id: &str,
        work_run_id: &str,
        workspace_id: &str,
        session_id: Option<&str>,
        runtime_request_id: Option<&str>,
        tool_call_id: Option<&str>,
        kind: PendingInteractionKind,
        title: &str,
        description: &str,
        payload: serde_json::Value,
    ) -> Result<PendingInteraction, String> {
        let _guard = INTERACTION_RESOLVE_MUTEX
            .lock()
            .map_err(|error| error.to_string())?;
        let recovery_key = payload
            .get("recoveryKey")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Recovery interaction is missing recoveryKey".to_string())?;
        if let Some(existing) = self
            .list_pending(Some(task_id), Some(work_run_id))?
            .into_iter()
            .find(|interaction| {
                interaction.tool_call_id.as_deref() == tool_call_id
                    && interaction
                        .payload
                        .get("recoveryKey")
                        .and_then(|value| value.as_str())
                        == Some(recovery_key)
            })
        {
            return Ok(existing);
        }
        self.create_interaction(
            task_id,
            work_run_id,
            workspace_id,
            session_id,
            runtime_request_id,
            tool_call_id,
            kind,
            title,
            description,
            payload,
        )
    }

    /// Retrieve an interaction by ID.
    pub fn get_interaction(&self, id: &str) -> Result<PendingInteraction, String> {
        let path = self
            .paths
            .inbox_dir()
            .join(format!("{id}.interaction.json"));
        if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read interaction {id}: {e}"))?;
            return serde_json::from_str(&content)
                .map_err(|e| format!("Failed to deserialize interaction {id}: {e}"));
        }

        // Backward-compat: check if Inbox item exists and synthesize PendingInteraction
        let inbox_path = self.paths.inbox_item_path(id)?;
        if inbox_path.exists() {
            let content = fs::read_to_string(&inbox_path)
                .map_err(|e| format!("Failed to read inbox item {id}: {e}"))?;
            let item = serde_json::from_str::<InboxItem>(&content)
                .map_err(|e| format!("Failed to deserialize inbox item {id}: {e}"))?;
            let kind = match item.item_type {
                InboxItemType::PermissionRequest => PendingInteractionKind::Permission,
                InboxItemType::QuestionElicitation => PendingInteractionKind::UserInput,
                InboxItemType::PlanApproval => PendingInteractionKind::PlanApproval,
                InboxItemType::ArtifactValidation => PendingInteractionKind::ArtifactValidation,
                InboxItemType::AccessRootRequest => PendingInteractionKind::AccessRootRequest,
                InboxItemType::AppConnectionRequest => PendingInteractionKind::AppConnectionRequest,
                InboxItemType::ConnectorAuthRequest => PendingInteractionKind::ConnectorAuthRequest,
            };
            let state = match item.status {
                InboxItemStatus::Pending => PendingInteractionState::Pending,
                InboxItemStatus::Approved | InboxItemStatus::Answered => {
                    PendingInteractionState::Resolved
                }
                InboxItemStatus::Rejected
                | InboxItemStatus::Cancelled
                | InboxItemStatus::Expired => PendingInteractionState::Cancelled,
            };
            let payload_value = serde_json::to_value(&item.payload).unwrap_or_default();
            return Ok(PendingInteraction {
                interaction_id: item.id,
                task_id: item.task_id,
                work_run_id: item.run_id,
                session_id: None,
                runtime_request_id: item.payload.request_id,
                tool_call_id: item.payload.tool_use_id,
                kind,
                state,
                title: item.title,
                description: item.description,
                payload: payload_value,
                prepared_target_state: None,
                prepared_inbox_status: None,
                resolution: item.response,
                created_at: item.created_at,
                resolved_at: item.resolved_at,
            });
        }

        Err(format!("Interaction not found: {id}"))
    }

    /// List all pending interactions for a run or task.
    pub fn list_pending(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<Vec<PendingInteraction>, String> {
        self.paths.ensure_layout()?;
        let inbox_dir = self.paths.inbox_dir();
        let entries = fs::read_dir(&inbox_dir).map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".interaction.json") {
                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let Ok(interaction) = serde_json::from_str::<PendingInteraction>(&content) else {
                    continue;
                };
                if interaction.state != PendingInteractionState::Pending
                    && interaction.state != PendingInteractionState::Delivering
                {
                    continue;
                }
                if let Some(t) = task_id {
                    if interaction.task_id != t {
                        continue;
                    }
                }
                if let Some(r) = run_id {
                    if interaction.work_run_id != r {
                        continue;
                    }
                }
                results.push(interaction);
            }
        }
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(results)
    }

    /// Find the durable interaction for an idempotent tool call, regardless of
    /// whether the user has already answered it. Work's polling tools retry the
    /// same call after a response, so looking only at pending interactions would
    /// incorrectly create a second question.
    pub fn find_by_tool_call(
        &self,
        task_id: &str,
        run_id: &str,
        tool_call_id: &str,
        arguments_hash: &str,
    ) -> Result<Option<PendingInteraction>, String> {
        self.paths.ensure_layout()?;
        let entries = fs::read_dir(self.paths.inbox_dir()).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".interaction.json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(interaction) = serde_json::from_str::<PendingInteraction>(&content) else {
                continue;
            };
            if interaction.task_id == task_id
                && interaction.work_run_id == run_id
                && interaction.tool_call_id.as_deref() == Some(tool_call_id)
                && interaction
                    .payload
                    .get("argumentsHash")
                    .and_then(|value| value.as_str())
                    == Some(arguments_hash)
            {
                return Ok(Some(interaction));
            }
        }
        Ok(None)
    }

    /// Prepare grant and transition PendingInteraction to Delivering before waking runtime.
    pub fn prepare_resolution_and_issue_grant(
        &self,
        id: &str,
        target_state: PendingInteractionState,
        inbox_status: Option<InboxItemStatus>,
        resolution: Option<serde_json::Value>,
    ) -> Result<(PendingInteraction, Option<ApprovalGrant>, bool), String> {
        let _guard = INTERACTION_RESOLVE_MUTEX
            .lock()
            .map_err(|e| e.to_string())?;

        let mut interaction = self.get_interaction(id)?;
        if interaction.state != PendingInteractionState::Pending {
            // If already delivering, enforce first-responder-wins immutability
            if interaction.state == PendingInteractionState::Delivering {
                if let Some(prepared) = interaction.prepared_target_state {
                    if prepared != target_state {
                        return Err(format!(
                            "Interaction '{id}' is already locked in prior {:?} decision (first responder wins)",
                            prepared
                        ));
                    }
                }
                if let (Some(prepared_status), Some(incoming_status)) =
                    (interaction.prepared_inbox_status, inbox_status)
                {
                    if prepared_status != incoming_status {
                        return Err(format!(
                            "Interaction '{id}' is already locked in prior inbox status {:?} (first responder wins, cannot be changed to {:?})",
                            prepared_status, incoming_status
                        ));
                    }
                }
            }
            return Ok((interaction, None, false));
        }

        let now = Utc::now().to_rfc3339();
        let mut issued_grant = None;

        if target_state == PendingInteractionState::Resolved
            && matches!(
                &interaction.kind,
                PendingInteractionKind::Permission | PendingInteractionKind::AccessRootRequest
            )
        {
            if let Some(tool_call_id) = &interaction.tool_call_id {
                let tool_name = interaction
                    .payload
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let action = interaction
                    .payload
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments_hash = interaction
                    .payload
                    .get("argumentsHash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let workspace_id = interaction
                    .payload
                    .get("workspaceId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let grant = ApprovalGrant {
                    grant_id: format!("grant-{}", Uuid::new_v4()),
                    interaction_id: interaction.interaction_id.clone(),
                    task_id: interaction.task_id.clone(),
                    work_run_id: interaction.work_run_id.clone(),
                    session_id: interaction.session_id.clone(),
                    tool_call_id: tool_call_id.clone(),
                    tool_name,
                    action,
                    arguments_hash,
                    workspace_id,
                    execution_lane: interaction
                        .payload
                        .get("executionLane")
                        .and_then(|value| value.as_str())
                        .map(String::from),
                    outcome: ApprovalOutcome::AllowedOnce,
                    consumed: false,
                    created_at: now.clone(),
                    consumed_at: None,
                };
                self.save_grant(&grant)?;

                if let Ok(ledger) = crate::work::ledger::WorkRuntimeLedger::open(
                    &self.paths,
                    &interaction.task_id,
                    &interaction.work_run_id,
                ) {
                    let _ = ledger.record(&crate::work::models::RuntimeFact::GrantIssued {
                        grant_id: grant.grant_id.clone(),
                        interaction_id: grant.interaction_id.clone(),
                        tool_call_id: tool_call_id.clone(),
                        outcome: ApprovalOutcome::AllowedOnce,
                        timestamp: now.clone(),
                    });
                }

                issued_grant = Some(grant);
            }
        }

        interaction.state = PendingInteractionState::Delivering;
        interaction.prepared_target_state = Some(target_state);
        interaction.prepared_inbox_status = inbox_status;
        interaction.resolution = resolution;
        self.save_interaction(&interaction)?;

        Ok((interaction, issued_grant, true))
    }

    /// Commit the final resolved state after delivery / runtime resumption succeeds.
    pub fn commit_resolution(
        &self,
        id: &str,
        final_state: PendingInteractionState,
        inbox_status: Option<InboxItemStatus>,
    ) -> Result<PendingInteraction, String> {
        self.commit_resolution_with_response(id, final_state, inbox_status, None)
    }

    /// Commit a durable interaction resolution while allowing recovery flows
    /// to persist their explicit decision without issuing a normal tool grant.
    /// Recovery decisions describe an already-uncertain invocation and must
    /// never silently authorize replay of that original call.
    pub fn commit_resolution_with_response(
        &self,
        id: &str,
        final_state: PendingInteractionState,
        inbox_status: Option<InboxItemStatus>,
        response: Option<serde_json::Value>,
    ) -> Result<PendingInteraction, String> {
        let _guard = INTERACTION_RESOLVE_MUTEX
            .lock()
            .map_err(|e| e.to_string())?;

        let mut interaction = self.get_interaction(id)?;
        if interaction.state == PendingInteractionState::Resolved
            || interaction.state == PendingInteractionState::Cancelled
        {
            return Ok(interaction);
        }

        let now = Utc::now().to_rfc3339();
        if response.is_some() {
            interaction.resolution = response;
        }
        interaction.state = final_state;
        interaction.resolved_at = Some(now.clone());
        self.save_interaction(&interaction)?;

        let final_inbox_status = inbox_status
            .or(interaction.prepared_inbox_status)
            .unwrap_or(match final_state {
                PendingInteractionState::Resolved => InboxItemStatus::Approved,
                PendingInteractionState::Cancelled => InboxItemStatus::Rejected,
                _ => InboxItemStatus::Cancelled,
            });

        // Update corresponding InboxItem
        let inbox_path = self.paths.inbox_item_path(id)?;
        if inbox_path.exists() {
            if let Ok(content) = fs::read_to_string(&inbox_path) {
                if let Ok(mut item) = serde_json::from_str::<InboxItem>(&content) {
                    item.status = final_inbox_status;
                    item.response = interaction.resolution.clone();
                    item.resolved_at = Some(now.clone());
                    let bytes = serde_json::to_vec_pretty(&item).unwrap_or_default();
                    let _ = crate::work::models::atomic_replace_file(&inbox_path, &bytes);
                }
            }
        }

        // Record resolution in ledger
        if let Ok(ledger) = crate::work::ledger::WorkRuntimeLedger::open(
            &self.paths,
            &interaction.task_id,
            &interaction.work_run_id,
        ) {
            let is_approved = final_state == PendingInteractionState::Resolved;
            let _ = ledger.record(&crate::work::models::RuntimeFact::ApprovalResolved {
                interaction_id: interaction.interaction_id.clone(),
                resolution: interaction.resolution.clone().unwrap_or_default(),
                approved: is_approved,
                timestamp: now.clone(),
            });

            // If permission or access root request was rejected, record durable ToolResult(status="denied")
            if !is_approved
                && (interaction.kind == PendingInteractionKind::Permission
                    || interaction.kind == PendingInteractionKind::AccessRootRequest)
            {
                if let Some(tool_call_id) = &interaction.tool_call_id {
                    let _ = ledger.record(&crate::work::models::RuntimeFact::ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        success: false,
                        status: "denied".to_string(),
                        failure_kind: None,
                        exit_code: Some(1),
                        error: Some("Action denied by user in Inbox".to_string()),
                        outputs: vec![],
                        side_effect_class: crate::work::models::SideEffectClass::Read,
                        timestamp: now,
                    });
                }
            }
        }

        Ok(interaction)
    }

    /// First-responder-wins, idempotent interaction resolution.
    pub fn resolve_interaction_once(
        &self,
        id: &str,
        next_state: PendingInteractionState,
        resolution: Option<serde_json::Value>,
    ) -> Result<(PendingInteraction, bool), String> {
        let (delivering, _grant, is_new) =
            self.prepare_resolution_and_issue_grant(id, next_state, None, resolution)?;
        if !is_new {
            return Ok((delivering, false));
        }
        let resolved = self.commit_resolution(id, next_state, None)?;
        Ok((resolved, true))
    }

    pub fn save_grant(&self, grant: &ApprovalGrant) -> Result<(), String> {
        let path = self
            .paths
            .inbox_dir()
            .join(format!("{}.grant.json", grant.grant_id));
        let bytes = serde_json::to_vec_pretty(grant)
            .map_err(|e| format!("Failed to serialize grant: {e}"))?;
        crate::work::models::atomic_replace_file(&path, &bytes)
    }

    pub fn get_grant(&self, grant_id: &str) -> Result<ApprovalGrant, String> {
        let path = self
            .paths
            .inbox_dir()
            .join(format!("{grant_id}.grant.json"));
        let content = fs::read_to_string(&path)
            .map_err(|_| format!("ApprovalGrant not found: {grant_id}"))?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn find_matching_grant(
        &self,
        task_id: &str,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        action: &str,
        arguments_hash: &str,
        workspace_id: &str,
    ) -> Result<Option<ApprovalGrant>, String> {
        self.paths.ensure_layout()?;
        let inbox_dir = self.paths.inbox_dir();
        let entries = fs::read_dir(&inbox_dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".grant.json") {
                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let Ok(grant) = serde_json::from_str::<ApprovalGrant>(&content) else {
                    continue;
                };
                if grant.task_id == task_id
                    && grant.work_run_id == run_id
                    && (grant.tool_call_id == tool_call_id
                        || (grant.tool_name == tool_name
                            && (grant.action.is_empty() || grant.action == action)
                            && (grant.arguments_hash.is_empty()
                                || grant.arguments_hash == arguments_hash)
                            && (grant.workspace_id.is_empty()
                                || grant.workspace_id == workspace_id)))
                    && grant.tool_name == tool_name
                    && (grant.action.is_empty() || grant.action == action)
                    && (grant.arguments_hash.is_empty() || grant.arguments_hash == arguments_hash)
                    && (grant.workspace_id.is_empty() || grant.workspace_id == workspace_id)
                    && !grant.consumed
                {
                    return Ok(Some(grant));
                }
            }
        }
        Ok(None)
    }

    pub fn consume_grant(&self, grant_id: &str) -> Result<ApprovalGrant, String> {
        let _guard = INTERACTION_RESOLVE_MUTEX
            .lock()
            .map_err(|e| e.to_string())?;

        let mut grant = self.get_grant(grant_id)?;
        if grant.consumed {
            return Err(format!(
                "ApprovalGrant '{grant_id}' has already been consumed"
            ));
        }

        grant.consumed = true;
        grant.consumed_at = Some(Utc::now().to_rfc3339());
        self.save_grant(&grant)?;
        Ok(grant)
    }

    /// Delete an interaction file from disk.
    pub fn delete_interaction(&self, id: &str) -> Result<(), String> {
        let path = self
            .paths
            .inbox_dir()
            .join(format!("{id}.interaction.json"));
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        Ok(())
    }

    fn save_interaction(&self, interaction: &PendingInteraction) -> Result<(), String> {
        let path = self
            .paths
            .inbox_dir()
            .join(format!("{}.interaction.json", interaction.interaction_id));
        let bytes = serde_json::to_vec_pretty(interaction)
            .map_err(|e| format!("Failed to serialize interaction: {e}"))?;
        crate::work::models::atomic_replace_file(&path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn standing_rule_proposal_is_carried_into_inbox_payload() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-sr",
                "run-sr",
                "ws-sr",
                None,
                None,
                Some("call-sr"),
                PendingInteractionKind::Permission,
                "Approval Needed: work-excel.export_table",
                "Execution is paused pending human approval.",
                serde_json::json!({
                    "toolName": "work_execute",
                    "parameters": {"resource_id": "work-excel", "action": "export_table"},
                    "standingRuleProposal": {
                        "toolName": "work_execute",
                        "targetPattern": "work-excel.*",
                        "scope": "task"
                    }
                }),
            )
            .unwrap();

        // Inbox item must carry the proposal so approval UIs can offer "always allow"
        let inbox_path = paths.inbox_item_path(&interaction.interaction_id).unwrap();
        let item: crate::work::models::InboxItem =
            serde_json::from_slice(&std::fs::read(&inbox_path).unwrap()).unwrap();
        let proposal = item
            .payload
            .standing_rule_proposal
            .expect("standing rule proposal must be present");
        assert_eq!(proposal.tool_name, "work_execute");
        assert_eq!(proposal.target_pattern, "work-excel.*");
        assert_eq!(proposal.scope, "task");
    }

    #[test]
    fn interaction_without_proposal_keeps_inbox_payload_empty() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-np",
                "run-np",
                "ws-np",
                None,
                None,
                Some("call-np"),
                PendingInteractionKind::Permission,
                "Approval Needed: ls -la",
                "Execution is paused pending human approval.",
                serde_json::json!({"toolName": "work_run_command", "parameters": {"target": "ls -la"}}),
            )
            .unwrap();

        let inbox_path = paths.inbox_item_path(&interaction.interaction_id).unwrap();
        let item: crate::work::models::InboxItem =
            serde_json::from_slice(&std::fs::read(&inbox_path).unwrap()).unwrap();
        assert!(item.payload.standing_rule_proposal.is_none());
    }

    #[test]
    fn recovery_interaction_reuses_the_same_pending_inbox_item() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths.clone());
        let payload = serde_json::json!({
            "recoveryKey": "run-recovery:call-1",
            "recoveryAction": "external_unknown_outcome",
            "originalToolCallId": "call-1",
            "sideEffectClass": "external_mutating",
            "expectedOutputs": []
        });

        let first = manager
            .create_recovery_interaction(
                "task-recovery",
                "run-recovery",
                "ws-recovery",
                Some("session-recovery"),
                Some("request-recovery"),
                Some("call-1"),
                PendingInteractionKind::Permission,
                "Restart Recovery",
                "The external result is unknown.",
                payload.clone(),
            )
            .unwrap();
        let second = manager
            .create_recovery_interaction(
                "task-recovery",
                "run-recovery",
                "ws-recovery",
                Some("session-recovery"),
                Some("request-recovery"),
                Some("call-1"),
                PendingInteractionKind::Permission,
                "Restart Recovery",
                "The external result is unknown.",
                payload,
            )
            .unwrap();

        assert_eq!(first.interaction_id, second.interaction_id);
        let pending = manager
            .list_pending(Some("task-recovery"), Some("run-recovery"))
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].session_id.as_deref(), Some("session-recovery"));
        assert_eq!(
            pending[0].runtime_request_id.as_deref(),
            Some("request-recovery")
        );

        let inbox = crate::work::inbox::InboxManager::new(paths)
            .get_item(&first.interaction_id)
            .unwrap();
        assert_eq!(
            inbox.payload.recovery_key.as_deref(),
            Some("run-recovery:call-1")
        );
        assert_eq!(
            inbox.payload.original_tool_call_id.as_deref(),
            Some("call-1")
        );
    }

    #[test]
    fn interaction_lifecycle_first_responder_wins_idempotent() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths);

        let interaction = manager
            .create_interaction(
                "task-1",
                "run-1",
                "ws-1",
                Some("session-1"),
                Some("req-1"),
                Some("call-1"),
                PendingInteractionKind::Permission,
                "Approve Delete",
                "Confirm file deletion",
                serde_json::json!({"path": "scratch/temp.txt"}),
            )
            .unwrap();

        assert_eq!(interaction.state, PendingInteractionState::Pending);

        // First resolution succeeds
        let (res1, is_first1) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"allow": true})),
            )
            .unwrap();
        assert!(is_first1);
        assert_eq!(res1.state, PendingInteractionState::Resolved);

        // Second resolution is idempotent
        let (res2, is_first2) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                None,
            )
            .unwrap();
        assert!(!is_first2);
        assert_eq!(res2.state, PendingInteractionState::Resolved);
    }

    #[test]
    fn interaction_reject_updates_inbox_and_leaves_no_grant() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-2",
                "run-2",
                "ws-2",
                Some("session-2"),
                Some("req-2"),
                Some("call-2"),
                PendingInteractionKind::Permission,
                "Approve Format",
                "Confirm formatting",
                serde_json::json!({
                    "toolName": "work_write_file",
                    "action": "write",
                    "argumentsHash": "hash2",
                    "workspaceId": "ws-2"
                }),
            )
            .unwrap();

        // Prepare reject
        let (delivering, grant, is_first) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Rejected),
                Some(serde_json::json!({"decision": "deny"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);
        assert!(grant.is_none(), "reject must not issue a grant");

        // Commit reject
        let committed = manager
            .commit_resolution(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Rejected),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Cancelled);

        // Verify inbox item projection is Rejected
        let inbox_mgr = crate::work::inbox::InboxManager::new(paths.clone());
        let item = inbox_mgr.get_item(&interaction.interaction_id).unwrap();
        assert_eq!(item.status, InboxItemStatus::Rejected);

        // Verify no matching grant exists
        let found = manager
            .find_matching_grant(
                "task-2",
                "run-2",
                "call-2",
                "work_write_file",
                "write",
                "hash2",
                "ws-2",
            )
            .unwrap();
        assert!(found.is_none());

        // Verify denied ToolResult fact in ledger
        let ledger =
            crate::work::ledger::WorkRuntimeLedger::open(&paths, "task-2", "run-2").unwrap();
        let facts = ledger.list_facts().unwrap();
        let denied_tool_result = facts.iter().find(|f| match f {
            crate::work::models::RuntimeFact::ToolResult {
                tool_call_id,
                status,
                success,
                ..
            } => tool_call_id == "call-2" && status == "denied" && !success,
            _ => false,
        });
        assert!(
            denied_tool_result.is_some(),
            "must record authoritative denied ToolResult fact"
        );
    }

    #[test]
    fn idempotent_approval_issues_single_grant() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-3",
                "run-3",
                "ws-3",
                Some("session-3"),
                Some("req-3"),
                Some("call-3"),
                PendingInteractionKind::Permission,
                "Approve Operation",
                "Details",
                serde_json::json!({
                    "toolName": "work_write_file",
                    "action": "write",
                    "argumentsHash": "hash3",
                    "workspaceId": "ws-3"
                }),
            )
            .unwrap();

        // First approval
        let (delivering1, grant1, is_first1) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
                Some(serde_json::json!({"decision": "allow"})),
            )
            .unwrap();
        assert!(is_first1);
        assert_eq!(delivering1.state, PendingInteractionState::Delivering);
        assert!(grant1.is_some());
        let grant = grant1.unwrap();

        // Second attempt to prepare/issue with same decision
        let (delivering2, grant2, is_first2) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
                Some(serde_json::json!({"decision": "allow"})),
            )
            .unwrap();
        assert!(!is_first2);
        assert_eq!(delivering2.state, PendingInteractionState::Delivering);
        assert!(
            grant2.is_none(),
            "second attempt must not issue another grant"
        );

        // Commit resolution
        let committed = manager
            .commit_resolution(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Resolved);

        // Third attempt via resolve_interaction_once
        let (res3, is_first3) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"decision": "allow"})),
            )
            .unwrap();
        assert!(!is_first3);
        assert_eq!(res3.state, PendingInteractionState::Resolved);

        // Verify only 1 grant file was created
        let inbox_dir = paths.inbox_dir();
        let grant_count = fs::read_dir(inbox_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".grant.json"))
            .count();
        assert_eq!(grant_count, 1);
        assert_eq!(grant.grant_id, grant.grant_id);
    }

    #[test]
    fn delivering_decision_is_immutable_rejects_conflicting_second_resolution() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-4",
                "run-4",
                "ws-4",
                Some("session-4"),
                Some("req-4"),
                Some("call-4"),
                PendingInteractionKind::Permission,
                "Approve Deploy",
                "Confirm deploy",
                serde_json::json!({
                    "toolName": "work_write_file",
                    "action": "write",
                    "argumentsHash": "hash4",
                    "workspaceId": "ws-4"
                }),
            )
            .unwrap();

        // 1. User clicks Approve -> Delivering + AllowedOnce grant
        let (delivering, grant, is_first) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
                Some(serde_json::json!({"decision": "allow"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);
        assert!(grant.is_some());

        // 2. While delivering (e.g. dead actor resume in progress), a conflicting Reject is attempted
        let conflict_err = manager.prepare_resolution_and_issue_grant(
            &interaction.interaction_id,
            PendingInteractionState::Cancelled,
            Some(InboxItemStatus::Rejected),
            Some(serde_json::json!({"decision": "deny"})),
        );
        assert!(
            conflict_err.is_err(),
            "conflicting decision while delivering must be rejected"
        );

        // 3. The initial grant remains intact and valid
        let matching_grant = manager
            .find_matching_grant(
                "task-4",
                "run-4",
                "call-4",
                "work_write_file",
                "write",
                "hash4",
                "ws-4",
            )
            .unwrap();
        assert!(
            matching_grant.is_some(),
            "original grant must remain intact"
        );
    }

    #[test]
    fn reject_vs_cancel_run_conflict_is_rejected_first_responder_wins() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-5",
                "run-5",
                "ws-5",
                Some("session-5"),
                Some("req-5"),
                Some("call-5"),
                PendingInteractionKind::Permission,
                "Approve Send",
                "Confirm send email",
                serde_json::json!({
                    "toolName": "work_write_file",
                    "action": "write",
                    "argumentsHash": "hash5",
                    "workspaceId": "ws-5"
                }),
            )
            .unwrap();

        // 1. User Rejects tool -> Delivering + prepared_inbox_status = Rejected
        let (delivering, grant, is_first) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Rejected),
                Some(serde_json::json!({"decision": "deny"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);
        assert!(grant.is_none());

        // 2. Conflicting Cancel Run is attempted on same delivering interaction
        let conflict_err = manager.prepare_resolution_and_issue_grant(
            &interaction.interaction_id,
            PendingInteractionState::Cancelled,
            Some(InboxItemStatus::Cancelled),
            Some(serde_json::json!({"decision": "cancel"})),
        );
        assert!(
            conflict_err.is_err(),
            "conflicting inbox_status while delivering must be rejected"
        );

        // 3. Commit with locked status preserves Rejected
        let locked_inbox_status = delivering
            .prepared_inbox_status
            .unwrap_or(InboxItemStatus::Rejected);
        let committed = manager
            .commit_resolution(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(locked_inbox_status),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Cancelled);

        let inbox_mgr = crate::work::inbox::InboxManager::new(paths);
        let item = inbox_mgr.get_item(&interaction.interaction_id).unwrap();
        assert_eq!(item.status, InboxItemStatus::Rejected);
    }

    #[test]
    fn cancel_run_vs_reject_conflict_is_rejected_first_responder_wins() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = InteractionManager::new(paths.clone());

        let interaction = manager
            .create_interaction(
                "task-6",
                "run-6",
                "ws-6",
                Some("session-6"),
                Some("req-6"),
                Some("call-6"),
                PendingInteractionKind::Permission,
                "Approve Send",
                "Confirm send email",
                serde_json::json!({
                    "toolName": "work_write_file",
                    "action": "write",
                    "argumentsHash": "hash6",
                    "workspaceId": "ws-6"
                }),
            )
            .unwrap();

        // 1. User Cancels run -> Delivering + prepared_inbox_status = Cancelled
        let (delivering, _, is_first) = manager
            .prepare_resolution_and_issue_grant(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Cancelled),
                Some(serde_json::json!({"decision": "cancel"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);

        // 2. Conflicting Reject is attempted on same delivering interaction
        let conflict_err = manager.prepare_resolution_and_issue_grant(
            &interaction.interaction_id,
            PendingInteractionState::Cancelled,
            Some(InboxItemStatus::Rejected),
            Some(serde_json::json!({"decision": "deny"})),
        );
        assert!(
            conflict_err.is_err(),
            "conflicting inbox_status while delivering must be rejected"
        );

        // 3. Commit with locked status preserves Cancelled
        let locked_inbox_status = delivering
            .prepared_inbox_status
            .unwrap_or(InboxItemStatus::Cancelled);
        let committed = manager
            .commit_resolution(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                Some(locked_inbox_status),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Cancelled);

        let inbox_mgr = crate::work::inbox::InboxManager::new(paths);
        let item = inbox_mgr.get_item(&interaction.interaction_id).unwrap();
        assert_eq!(item.status, InboxItemStatus::Cancelled);
    }
}
