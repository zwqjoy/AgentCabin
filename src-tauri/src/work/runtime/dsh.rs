//! DSH Work Runtime Adapter.
//!
//! DSH is used only as the model/session transport here. Work remains the
//! authority for tools, paths, policy, approvals, Inbox, artifacts, and
//! external integrations; the adapter exposes those capabilities through the
//! authenticated Work MCP bridge.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::agent::dsh_session_actor::{self, DshProtocol};
use crate::agent::session_actor::ActorCommand;
use crate::web_server::broadcaster::BroadcastEmitter;

use super::{
    WorkRuntimeAdapter, WorkRuntimeCapabilities, WorkRuntimeError, WorkRuntimeLaunchRequest,
    WorkRuntimePrepareContext,
};

#[derive(Debug)]
pub struct DshWorkRuntimeAdapter;

#[async_trait]
impl WorkRuntimeAdapter for DshWorkRuntimeAdapter {
    fn provider(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Dsh
    }

    fn capabilities(&self) -> WorkRuntimeCapabilities {
        WorkRuntimeCapabilities {
            supports_resume: true,
            supports_continuation: true,
            supports_follow_up: true,
            // DSH's native subagent/job surface is intentionally not exposed
            // in Work. Work delegation must remain a Host-owned capability.
            supports_subagents: false,
        }
    }

    fn session_persistence_enabled(
        &self,
        run: &crate::models::RunMeta,
    ) -> Result<bool, WorkRuntimeError> {
        Ok(!run.no_session_persistence)
    }

    async fn prepare_runtime(
        &self,
        context: &WorkRuntimePrepareContext,
    ) -> Result<(), WorkRuntimeError> {
        if context.run.app_mode != crate::work::models::AppMode::Work {
            return Err(WorkRuntimeError::LaunchFailed(
                "DSH Work adapter received a non-Work run".to_string(),
            ));
        }
        if context.capabilities.app_mode != crate::work::models::AppMode::Work
            || context.capabilities.runtime != RuntimeProviderKind::Dsh
        {
            return Err(WorkRuntimeError::LaunchFailed(
                "DSH Work adapter received capabilities for another mode or runtime".to_string(),
            ));
        }
        if let Some(plan) = &context.context_plan {
            if plan.runtime != RuntimeProviderKind::Dsh {
                return Err(WorkRuntimeError::LaunchFailed(format!(
                    "DSH Work adapter received Context Plan for '{}'",
                    plan.runtime.as_str()
                )));
            }
        }
        // The shared provider adapter performs the managed DSH projection after
        // this method returns. No Pi package or second Work state machine is
        // created here.
        Ok(())
    }

    async fn spawn_session_actor(
        &self,
        emitter: Arc<BroadcastEmitter>,
        sessions: ActorSessionMap,
        request: WorkRuntimeLaunchRequest,
        cancel_token: CancellationToken,
    ) -> Result<Sender<ActorCommand>, String> {
        if request.run.app_mode != crate::work::models::AppMode::Work {
            return Err("DSH Work adapter received a non-Work run".to_string());
        }
        if request.context_plan.run_id != request.run.id {
            return Err(format!(
                "Work Context Plan belongs to run {}, expected {}",
                request.context_plan.run_id, request.run.id
            ));
        }
        if request.context_plan.runtime != RuntimeProviderKind::Dsh {
            return Err(format!(
                "DSH Work Runtime received Context Plan for '{}'",
                request.context_plan.runtime.as_str()
            ));
        }
        let bridge_token = request
            .extra_env
            .get("AGENTCABIN_WORK_BRIDGE_TOKEN")
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "DSH Work launch is missing the authenticated Work bridge token".to_string()
            })?;

        let mut extra_env = request.extra_env;
        extra_env.insert("AGENTCABIN_WORK_RUNTIME".to_string(), "dsh".to_string());
        extra_env.insert(
            "AGENTCABIN_WORKSPACE_ROOT".to_string(),
            request.run.cwd.clone(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_MCP_SERVERS".to_string(),
            serde_json::to_string(
                &request
                    .capabilities
                    .mcp_servers
                    .iter()
                    .enumerate()
                    .map(|(index, server)| {
                        serde_json::json!({
                            "key": format!("s{index}"),
                            "server": server.id,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| format!("Failed to serialize Work MCP server map: {error}"))?,
        );
        if !request.capabilities.mcp_servers.is_empty() {
            extra_env.insert("AGENTCABIN_WORK_MCP_ENABLED".to_string(), "1".to_string());
        }
        if request.capabilities.browser_enabled {
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_ENABLED".to_string(),
                "1".to_string(),
            );
        }
        if request.capabilities.browser_use_enabled {
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_USE_ENABLED".to_string(),
                "1".to_string(),
            );
        }
        if request.desktop_use_enabled {
            extra_env.insert(
                "AGENTCABIN_WORK_DESKTOP_USE_ENABLED".to_string(),
                "1".to_string(),
            );
        }

        let mut system_prompt = crate::work::resources::base_work_system_prompt();
        system_prompt.push_str(
            "\n\n## DSH Work bridge\nUse the AgentCabin Work tools exposed under the `mcp__agentcabin_work__` namespace. The Work Host is authoritative for every file, command, browser, app, MCP, approval, Inbox, and Artifact operation. For any native desktop task, use only launch_app, find_roots, observe_ui, search_ui, expand_ui, inspect_ui, act_ui, read_text, and wait_for. Never use work_run_command, Bash, Swift, AppleScript, System Events, open, or another scripting path to operate a GUI. If a Computer Use tool fails, stop and report the failure; do not fall back to a shell or script. Never use DSH native shell, terminal, subprocess, sandbox, timer, or job tools for Work actions.",
        );
        extra_env.insert("AGENTCABIN_WORK_SYSTEM_PROMPT".to_string(), system_prompt);

        let requested_model = request
            .launch_overrides
            .as_ref()
            .and_then(|overrides| overrides.model.clone())
            .or(request.run.model.clone());
        // Match Pi's Work semantics: New with an existing persisted run may
        // reopen that run, while a persistence-disabled run must never leak a
        // stale ACP session id into a fresh process.
        let session_persistence_enabled = !request.run.no_session_persistence;
        let session_key = if session_persistence_enabled {
            request.session_id.or(request.run.session_id.clone())
        } else {
            None
        };

        dsh_session_actor::spawn_actor_with_protocol(
            emitter,
            sessions,
            request.run.id,
            request.run.cwd,
            requested_model,
            cancel_token,
            None,
            extra_env,
            Some(bridge_token),
            session_key,
            DshProtocol::Acp,
            crate::work::models::AppMode::Work,
        )
        .await
    }
}
