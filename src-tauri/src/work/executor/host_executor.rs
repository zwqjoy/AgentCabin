use chrono::Utc;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::work::executor::request::{
    ExecutionFailureKind, WorkExecutionRequest, WorkExecutionResult, WorkExecutionStatus,
};
use crate::work::executor::runtime::RuntimeResolver;
use crate::work::models::{ExecutionNetworkPolicy, WorkResourceManifest, WorkWorkspace};
use crate::work::paths::{validate_workspace_id, WorkPaths};
use crate::work::sandbox::{
    create_default_sandbox_provider, ExecutionCommand, NativeSandboxProvider,
    SandboxDenialClassifier, SandboxPolicyResolver,
};

const MAX_OUTPUT_BYTES: usize = 1_024 * 1_024; // 1MB buffer cap

/// RAII Guard ensuring per-execution temporary directory is cleaned up upon any exit or error path.
struct ExecutionSandboxDirGuard {
    path: PathBuf,
}

impl Drop for ExecutionSandboxDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub struct RestrictedHostExecutor;

impl RestrictedHostExecutor {
    pub async fn execute(
        paths: &WorkPaths,
        resource_manifest: &WorkResourceManifest,
        resource_dir: &Path,
        request: &WorkExecutionRequest,
        is_builtin_trusted: bool,
    ) -> Result<WorkExecutionResult, String> {
        let default_sandbox = create_default_sandbox_provider();
        Self::execute_with_sandbox(
            paths,
            resource_manifest,
            resource_dir,
            request,
            is_builtin_trusted,
            default_sandbox.as_ref(),
        )
        .await
    }

    pub async fn execute_with_sandbox(
        paths: &WorkPaths,
        resource_manifest: &WorkResourceManifest,
        resource_dir: &Path,
        request: &WorkExecutionRequest,
        is_builtin_trusted: bool,
        sandbox_provider: &dyn NativeSandboxProvider,
    ) -> Result<WorkExecutionResult, String> {
        let execution_id = format!("exec-{}", Uuid::new_v4());
        let started_at = Utc::now().to_rfc3339();

        // 1. Verify workspace ID and get workspace dir
        let is_standalone = request.workspace_id.trim().is_empty();
        let workspace_dir = if is_standalone {
            let task_run_id = request
                .work_run_id
                .as_deref()
                .or(request.task_id.as_deref())
                .unwrap_or(&execution_id);
            paths.ensure_standalone_task_dir(task_run_id)?
        } else {
            validate_workspace_id(&request.workspace_id)?;
            let dir = paths.workspace_dir(&request.workspace_id)?;
            if !dir.exists() {
                return Err(format!("Workspace directory not found: {:?}", dir));
            }
            dir
        };

        // 2. Verify Resource Kind & Execution Manifest
        if resource_manifest.kind != crate::work::models::WorkResourceKind::Capability {
            return Err(format!(
                "Resource '{}' is of kind '{:?}', but Restricted Host Executor only executes resources of kind 'Capability'",
                resource_manifest.id, resource_manifest.kind
            ));
        }

        let exec_manifest = resource_manifest.execution.as_ref().ok_or_else(|| {
            format!(
                "Resource '{}' has no execution manifest",
                resource_manifest.id
            )
        })?;

        // 3. Security Check: Trust Model
        if !is_builtin_trusted || !exec_manifest.trusted {
            return Err(format!(
                "Resource '{}' contains executable entry, but Restricted Host Executor only permits trusted built-in Work Resources. Third-party executable capabilities will be supported in future isolated VM executors.",
                resource_manifest.id
            ));
        }

        // 4. Security Check: Network Policy
        if exec_manifest.network != ExecutionNetworkPolicy::None {
            return Err(format!(
                "Current Restricted Host Executor does not support network-enabled executable capabilities (requested network: {:?})",
                exec_manifest.network
            ));
        }

        // 5. Security Check: Action authorization
        if !exec_manifest.actions.contains(&request.action) {
            return Err(format!(
                "Action '{}' is not declared in execution manifest for resource '{}'",
                request.action, resource_manifest.id
            ));
        }

        // 6. Path Confinement: Entry resolution & Symlink/Traversal guard
        let entry_relative = Path::new(&exec_manifest.entry);
        Self::validate_entry_path(resource_dir, entry_relative)?;
        let entry_path = resource_dir.join(entry_relative);

        // 7. Path Confinement: Input and Output path validations
        for input_rel in &request.input_paths {
            if is_standalone {
                Self::resolve_standalone_path(&workspace_dir, Path::new(input_rel), false)?;
            } else {
                paths.resolve_workspace_path(&request.workspace_id, Path::new(input_rel), false)?;
            }
        }
        for output_rel in &request.expected_outputs {
            if is_standalone {
                Self::resolve_standalone_path(&workspace_dir, Path::new(output_rel), true)?;
            } else {
                paths.resolve_workspace_path(&request.workspace_id, Path::new(output_rel), true)?;
            }
        }

        // 8. Runtime Descriptor Resolution (binary + runtime roots)
        let runtime_descriptor = RuntimeResolver::resolve_runtime(exec_manifest.runtime)?;

        // 9. Load workspace access roots from the authoritative manifest.
        // A missing or malformed manifest must not silently collapse the
        // sandbox to an empty access-root set: that would make the result
        // depend on a permissive fallback instead of the workspace contract.
        let access_roots = if is_standalone {
            Vec::new()
        } else {
            let manifest_path = paths.manifest_path(&request.workspace_id)?;
            let content = std::fs::read_to_string(&manifest_path).map_err(|error| {
                format!(
                    "Workspace manifest {} could not be read: {error}",
                    manifest_path.display()
                )
            })?;
            serde_json::from_str::<WorkWorkspace>(&content)
                .map_err(|error| {
                    format!(
                        "Workspace manifest {} is invalid: {error}",
                        manifest_path.display()
                    )
                })?
                .access_roots
        };

        let work_run_id = request.work_run_id.as_deref().unwrap_or(&execution_id);

        // RAII Guard: guarantees per-execution temporary sandbox directory is wiped upon any exit path
        let per_exec_dir = paths
            .data_root()
            .join("sandbox-tmp")
            .join(work_run_id)
            .join(&execution_id);
        let _dir_guard = ExecutionSandboxDirGuard {
            path: per_exec_dir.clone(),
        };

        // 10. Sandbox Policy Resolution (per-call policy)
        let policy = SandboxPolicyResolver::resolve(
            &workspace_dir,
            Some(resource_dir),
            exec_manifest,
            &access_roots,
            &runtime_descriptor.runtime_read_roots,
            work_run_id,
            &execution_id,
            paths.data_root(),
        )?;

        // 11. Prepare minimal safe environment variables. Work keeps the real
        // user HOME so CLIs can discover their native credentials/config; the
        // native sandbox remains the write boundary.
        let mut safe_envs = Vec::new();
        if let Ok(val) = std::env::var("PATH") {
            safe_envs.push(("PATH".to_string(), val));
        }
        if let Ok(val) = std::env::var("HOME") {
            safe_envs.push(("HOME".to_string(), val));
        }
        if let Ok(val) = std::env::var("LANG") {
            safe_envs.push(("LANG".to_string(), val));
        }
        if let Some(run_id) = &request.work_run_id {
            safe_envs.push(("WORK_RUN_ID".to_string(), run_id.clone()));
            safe_envs.push(("AGENTCABIN_WORK_RUN_ID".to_string(), run_id.clone()));
        }

        // Isolate TMPDIR only to the private per-execution temp root when write mode is active
        if let Some(temp_root) = &policy.temp_root {
            let temp_str = temp_root.to_string_lossy().to_string();
            safe_envs.push(("TMPDIR".to_string(), temp_str.clone()));
            safe_envs.push(("TMP".to_string(), temp_str.clone()));
            safe_envs.push(("TEMP".to_string(), temp_str));
        }

        let args_json =
            serde_json::to_string(&request.arguments).unwrap_or_else(|_| "{}".to_string());

        let raw_command = ExecutionCommand {
            program: runtime_descriptor.binary_path,
            args: vec![
                entry_path.into_os_string(),
                request.action.clone().into(),
                args_json.into(),
            ],
            current_dir: workspace_dir.clone(),
            envs: safe_envs,
        };

        // 12. Native Sandbox Confinement (Fail-closed)
        let confined = match sandbox_provider.confine(&raw_command, &policy) {
            Ok(cmd) => cmd,
            Err(e) => {
                let finished_at = Utc::now().to_rfc3339();
                return Ok(WorkExecutionResult {
                    execution_id,
                    resource_id: request.resource_id.clone(),
                    action: request.action.clone(),
                    status: WorkExecutionStatus::Failed,
                    failure_kind: Some(ExecutionFailureKind::SandboxInfrastructureFailure),
                    exit_code: Some(-1),
                    stdout: String::new(),
                    stderr: format!("Native Sandbox confinement failed (fail-closed): {e}"),
                    outputs: Vec::new(),
                    started_at,
                    finished_at,
                });
            }
        };

        // 13. Build Process Command with dedicated Process Group
        let mut cmd = Command::new(&confined.program);
        cmd.kill_on_drop(true);
        cmd.current_dir(&confined.current_dir);
        cmd.env_clear();
        for (k, v) in &confined.envs {
            cmd.env(k, v);
        }
        for arg in &confined.args {
            cmd.arg(arg);
        }

        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // 14. Spawn process and monitor with timeout
        let timeout_duration = Duration::from_secs(exec_manifest.timeout_seconds.clamp(1, 600));
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let finished_at = Utc::now().to_rfc3339();
                return Ok(WorkExecutionResult {
                    execution_id,
                    resource_id: request.resource_id.clone(),
                    action: request.action.clone(),
                    status: WorkExecutionStatus::Failed,
                    failure_kind: Some(ExecutionFailureKind::SandboxInfrastructureFailure),
                    exit_code: Some(-1),
                    stdout: String::new(),
                    stderr: format!("Failed to spawn sandboxed execution process: {e}"),
                    outputs: Vec::new(),
                    started_at,
                    finished_at,
                });
            }
        };

        // pre_exec setpgid(0,0) guarantees pgid == child PID post-spawn (spawn fails if pre_exec fails).
        #[cfg(unix)]
        let spawned_pgid: i32 = child.id().map(|pid| pid as i32).unwrap_or(0);
        #[cfg(not(unix))]
        let spawned_pgid: i32 = 0;

        // Take pipes and launch concurrent reader tasks immediately.
        // Readers drain concurrently while child runs — prevents pipe-buffer backpressure deadlock
        // (child filling >64KB to stdout while Host is blocked in child.wait() would deadlock otherwise).
        let stdout_pipe = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr_pipe = child.stderr.take().ok_or("Failed to capture stderr")?;

        let out_handle = tokio::spawn(async move {
            read_capped_pipe(stdout_pipe, MAX_OUTPUT_BYTES)
                .await
                .unwrap_or_default()
        });
        let err_handle = tokio::spawn(async move {
            read_capped_pipe(stderr_pipe, MAX_OUTPUT_BYTES)
                .await
                .unwrap_or_default()
        });

        // Gate only on the direct child process exiting.
        // Background helpers that inherited pipe fds will not block child.wait().
        let direct_child_future = async move { child.wait().await };
        let (exec_status, failure_kind, exit_code, stdout, stderr) =
            match tokio::time::timeout(timeout_duration, direct_child_future).await {
                Ok(Ok(exit_status)) => {
                    // Kill the process group immediately — reaps background helpers and closes
                    // their write ends of the pipes, allowing the reader tasks to drain and finish.
                    Self::kill_process_group(spawned_pgid).await;

                    // Collect output with bounded grace (pipe write ends are now all closed).
                    let grace = Duration::from_millis(200);
                    let (out_bytes, err_bytes) = tokio::time::timeout(grace, async {
                        let o = out_handle.await.unwrap_or_default();
                        let e = err_handle.await.unwrap_or_default();
                        (o, e)
                    })
                    .await
                    .unwrap_or_else(|_| (Vec::new(), Vec::new()));

                    let out_str = String::from_utf8_lossy(&out_bytes).to_string();
                    let err_str = String::from_utf8_lossy(&err_bytes).to_string();

                    if exit_status.success() {
                        (
                            WorkExecutionStatus::Success,
                            None,
                            exit_status.code(),
                            out_str,
                            err_str,
                        )
                    } else {
                        let failure_kind = SandboxDenialClassifier::classify_failure(
                            &err_str,
                            exit_status.code(),
                            &confined.denial_signatures,
                            &confined.runner_failure_signatures,
                        );
                        let status = match failure_kind {
                            ExecutionFailureKind::SandboxDenied => WorkExecutionStatus::Denied,
                            _ => WorkExecutionStatus::Failed,
                        };
                        (
                            status,
                            Some(failure_kind),
                            exit_status.code(),
                            out_str,
                            err_str,
                        )
                    }
                }
                Ok(Err(e)) => {
                    Self::kill_process_group(spawned_pgid).await;
                    out_handle.abort();
                    err_handle.abort();
                    (
                        WorkExecutionStatus::Failed,
                        Some(ExecutionFailureKind::SandboxInfrastructureFailure),
                        None,
                        String::new(),
                        format!("Execution error: {e}"),
                    )
                }
                Err(_) => {
                    // Timeout: kill entire process group then abort readers
                    Self::kill_process_group(spawned_pgid).await;
                    out_handle.abort();
                    err_handle.abort();
                    (
                        WorkExecutionStatus::TimedOut,
                        None,
                        None,
                        String::new(),
                        format!(
                            "Execution timed out after {} seconds and process was killed",
                            exec_manifest.timeout_seconds
                        ),
                    )
                }
            };

        // 15. Collect verified expected outputs
        let mut verified_outputs = Vec::new();
        for output_rel in &request.expected_outputs {
            let res = if is_standalone {
                Self::resolve_standalone_path(&workspace_dir, Path::new(output_rel), false)
            } else {
                paths.resolve_workspace_path(&request.workspace_id, Path::new(output_rel), false)
            };
            if let Ok(resolved_output) = res {
                if resolved_output.exists() {
                    verified_outputs.push(output_rel.clone());
                }
            }
        }

        let finished_at = Utc::now().to_rfc3339();

        Ok(WorkExecutionResult {
            execution_id,
            resource_id: request.resource_id.clone(),
            action: request.action.clone(),
            status: exec_status,
            failure_kind,
            exit_code,
            stdout,
            stderr,
            outputs: verified_outputs,
            started_at,
            finished_at,
        })
    }

    /// Resolve and validate paths for standalone (workspace-less) Work tasks.
    fn resolve_standalone_path(
        standalone_dir: &Path,
        relative: &Path,
        _writable: bool,
    ) -> Result<std::path::PathBuf, String> {
        if relative.is_absolute() {
            if relative.starts_with(standalone_dir) {
                return Ok(relative.to_path_buf());
            }
            return Err("Standalone task path must be within task directory".into());
        }
        for component in relative.components() {
            match component {
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    return Err("Standalone path cannot contain ..".into());
                }
                _ => return Err("Invalid path component".into()),
            }
        }
        Ok(standalone_dir.join(relative))
    }

    /// Kill all descendant processes in the dedicated process group.
    /// `pgid` == child PID (set by pre_exec setpgid(0,0)). 0 = no group established.
    /// Sends SIGTERM first (graceful), then waits 50ms, then SIGKILL (forceful).
    async fn kill_process_group(pgid: i32) {
        #[cfg(unix)]
        if pgid > 0 {
            unsafe { libc::kill(-pgid, libc::SIGTERM) };
            tokio::time::sleep(Duration::from_millis(50)).await;
            unsafe { libc::kill(-pgid, libc::SIGKILL) };
        }
        #[cfg(not(unix))]
        let _ = pgid;
    }

    /// Ensure entry relative path does not escape resource directory via `..`, absolute paths, or symlinks.
    fn validate_entry_path(resource_dir: &Path, relative_entry: &Path) -> Result<(), String> {
        if relative_entry.is_absolute() {
            return Err("Execution entry path cannot be absolute".to_string());
        }

        for component in relative_entry.components() {
            match component {
                std::path::Component::Normal(_) => {}
                std::path::Component::CurDir => {}
                _ => {
                    return Err(
                        "Execution entry path cannot contain '..' or root prefixes".to_string()
                    )
                }
            }
        }

        let target = resource_dir.join(relative_entry);
        if target.exists() {
            let canonical_base = std::fs::canonicalize(resource_dir).map_err(|e| e.to_string())?;
            let canonical_target = std::fs::canonicalize(&target).map_err(|e| e.to_string())?;
            if !canonical_target.starts_with(&canonical_base) {
                return Err("Execution entry escapes resource directory via symlink".to_string());
            }
        }

        Ok(())
    }
}

async fn read_capped_pipe<R: AsyncReadExt + Unpin>(
    mut reader: R,
    max_bytes: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        if buf.len() < max_bytes {
            let to_take = n.min(max_bytes - buf.len());
            buf.extend_from_slice(&chunk[..to_take]);
        }
    }
    Ok(buf)
}
