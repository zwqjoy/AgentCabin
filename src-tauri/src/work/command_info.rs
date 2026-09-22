//! Host-side command inspection for Work runtime.
//!
//! Provides `work_command_info` to inspect executables, versions, and recommended
//! execution channels without running arbitrary shell scripts, probing host
//! binaries with `cat` or `ps`, or guessing through blind trial-and-error.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfoResult {
    pub command: String,
    pub installed: bool,
    pub resolved_path: Option<String>,
    pub file_type: String,
    pub version: Option<String>,
    pub is_trusted: bool,
    pub recommended_channel: String,
    pub guidance: String,
}

/// Search PATH and platform-specific standard paths for an executable.
fn find_executable(command: &str) -> Option<PathBuf> {
    let p = Path::new(command);
    if p.is_absolute() && p.is_file() {
        return Some(p.to_path_buf());
    }

    let is_office = matches!(
        command.to_ascii_lowercase().as_str(),
        "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
    );

    // Platform-specific well-known paths for office
    if is_office {
        #[cfg(target_os = "macos")]
        {
            let mac_candidates = [
                "/Applications/LibreOffice.app/Contents/MacOS/soffice",
                "/opt/homebrew/bin/soffice",
                "/usr/local/bin/soffice",
            ];
            for candidate in mac_candidates {
                let candidate_path = Path::new(candidate);
                if candidate_path.is_file() {
                    return Some(candidate_path.to_path_buf());
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
                let candidate_path = Path::new(candidate);
                if candidate_path.is_file() {
                    return Some(candidate_path.to_path_buf());
                }
            }
        }
    }

    // PATH lookup
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(command);
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                for ext in &[".exe", ".cmd", ".bat"] {
                    let with_ext = dir.join(format!("{command}{ext}"));
                    if with_ext.is_file() {
                        return Some(with_ext);
                    }
                }
            }
        }
    }

    None
}

/// Resolve wrapper scripts (e.g. Homebrew's `/opt/homebrew/bin/soffice` shell wrapper)
/// to the actual underlying binary.
fn resolve_real_binary(path: &Path) -> PathBuf {
    let Ok(canonical) = path.canonicalize() else {
        return path.to_path_buf();
    };

    let Ok(source) = fs::read_to_string(&canonical) else {
        return canonical;
    };

    if source.starts_with("#!") {
        for token in source.split(|c: char| c.is_whitespace() || c == '\'' || c == '"') {
            if token.starts_with('/') {
                let candidate = Path::new(token);
                if candidate.is_file() {
                    let file_name = candidate
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if matches!(
                        file_name.as_str(),
                        "soffice" | "soffice.bin" | "libreoffice" | "ooffice" | "node" | "python"
                    ) {
                        return candidate
                            .canonicalize()
                            .unwrap_or_else(|_| candidate.to_path_buf());
                    }
                }
            }
        }
    }

    canonical
}

/// Detect basic binary/script file type using magic header bytes.
fn detect_file_type(path: &Path) -> String {
    let mut header = [0u8; 16];
    let Ok(mut file) = fs::File::open(path) else {
        return "unknown".to_string();
    };
    use std::io::Read;
    let bytes_read = file.read(&mut header).unwrap_or(0);
    if bytes_read < 2 {
        return "empty_or_small".to_string();
    }

    if header.starts_with(b"#!") {
        return "shell_script".to_string();
    }
    if header.starts_with(b"\x7fELF") {
        return "elf".to_string();
    }
    // Mach-O headers (x86_64, arm64, universal fat binary)
    if header.starts_with(b"\xfe\xed\xfa\xce")
        || header.starts_with(b"\xfe\xed\xfa\xcf")
        || header.starts_with(b"\xcf\xfa\xed\xfe")
        || header.starts_with(b"\xce\xfa\xed\xfe")
        || header.starts_with(b"\xca\xfe\xba\xbe")
    {
        return "mach-o".to_string();
    }
    if header.starts_with(b"MZ") {
        return "pe_windows".to_string();
    }

    "binary".to_string()
}

pub const SAFE_VERSION_PROBE_COMMANDS: &[&str] = &[
    "soffice",
    "soffice.bin",
    "libreoffice",
    "ooffice",
    "python",
    "python3",
    "node",
    "npm",
    "git",
    "cargo",
    "rustc",
    "pdftoppm",
    "pandoc",
    "go",
];

/// Check whether a physical path belongs to a trusted platform LibreOffice / OpenOffice installation.
/// Prevents untrusted scripts in /tmp, cwd, or user-controlled downloads from being elevated to host execution.
pub fn is_trusted_office_executable_path(path: &Path) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    let path_str = canonical.to_string_lossy().to_string();

    // Explicitly deny temporary, workspace, or user cache directories
    if path_str.contains("/tmp/")
        || path_str.starts_with("/tmp")
        || path_str.contains("/var/tmp/")
        || path_str.starts_with("/var/tmp")
        || path_str.contains("/private/tmp/")
        || path_str.starts_with("/private/tmp")
    {
        return false;
    }

    let file_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
        .unwrap_or_default();

    if !matches!(
        file_name.as_str(),
        "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
    ) {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        path_str.starts_with("/Applications/LibreOffice.app/")
            || path_str.starts_with("/Applications/OpenOffice.app/")
            || path_str.starts_with("/opt/homebrew/")
            || path_str.starts_with("/usr/local/")
            || path_str.starts_with("/usr/bin/")
    }

    #[cfg(target_os = "linux")]
    {
        path_str.starts_with("/usr/bin/")
            || path_str.starts_with("/usr/local/bin/")
            || path_str.starts_with("/usr/lib/libreoffice/")
            || path_str.starts_with("/snap/bin/")
            || path_str.starts_with("/var/lib/flatpak/")
    }

    #[cfg(windows)]
    {
        let lower = path_str.to_ascii_lowercase();
        lower.starts_with("c:\\program files\\libreoffice\\")
            || lower.starts_with("c:\\program files (x86)\\libreoffice\\")
            || lower.starts_with("c:\\program files\\openoffice\\")
            || lower.starts_with("c:\\program files (x86)\\openoffice\\")
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        false
    }
}

/// Safely probe version with short timeout, restricted to known-safe built-in commands.
async fn query_version(path: &Path, base_name: &str, is_office: bool) -> Option<String> {
    if !SAFE_VERSION_PROBE_COMMANDS.contains(&base_name) {
        return None;
    }
    if is_office && !is_trusted_office_executable_path(path) {
        return None;
    }

    let timeout_duration = Duration::from_secs(3);
    let mut cmd = Command::new(path);
    cmd.arg("--version");

    let run_res = timeout(timeout_duration, cmd.output()).await;
    match run_res {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                let first_line = stdout.lines().next().unwrap_or(&stdout).trim().to_string();
                return Some(first_line);
            }
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !stderr.is_empty() {
                let first_line = stderr.lines().next().unwrap_or(&stderr).trim().to_string();
                return Some(first_line);
            }
            None
        }
        _ => None,
    }
}

pub async fn inspect_command(raw_command: &str) -> CommandInfoResult {
    let command = raw_command.trim().to_string();
    let base_name = Path::new(&command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&command)
        .to_ascii_lowercase();

    let is_office_name = matches!(
        base_name.as_str(),
        "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
    );

    let is_trusted_name = matches!(
        base_name.as_str(),
        "soffice"
            | "soffice.bin"
            | "libreoffice"
            | "ooffice"
            | "python"
            | "python3"
            | "node"
            | "npm"
            | "git"
            | "pdftoppm"
            | "pandoc"
            | "cargo"
    );

    let found_path = find_executable(&command);
    if let Some(path) = found_path {
        let real_path = resolve_real_binary(&path);
        let file_type = detect_file_type(&real_path);
        let is_trusted_office = is_office_name && is_trusted_office_executable_path(&real_path);
        let version = query_version(&real_path, &base_name, is_office_name).await;
        let resolved_str = real_path.to_string_lossy().into_owned();

        let (recommended_channel, guidance, is_trusted) = if is_trusted_office {
            (
                "structured_host".to_string(),
                "soffice 已就绪。在 Work 中直接使用 work_run_command(command='soffice', args=[...], expected_outputs=[...])，Host 会自动以无界面模式和独立配置目录安全执行并校验产物。请勿使用 ps aux、阅读宿主二进制或安装 formulas。".to_string(),
                true,
            )
        } else if is_office_name {
            // Path is not in the trusted office whitelist (e.g. /tmp/soffice)
            (
                "blocked".to_string(),
                format!(
                    "Office 可执行文件位于非受信路径 ({resolved_str})，已拒绝通过宿主受信通道执行。"
                ),
                false,
            )
        } else if is_trusted_name {
            (
                "sandbox".to_string(),
                format!("{base_name} 已安装并在受控沙盒中就绪。在 Work 中使用 work_run_command 调用即可。"),
                true,
            )
        } else {
            (
                "host_approval".to_string(),
                format!("可执行文件已找到 ({resolved_str})，但该命令不是预置受信通道。若沙盒无法运行，Work 将生成一次性主机审批。"),
                false,
            )
        };

        CommandInfoResult {
            command,
            installed: true,
            resolved_path: Some(resolved_str),
            file_type,
            version,
            is_trusted,
            recommended_channel,
            guidance,
        }
    } else {
        CommandInfoResult {
            command: command.clone(),
            installed: false,
            resolved_path: None,
            file_type: "none".to_string(),
            version: None,
            is_trusted: false,
            recommended_channel: "blocked".to_string(),
            guidance: format!(
                "未在系统环境中找到命令 '{command}'。请检查可执行文件名，或请用户在宿主机安装相应依赖。"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn inspect_nonexistent_command_reports_not_installed() {
        let res = inspect_command("nonexistent_binary_xyz_123").await;
        assert!(!res.installed);
        assert_eq!(res.file_type, "none");
        assert_eq!(res.recommended_channel, "blocked");
        assert!(res.resolved_path.is_none());
    }

    #[tokio::test]
    async fn inspect_standard_tool_resolves_and_detects_file_type() {
        let res = inspect_command("git").await;
        if res.installed {
            assert!(res.is_trusted);
            assert_eq!(res.recommended_channel, "sandbox");
            assert!(res.resolved_path.is_some());
            assert!(res.version.is_some());
        }
    }

    #[test]
    fn trusted_office_path_validation_rejects_tmp_and_unknown() {
        assert!(!is_trusted_office_executable_path(Path::new(
            "/tmp/soffice"
        )));
        assert!(!is_trusted_office_executable_path(Path::new(
            "/var/tmp/soffice"
        )));
        assert!(!is_trusted_office_executable_path(Path::new("./soffice")));
        assert!(!is_trusted_office_executable_path(Path::new(
            "/tmp/malicious/libreoffice"
        )));
    }

    #[tokio::test]
    async fn query_version_skips_non_whitelisted_commands() {
        let temp = tempfile::tempdir().unwrap();
        let fake_bin = temp.path().join("unknown_tool");
        fs::write(&fake_bin, b"#!/bin/sh\necho 1.0.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&fake_bin, fs::Permissions::from_mode(0o755));
        }

        // Must be None because "unknown_tool" is not in SAFE_VERSION_PROBE_COMMANDS
        let ver = query_version(&fake_bin, "unknown_tool", false).await;
        assert_eq!(ver, None);
    }
}
