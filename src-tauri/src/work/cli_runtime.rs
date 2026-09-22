//! AgentCabin-owned application-level CLI runtime.
//!
//! Connector CLIs follow the same shape as WorkBuddy: npm's "global" prefix
//! belongs to the application, not to the user's system Node installation.
//! Work Profile trust, auth, and Skill projection remain separate concerns.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::work::paths::WorkPaths;

const MAX_PACKAGE_SPEC_CHARS: usize = 256;

/// Return the managed bin directories in precedence order.
///
/// The first directory is the WorkBuddy-compatible global npm bin. The second
/// is the old local-prefix layout retained only for installations migrated
/// from an earlier AgentCabin release.
pub fn managed_bin_dirs(paths: &WorkPaths) -> Vec<PathBuf> {
    vec![
        paths.cli_connector_bin_dir(),
        paths.cli_connector_packages_dir().join("node_modules/.bin"),
    ]
}

/// Put AgentCabin-managed Connector CLI bins before the host PATH.
pub fn augment_path(paths: &WorkPaths, current: &str) -> String {
    std::env::join_paths(
        managed_bin_dirs(paths)
            .into_iter()
            .chain(std::env::split_paths(current)),
    )
    .map(|value| value.to_string_lossy().into_owned())
    .unwrap_or_else(|_| current.to_string())
}

/// Resolve a managed CLI command without consulting the host PATH.
///
/// A missing command still returns the path where the next managed install is
/// expected to place it. This keeps install/status code deterministic.
pub fn managed_command_path(paths: &WorkPaths, command: &str) -> PathBuf {
    let name = Path::new(command)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(command);
    let candidates = if cfg!(target_os = "windows") && !name.ends_with(".cmd") {
        vec![format!("{name}.cmd"), name.to_string()]
    } else {
        vec![name.to_string()]
    };

    for directory in managed_bin_dirs(paths) {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return path;
            }
        }
    }

    paths.cli_connector_bin_dir().join(&candidates[0])
}

pub fn managed_command_exists(paths: &WorkPaths, command: &str) -> bool {
    managed_command_path(paths, command).is_file()
}

/// Install one npm package into AgentCabin's application-level prefix.
///
/// The package spec is deliberately argv-shaped and validated before npm is
/// started. No shell is used, and npm's cache is kept beside the managed
/// prefix. Callers should invoke this only after the Connector Package has
/// passed the explicit trust boundary.
pub fn install_npm_package(paths: &WorkPaths, package_spec: &str) -> Result<String, String> {
    validate_package_spec(package_spec)?;
    paths.ensure_layout()?;
    let prefix = paths.cli_connector_packages_dir();
    let prefix_arg = prefix.to_string_lossy().to_string();
    let output = Command::new("npm")
        .args([
            "install",
            "--global",
            "--prefix",
            prefix_arg.as_str(),
            "--no-package-lock",
            "--no-audit",
            "--fund=false",
            package_spec,
        ])
        .env("npm_config_cache", paths.cli_connector_cache_dir())
        .output()
        .map_err(|error| format!("启动 AgentCabin 应用级 npm 安装失败: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!(
            "安装 npm 包 '{package_spec}' 到 AgentCabin 应用级运行时失败: {detail}"
        ));
    }
    Ok(if stdout.is_empty() {
        format!("npm 包 '{package_spec}' 已安装到 AgentCabin 应用级运行时")
    } else {
        stdout
    })
}

pub fn validate_package_spec(package_spec: &str) -> Result<(), String> {
    let trimmed = package_spec.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_PACKAGE_SPEC_CHARS
        || trimmed.starts_with('-')
        || trimmed.chars().any(char::is_control)
        || trimmed.chars().any(char::is_whitespace)
        || trimmed
            .chars()
            .any(|character| ";&|<>`$\\'\"".contains(character))
    {
        return Err("应用级 npm 包名无效：只允许包名/版本字符串，不允许 shell 语法或空白".into());
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "@/._+-^~<>=*".contains(character))
    {
        return Err(format!("应用级 npm 包名包含不支持的字符: {trimmed}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn managed_path_precedes_host_path() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let value = augment_path(&paths, "/usr/local/bin:/usr/bin");
        let entries: Vec<_> = std::env::split_paths(&value).collect();
        assert_eq!(entries[0], paths.cli_connector_bin_dir());
        assert_eq!(
            entries[1],
            paths.cli_connector_packages_dir().join("node_modules/.bin")
        );
        assert_eq!(entries[2], PathBuf::from("/usr/local/bin"));
    }

    #[test]
    fn managed_command_uses_global_bin_before_legacy_bin() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let managed = paths.cli_connector_bin_dir().join("demo");
        let legacy = paths
            .cli_connector_packages_dir()
            .join("node_modules/.bin/demo");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"legacy").unwrap();
        assert_eq!(managed_command_path(&paths, "demo"), legacy);
        std::fs::write(&managed, b"managed").unwrap();
        assert_eq!(managed_command_path(&paths, "demo"), managed);
    }

    #[test]
    fn rejects_shell_syntax_in_package_spec() {
        assert!(validate_package_spec("@scope/tool@1.2.3").is_ok());
        assert!(validate_package_spec("tool; touch /tmp/pwned").is_err());
        assert!(validate_package_spec("tool --global").is_err());
    }
}
