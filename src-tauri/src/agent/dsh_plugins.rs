//! DeepSeek Harness (DSH) plugin manager and security isolation boundary.
//!
//! Plugin metadata is user-owned state, but it is also input to the DSH patch
//! that starts a provider process. This module therefore validates every
//! package/path reference and keeps Work admission fail-closed. A third-party
//! plugin can be enabled for Code immediately, but it cannot self-declare that
//! it is safe inside Work.

use crate::work::models::AppMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

const BUILTIN_PLUGIN_IDS: &[&str] = &["dsh-skill", "dsh-web", "dsh-mcp-client"];

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DshPluginSecurity {
    /// Explicitly certified by AgentCabin for the Work Bridge boundary.
    SafeInWork,
    /// Allowed only for Code; Work must omit it even when enabled.
    #[default]
    CodeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DshPluginManifest {
    pub id: String,
    /// Display name and legacy package reference.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    /// Legacy compatibility field. New writes also carry `security`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub safe_in_work: bool,
    #[serde(default)]
    pub security: DshPluginSecurity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

impl DshPluginManifest {
    pub fn is_safe_in_work(&self) -> bool {
        self.safe_in_work || self.security == DshPluginSecurity::SafeInWork
    }

    pub fn reference(&self) -> Option<&str> {
        self.path
            .as_deref()
            .or(self.package.as_deref())
            .or_else(|| (!self.name.trim().is_empty()).then_some(self.name.as_str()))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshPluginRegistration {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub security: DshPluginSecurity,
    #[serde(default)]
    pub config: Option<Value>,
}

fn default_enabled() -> bool {
    true
}

pub fn package_name_only(spec: &str) -> &str {
    let trimmed = spec.trim();
    if let Some(rest) = trimmed.strip_prefix('@') {
        if let Some(idx) = rest.find('@') {
            &trimmed[..=idx]
        } else {
            trimmed
        }
    } else if let Some(idx) = trimmed.find('@') {
        &trimmed[..idx]
    } else {
        trimmed
    }
}

fn should_skip_plugin_cli() -> bool {
    if std::env::var("AGENTCABIN_SKIP_PLUGIN_INSTALL").is_ok() {
        return true;
    }
    #[cfg(test)]
    {
        std::env::var("AGENTCABIN_TEST_PHYSICAL_PLUGIN_INSTALL").is_err()
    }
    #[cfg(not(test))]
    {
        false
    }
}

fn ensure_dsh_code_profile_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let dsh_home = crate::storage::profile_bindings::dsh_code_profile_dir_with_root(data_dir);
    fs::create_dir_all(&dsh_home)
        .map_err(|e| format!("Failed to create DSH home {}: {e}", dsh_home.display()))?;

    // Migrate from legacy runtime/dsh/profiles/sdk-minimal if present
    let legacy_profile = data_dir
        .join("runtime")
        .join("dsh")
        .join("profiles")
        .join("sdk-minimal");
    let target_profile = dsh_home.join("profiles").join("sdk-minimal");
    if legacy_profile.exists() && !target_profile.exists() {
        let _ = fs::create_dir_all(dsh_home.join("profiles"));
        let _ = fs::rename(&legacy_profile, &target_profile);
    }
    Ok(dsh_home)
}

fn run_dsh_plugin_cli(data_dir: &Path, action: &str, target: &str) -> Result<String, String> {
    let dsh_home = ensure_dsh_code_profile_dir(data_dir)?;

    let binary = crate::agent::claude_stream::resolve_dsh_path();
    let mut cmd = std::process::Command::new(binary);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.arg("plugin")
        .arg("--profile")
        .arg("sdk-minimal")
        .arg(action)
        .arg(target)
        .env("DSH_HOME", &dsh_home)
        .env("PATH", crate::agent::claude_stream::augmented_path());

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute dsh plugin {action} {target}: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if !stdout.is_empty() { stdout } else { stderr };

    if output.status.success() {
        Ok(message)
    } else {
        Err(if message.is_empty() {
            format!("dsh plugin {} exited with {}", action, output.status)
        } else {
            message
        })
    }
}

pub struct DshPluginManager {
    data_dir: PathBuf,
}

impl DshPluginManager {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn list_plugins(&self) -> Vec<DshPluginManifest> {
        load_plugins(&self.data_dir).unwrap_or_else(|_| default_built_in_plugins())
    }

    pub fn save_plugins(&self, plugins: &[DshPluginManifest]) -> Result<(), String> {
        save_dsh_plugins(&self.data_dir, plugins)
    }

    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> Result<DshPluginManifest, String> {
        toggle_dsh_plugin(&self.data_dir, plugin_id, enabled)
    }

    pub fn enable_plugin(&self, plugin_id: &str) -> Result<DshPluginManifest, String> {
        self.set_enabled(plugin_id, true)
    }

    pub fn disable_plugin(&self, plugin_id: &str) -> Result<DshPluginManifest, String> {
        self.set_enabled(plugin_id, false)
    }

    pub fn register_plugin(
        &self,
        registration: DshPluginRegistration,
    ) -> Result<DshPluginManifest, String> {
        let manifest = manifest_from_registration(registration)?;
        let mut plugins = self.list_plugins();
        if plugins.iter().any(|plugin| plugin.id == manifest.id) {
            return Err(format!(
                "DSH plugin '{}' is already registered",
                manifest.id
            ));
        }
        if !should_skip_plugin_cli() {
            if let Some(package) = manifest.package.as_deref() {
                run_dsh_plugin_cli(&self.data_dir, "add", package)?;
            }
        }
        plugins.push(manifest.clone());
        self.save_plugins(&plugins)?;
        Ok(manifest)
    }

    pub fn unregister_plugin(&self, plugin_id: &str) -> Result<(), String> {
        validate_plugin_id(plugin_id)?;
        if BUILTIN_PLUGIN_IDS.contains(&plugin_id) {
            return Err(format!(
                "Built-in DSH plugin '{plugin_id}' cannot be removed"
            ));
        }
        let mut plugins = self.list_plugins();
        let target = plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .cloned()
            .ok_or_else(|| format!("DSH plugin '{plugin_id}' not found"))?;

        if !should_skip_plugin_cli() {
            if let Some(package) = target.package.as_deref() {
                let pkg_name = package_name_only(package);
                run_dsh_plugin_cli(&self.data_dir, "remove", pkg_name)?;
            }
        }
        plugins.retain(|plugin| plugin.id != plugin_id);
        self.save_plugins(&plugins)
    }

    pub fn update_plugin(&self, plugin_id: &str) -> Result<DshPluginManifest, String> {
        validate_plugin_id(plugin_id)?;
        if BUILTIN_PLUGIN_IDS.contains(&plugin_id) {
            return Err(format!(
                "Built-in DSH plugin '{plugin_id}' cannot be updated"
            ));
        }
        let mut plugins = self.list_plugins();
        let plugin = plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
            .ok_or_else(|| format!("DSH plugin '{plugin_id}' not found"))?;
        if !should_skip_plugin_cli() {
            if let Some(package) = plugin.package.as_deref() {
                let pkg_name = package_name_only(package);
                run_dsh_plugin_cli(&self.data_dir, "update", pkg_name)?;
                let dsh_home = crate::storage::profile_bindings::dsh_code_profile_dir_with_root(
                    &self.data_dir,
                );
                let pkg_json = dsh_home
                    .join("profiles")
                    .join("sdk-minimal")
                    .join("node_modules")
                    .join(pkg_name)
                    .join("package.json");
                if let Ok(content) = fs::read_to_string(&pkg_json) {
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        if let Some(ver) = val.get("version").and_then(|v| v.as_str()) {
                            plugin.version = Some(ver.to_string());
                        }
                    }
                }
            }
        }
        let updated = plugin.clone();
        self.save_plugins(&plugins)?;
        Ok(updated)
    }

    pub fn active_plugins(&self, app_mode: AppMode) -> Vec<DshPluginManifest> {
        active_plugins(&self.data_dir, app_mode)
    }
}

fn plugins_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("runtime").join("dsh").join("plugins.json")
}

pub fn default_built_in_plugins() -> Vec<DshPluginManifest> {
    vec![
        DshPluginManifest {
            id: "dsh-skill".to_string(),
            name: "@deepseek-ai/dsh-skill".to_string(),
            package: Some("@deepseek-ai/dsh-skill".to_string()),
            path: None,
            version: Some("0.1.0".to_string()),
            description: Some("DeepSeek Harness native Skill provider".to_string()),
            enabled: true,
            safe_in_work: true,
            security: DshPluginSecurity::SafeInWork,
            config: None,
        },
        DshPluginManifest {
            id: "dsh-web".to_string(),
            name: "@deepseek-ai/dsh-web".to_string(),
            package: Some("@deepseek-ai/dsh-web".to_string()),
            path: None,
            version: Some("0.1.0".to_string()),
            description: Some("DeepSeek Official Web Search & Fetch capability".to_string()),
            enabled: true,
            safe_in_work: false,
            security: DshPluginSecurity::CodeOnly,
            config: None,
        },
        DshPluginManifest {
            id: "dsh-mcp-client".to_string(),
            name: "@deepseek-ai/dsh-mcp-client".to_string(),
            package: Some("@deepseek-ai/dsh-mcp-client".to_string()),
            path: None,
            version: Some("0.1.0".to_string()),
            description: Some("MCP Client integration for DSH".to_string()),
            enabled: true,
            safe_in_work: false,
            security: DshPluginSecurity::CodeOnly,
            config: None,
        },
    ]
}

fn load_plugins(data_dir: &Path) -> Result<Vec<DshPluginManifest>, String> {
    let path = plugins_file_path(data_dir);
    if !path.exists() {
        return Ok(default_built_in_plugins());
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read DSH plugins config: {error}"))?;
    let mut plugins = serde_json::from_str::<Vec<DshPluginManifest>>(&content)
        .map_err(|error| format!("Failed to parse DSH plugins config: {error}"))?;

    // Keep newly shipped built-ins visible without overwriting user toggles.
    for builtin in default_built_in_plugins() {
        if !plugins.iter().any(|plugin| plugin.id == builtin.id) {
            plugins.push(builtin);
        }
    }

    // A hand-edited or stale entry must never become a DSH patch row. It is
    // omitted from the active catalog and can be re-registered after repair.
    plugins.retain(|plugin| validate_manifest(plugin).is_ok());
    Ok(plugins)
}

pub fn list_dsh_plugins(data_dir: &Path) -> Vec<DshPluginManifest> {
    DshPluginManager::new(data_dir).list_plugins()
}

pub fn save_dsh_plugins(data_dir: &Path, plugins: &[DshPluginManifest]) -> Result<(), String> {
    for plugin in plugins {
        validate_manifest(plugin)?;
    }
    let path = plugins_file_path(data_dir);
    let parent = path
        .parent()
        .ok_or_else(|| "DSH plugin config has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create DSH plugin config directory: {error}"))?;
    let json = serde_json::to_string_pretty(plugins)
        .map_err(|error| format!("Failed to serialize DSH plugins: {error}"))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, json)
        .map_err(|error| format!("Failed to write DSH plugin temp file: {error}"))?;
    fs::rename(&temp, &path)
        .map_err(|error| format!("Failed to atomically replace DSH plugins config: {error}"))
}

pub fn toggle_dsh_plugin(
    data_dir: &Path,
    plugin_id: &str,
    enabled: bool,
) -> Result<DshPluginManifest, String> {
    validate_plugin_id(plugin_id)?;
    let manager = DshPluginManager::new(data_dir);
    let mut plugins = manager.list_plugins();
    let plugin = plugins
        .iter_mut()
        .find(|plugin| plugin.id == plugin_id)
        .ok_or_else(|| format!("DSH plugin '{plugin_id}' not found"))?;
    plugin.enabled = enabled;
    let updated = plugin.clone();
    manager.save_plugins(&plugins)?;
    Ok(updated)
}

pub fn enable_dsh_plugin(data_dir: &Path, plugin_id: &str) -> Result<DshPluginManifest, String> {
    DshPluginManager::new(data_dir).enable_plugin(plugin_id)
}

pub fn disable_dsh_plugin(data_dir: &Path, plugin_id: &str) -> Result<DshPluginManifest, String> {
    DshPluginManager::new(data_dir).disable_plugin(plugin_id)
}

pub fn update_dsh_plugin(data_dir: &Path, plugin_id: &str) -> Result<DshPluginManifest, String> {
    DshPluginManager::new(data_dir).update_plugin(plugin_id)
}

/// Retrieve active plugins filtered for the target AppMode.
/// Work admits only explicitly certified built-ins; Code admits all enabled
/// and valid entries.
pub fn active_plugins(data_dir: &Path, app_mode: AppMode) -> Vec<DshPluginManifest> {
    let manager = DshPluginManager::new(data_dir);
    manager
        .list_plugins()
        .into_iter()
        .filter(|plugin| {
            plugin.enabled
                && match app_mode {
                    AppMode::Work => plugin.is_safe_in_work(),
                    AppMode::Code => true,
                }
        })
        .collect()
}

pub fn get_active_dsh_plugins_for_mode(
    data_dir: &Path,
    app_mode: AppMode,
) -> Vec<DshPluginManifest> {
    active_plugins(data_dir, app_mode)
}

fn manifest_from_registration(
    registration: DshPluginRegistration,
) -> Result<DshPluginManifest, String> {
    let security = registration.security;
    let safe_in_work = security == DshPluginSecurity::SafeInWork;
    let manifest = DshPluginManifest {
        id: registration.id,
        name: registration.name,
        package: registration.package,
        path: registration.path,
        version: registration.version,
        description: registration.description,
        enabled: registration.enabled,
        safe_in_work,
        security,
        config: registration.config,
    };
    validate_manifest(&manifest)?;
    if safe_in_work && !is_certified_work_plugin(&manifest) {
        return Err(
            "Third-party DSH plugins cannot self-declare SafeInWork; register them as CodeOnly and use the Work Bridge instead."
                .to_string(),
        );
    }
    Ok(manifest)
}

fn validate_manifest(plugin: &DshPluginManifest) -> Result<(), String> {
    validate_plugin_id(&plugin.id)?;
    if plugin.name.trim().is_empty() {
        return Err("DSH plugin name cannot be empty".to_string());
    }
    if plugin.package.is_some() && plugin.path.is_some() {
        return Err(format!(
            "DSH plugin '{}' must specify package or path, not both",
            plugin.id
        ));
    }
    if let Some(package) = plugin.package.as_deref() {
        validate_package(package)?;
    }
    if let Some(path) = plugin.path.as_deref() {
        validate_plugin_path(path)?;
    }
    if plugin.package.is_none() && plugin.path.is_none() {
        validate_package(&plugin.name)?;
    }
    if plugin.is_safe_in_work() && !is_certified_work_plugin(plugin) {
        return Err(format!(
            "DSH plugin '{}' cannot self-declare SafeInWork; only AgentCabin-certified plugins may run in Work mode",
            plugin.id
        ));
    }
    Ok(())
}

fn validate_plugin_id(id: &str) -> Result<(), String> {
    let value = id.trim();
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(
            "DSH plugin id must be 1-80 ASCII letters, numbers, '.', '_' or '-'".to_string(),
        );
    }
    Ok(())
}

fn validate_package(package: &str) -> Result<(), String> {
    let value = package.trim();
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(char::is_whitespace)
        || value.contains("..")
        || value.starts_with('-')
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'/' | b'_' | b'-' | b'.')
        })
    {
        return Err(format!("Invalid DSH plugin package reference '{package}'"));
    }
    Ok(())
}

fn validate_plugin_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path.trim());
    if !candidate.is_absolute()
        || candidate.as_os_str().is_empty()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("DSH plugin path must be an absolute path without '..'".to_string());
    }
    if !candidate.is_file() {
        return Err(format!("DSH plugin path does not exist: {path}"));
    }
    Ok(())
}

fn is_certified_work_plugin(plugin: &DshPluginManifest) -> bool {
    if plugin.path.is_some() {
        return false;
    }
    matches!(
        plugin.reference(),
        Some("@deepseek-ai/dsh-skill")
            | Some("@deepseek-ai/dsh-skill-filesystem")
            | Some("@deepseek-ai/dsh-tool-skill")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_and_toggle_are_persisted_atomically() {
        let temp = tempdir().unwrap();
        let list = list_dsh_plugins(temp.path());
        assert_eq!(list.len(), 3);
        assert!(list
            .iter()
            .any(|plugin| plugin.id == "dsh-skill" && plugin.is_safe_in_work()));

        let toggled = toggle_dsh_plugin(temp.path(), "dsh-web", false).unwrap();
        assert!(!toggled.enabled);
        assert!(
            !list_dsh_plugins(temp.path())
                .iter()
                .find(|plugin| plugin.id == "dsh-web")
                .unwrap()
                .enabled
        );
        assert!(!plugins_file_path(temp.path())
            .with_extension("json.tmp")
            .exists());
    }

    #[test]
    fn work_mode_is_fail_closed_for_third_party_plugins() {
        let temp = tempdir().unwrap();
        let manager = DshPluginManager::new(temp.path());
        let registered = manager
            .register_plugin(DshPluginRegistration {
                id: "community-tool".into(),
                name: "@example/community-tool".into(),
                package: Some("@example/community-tool".into()),
                path: None,
                version: Some("1.0.0".into()),
                description: None,
                enabled: true,
                security: DshPluginSecurity::CodeOnly,
                config: None,
            })
            .unwrap();
        assert_eq!(registered.security, DshPluginSecurity::CodeOnly);
        assert!(manager
            .active_plugins(AppMode::Code)
            .iter()
            .any(|plugin| plugin.id == "community-tool"));
        assert!(!manager
            .active_plugins(AppMode::Work)
            .iter()
            .any(|plugin| plugin.id == "community-tool"));
    }

    #[test]
    fn third_party_plugin_cannot_claim_work_safety() {
        let temp = tempdir().unwrap();
        let manager = DshPluginManager::new(temp.path());
        let error = manager
            .register_plugin(DshPluginRegistration {
                id: "unsafe-tool".into(),
                name: "@example/unsafe-tool".into(),
                package: Some("@example/unsafe-tool".into()),
                path: None,
                version: None,
                description: None,
                enabled: true,
                security: DshPluginSecurity::SafeInWork,
                config: None,
            })
            .unwrap_err();
        assert!(error.contains("cannot self-declare SafeInWork"));
    }

    #[test]
    fn builtin_plugins_cannot_be_unregistered_or_updated() {
        let temp = tempdir().unwrap();
        let manager = DshPluginManager::new(temp.path());
        assert!(manager
            .unregister_plugin("dsh-skill")
            .unwrap_err()
            .contains("cannot be removed"));
        assert!(manager
            .update_plugin("dsh-skill")
            .unwrap_err()
            .contains("cannot be updated"));
    }

    #[test]
    fn unregister_and_update_fail_on_missing_plugin() {
        let temp = tempdir().unwrap();
        let manager = DshPluginManager::new(temp.path());
        assert!(manager
            .unregister_plugin("non-existent")
            .unwrap_err()
            .contains("not found"));
        assert!(manager
            .update_plugin("non-existent")
            .unwrap_err()
            .contains("not found"));
    }
}
