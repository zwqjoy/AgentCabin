//! Pi Work Runtime Adapter.
//!
//! This adapter bridges the Work Harness to the Pi Agent runtime.
//! It owns all Pi-specific Work preparation:
//!   - Pi AdapterSettings for Work (isolated from Code-mode Pi features)
//!   - PI_CODING_AGENT_DIR and all other Work-specific Pi env vars
//!   - pi_work_extension, pi_work_mcp_adapter, pi_work_browser_adapter
//!   - pi_work_subagents_adapter paths
//!   - PiRuntimeAdapter.prepare_runtime for the managed pi_home
//!
//! The Work Harness (session.rs, session_dispatch.rs) does NOT need to know
//! any of these details; it calls `WorkRuntimeRouter::route(Pi)` and receives
//! this adapter.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::{self, ActorSessionMap, AdapterSettings};
use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::agent::pi_session_actor;
use crate::agent::session_actor::ActorCommand;
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;

use super::{
    WorkRuntimeAdapter, WorkRuntimeCapabilities, WorkRuntimeError, WorkRuntimeLaunchRequest,
    WorkRuntimePrepareContext,
};

pub(crate) static LAST_LAUNCH_EFFORT: once_cell::sync::Lazy<
    std::sync::Mutex<HashMap<String, Option<String>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn get_last_launch_effort(run_id: &str) -> Option<Option<String>> {
    LAST_LAUNCH_EFFORT.lock().ok()?.get(run_id).cloned()
}

struct PiLaunchContext {
    settings: AdapterSettings,
    extra_env: HashMap<String, String>,
}

#[derive(Debug)]
pub struct PiWorkRuntimeAdapter;

#[async_trait]
impl WorkRuntimeAdapter for PiWorkRuntimeAdapter {
    fn provider(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Pi
    }

    fn capabilities(&self) -> WorkRuntimeCapabilities {
        WorkRuntimeCapabilities {
            supports_resume: true,
            supports_continuation: true,
            supports_follow_up: true,
            supports_subagents: true,
        }
    }

    fn session_persistence_enabled(
        &self,
        run: &crate::models::RunMeta,
    ) -> Result<bool, WorkRuntimeError> {
        let agent_settings = storage::settings::get_agent_settings(&run.agent);
        let user_settings = storage::settings::get_user_settings();
        let settings = adapter::build_adapter_settings(&agent_settings, &user_settings, None);
        Ok(!settings.no_session_persistence)
    }

    async fn prepare_runtime(
        &self,
        context: &WorkRuntimePrepareContext,
    ) -> Result<(), WorkRuntimeError> {
        self.validate_work_run(&context.run)?;
        // Provision Pi-specific packages and MCP adapter for Work.
        crate::work::system_packages::ensure_required_work_packages(&context.paths)
            .await
            .map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!("Failed to ensure Pi Work packages: {e}"))
            })?;
        crate::work::system_packages::ensure_pi_interaction_packages(&context.paths)
            .await
            .map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!(
                    "Failed to ensure Pi interaction extensions: {e}"
                ))
            })?;
        crate::work::mcp::ensure_adapter_for_paths(&context.paths)
            .await
            .map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!("Failed to ensure Pi MCP adapter: {e}"))
            })?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_session_actor(
        &self,
        emitter: Arc<BroadcastEmitter>,
        sessions: ActorSessionMap,
        request: WorkRuntimeLaunchRequest,
        cancel_token: CancellationToken,
    ) -> Result<Sender<ActorCommand>, String> {
        let launch = self
            .prepare_pi_launch_context(&request)
            .map_err(|error| format!("Pi Work launch context rejected: {error}"))?;
        let session_id = request.session_id.clone();
        let resume_session_id = match &request.session_mode {
            crate::models::SessionMode::Resume | crate::models::SessionMode::Continue => {
                if launch.settings.no_session_persistence {
                    return Err("Cannot resume: Work session persistence is disabled".to_string());
                }
                Some(
                    session_id
                        .or_else(|| request.run.session_id.clone())
                        .ok_or_else(|| {
                            "Work session_id is required for resume/continue".to_string()
                        })?,
                )
            }
            crate::models::SessionMode::New => {
                if launch.settings.no_session_persistence {
                    None
                } else {
                    session_id.or_else(|| request.run.session_id.clone())
                }
            }
            crate::models::SessionMode::Fork => {
                return Err("Fork mode not supported in Work mode".to_string());
            }
        };
        let run_id = request.run.id;
        let cwd = request.run.cwd;
        let mut extra_env = launch.extra_env;
        extra_env.extend(request.extra_env);
        if let Ok(mut map) = LAST_LAUNCH_EFFORT.lock() {
            map.insert(run_id.clone(), launch.settings.effort.clone());
        }
        pi_session_actor::spawn_actor(
            emitter,
            sessions,
            run_id,
            cwd,
            &launch.settings,
            resume_session_id,
            extra_env,
            cancel_token,
        )
        .await
    }
}

impl PiWorkRuntimeAdapter {
    pub(crate) fn validate_work_run(
        &self,
        run: &crate::models::RunMeta,
    ) -> Result<(), WorkRuntimeError> {
        work_run_context(run).map(|_| ())
    }

    fn prepare_pi_launch_context(
        &self,
        request: &WorkRuntimeLaunchRequest,
    ) -> Result<PiLaunchContext, WorkRuntimeError> {
        // This function is called only for Work-mode runs (app_mode == Work).
        // The caller (session_dispatch) has already run async prerequisites
        // (ensure_required_work_packages, CapabilityResolver::resolve, etc.).
        let run = &request.run;
        let caps = &request.capabilities;
        let permission_mode_override = request.permission_mode_override.as_deref();

        if request.context_plan.run_id != run.id {
            return Err(WorkRuntimeError::LaunchFailed(format!(
                "Work Context Plan belongs to run {}, expected {}",
                request.context_plan.run_id, run.id
            )));
        }
        if request.context_plan.runtime != RuntimeProviderKind::Pi {
            return Err(WorkRuntimeError::LaunchFailed(format!(
                "Pi Work Runtime received Context Plan for '{}'",
                request.context_plan.runtime.as_str()
            )));
        }

        let ws_id = run.workspace_id.clone();
        let (_task_id, work_run_id, exec_ctx) = work_run_context(run)?;

        let work_paths = crate::work::paths::WorkPaths::app();
        let runtime = crate::work::resources::prepare_pi_runtime_with_paths(&work_paths).map_err(
            |error| {
                WorkRuntimeError::LaunchFailed(format!(
                    "Failed to prepare the managed Pi Work profile: {error}"
                ))
            },
        )?;

        // Build base Pi AdapterSettings from agent + user settings.
        let agent_settings = storage::settings::get_agent_settings(&run.agent);
        let user_settings = storage::settings::get_user_settings();
        let mut settings =
            adapter::build_adapter_settings(&agent_settings, &user_settings, run.model.clone());
        adapter::append_continuation_context(&mut settings, run.continuation_context.as_deref());
        if let Some(mode) = permission_mode_override.filter(|m| !m.trim().is_empty()) {
            settings.permission_mode = Some(mode.to_string());
        }

        // Apply per-run launch overrides if supplied (e.g. Smoke runner)
        if let Some(overrides) = &request.launch_overrides {
            if let Some(effort) = &overrides.effort {
                settings.effort = Some(effort.clone());
            }
            if let Some(model) = &overrides.model {
                settings.model = Some(model.clone());
            }
            if overrides.forbid_model_fallback {
                settings.fallback_model = None;
            }
        }

        // Isolate Work Pi features — Code-mode Pi packages must never leak
        // into the isolated Work profile through shared AgentSettings.
        isolate_work_pi_features(&mut settings);

        // Canonical managed runtime directory from CapabilityResolver
        // (~/.agentcabin/runtime/work/pi/<run-id>/). Single source of truth.
        let agent_dir = caps.managed_runtime_dir.to_string_lossy().into_owned();
        settings.pi_agent_dir = Some(agent_dir.clone());
        settings.pi_work_extension = Some(runtime.extension_entry.to_string_lossy().into_owned());
        settings.pi_work_mcp_adapter = runtime
            .mcp_enabled
            .then(|| runtime.mcp_adapter_entry.to_string_lossy().into_owned());
        settings.pi_work_browser_adapter = runtime
            .browser_enabled
            .then(|| runtime.browser_adapter_entry.to_string_lossy().into_owned());
        settings.pi_work_subagents_adapter = Some(
            runtime
                .subagents_adapter_entry
                .to_string_lossy()
                .into_owned(),
        );
        settings.pi_work_package_sources = runtime.package_sources.clone();
        settings.pi_shared_extension_sources =
            crate::storage::profile_bindings::list_enabled_pi_extension_sources("work")
                .into_iter()
                .filter(|path| {
                    !crate::work::system_packages::is_pi_interaction_source(&path.to_string_lossy())
                })
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
        settings.pi_shared_extension_sources.push(
            crate::work::system_packages::common_system_package_entry_path(
                &work_paths,
                crate::work::system_packages::PI_ASK_USER_QUESTION_PACKAGE_NAME,
            )
            .to_string_lossy()
            .into_owned(),
        );
        settings.pi_shared_extension_sources.push(
            crate::work::system_packages::common_system_package_entry_path(
                &work_paths,
                crate::work::system_packages::PI_TODO_PACKAGE_NAME,
            )
            .to_string_lossy()
            .into_owned(),
        );
        let context_usage_extension = crate::pi_context_runtime::ensure_context_usage_extension(
            &work_paths,
        )
        .map_err(|error| {
            WorkRuntimeError::LaunchFailed(format!(
                "Failed to prepare the Pi context usage extension: {error}"
            ))
        })?;
        settings
            .pi_shared_extension_sources
            .push(context_usage_extension.to_string_lossy().into_owned());
        settings.pi_work_skill_sources = runtime.skill_sources.clone();
        settings.pi_workspace_root = Some(run.cwd.clone());
        settings.pi_work_access_roots = if let Some(ws) = &ws_id {
            crate::work::workspace::manager()
                .list_access_roots(ws)
                .map_err(WorkRuntimeError::LaunchFailed)?
                .into_iter()
                .map(|root| (root.path, root.writable))
                .collect()
        } else {
            vec![(run.cwd.clone(), true)]
        };
        settings.pi_work_attachment_root = Some(
            storage::run_dir(&run.id)
                .join("attachments")
                .to_string_lossy()
                .into_owned(),
        );

        // Assemble extra env vars.
        let mut extra_env = HashMap::new();
        let shared_skills_root = crate::storage::profile_bindings::shared_skills_dir();
        extra_env.insert(
            "AGENTCABIN_SHARED_SKILLS_DIR".to_string(),
            shared_skills_root.to_string_lossy().into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_RUN_DIR".to_string(),
            storage::run_dir(&run.id).to_string_lossy().into_owned(),
        );
        let scope = crate::work::models::WorkScope::from_meta(run);
        let permission_path = if let Some(tid) = &scope.automation_task_id {
            work_paths
                .task_manifest_path(tid)
                .map_err(WorkRuntimeError::LaunchFailed)?
        } else {
            storage::run_dir(&run.id).join("meta.json")
        };
        extra_env.insert(
            "AGENTCABIN_WORK_PERMISSION_PATH".to_string(),
            permission_path.to_string_lossy().into_owned(),
        );
        if let Some(tid) = &scope.automation_task_id {
            extra_env.insert("AGENTCABIN_WORK_TASK_ID".to_string(), tid.to_string());
            let tm = crate::work::tasks::TaskManager::new(crate::work::paths::WorkPaths::app());
            if let Ok(task) = tm.get_task(tid) {
                if let Ok(pol_json) = serde_json::to_string(&task.policy) {
                    extra_env.insert("AGENTCABIN_WORK_POLICY".to_string(), pol_json);
                }
            }
        }
        if scope.is_standalone() {
            extra_env.insert("AGENTCABIN_WORK_STANDALONE".to_string(), "1".to_string());
        }
        extra_env.insert("AGENTCABIN_WORK_RUN_ID".to_string(), work_run_id.clone());
        extra_env.insert(
            "AGENTCABIN_WORK_PRESET".to_string(),
            run.work_preset.unwrap_or_default().as_str().to_string(),
        );
        let ctx_str = match exec_ctx {
            crate::work::models::ExecutionContext::Attended => "attended",
            crate::work::models::ExecutionContext::Unattended => "unattended",
        };
        extra_env.insert(
            "AGENTCABIN_WORK_EXECUTION_CONTEXT".to_string(),
            ctx_str.to_string(),
        );
        if settings.pi_work_mcp_adapter.is_some() {
            extra_env.insert("AGENTCABIN_WORK_MCP_ENABLED".to_string(), "1".to_string());
        }
        if runtime.browser_enabled {
            let browser_run_dir = storage::run_dir(&run.id).join("browser");
            std::fs::create_dir_all(&browser_run_dir).map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!("创建网络访问 run 目录失败: {e}"))
            })?;
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_ENABLED".to_string(),
                "1".to_string(),
            );
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_PROVIDER".to_string(),
                runtime.browser_provider.clone(),
            );
            if let Some(endpoint) = runtime.browser_endpoint_url.as_ref() {
                extra_env.insert(
                    "AGENTCABIN_WORK_BROWSER_ENDPOINT_URL".to_string(),
                    endpoint.clone(),
                );
            }
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_MAX_RESULTS".to_string(),
                runtime.browser_max_results.to_string(),
            );
            extra_env.insert(
                "AGENTCABIN_WORK_BROWSER_RUN_DIR".to_string(),
                browser_run_dir.to_string_lossy().into_owned(),
            );
            if !runtime.browser_allowed_hosts.is_empty() {
                extra_env.insert(
                    "AGENTCABIN_WORK_BROWSER_ALLOWED_HOSTS".to_string(),
                    runtime.browser_allowed_hosts.join(","),
                );
            }
        }
        if runtime.browser_use_enabled {
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
        // PI_CODING_AGENT_DIR: Pi-specific env var that sets the managed HOME.
        // Kept here (not in session_dispatch) because it is Pi-specific.
        extra_env.insert("PI_CODING_AGENT_DIR".to_string(), agent_dir.clone());
        extra_env.insert(
            "AGENTCABIN_WORKSPACE_ID".to_string(),
            ws_id.as_deref().unwrap_or("").to_string(),
        );
        if let Some(ws) = &ws_id {
            if let Ok(managed_state_dir) = work_paths.workspace_dir(ws) {
                extra_env.insert(
                    "AGENTCABIN_MANAGED_STATE_DIR".to_string(),
                    managed_state_dir.to_string_lossy().into_owned(),
                );
            }
        }
        extra_env.insert("AGENTCABIN_WORKSPACE_ROOT".to_string(), run.cwd.clone());
        extra_env.insert(
            "AGENTCABIN_WORK_PROFILE_DIR".to_string(),
            runtime.agent_dir.to_string_lossy().into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS".to_string(),
            serde_json::to_string(&runtime.system_agent_file_digests).map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!("序列化 Work system agent digest 失败: {e}"))
            })?,
        );
        extra_env.insert(
            "AGENTCABIN_WORK_RESOURCE_CATALOG".to_string(),
            runtime.resource_catalog_path.to_string_lossy().into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_MCP_CONFIG".to_string(),
            runtime.mcp_config_path.to_string_lossy().into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_PI_SYSTEM_MCP_ADAPTER_ENTRY".to_string(),
            crate::work::system_packages::common_system_package_entry_path(
                &work_paths,
                crate::work::system_packages::PI_MCP_ADAPTER_PACKAGE_NAME,
            )
            .join("index.ts")
            .to_string_lossy()
            .into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_MCP_SECRETS".to_string(),
            runtime.mcp_secrets_path.to_string_lossy().into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_PACKAGE_MCP_CONFIG".to_string(),
            runtime
                .mcp_package_config_path
                .to_string_lossy()
                .into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_PACKAGE_CLI_CONFIG".to_string(),
            runtime
                .cli_package_config_path
                .to_string_lossy()
                .into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_SKILL_SOURCES".to_string(),
            serde_json::to_string(&runtime.skill_sources).map_err(|e| {
                WorkRuntimeError::LaunchFailed(format!("序列化 Work skill sources 失败: {e}"))
            })?,
        );
        extra_env.insert(
            "AGENTCABIN_AGENT_PLUGINS_DIR".to_string(),
            crate::storage::agent_plugins::agent_plugins_root()
                .to_string_lossy()
                .into_owned(),
        );
        extra_env.insert(
            "MCP_OAUTH_DIR".to_string(),
            runtime
                .agent_dir
                .join("mcp-oauth")
                .to_string_lossy()
                .into_owned(),
        );
        extra_env.insert(
            "AGENTCABIN_WORK_CONTEXT_PLAN_PATH".to_string(),
            crate::work::context::context_path(&run.id)
                .to_string_lossy()
                .into_owned(),
        );

        // Append the stable Work runtime system prompt (contains WORK_PI_SYSTEM_PROMPT,
        // artifact/tool instructions). The turn-scoped Context Plan is injected by the Pi actor
        // with each user message so a resumed or follow-up turn cannot retain a stale launch plan
        // or receive the initial plan twice.
        append_work_system_prompt(&mut settings, &runtime.system_prompt);
        adapter::append_continuation_context(&mut settings, Some(&caps.capability_guidance()));

        Ok(PiLaunchContext {
            settings,
            extra_env,
        })
    }
}

fn work_run_context(
    run: &crate::models::RunMeta,
) -> Result<
    (
        Option<String>,
        String,
        crate::work::models::ExecutionContext,
    ),
    WorkRuntimeError,
> {
    let work_run_id = run.work_run_id.clone().unwrap_or_else(|| run.id.clone());
    let exec_ctx = run
        .work_execution_context
        .unwrap_or(crate::work::models::ExecutionContext::Attended);
    let task_id = run.work_task_id.clone();

    Ok((task_id, work_run_id, exec_ctx))
}

/// Work owns its capability policy and lifecycle state. Code-mode Pi feature
/// packages must never leak into the isolated Work profile through shared
/// AgentSettings, even when they are enabled for ordinary Pi conversations.
fn isolate_work_pi_features(settings: &mut AdapterSettings) {
    settings.permission_mode = None;
    settings.pi_permission_system_enabled = false;
    settings.pi_plan_mode_enabled = false;
    settings.pi_goal_enabled = false;
    settings.pi_todo_enabled = false;
    settings.pi_context_prune_enabled = false;
    settings.pi_subagents_enabled = false;
    settings.pi_multi_edit_enabled = false;
    settings.pi_lsp_enabled = false;
}

fn append_work_system_prompt(settings: &mut AdapterSettings, system_prompt: &str) {
    if system_prompt.is_empty() {
        return;
    }
    match settings.append_system_prompt.as_mut() {
        Some(existing) => {
            existing.push('\n');
            existing.push_str(system_prompt);
        }
        None => {
            settings.append_system_prompt = Some(system_prompt.to_string());
        }
    }
}
