//! Confined command runner for `work_run_command`.
//!
//! Executes an argv array (no shell) inside a workspace-confined working directory
//! with a dedicated process group, capped output capture, and timeout enforcement.
//! Unlike the Capability host executor, this runner performs no manifest /
//! trusted-resource checks — trust is established by the Work policy approval
//! gate upstream in the pipeline. It still uses the Work OS sandbox, and passes
//! only a small runtime environment so PATH-resolved tools remain available.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::Utc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::work::executor::{ExecutionFailureKind, WorkExecutionResult, WorkExecutionStatus};
use crate::work::models::WorkWorkspace;
use crate::work::paths::WorkPaths;
use crate::work::sandbox::{
    ExecutionCommand, SandboxDenialClassifier, SandboxError, WorkSandboxLauncher,
};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_COMMAND_ARGS: usize = 64;

/// Run a command for `work_run_command`. Normal Work modes use the confined
/// host runner; explicit FullAccess skips that OS-level confinement.
///
/// `cwd_rel` is resolved and confined to the workspace boundary (or an authorized
/// external access root) exactly like `work_write_file` path resolution.
/// `expected_outputs` are verified post-execution and returned as `outputs` so
/// the pipeline's artifact auto-registration can pick them up.
#[allow(clippy::too_many_arguments)]
pub async fn run_command(
    paths: &WorkPaths,
    workspace_id: &str,
    run_id: Option<&str>,
    tool_name: &str,
    action: &str,
    command: &str,
    args: &[String],
    cwd_rel: &str,
    timeout_secs: u64,
    expected_outputs: &[String],
    home_paths: &[String],
    full_access: bool,
    host_fallback: bool,
) -> Result<WorkExecutionResult, String> {
    let started_at = Utc::now().to_rfc3339();
    let execution_id = format!("exec-{}", uuid::Uuid::new_v4());

    let (clean_cmd_str, clean_args_vec) = sanitize_command_args(command, args);
    let command = clean_cmd_str.as_str();
    let args = clean_args_vec.as_slice();

    if command.trim().is_empty() {
        return Ok(failed_result(
            &execution_id,
            tool_name,
            action,
            &started_at,
            "work_run_command requires a non-empty 'command' parameter.",
        ));
    }
    if args.len() > MAX_COMMAND_ARGS {
        return Ok(failed_result(
            &execution_id,
            tool_name,
            action,
            &started_at,
            &format!(
                "work_run_command exceeds maximum {MAX_COMMAND_ARGS} arguments (got {}).",
                args.len()
            ),
        ));
    }

    let cwd_path = match paths.resolve_command_cwd(workspace_id, run_id, cwd_rel, full_access) {
        Ok(p) => p,
        Err(e) => {
            return Ok(failed_result(
                &execution_id,
                tool_name,
                action,
                &started_at,
                &format!("cwd resolution failed: {e}"),
            ));
        }
    };

    // `work_run_command` is called by the Work Pi extension, but this process
    // is created by the Rust host rather than by Pi itself. It therefore does
    // not inherit Pi's Seatbelt unless we explicitly put it through the same
    // launcher. Keep the command argv-shaped (no implicit shell) while giving
    // the child process the same Work profile, real HOME, and temp root.
    if let Err(error) = paths.ensure_layout() {
        return Ok(failed_sandbox_result(
            &execution_id,
            tool_name,
            action,
            &started_at,
            &format!("Work sandbox layout failed: {error}"),
        ));
    }

    let connector_home_roots = if tool_name == "work_run_connector_cli" && !full_access {
        match prepare_home_paths(home_paths) {
            Ok(roots) => roots,
            Err(error) => {
                return Ok(failed_sandbox_result(
                    &execution_id,
                    tool_name,
                    action,
                    &started_at,
                    &format!("Connector CLI HOME configuration failed: {error}"),
                ));
            }
        }
    } else {
        Vec::new()
    };

    let mut envs = Vec::new();
    for key in ["PATH", "LANG", "LC_ALL", "LC_CTYPE", "TZ", "TERM"] {
        if let Ok(value) = std::env::var(key) {
            envs.push((key.to_string(), value));
        }
    }

    if tool_name == "work_run_connector_cli" {
        let current_path = envs
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        let private_path = crate::work::cli_runtime::augment_path(paths, &current_path);
        if let Some((_, value)) = envs.iter_mut().find(|(key, _)| key == "PATH") {
            *value = private_path;
        } else {
            envs.push(("PATH".into(), private_path));
        }
    }

    let npm_cache_dir = paths.work_profile_dir().join("npm-cache");
    if let Err(error) = std::fs::create_dir_all(&npm_cache_dir) {
        return Ok(failed_sandbox_result(
            &execution_id,
            tool_name,
            action,
            &started_at,
            &format!(
                "Failed to create Work npm cache directory {}: {error}",
                npm_cache_dir.display()
            ),
        ));
    }
    let npm_cache_str = npm_cache_dir.to_string_lossy().into_owned();
    if !envs.iter().any(|(k, _)| k == "npm_config_cache") {
        envs.push(("npm_config_cache".to_string(), npm_cache_str.clone()));
    }
    if !envs.iter().any(|(k, _)| k == "NPM_CONFIG_CACHE") {
        envs.push(("NPM_CONFIG_CACHE".to_string(), npm_cache_str));
    }

    let pip_cache_dir = paths.work_profile_dir().join("pip-cache");
    let _ = fs::create_dir_all(&pip_cache_dir);
    let pip_cache_str = pip_cache_dir.to_string_lossy().into_owned();
    if !envs.iter().any(|(k, _)| k == "PIP_CACHE_DIR") {
        envs.push(("PIP_CACHE_DIR".to_string(), pip_cache_str));
    }
    let pycache_dir = paths.work_profile_dir().join("pycache");
    let _ = fs::create_dir_all(&pycache_dir);
    let pycache_str = pycache_dir.to_string_lossy().into_owned();
    if !envs.iter().any(|(k, _)| k == "PYTHONPYCACHEPREFIX") {
        envs.push(("PYTHONPYCACHEPREFIX".to_string(), pycache_str));
    }

    let is_office = is_trusted_office_command(command);
    if is_office && !full_access {
        if let Some(outdir) = extract_office_outdir(args) {
            let p = Path::new(&outdir);
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd_path.join(p)
            };
            let check_path = if resolved.exists() {
                resolved.canonicalize().unwrap_or(resolved)
            } else if let Some(parent) = resolved.parent() {
                if parent.exists() {
                    parent
                        .canonicalize()
                        .map(|c| {
                            if let Some(name) = resolved.file_name() {
                                c.join(name)
                            } else {
                                c
                            }
                        })
                        .unwrap_or(resolved)
                } else {
                    resolved
                }
            } else {
                resolved
            };
            if !is_path_within_workspace_or_access_roots(&check_path, paths, workspace_id, run_id)?
            {
                return Ok(failed_result(
                    &execution_id,
                    tool_name,
                    action,
                    &started_at,
                    &format!("Office 命令输出目录 '--outdir {outdir}' 超出已授权的 Workspace 范围"),
                ));
            }
        }

        for arg in args {
            if is_office_document_arg(arg) {
                let p = Path::new(arg);
                let resolved = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    cwd_path.join(p)
                };
                if let Ok(canon) = resolved.canonicalize() {
                    if !is_path_within_workspace_or_access_roots(
                        &canon,
                        paths,
                        workspace_id,
                        run_id,
                    )? {
                        return Ok(failed_result(
                            &execution_id,
                            tool_name,
                            action,
                            &started_at,
                            &format!("Office 命令输入文件 '{}' 超出已授权的 Workspace 范围", arg),
                        ));
                    }
                }
            }
        }
    }

    let (mut cmd, denial_signatures, runner_failure_signatures) = if full_access
        || host_fallback
        || is_office
    {
        let raw_command = ExecutionCommand {
            program: command.into(),
            args: args.iter().cloned().map(Into::into).collect(),
            current_dir: cwd_path.clone(),
            envs: envs.clone(),
        };
        let prepared_command = if host_fallback || is_office {
            let temp_dir = paths.work_profile_dir().join("tmp").join(&execution_id);
            fs::create_dir_all(&temp_dir).map_err(|error| {
                format!(
                    "failed to create host execution temp directory {}: {error}",
                    temp_dir.display()
                )
            })?;
            match WorkSandboxLauncher::prepare_host_command(&raw_command, &temp_dir) {
                Ok(cmd) => cmd,
                Err(SandboxError::PolicyUnsupported(msg))
                    if msg.contains("Work executable was not found") =>
                {
                    return Ok(failed_result(
                        &execution_id,
                        tool_name,
                        action,
                        &started_at,
                        &format!("{msg}. Please ensure the executable is installed or use an alternative command."),
                    ));
                }
                Err(error) => return Err(format!("host command preparation failed: {error}")),
            }
        } else {
            raw_command
        };

        let mut cmd = Command::new(&prepared_command.program);
        cmd.current_dir(&prepared_command.current_dir);
        for (key, value) in &prepared_command.envs {
            cmd.env(key, value);
        }
        for arg in &prepared_command.args {
            cmd.arg(arg);
        }
        (cmd, Vec::new(), Vec::new())
    } else {
        let mut writable_roots = Vec::new();
        if workspace_id.trim().is_empty() {
            let run_id = run_id.ok_or_else(|| {
                "Standalone Work command requires a WorkRun ID for sandbox confinement".to_string()
            })?;
            let standalone_dir = paths
                .standalone_task_dir(run_id)
                .map_err(|error| format!("standalone Work directory resolution failed: {error}"))?;
            writable_roots.push(standalone_dir);
        } else {
            let ws_dir = paths
                .workspace_dir(workspace_id)
                .map_err(|error| format!("workspace directory resolution failed: {error}"))?;
            writable_roots.push(ws_dir);
            if !writable_roots.contains(&cwd_path) {
                writable_roots.push(cwd_path.clone());
            }
        }
        writable_roots.extend(connector_home_roots);
        let mut read_only_roots = Vec::new();
        if !workspace_id.trim().is_empty() {
            let manifest_path = paths
                .manifest_path(workspace_id)
                .map_err(|error| format!("workspace manifest resolution failed: {error}"))?;
            let content = std::fs::read_to_string(&manifest_path).map_err(|error| {
                format!(
                    "workspace manifest {} could not be read: {error}",
                    manifest_path.display()
                )
            })?;
            let workspace = serde_json::from_str::<WorkWorkspace>(&content).map_err(|error| {
                format!(
                    "workspace manifest {} is invalid: {error}",
                    manifest_path.display()
                )
            })?;
            for root in workspace.access_roots {
                if root.writable {
                    writable_roots.push(root.path.into());
                } else {
                    read_only_roots.push(root.path.into());
                }
            }
        }
        let raw_command = ExecutionCommand {
            program: command.into(),
            args: args.iter().cloned().map(Into::into).collect(),
            current_dir: cwd_path.clone(),
            envs,
        };
        let confined = match WorkSandboxLauncher::confine_work_command(
            &raw_command,
            &paths.work_profile_dir(),
            &writable_roots,
            &read_only_roots,
        ) {
            Ok(confined) => confined,
            Err(SandboxError::PolicyUnsupported(msg))
                if msg.contains("Work executable was not found") =>
            {
                return Ok(failed_result(
                    &execution_id,
                    tool_name,
                    action,
                    &started_at,
                    &format!("{msg}. Please ensure the executable is installed or use an alternative command."),
                ));
            }
            Err(error) => {
                return Ok(failed_sandbox_result(
                    &execution_id,
                    tool_name,
                    action,
                    &started_at,
                    &format!("Work sandbox confinement failed: {error}"),
                ));
            }
        };

        let mut cmd = Command::new(&confined.program);
        cmd.current_dir(&confined.current_dir);
        cmd.env_clear();
        for (key, value) in &confined.envs {
            cmd.env(key, value);
        }
        for arg in &confined.args {
            cmd.arg(arg);
        }
        (
            cmd,
            confined.denial_signatures,
            confined.runner_failure_signatures,
        )
    };
    cmd.kill_on_drop(true);

    // Dedicated process group so timeout can kill the whole tree (background helpers included).
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

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(failed_sandbox_result(
                &execution_id,
                tool_name,
                action,
                &started_at,
                &format!("Failed to spawn command '{command}': {e}"),
            ));
        }
    };

    // pre_exec setpgid(0,0) guarantees pgid == child PID post-spawn.
    #[cfg(unix)]
    let spawned_pgid: i32 = child.id().map(|pid| pid as i32).unwrap_or(0);
    #[cfg(not(unix))]
    let spawned_pgid: i32 = 0;

    let stdout_pipe = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr_pipe = child.stderr.take().ok_or("Failed to capture stderr")?;

    // Concurrent readers prevent pipe-buffer backpressure deadlock.
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

    let timeout_duration = Duration::from_secs(timeout_secs.clamp(1, 600));
    let (mut status, mut failure_kind, exit_code, stdout, mut stderr) =
        match tokio::time::timeout(timeout_duration, child.wait()).await {
            Ok(Ok(exit_status)) => {
                kill_process_group(spawned_pgid).await;
                let grace = Duration::from_millis(200);
                let (out_bytes, err_bytes) = tokio::time::timeout(grace, async {
                    (
                        out_handle.await.unwrap_or_default(),
                        err_handle.await.unwrap_or_default(),
                    )
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
                    (
                        WorkExecutionStatus::Failed,
                        Some(SandboxDenialClassifier::classify_failure(
                            &err_str,
                            exit_status.code(),
                            &denial_signatures,
                            &runner_failure_signatures,
                        )),
                        exit_status.code(),
                        out_str,
                        err_str,
                    )
                }
            }
            Ok(Err(e)) => {
                kill_process_group(spawned_pgid).await;
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
                kill_process_group(spawned_pgid).await;
                out_handle.abort();
                err_handle.abort();
                (
                    WorkExecutionStatus::TimedOut,
                    None,
                    None,
                    String::new(),
                    format!(
                        "Command timed out after {} seconds and process was killed",
                        timeout_secs
                    ),
                )
            }
        };

    // Verify expected outputs exist on disk (mirrors host_executor behaviour).
    let mut verified_outputs = Vec::new();
    if status == WorkExecutionStatus::Success {
        for output_rel in expected_outputs {
            let resolved_output = if workspace_id.trim().is_empty() {
                let run_id = run_id.ok_or_else(|| {
                    "Standalone Work output verification requires a WorkRun ID".to_string()
                })?;
                paths.resolve_standalone_path(run_id, std::path::Path::new(output_rel), false)?
            } else {
                paths.resolve_workspace_path(
                    workspace_id,
                    std::path::Path::new(output_rel),
                    false,
                )?
            };
            if is_office {
                if let Err(err) = verify_office_output(&resolved_output) {
                    status = WorkExecutionStatus::Failed;
                    failure_kind = Some(ExecutionFailureKind::CapabilityFailure);
                    stderr = format!("Office 引擎执行后成果校验失败: {err}");
                    verified_outputs.clear();
                    break;
                }
            }
            if resolved_output.exists() {
                verified_outputs.push(output_rel.clone());
            }
        }
    }

    let finished_at = Utc::now().to_rfc3339();

    Ok(WorkExecutionResult {
        execution_id,
        resource_id: tool_name.to_string(),
        action: action.to_string(),
        status,
        failure_kind,
        exit_code,
        stdout,
        stderr,
        outputs: verified_outputs,
        started_at,
        finished_at,
    })
}

fn sanitize_command_args(command: &str, args: &[String]) -> (String, Vec<String>) {
    let mut clean_cmd = command.trim().to_string();
    if clean_cmd.ends_with("2>&1") {
        clean_cmd = clean_cmd.trim_end_matches("2>&1").trim().to_string();
    }
    let mut clean_args: Vec<String> = args.iter().filter(|a| *a != "2>&1").cloned().collect();
    if clean_cmd.contains(' ') {
        let parts: Vec<String> = clean_cmd
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if !parts.is_empty() {
            clean_cmd = parts[0].clone();
            let mut prefix_args: Vec<String> = parts[1..]
                .iter()
                .filter(|s| s.as_str() != "2>&1")
                .cloned()
                .collect();
            prefix_args.append(&mut clean_args);
            clean_args = prefix_args;
        }
    }
    (clean_cmd, clean_args)
}

fn is_office_document_arg(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    lower.ends_with(".xlsx")
        || lower.ends_with(".xls")
        || lower.ends_with(".csv")
        || lower.ends_with(".docx")
        || lower.ends_with(".doc")
        || lower.ends_with(".pptx")
        || lower.ends_with(".ppt")
        || lower.ends_with(".pdf")
        || lower.ends_with(".ods")
        || lower.ends_with(".odt")
        || lower.ends_with(".odp")
        || lower.ends_with(".txt")
        || lower.ends_with(".rtf")
}

fn is_path_within_workspace_or_access_roots(
    canon: &Path,
    paths: &WorkPaths,
    workspace_id: &str,
    run_id: Option<&str>,
) -> Result<bool, String> {
    if workspace_id.trim().is_empty() {
        if let Some(r_id) = run_id {
            if let Ok(s_dir) = paths.standalone_task_dir(r_id) {
                if let Ok(s_canon) = s_dir.canonicalize() {
                    if canon.starts_with(&s_canon) {
                        return Ok(true);
                    }
                }
            }
        }
    } else {
        if let Ok(ws_dir) = paths.workspace_dir(workspace_id) {
            if let Ok(ws_canon) = ws_dir.canonicalize() {
                if canon.starts_with(&ws_canon) {
                    return Ok(true);
                }
            }
        }
        if let Ok(manifest_path) = paths.manifest_path(workspace_id) {
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(ws) = serde_json::from_str::<WorkWorkspace>(&content) {
                    for root in ws.access_roots {
                        let root_path = Path::new(&root.path);
                        if let Ok(r_canon) = root_path.canonicalize() {
                            if canon.starts_with(&r_canon) {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}

fn verify_office_output(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("输出文件不存在: {}", path.display()));
    }
    let metadata = fs::metadata(path).map_err(|e| format!("无法读取输出文件元数据: {e}"))?;
    if metadata.len() == 0 {
        return Err(format!("输出文件为空 (0 字节): {}", path.display()));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "xlsx" || ext == "docx" || ext == "pptx" {
        use std::io::Read;
        let mut header = [0u8; 4];
        let mut file = fs::File::open(path).map_err(|e| format!("无法打开输出文件: {e}"))?;
        if file.read_exact(&mut header).is_err() || &header != b"PK\x03\x04" {
            return Err(format!(
                "输出文件不是有效的 Office 压缩文档 (缺少 ZIP 签名): {}",
                path.display()
            ));
        }
    } else if ext == "pdf" {
        use std::io::Read;
        let mut header = [0u8; 4];
        let mut file = fs::File::open(path).map_err(|e| format!("无法打开输出文件: {e}"))?;
        if file.read_exact(&mut header).is_err() || &header != b"%PDF" {
            return Err(format!("输出文件不是有效的 PDF 文档: {}", path.display()));
        }
    }
    Ok(())
}

fn extract_office_outdir(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--outdir" || arg == "-outdir" {
            if let Some(next) = iter.next() {
                return Some(next.clone());
            }
        } else if let Some(stripped) = arg.strip_prefix("--outdir=") {
            return Some(stripped.to_string());
        } else if let Some(stripped) = arg.strip_prefix("-outdir=") {
            return Some(stripped.to_string());
        }
    }
    None
}

fn is_trusted_office_command(command: &str) -> bool {
    let cmd_path = Path::new(command);
    let file_name = cmd_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();
    if !matches!(
        file_name.as_str(),
        "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
    ) {
        return false;
    }

    if cmd_path.is_absolute() || command.contains('/') || command.contains('\\') {
        return crate::work::command_info::is_trusted_office_executable_path(cmd_path);
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(command);
            if candidate.is_file()
                && crate::work::command_info::is_trusted_office_executable_path(&candidate)
            {
                return true;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mac_candidates = [
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
            "/Applications/OpenOffice.app/Contents/MacOS/soffice",
            "/opt/homebrew/bin/soffice",
            "/usr/local/bin/soffice",
        ];
        for candidate in mac_candidates {
            let p = Path::new(candidate);
            if p.is_file() && crate::work::command_info::is_trusted_office_executable_path(p) {
                return true;
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let linux_candidates = [
            "/usr/bin/soffice",
            "/usr/bin/libreoffice",
            "/usr/local/bin/soffice",
            "/snap/bin/libreoffice",
        ];
        for candidate in linux_candidates {
            let p = Path::new(candidate);
            if p.is_file() && crate::work::command_info::is_trusted_office_executable_path(p) {
                return true;
            }
        }
    }

    false
}

fn failed_result(
    execution_id: &str,
    tool_name: &str,
    action: &str,
    started_at: &str,
    err: &str,
) -> WorkExecutionResult {
    WorkExecutionResult {
        execution_id: execution_id.to_string(),
        resource_id: tool_name.to_string(),
        action: action.to_string(),
        status: WorkExecutionStatus::Failed,
        failure_kind: Some(ExecutionFailureKind::CapabilityFailure),
        exit_code: Some(-1),
        stdout: String::new(),
        stderr: err.to_string(),
        outputs: Vec::new(),
        started_at: started_at.to_string(),
        finished_at: Utc::now().to_rfc3339(),
    }
}

fn failed_sandbox_result(
    execution_id: &str,
    tool_name: &str,
    action: &str,
    started_at: &str,
    err: &str,
) -> WorkExecutionResult {
    WorkExecutionResult {
        execution_id: execution_id.to_string(),
        resource_id: tool_name.to_string(),
        action: action.to_string(),
        status: WorkExecutionStatus::Failed,
        failure_kind: Some(ExecutionFailureKind::SandboxInfrastructureFailure),
        exit_code: Some(-1),
        stdout: String::new(),
        stderr: err.to_string(),
        outputs: Vec::new(),
        started_at: started_at.to_string(),
        finished_at: Utc::now().to_rfc3339(),
    }
}

/// Kill all descendant processes in the dedicated process group.
/// `pgid` == child PID (set by pre_exec setpgid(0,0)). 0 = no group established.
/// Sends SIGTERM first (graceful), then waits 50ms, then SIGKILL (forceful).
async fn kill_process_group(pgid: i32) {
    #[cfg(unix)]
    if pgid > 0 {
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pgid;
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

/// Materialize and validate Connector-declared HOME-relative directories.
///
/// HOME itself is intentionally not a writable sandbox root. A package may
/// request only directories such as `.lark-cli` or `.config/my-cli`; those
/// directories are created before the sandbox is launched and then mounted as
/// writable roots for that Connector CLI invocation.
fn prepare_home_paths(home_paths: &[String]) -> Result<Vec<PathBuf>, String> {
    if home_paths.is_empty() {
        return Ok(Vec::new());
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not available".to_string())?;
    let home = fs::canonicalize(&home)
        .map_err(|error| format!("cannot resolve HOME {}: {error}", home.display()))?;
    let mut roots = Vec::new();
    for relative in home_paths {
        let relative_path = Path::new(relative);
        if relative.trim().is_empty()
            || relative_path.is_absolute()
            || relative_path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "HOME path must be a non-empty relative directory: {relative}"
            ));
        }

        let target = home.join(relative_path);
        fs::create_dir_all(&target)
            .map_err(|error| format!("cannot prepare {}: {error}", target.display()))?;
        let canonical = fs::canonicalize(&target)
            .map_err(|error| format!("cannot resolve {}: {error}", target.display()))?;
        if !canonical.starts_with(&home) || canonical == home {
            return Err(format!(
                "HOME path escapes the user HOME: {}",
                target.display()
            ));
        }
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_command_and_args_redirection() {
        let (cmd, args) = sanitize_command_args(
            "soffice --convert-to xlsx 2>&1",
            &["input.csv".to_string(), "2>&1".to_string()],
        );
        assert_eq!(cmd, "soffice");
        assert_eq!(args, vec!["--convert-to", "xlsx", "input.csv"]);
    }

    #[test]
    fn sanitizes_single_string_command() {
        let (cmd, args) =
            sanitize_command_args("soffice --headless --convert-to pdf sales.xlsx 2>&1", &[]);
        assert_eq!(cmd, "soffice");
        assert_eq!(
            args,
            vec!["--headless", "--convert-to", "pdf", "sales.xlsx"]
        );
    }

    #[test]
    fn identifies_office_document_args() {
        assert!(is_office_document_arg("input/sales.xlsx"));
        assert!(is_office_document_arg("/tmp/data.csv"));
        assert!(is_office_document_arg("out.docx"));
        assert!(is_office_document_arg("out.pdf"));
        assert!(!is_office_document_arg("--headless"));
        assert!(!is_office_document_arg("-env:UserInstallation"));
    }

    #[test]
    fn verifies_office_output_validation() {
        let dir = tempfile::tempdir().unwrap();

        // Missing file
        let missing = dir.path().join("missing.xlsx");
        assert!(verify_office_output(&missing).is_err());

        // Empty file
        let empty = dir.path().join("empty.xlsx");
        fs::write(&empty, b"").unwrap();
        assert!(verify_office_output(&empty).is_err());

        // Invalid zip header
        let corrupted = dir.path().join("corrupt.xlsx");
        fs::write(&corrupted, b"not a zip file").unwrap();
        assert!(verify_office_output(&corrupted).is_err());

        // Valid zip magic header PK\x03\x04
        let valid_xlsx = dir.path().join("valid.xlsx");
        fs::write(&valid_xlsx, b"PK\x03\x04something").unwrap();
        assert!(verify_office_output(&valid_xlsx).is_ok());

        // Valid PDF header
        let valid_pdf = dir.path().join("valid.pdf");
        fs::write(&valid_pdf, b"%PDF-1.4\nsome content").unwrap();
        assert!(verify_office_output(&valid_pdf).is_ok());
    }

    #[test]
    fn extracts_office_outdir_from_args() {
        assert_eq!(
            extract_office_outdir(&[
                "--headless".into(),
                "--convert-to".into(),
                "pdf".into(),
                "--outdir".into(),
                "output/dir".into(),
                "file.xlsx".into(),
            ]),
            Some("output/dir".to_string())
        );

        assert_eq!(
            extract_office_outdir(&[
                "--headless".into(),
                "--outdir=output/dir".into(),
                "file.xlsx".into(),
            ]),
            Some("output/dir".to_string())
        );

        assert_eq!(
            extract_office_outdir(&[
                "--headless".into(),
                "--convert-to".into(),
                "pdf".into(),
                "file.xlsx".into(),
            ]),
            None
        );
    }

    #[test]
    fn is_trusted_office_command_rejects_untrusted_paths() {
        assert!(!is_trusted_office_command("/tmp/soffice"));
        assert!(!is_trusted_office_command("./soffice"));
        assert!(!is_trusted_office_command("/var/tmp/libreoffice"));
    }
}
