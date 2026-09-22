//! Pi packages owned by the AgentCabin runtime.
//!
//! Common MCP packages are installed once under `~/.agentcabin/pi/system` and
//! injected through mode-specific adapters. Native Web is backend-owned; the
//! legacy `pi-web-access` source remains reserved only to prevent stale
//! profiles from loading a second provider. Work-only packages (such as
//! subagents) remain in the Work profile.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::agent::claude_stream::{augmented_path, resolve_pi_path};
use crate::process_ext::HideConsole;
use crate::work::paths::WorkPaths;
use crate::work::sandbox::{ExecutionCommand, WorkSandboxLauncher};

pub const PI_MCP_ADAPTER_PACKAGE_NAME: &str = "pi-mcp-adapter";
pub const PI_MCP_ADAPTER_VERSION: &str = "2.22.0";
pub const PI_MCP_ADAPTER_SOURCE: &str = "npm:pi-mcp-adapter@2.22.0";

pub const PI_WEB_ACCESS_PACKAGE_NAME: &str = "pi-web-access";
/// Legacy source retained for stale-profile detection; it is no longer
/// provisioned or loaded by AgentCabin.
pub const PI_WEB_ACCESS_VERSION: &str = "0.23.0";
pub const PI_WEB_ACCESS_SOURCE: &str = "npm:pi-web-access@0.23.0";

pub const PI_SUBAGENTS_PACKAGE_NAME: &str = "pi-subagents";
pub const PI_SUBAGENTS_VERSION: &str = "0.51.0";
pub const PI_SUBAGENTS_SOURCE: &str = "npm:pi-subagents@0.51.0";

pub const SYSTEM_MANAGED_PACKAGE_MESSAGE: &str =
    "这是 AgentCabin Work 系统组件，已由系统管理，请在对应能力设置中启用或停用。";

const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

static PROVISIONING_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

fn provisioning_lock() -> &'static Mutex<()> {
    PROVISIONING_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPackageStatus {
    Missing,
    Installed {
        version: String,
    },
    VersionMismatch {
        installed_version: String,
        expected_version: String,
    },
    Corrupt {
        error: String,
    },
}

fn package_reference_matches(reference: &str, package_name: &str) -> bool {
    reference == package_name
        || reference
            .strip_prefix(package_name)
            .is_some_and(|suffix| suffix.starts_with('@'))
}

fn final_reference_segment(source: &str) -> &str {
    source
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or(source.trim())
}

pub fn is_pi_mcp_adapter_source(source: &str) -> bool {
    let normalized = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    package_reference_matches(normalized, PI_MCP_ADAPTER_PACKAGE_NAME)
        || package_reference_matches(
            final_reference_segment(normalized),
            PI_MCP_ADAPTER_PACKAGE_NAME,
        )
}

pub fn is_pi_web_access_source(source: &str) -> bool {
    let normalized = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    package_reference_matches(normalized, PI_WEB_ACCESS_PACKAGE_NAME)
        || package_reference_matches(
            final_reference_segment(normalized),
            PI_WEB_ACCESS_PACKAGE_NAME,
        )
}

pub fn is_pi_subagents_source(source: &str) -> bool {
    let normalized = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    package_reference_matches(normalized, PI_SUBAGENTS_PACKAGE_NAME)
        || package_reference_matches(
            final_reference_segment(normalized),
            PI_SUBAGENTS_PACKAGE_NAME,
        )
}

pub fn is_system_managed_source(source: &str) -> bool {
    is_pi_mcp_adapter_source(source)
        || is_pi_web_access_source(source)
        || is_pi_subagents_source(source)
}

pub fn is_system_managed_package_name(name: &str) -> bool {
    is_system_managed_source(name)
}

pub fn package_manifest_path(profile_dir: &Path, package_name: &str) -> PathBuf {
    profile_dir
        .join("npm")
        .join("node_modules")
        .join(package_name)
        .join("package.json")
}

pub fn common_system_package_manifest_path(paths: &WorkPaths, package_name: &str) -> PathBuf {
    package_manifest_path(&paths.pi_system_dir(), package_name)
}

pub fn common_system_package_entry_path(paths: &WorkPaths, package_name: &str) -> PathBuf {
    paths
        .pi_system_dir()
        .join("npm")
        .join("node_modules")
        .join(package_name)
}

fn managed_package_name(source: &str) -> Option<&'static str> {
    if is_pi_mcp_adapter_source(source) {
        Some(PI_MCP_ADAPTER_PACKAGE_NAME)
    } else if is_pi_subagents_source(source) {
        Some(PI_SUBAGENTS_PACKAGE_NAME)
    } else if is_pi_web_access_source(source) {
        Some(PI_WEB_ACCESS_PACKAGE_NAME)
    } else {
        None
    }
}

/// Return the local extension entry for a Work-only system package when the
/// expected version is already installed. Passing this path to Pi avoids a
/// per-run `npm:` installation into its temporary extension directory.
pub fn installed_system_package_entry_path(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
) -> Option<PathBuf> {
    if !matches!(
        system_package_status(paths, package_name, expected_version),
        SystemPackageStatus::Installed { .. }
    ) {
        return None;
    }

    let entry = paths
        .work_profile_dir()
        .join("npm")
        .join("node_modules")
        .join(package_name)
        .join("index.ts");
    entry.is_file().then_some(entry)
}

fn package_status_at(
    profile_dir: &Path,
    package_name: &str,
    expected_version: &str,
) -> SystemPackageStatus {
    let manifest_path = package_manifest_path(profile_dir, package_name);
    if !manifest_path.is_file() {
        return SystemPackageStatus::Missing;
    }
    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(err) => {
            return SystemPackageStatus::Corrupt {
                error: err.to_string(),
            }
        }
    };
    let value: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(err) => {
            return SystemPackageStatus::Corrupt {
                error: err.to_string(),
            }
        }
    };
    let Some(version) = value.get("version").and_then(Value::as_str) else {
        return SystemPackageStatus::Corrupt {
            error: "Missing or invalid version in package.json".to_string(),
        };
    };
    if version == expected_version {
        SystemPackageStatus::Installed {
            version: version.to_string(),
        }
    } else {
        SystemPackageStatus::VersionMismatch {
            installed_version: version.to_string(),
            expected_version: expected_version.to_string(),
        }
    }
}

pub fn system_package_status(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
) -> SystemPackageStatus {
    package_status_at(&paths.work_profile_dir(), package_name, expected_version)
}

pub fn common_system_package_status(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
) -> SystemPackageStatus {
    package_status_at(&paths.pi_system_dir(), package_name, expected_version)
}

pub fn is_package_installed(paths: &WorkPaths, package_name: &str, expected_version: &str) -> bool {
    matches!(
        system_package_status(paths, package_name, expected_version),
        SystemPackageStatus::Installed { .. }
    )
}

pub fn is_common_system_package_installed(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
) -> bool {
    matches!(
        common_system_package_status(paths, package_name, expected_version),
        SystemPackageStatus::Installed { .. }
    )
}

fn command_output_message(stdout: &[u8], stderr: &[u8]) -> String {
    let output = if stderr.is_empty() { stdout } else { stderr };
    let output_text = String::from_utf8_lossy(output);
    let lines = output_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(8);
    lines[start..].join("\n").chars().take(1200).collect()
}

pub async fn install_system_package(paths: &WorkPaths, source: &str) -> Result<String, String> {
    install_package_at(&paths.work_profile_dir(), source, "Work Pi 系统包").await
}

fn system_package_install_environment(profile: &Path) -> Vec<(String, String)> {
    vec![
        (
            "PI_CODING_AGENT_DIR".into(),
            profile.to_string_lossy().into_owned(),
        ),
        (
            "MCP_OAUTH_DIR".into(),
            profile.join("mcp-oauth").to_string_lossy().into_owned(),
        ),
        (
            "npm_config_cache".into(),
            profile.join("npm-cache").to_string_lossy().into_owned(),
        ),
        // The installer must reach the npm registry. Without this marker the
        // Work sandbox resolves to NetworkPolicy::Deny and npm fails with
        // `getaddrinfo ENOTFOUND` because Seatbelt blocks DNS too.
        // `WORK_PROVIDER_NETWORK_POLICY` maps to outbound-only network
        // (inbound listeners stay denied by the sandbox profile).
        (
            crate::work::sandbox::WORK_NETWORK_POLICY_ENV.into(),
            crate::work::sandbox::WORK_PROVIDER_NETWORK_POLICY.into(),
        ),
        ("PATH".into(), augmented_path()),
    ]
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("无法创建目录 {}: {e}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("无法读取目录 {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("无法读取目录项: {e}"))?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("无法获取文件类型: {e}"))?;
        if file_type.is_dir() {
            copy_dir_all(&entry_path, &target_path)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            {
                if let Ok(link_target) = fs::read_link(&entry_path) {
                    let _ = std::os::unix::fs::symlink(link_target, &target_path);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = fs::copy(&entry_path, &target_path);
            }
        } else {
            fs::copy(&entry_path, &target_path).map_err(|e| {
                format!(
                    "无法复制文件 {} -> {}: {e}",
                    entry_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn deploy_bundled_system_package(bundled_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建系统包上级目录 {}: {error}", parent.display()))?;
    }
    if target_dir.is_symlink() {
        let _ = fs::remove_file(target_dir);
    } else if target_dir.exists() {
        let _ = fs::remove_dir_all(target_dir);
    }
    #[cfg(unix)]
    {
        match std::os::unix::fs::symlink(bundled_dir, target_dir) {
            Ok(()) => return Ok(()),
            Err(err) => {
                log::warn!(
                    "无法创建系统包软链接 {} -> {}: {err}，尝试直接拷贝",
                    target_dir.display(),
                    bundled_dir.display()
                );
            }
        }
    }
    copy_dir_all(bundled_dir, target_dir)
}

async fn install_package_at(profile: &Path, source: &str, label: &str) -> Result<String, String> {
    fs::create_dir_all(profile)
        .map_err(|error| format!("无法创建 Pi 系统包目录 {}: {error}", profile.display()))?;

    let bundled_pkg = managed_package_name(source).and_then(|package_name| {
        crate::agent::claude_stream::bundled_pi_package_path(package_name)
            .map(|path| (package_name, PathBuf::from(path)))
    });

    if let Some((pkg_name, bundled_path)) = bundled_pkg {
        let target_dir = profile.join("npm").join("node_modules").join(pkg_name);
        deploy_bundled_system_package(&bundled_path, &target_dir)?;
        return Ok(format!("{label} {source} (内置包) 部署完成"));
    }

    // A packaged app may have left a symlink to its old installation path. Pi's
    // installer can report success without replacing that broken link, leaving
    // the post-install status check at Missing. Remove only this app-owned
    // package link before falling back to the real installer.
    if let Some(package_name) = managed_package_name(source) {
        let target_dir = profile.join("npm").join("node_modules").join(package_name);
        if target_dir.is_symlink() {
            fs::remove_file(&target_dir).map_err(|error| {
                format!(
                    "无法移除失效的 Pi 系统包链接 {}: {error}",
                    target_dir.display()
                )
            })?;
        }
    }

    let install_source = source.to_string();
    let raw_command = ExecutionCommand {
        program: PathBuf::from(resolve_pi_path()),
        args: vec![
            OsString::from("install"),
            OsString::from(&install_source),
            OsString::from("--no-approve"),
        ],
        current_dir: profile.to_path_buf(),
        envs: system_package_install_environment(profile),
    };
    let confined = WorkSandboxLauncher::confine_work_command(&raw_command, profile, &[], &[])
        .map_err(|error| format!("无法在 Work sandbox 中启动 Pi 安装器: {error}"))?;
    let mut command = Command::new(&confined.program);
    command
        .env_clear()
        .args(&confined.args)
        .current_dir(&confined.current_dir)
        .env_remove("CLAUDECODE");
    for (key, value) in &confined.envs {
        command.env(key, value);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .hide_console();

    let output = timeout(INSTALL_TIMEOUT, command.output())
        .await
        .map_err(|_| format!("Pi 系统包 {source} 安装超时"))?
        .map_err(|error| format!("无法运行 Pi 安装器: {error}"))?;
    if !output.status.success() {
        let message = command_output_message(&output.stdout, &output.stderr);
        return Err(if message.is_empty() {
            format!("Pi 系统包 {source} 安装失败: {}", output.status)
        } else {
            format!("Pi 系统包 {source} 安装失败: {message}")
        });
    }
    Ok(format!("{label} {source} 安装完成"))
}

pub async fn install_common_system_package(
    paths: &WorkPaths,
    source: &str,
) -> Result<String, String> {
    install_package_at(&paths.pi_system_dir(), source, "AgentCabin 通用 Pi 系统包").await
}

pub async fn ensure_system_package(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
    source: &str,
) -> Result<(), String> {
    let _guard = provisioning_lock().lock().await;
    match system_package_status(paths, package_name, expected_version) {
        SystemPackageStatus::Installed { .. } => Ok(()),
        SystemPackageStatus::Missing
        | SystemPackageStatus::VersionMismatch { .. }
        | SystemPackageStatus::Corrupt { .. } => {
            install_system_package(paths, source).await?;
            match system_package_status(paths, package_name, expected_version) {
                SystemPackageStatus::Installed { .. } => Ok(()),
                status => Err(format!(
                    "Pi 系统包 {package_name} 安装后状态检查未通过: {status:?}"
                )),
            }
        }
    }
}

pub async fn ensure_common_system_package(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
    source: &str,
) -> Result<(), String> {
    let _guard = provisioning_lock().lock().await;
    paths.ensure_layout()?;
    match common_system_package_status(paths, package_name, expected_version) {
        SystemPackageStatus::Installed { .. } => Ok(()),
        SystemPackageStatus::Missing
        | SystemPackageStatus::VersionMismatch { .. }
        | SystemPackageStatus::Corrupt { .. } => {
            install_common_system_package(paths, source).await?;
            match common_system_package_status(paths, package_name, expected_version) {
                SystemPackageStatus::Installed { .. } => Ok(()),
                status => Err(format!(
                    "通用 Pi 系统包 {package_name} 安装后状态检查未通过: {status:?}"
                )),
            }
        }
    }
}

pub async fn repair_system_package(
    paths: &WorkPaths,
    package_name: &str,
    expected_version: &str,
    source: &str,
) -> Result<String, String> {
    let manifest = package_manifest_path(&paths.work_profile_dir(), package_name);
    if let Some(parent) = manifest.parent() {
        if parent.is_symlink() {
            let _ = fs::remove_file(parent);
        } else if parent.exists() {
            let _ = fs::remove_dir_all(parent);
        }
    }
    install_system_package(paths, source).await?;
    match system_package_status(paths, package_name, expected_version) {
        SystemPackageStatus::Installed { version } => {
            Ok(format!("已成功修复 {package_name}@{version}"))
        }
        status => Err(format!("修复 {package_name} 失败: {status:?}")),
    }
}

/// Ensure mandatory system packages required for Work Mode runtime.
/// Runs with single-flight mutex protection.
pub async fn ensure_required_work_packages(paths: &WorkPaths) -> Result<(), String> {
    paths.ensure_layout()?;
    fs::create_dir_all(paths.work_profile_dir().join("npm-cache"))
        .map_err(|error| format!("无法创建 Work Pi npm 缓存目录: {error}"))?;
    ensure_system_package(
        paths,
        PI_SUBAGENTS_PACKAGE_NAME,
        PI_SUBAGENTS_VERSION,
        PI_SUBAGENTS_SOURCE,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn recognizes_reserved_npm_and_repository_references() {
        assert!(is_pi_mcp_adapter_source("pi-mcp-adapter"));
        assert!(is_pi_mcp_adapter_source("npm:pi-mcp-adapter@2.22.0"));
        assert!(is_pi_mcp_adapter_source(
            "git:github.com/nicobailon/pi-mcp-adapter.git"
        ));
        assert!(is_pi_web_access_source("npm:pi-web-access@0.23.0"));
        assert!(is_pi_web_access_source(
            "https://github.com/nicobailon/pi-web-access"
        ));
        assert!(is_pi_subagents_source("pi-subagents"));
        assert!(is_pi_subagents_source("npm:pi-subagents@0.51.0"));
        assert!(is_pi_subagents_source(
            "git:github.com/nicobailon/pi-subagents.git"
        ));
        assert!(is_system_managed_source("npm:pi-subagents@0.51.0"));
        assert!(is_system_managed_source("npm:pi-web-access@0.23.0"));
        assert!(!is_system_managed_source("npm:@acme/pi-web-access-wrapper"));
        assert!(!is_system_managed_source("npm:@user/pi-subagents-fork"));
    }

    #[test]
    fn checks_system_package_status_correctly() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();

        // 1. Missing
        assert_eq!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::Missing
        );

        // 2. Corrupted json
        let manifest = package_manifest_path(&paths.work_profile_dir(), PI_SUBAGENTS_PACKAGE_NAME);
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(&manifest, "not valid json").unwrap();
        assert!(matches!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::Corrupt { .. }
        ));

        // 3. Version mismatch
        fs::write(&manifest, r#"{"name":"pi-subagents","version":"0.50.0"}"#).unwrap();
        assert_eq!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::VersionMismatch {
                installed_version: "0.50.0".into(),
                expected_version: "0.51.0".into(),
            }
        );

        // 4. Exact installed
        fs::write(&manifest, r#"{"name":"pi-subagents","version":"0.51.0"}"#).unwrap();
        assert_eq!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::Installed {
                version: "0.51.0".into(),
            }
        );
        assert!(is_package_installed(
            &paths,
            PI_SUBAGENTS_PACKAGE_NAME,
            PI_SUBAGENTS_VERSION
        ));
    }

    #[test]
    fn system_package_install_environment_is_profile_scoped() {
        let profile = Path::new("/tmp/agentcabin-pi-system");
        let envs = system_package_install_environment(profile);
        let cache = envs
            .iter()
            .find(|(key, _)| key == "npm_config_cache")
            .map(|(_, value)| value.as_str());

        assert_eq!(
            cache,
            Some("/tmp/agentcabin-pi-system/npm-cache"),
            "system package installs must not write to the user's global ~/.npm cache"
        );
    }

    #[test]
    fn command_output_message_keeps_the_actionable_error_tail() {
        let message = command_output_message(
            b"",
            b"npm error code EPERM\nnpm error syscall open\nnpm error path /Users/test/.npm\nError: install failed",
        );

        assert!(message.contains("npm error code EPERM"));
        assert!(message.contains("npm error path /Users/test/.npm"));
        assert!(message.contains("Error: install failed"));
    }

    #[test]
    fn deploy_bundled_system_package_creates_valid_target_and_updates_status() {
        let temp = TempDir::new().unwrap();
        let bundled_dir = temp.path().join("mock_bundled").join("pi-subagents");
        fs::create_dir_all(&bundled_dir).unwrap();
        fs::write(
            bundled_dir.join("package.json"),
            r#"{"name":"pi-subagents","version":"0.51.0"}"#,
        )
        .unwrap();
        fs::write(bundled_dir.join("index.ts"), "export default {};").unwrap();

        let paths = WorkPaths::new(temp.path().join("user_data"));
        paths.ensure_layout().unwrap();

        let target_dir = paths
            .work_profile_dir()
            .join("npm")
            .join("node_modules")
            .join(PI_SUBAGENTS_PACKAGE_NAME);

        // Before deploy, it must be Missing
        assert_eq!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::Missing
        );

        // Deploy bundled package
        deploy_bundled_system_package(&bundled_dir, &target_dir).unwrap();

        // After deploy, status must be Installed
        assert_eq!(
            system_package_status(&paths, PI_SUBAGENTS_PACKAGE_NAME, PI_SUBAGENTS_VERSION),
            SystemPackageStatus::Installed {
                version: "0.51.0".into(),
            }
        );

        // installed_system_package_entry_path must resolve index.ts
        let entry = installed_system_package_entry_path(
            &paths,
            PI_SUBAGENTS_PACKAGE_NAME,
            PI_SUBAGENTS_VERSION,
        );
        assert_eq!(entry, Some(target_dir.join("index.ts")));
    }
}
