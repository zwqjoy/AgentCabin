use super::plugins::validate_plugin_cwd;
use crate::models::{CliCommand, InstalledPlugin, PluginOperationResult, StandaloneSkill};
use crate::process_ext::HideConsole;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::web_server::broadcaster::BroadcastEmitter;

fn pi_settings_path(scope: &str, cwd: Option<&str>) -> Result<PathBuf, String> {
    match scope {
        "user" => crate::storage::home_dir()
            .map(PathBuf::from)
            .map(|home| home.join(".pi").join("agent").join("settings.json"))
            .ok_or_else(|| "Home directory is unavailable".to_string()),
        "project" | "local" => {
            let cwd = validate_plugin_cwd(scope, cwd)?
                .ok_or_else(|| "Project scope requires cwd".to_string())?;
            Ok(Path::new(cwd).join(".pi").join("settings.json"))
        }
        _ => Err(format!("Unsupported Pi extension scope: {}", scope)),
    }
}

fn pi_profile_dir(mode: &str) -> Result<PathBuf, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "code" | "work" => crate::storage::pi_profile_bindings::pi_profile_dir(mode),
        other => Err(format!(
            "Unsupported Pi profile '{}'; expected 'code' or 'work'",
            other
        )),
    }
}

fn pi_code_profile_dir(mode: &str) -> Result<PathBuf, String> {
    if mode.trim().eq_ignore_ascii_case("code") {
        return pi_profile_dir("code");
    }
    Err(
        "Pi Work extensions are managed as AgentCabin resources, not through Pi CLI packages"
            .to_string(),
    )
}

fn pi_profile_settings_path(mode: &str, scope: &str, cwd: Option<&str>) -> Result<PathBuf, String> {
    let profile_dir = pi_code_profile_dir(mode)?;
    match scope {
        "user" => Ok(profile_dir.join("settings.json")),
        "project" | "local" => {
            let cwd = validate_plugin_cwd(scope, cwd)?
                .ok_or_else(|| "Project scope requires cwd".to_string())?;
            Ok(Path::new(cwd).join(".pi").join("settings.json"))
        }
        _ => Err(format!("Unsupported Pi extension scope: {}", scope)),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiProfileInfo {
    pub mode: String,
    pub profile_dir: String,
    pub settings_path: String,
    pub rules_path: String,
    pub extensions_dir: String,
    pub npm_dir: String,
}

#[tauri::command]
pub fn get_pi_profile_info(mode: String) -> Result<PiProfileInfo, String> {
    let mode = mode.trim().to_ascii_lowercase();
    let profile_dir = pi_profile_dir(&mode)?;
    let rules_dir = if mode == "code" {
        crate::storage::pi_profile_bindings::profile_dir("code")?
    } else {
        profile_dir.clone()
    };
    Ok(PiProfileInfo {
        mode,
        settings_path: profile_dir.join("settings.json").display().to_string(),
        rules_path: rules_dir.join("AGENTS.md").display().to_string(),
        extensions_dir: profile_dir.join("extensions").display().to_string(),
        npm_dir: profile_dir.join("npm").display().to_string(),
        profile_dir: profile_dir.display().to_string(),
    })
}

fn read_pi_settings(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(error) => {
            log::warn!(
                "[pi_extensions] Failed to read {}: {}",
                path.display(),
                error
            );
            return Ok(json!({}));
        }
    };
    match serde_json::from_str(&raw) {
        Ok(v) => Ok(v),
        Err(error) => {
            log::warn!(
                "[pi_extensions] Invalid Pi settings {}: {}",
                path.display(),
                error
            );
            Ok(json!({}))
        }
    }
}

fn pi_package_plugin(
    entry: &Value,
    scope: &str,
    project_path: Option<&str>,
    profile: Option<&str>,
) -> Option<InstalledPlugin> {
    let (source, enabled) = match entry {
        Value::String(source) => (source.clone(), true),
        Value::Object(object) => {
            let source = object.get("source")?.as_str()?.to_string();
            let enabled = object
                .get("extensions")
                .and_then(Value::as_array)
                .map(|extensions| !extensions.is_empty())
                .unwrap_or(true);
            (source, enabled)
        }
        _ => return None,
    };
    let mut extra = serde_json::Map::new();
    extra.insert("source".to_string(), json!(source));
    extra.insert("kind".to_string(), json!("package"));
    if let Some(profile) = profile {
        extra.insert("profile".to_string(), json!(profile));
    }
    Some(InstalledPlugin {
        name: source.clone(),
        description: "Pi package".to_string(),
        version: None,
        scope: Some(scope.to_string()),
        enabled: Some(enabled),
        marketplace: None,
        plugin_id: Some(source),
        agent: Some("pi".to_string()),
        project_path: project_path.map(str::to_string),
        extra,
    })
}

fn collect_pi_packages(
    settings_path: &Path,
    scope: &str,
    project_path: Option<&str>,
    profile: Option<&str>,
) -> Result<Vec<InstalledPlugin>, String> {
    let settings = read_pi_settings(settings_path)?;
    Ok(settings
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| pi_package_plugin(entry, scope, project_path, profile))
        .collect())
}

#[tauri::command]
pub fn list_pi_installed_plugins(cwd: Option<String>) -> Result<Vec<InstalledPlugin>, String> {
    let mut plugins = Vec::new();
    if let Ok(path) = pi_settings_path("user", None) {
        if let Ok(user_plugins) = collect_pi_packages(&path, "user", None, None) {
            plugins.extend(user_plugins);
        }
    }
    if let Some(project) = cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
        if let Ok(path) = pi_settings_path("project", Some(project)) {
            if let Ok(proj_plugins) = collect_pi_packages(&path, "project", Some(project), None) {
                plugins.extend(proj_plugins);
            }
        }
    }
    Ok(plugins)
}

#[tauri::command]
pub fn list_pi_profile_extensions(
    mode: String,
    cwd: Option<String>,
) -> Result<Vec<InstalledPlugin>, String> {
    let mode = mode.trim().to_ascii_lowercase();
    if mode == "code" {
        crate::storage::pi_profile_bindings::migrate_legacy_pi_code_profile_if_needed()?;
    }
    let mut plugins = Vec::new();
    let path = pi_profile_settings_path(&mode, "user", None)?;
    if let Ok(user_plugins) = collect_pi_packages(&path, "user", None, Some(&mode)) {
        plugins.extend(user_plugins);
    }
    if let Some(project) = cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
        let path = pi_profile_settings_path(&mode, "project", Some(project))?;
        if let Ok(project_plugins) =
            collect_pi_packages(&path, "project", Some(project), Some(&mode))
        {
            plugins.extend(project_plugins);
        }
    }
    Ok(plugins)
}

fn validate_pi_package_source(source: &str) -> Result<&str, String> {
    let source = source.trim();
    if source.is_empty() || source.starts_with('-') || source.contains('\0') {
        return Err("Invalid Pi package source".to_string());
    }
    Ok(source)
}

async fn run_pi_package_command(
    verb: &str,
    source: &str,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let source = validate_pi_package_source(source)?;
    let effective_cwd = validate_plugin_cwd(scope, cwd)?;
    let binary = crate::agent::claude_stream::resolve_pi_path();
    let mut command = tokio::process::Command::new(binary);
    command.hide_console();
    command.arg(verb);
    if verb == "update" {
        command.arg("--extension");
    }
    command.arg(source);
    if matches!(verb, "install" | "remove" | "uninstall") && matches!(scope, "project" | "local") {
        command.arg("-l");
    }
    if let Some(cwd) = effective_cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .await
        .map_err(|error| format!("Failed to run pi {}: {}", verb, error))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if !stdout.is_empty() { stdout } else { stderr };
    if output.status.success() {
        Ok(PluginOperationResult {
            success: true,
            message,
        })
    } else {
        Err(if message.is_empty() {
            format!("pi {} exited with {}", verb, output.status)
        } else {
            message
        })
    }
}

async fn run_pi_profile_package_command(
    mode: &str,
    verb: &str,
    source: &str,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    if mode.eq_ignore_ascii_case("code") {
        crate::storage::pi_profile_bindings::migrate_legacy_pi_code_profile_if_needed()?;
    }
    let profile_dir = pi_code_profile_dir(mode)?;
    let settings_path = profile_dir.join("settings.json");
    if let (Ok(node), Ok(npm_cli)) = (
        crate::agent::runtime_locator::resolve_node(),
        crate::agent::runtime_locator::resolve_npm_cli(),
    ) {
        let _ = std::fs::create_dir_all(&profile_dir);
        let mut settings: serde_json::Value = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let cache_dir = profile_dir.join("npm-cache");
        let cmd = vec![
            serde_json::Value::String(node),
            serde_json::Value::String(npm_cli),
            serde_json::Value::String("--cache".into()),
            serde_json::Value::String(cache_dir.to_string_lossy().into_owned()),
        ];
        if let Some(obj) = settings.as_object_mut() {
            obj.insert("npmCommand".into(), serde_json::Value::Array(cmd));
            if let Ok(serialized) = serde_json::to_string_pretty(&settings) {
                let _ = std::fs::write(&settings_path, format!("{serialized}\n"));
            }
        }
    }
    let source = validate_pi_package_source(source)?;
    let effective_cwd = validate_plugin_cwd(scope, cwd)?;
    let binary = crate::agent::claude_stream::resolve_pi_path();
    let mut command = tokio::process::Command::new(binary);
    command.hide_console();
    command.env("PATH", crate::agent::claude_stream::augmented_path());
    command.env("PI_CODING_AGENT_DIR", &profile_dir).arg(verb);
    if verb == "update" {
        command.arg("--extension");
    }
    command.arg(source);
    if matches!(verb, "install" | "remove" | "uninstall") && matches!(scope, "project" | "local") {
        command.arg("-l");
    }
    if let Some(cwd) = effective_cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .await
        .map_err(|error| format!("Failed to run Pi {} extension command: {}", mode, error))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if !stdout.is_empty() { stdout } else { stderr };
    if output.status.success() {
        Ok(PluginOperationResult {
            success: true,
            message,
        })
    } else {
        Err(if message.is_empty() {
            format!("pi {} exited with {}", verb, output.status)
        } else {
            message
        })
    }
}

#[tauri::command]
pub async fn install_pi_extension(
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_package_command("install", &source, &scope, cwd.as_deref()).await
}

#[tauri::command]
pub async fn uninstall_pi_extension(
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_package_command("remove", &source, &scope, cwd.as_deref()).await
}

#[tauri::command]
pub async fn update_pi_extension(
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_package_command("update", &source, &scope, cwd.as_deref()).await
}

#[tauri::command]
pub async fn install_pi_profile_extension(
    mode: String,
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_profile_package_command(&mode, "install", &source, &scope, cwd.as_deref()).await
}

#[tauri::command]
pub async fn uninstall_pi_profile_extension(
    mode: String,
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_profile_package_command(&mode, "remove", &source, &scope, cwd.as_deref()).await
}

#[tauri::command]
pub async fn update_pi_profile_extension(
    mode: String,
    source: String,
    scope: String,
    cwd: Option<String>,
) -> Result<PluginOperationResult, String> {
    run_pi_profile_package_command(&mode, "update", &source, &scope, cwd.as_deref()).await
}

fn toggle_pi_extension_at_path(source: &str, path: PathBuf, enabled: bool) -> Result<(), String> {
    let source = validate_pi_package_source(source)?;
    let mut settings = read_pi_settings(&path)?;
    let packages = settings
        .as_object_mut()
        .ok_or_else(|| "Pi settings root must be an object".to_string())?
        .entry("packages")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Pi settings packages must be an array".to_string())?;
    let entry = packages
        .iter_mut()
        .find(|entry| match entry {
            Value::String(value) => value == source,
            Value::Object(object) => object.get("source").and_then(Value::as_str) == Some(source),
            _ => false,
        })
        .ok_or_else(|| format!("Pi package is not configured: {}", source))?;
    if enabled {
        if let Value::Object(object) = entry {
            object.remove("extensions");
        }
    } else {
        let mut object = match entry.take() {
            Value::String(value) => {
                serde_json::Map::from_iter([("source".to_string(), json!(value))])
            }
            Value::Object(object) => object,
            _ => unreachable!(),
        };
        object.insert("extensions".to_string(), json!([]));
        *entry = Value::Object(object);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {}", parent.display(), error))?;
    }
    let serialized = serde_json::to_string_pretty(&settings)
        .map_err(|error| format!("Failed to encode Pi settings: {}", error))?;
    std::fs::write(&path, format!("{}\n", serialized))
        .map_err(|error| format!("Failed to write {}: {}", path.display(), error))
}

#[tauri::command]
pub fn toggle_pi_extension(
    source: String,
    scope: String,
    enabled: bool,
    cwd: Option<String>,
) -> Result<(), String> {
    let path = pi_settings_path(&scope, cwd.as_deref())?;
    toggle_pi_extension_at_path(&source, path, enabled)
}

#[tauri::command]
pub fn toggle_pi_profile_extension(
    mode: String,
    source: String,
    scope: String,
    enabled: bool,
    cwd: Option<String>,
) -> Result<(), String> {
    if mode.eq_ignore_ascii_case("code") {
        crate::storage::pi_profile_bindings::migrate_legacy_pi_code_profile_if_needed()?;
    }
    let path = pi_profile_settings_path(&mode, &scope, cwd.as_deref())?;
    toggle_pi_extension_at_path(&source, path, enabled)
}

// ── Shared Pi Extension Catalog & Profile Bindings ──

fn shared_pi_extension_id(source: &str) -> String {
    format!(
        "pi-{}",
        crate::storage::cli_sessions_common::sha256_short(source.trim())
    )
}

fn npm_package_name(source: &str) -> Option<String> {
    let spec = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    if spec.starts_with('@') {
        let slash = spec.find('/')?;
        let end = spec[slash + 1..]
            .find('@')
            .map(|offset| slash + 1 + offset)
            .unwrap_or(spec.len());
        Some(spec[..end].to_string())
    } else {
        Some(
            spec.find('@')
                .map(|index| spec[..index].to_string())
                .unwrap_or_else(|| spec.to_string()),
        )
    }
}

fn find_package_manifest(root: &Path, source: &str) -> Option<PathBuf> {
    if source.trim().starts_with("npm:") || !source.contains('/') {
        let name = npm_package_name(source)?;
        let path = root
            .join("npm")
            .join("node_modules")
            .join(name)
            .join("package.json");
        if path.is_file() {
            return Some(path);
        }
    }

    fn visit(dir: &Path, source: &str, depth: usize) -> Option<PathBuf> {
        if depth > 8 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("node_modules") {
                continue;
            }
            if path.is_file()
                && path.file_name().and_then(|name| name.to_str()) == Some("package.json")
            {
                let value = serde_json::from_str::<Value>(&fs::read_to_string(&path).ok()?).ok()?;
                let name = value.get("name").and_then(Value::as_str)?;
                let source_name = npm_package_name(source);
                if source_name.as_deref() == Some(name)
                    || source.trim().ends_with(name)
                    || source.trim().trim_end_matches(".git").ends_with(name)
                {
                    return Some(path);
                }
            } else if path.is_dir() {
                if let Some(found) = visit(&path, source, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    visit(&root.join("git"), source, 0)
}

fn shared_pi_package_path(source: &str) -> Result<PathBuf, String> {
    let root = crate::storage::pi_profile_bindings::shared_pi_agent_dir();
    if let Some(path) = find_package_manifest(&root, source) {
        return Ok(path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.clone()));
    }
    if Path::new(source).is_absolute() && Path::new(source).exists() {
        return Ok(PathBuf::from(source));
    }
    Err(format!(
        "Pi shared package was installed but its package path could not be resolved: {}",
        source
    ))
}

fn shared_pi_package_metadata(
    source: &str,
    package_path: &Path,
) -> (String, Option<String>, Option<String>) {
    let manifest = package_path.join("package.json");
    let value = fs::read_to_string(manifest)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok());
    let name = value
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| source.to_string());
    let version = value
        .as_ref()
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let description = value
        .as_ref()
        .and_then(|value| value.get("description"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (name, version, description)
}

fn validate_shared_pi_source(source: &str) -> Result<&str, String> {
    let source = validate_pi_package_source(source)?;
    if source.starts_with('.') || source.starts_with('/') {
        return Err("Shared Pi extension installation accepts npm or git package sources, not project-local paths".into());
    }
    if crate::work::system_packages::is_pi_mcp_adapter_source(source) {
        return Err("pi-mcp-adapter 是 AgentCabin 系统级内置组件，无需手动安装。请在「能力配置」中配置并启用 MCP 服务即可自动注入。".into());
    }
    if crate::work::system_packages::is_pi_web_access_source(source) {
        return Err("pi-web-access 已由 AgentCabin 浏览器与联网系统级接管，无需手动安装。".into());
    }
    if crate::storage::pi_profile_bindings::is_pi_code_native_extension_source(source) {
        return Err("该扩展已作为系统底层 Native Pi 原生能力默认内置加载，请勿重复手动安装，以防引起运行时冲突或重复加载异常。".into());
    }
    Ok(source)
}

async fn run_pi_shared_package_command(
    verb: &str,
    source: &str,
) -> Result<PluginOperationResult, String> {
    let source = validate_shared_pi_source(source)?;
    let agent_dir = crate::storage::pi_profile_bindings::shared_pi_agent_dir();
    fs::create_dir_all(&agent_dir).map_err(|error| {
        format!(
            "Failed to create shared Pi agent directory {}: {}",
            agent_dir.display(),
            error
        )
    })?;
    let binary = crate::agent::claude_stream::resolve_pi_path();
    let mut command = tokio::process::Command::new(binary);
    command
        .hide_console()
        .env("PI_CODING_AGENT_DIR", &agent_dir)
        .current_dir(&agent_dir)
        .arg(verb);
    if verb == "update" {
        command.arg("--extension");
    }
    command.arg(source);
    let output = command
        .output()
        .await
        .map_err(|error| format!("Failed to run shared Pi {}: {}", verb, error))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if !stdout.is_empty() { stdout } else { stderr };
    if output.status.success() {
        Ok(PluginOperationResult {
            success: true,
            message,
        })
    } else {
        Err(if message.is_empty() {
            format!("pi {} exited with {}", verb, output.status)
        } else {
            message
        })
    }
}

fn installed_shared_pi_extension_plugin(
    item: &crate::storage::pi_profile_bindings::PiExtensionCatalogItem,
    mode: &str,
) -> InstalledPlugin {
    let work_native_conflict = mode == "work"
        && crate::storage::pi_profile_bindings::is_pi_code_native_extension_source(&item.source);
    let enabled = crate::storage::pi_profile_bindings::is_pi_extension_enabled_with_root(
        &crate::storage::data_dir(),
        mode,
        &item.id,
    ) && !work_native_conflict;
    let mut extra = serde_json::Map::new();
    extra.insert("source".into(), json!(item.source));
    extra.insert("bindingId".into(), json!(item.id));
    extra.insert("packagePath".into(), json!(item.package_path));
    extra.insert("shared".into(), json!(true));
    extra.insert("profile".into(), json!(mode));
    extra.insert("workCompatible".into(), json!(!work_native_conflict));
    InstalledPlugin {
        name: item.name.clone(),
        description: item.description.clone().unwrap_or_else(|| "Pi 扩展".into()),
        version: item.version.clone(),
        scope: Some("shared".into()),
        enabled: Some(enabled),
        marketplace: None,
        plugin_id: Some(item.id.clone()),
        agent: Some("pi".into()),
        project_path: None,
        extra,
    }
}

#[tauri::command]
pub fn list_pi_shared_extensions(mode: String) -> Result<Vec<InstalledPlugin>, String> {
    let mode = crate::storage::pi_profile_bindings::validate_mode(&mode)?.to_string();
    Ok(
        crate::storage::pi_profile_bindings::read_pi_extension_catalog()
            .iter()
            .map(|item| installed_shared_pi_extension_plugin(item, &mode))
            .collect(),
    )
}

#[tauri::command]
pub async fn install_pi_shared_extension(source: String) -> Result<PluginOperationResult, String> {
    let source = validate_shared_pi_source(&source)?.to_string();
    let result = run_pi_shared_package_command("install", &source).await?;
    let package_path = shared_pi_package_path(&source)?;
    let (name, version, description) = shared_pi_package_metadata(&source, &package_path);
    let item = crate::storage::pi_profile_bindings::PiExtensionCatalogItem {
        id: shared_pi_extension_id(&source),
        source: source.clone(),
        name,
        description,
        version,
        package_path: package_path.to_string_lossy().into_owned(),
    };
    crate::storage::pi_profile_bindings::save_pi_extension_catalog_item(&item)?;
    // Installation is shared. Each runtime profile opts in independently.
    if !crate::storage::pi_profile_bindings::read_pi_extension_bindings("code")
        .iter()
        .any(|binding| binding.extension_id == item.id)
    {
        crate::storage::pi_profile_bindings::set_pi_extension_binding(
            "code",
            &item.id,
            false,
            Some("installed-disabled".into()),
        )?;
    }
    if !crate::storage::pi_profile_bindings::read_pi_extension_bindings("work")
        .iter()
        .any(|binding| binding.extension_id == item.id)
    {
        crate::storage::pi_profile_bindings::set_pi_extension_binding(
            "work",
            &item.id,
            false,
            Some("installed-disabled".into()),
        )?;
    }
    Ok(result)
}

#[tauri::command]
pub async fn uninstall_pi_shared_extension(
    extension_id: String,
) -> Result<PluginOperationResult, String> {
    let item = crate::storage::pi_profile_bindings::read_pi_extension_catalog()
        .into_iter()
        .find(|item| item.id.eq_ignore_ascii_case(&extension_id))
        .ok_or_else(|| format!("Shared Pi extension is not installed: {}", extension_id))?;
    let result = run_pi_shared_package_command("remove", &item.source).await?;
    crate::storage::pi_profile_bindings::delete_pi_extension_catalog_item(&item.id)?;
    Ok(result)
}

#[tauri::command]
pub async fn update_pi_shared_extension(
    extension_id: String,
) -> Result<PluginOperationResult, String> {
    let item = crate::storage::pi_profile_bindings::read_pi_extension_catalog()
        .into_iter()
        .find(|item| item.id.eq_ignore_ascii_case(&extension_id))
        .ok_or_else(|| format!("Shared Pi extension is not installed: {}", extension_id))?;
    let result = run_pi_shared_package_command("update", &item.source).await?;
    let package_path = shared_pi_package_path(&item.source)?;
    let (name, version, description) = shared_pi_package_metadata(&item.source, &package_path);
    crate::storage::pi_profile_bindings::save_pi_extension_catalog_item(
        &crate::storage::pi_profile_bindings::PiExtensionCatalogItem {
            id: item.id,
            source: item.source,
            name,
            description,
            version,
            package_path: package_path.to_string_lossy().into_owned(),
        },
    )?;
    Ok(result)
}

#[tauri::command]
pub fn toggle_pi_shared_extension(
    mode: String,
    extension_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mode = crate::storage::pi_profile_bindings::validate_mode(&mode)?.to_string();
    crate::storage::pi_profile_bindings::set_pi_extension_binding(
        &mode,
        &extension_id,
        enabled,
        (!enabled).then(|| "user".into()),
    )
}

// ── Universal Skills Commands ──

#[tauri::command]
pub fn list_skills(cwd: Option<String>) -> Result<Vec<StandaloneSkill>, String> {
    log::debug!("[capabilities] list_skills: cwd={:?}", cwd);
    Ok(crate::storage::skills::list_skills(cwd.as_deref()))
}

#[tauri::command]
pub fn list_pi_skills(cwd: Option<String>) -> Result<Vec<StandaloneSkill>, String> {
    list_skills(cwd)
}

#[tauri::command]
pub fn list_pi_commands(cwd: Option<String>) -> Result<Vec<CliCommand>, String> {
    log::debug!("[capabilities] list_pi_commands: cwd={:?}", cwd);
    Ok(crate::storage::pi_commands::list_pi_commands(
        cwd.as_deref(),
    ))
}

#[tauri::command]
pub fn create_shared_skill(
    name: String,
    description: String,
    content: String,
    scope: String,
    cwd: Option<String>,
) -> Result<StandaloneSkill, String> {
    log::debug!(
        "[capabilities] create_shared_skill: name={}, scope={}, cwd={:?}",
        name,
        scope,
        cwd
    );
    crate::storage::skills::create_skill(&name, &description, &content, &scope, cwd.as_deref())
}

#[tauri::command]
pub fn create_pi_skill(
    name: String,
    description: String,
    content: String,
    scope: String,
    cwd: Option<String>,
) -> Result<StandaloneSkill, String> {
    create_shared_skill(name, description, content, scope, cwd)
}

#[tauri::command]
pub fn delete_shared_skill(path: String) -> Result<PluginOperationResult, String> {
    log::debug!("[capabilities] delete_shared_skill: path={}", path);
    crate::storage::skills::delete_skill(&path)
}

#[tauri::command]
pub fn delete_pi_skill(path: String) -> Result<PluginOperationResult, String> {
    delete_shared_skill(path)
}

#[tauri::command]
pub fn toggle_skill_binding(
    skill_id: String,
    enabled: bool,
) -> Result<PluginOperationResult, String> {
    log::debug!(
        "[capabilities] toggle_skill_binding: skill_id={}, enabled={}",
        skill_id,
        enabled
    );
    crate::storage::skills::toggle_skill(&skill_id, enabled)
}

#[tauri::command]
pub fn toggle_pi_skill_binding(
    skill_id: String,
    enabled: bool,
) -> Result<PluginOperationResult, String> {
    toggle_skill_binding(skill_id, enabled)
}

#[tauri::command]
pub fn get_skill_bindings() -> Result<Vec<crate::storage::profile_bindings::SkillBinding>, String> {
    log::debug!("[capabilities] get_skill_bindings: global");
    Ok(crate::storage::profile_bindings::read_skill_bindings().bindings)
}

#[tauri::command]
pub fn get_pi_skill_bindings() -> Result<Vec<crate::storage::profile_bindings::SkillBinding>, String>
{
    get_skill_bindings()
}

#[tauri::command]
pub fn get_web_access_binding() -> Result<bool, String> {
    log::debug!("[capabilities] get_web_access_binding: global");
    Ok(crate::storage::pi_profile_bindings::is_web_access_enabled())
}

#[tauri::command]
pub fn set_web_access_binding(enabled: bool) -> Result<(), String> {
    log::debug!("[capabilities] set_web_access_binding: enabled={}", enabled);
    crate::storage::pi_profile_bindings::set_web_access_binding(enabled)
}

#[tauri::command]
pub fn get_browser_config() -> Result<crate::work::models::WorkBrowserSummary, String> {
    crate::work::browser::get_config()
}

#[tauri::command]
pub fn save_browser_config(
    provider: String,
    enabled: bool,
    max_results: Option<u32>,
    api_key: Option<String>,
    endpoint_url: Option<String>,
    allowed_hosts: Option<Vec<String>>,
) -> Result<crate::work::models::WorkBrowserSummary, String> {
    crate::work::browser::save_config(
        &provider,
        enabled,
        max_results,
        api_key.as_deref(),
        endpoint_url.as_deref(),
        allowed_hosts,
    )
}

#[tauri::command]
pub async fn test_browser() -> Result<crate::work::models::WorkBrowserHealth, String> {
    crate::work::browser::test().await
}

#[tauri::command]
pub fn get_browser_use_binding() -> Result<bool, String> {
    log::debug!("[capabilities] get_browser_use_binding: global");
    Ok(crate::storage::pi_profile_bindings::is_browser_use_enabled())
}

#[tauri::command]
pub fn set_browser_use_binding(enabled: bool) -> Result<(), String> {
    log::debug!(
        "[capabilities] set_browser_use_binding: enabled={}",
        enabled
    );
    crate::storage::pi_profile_bindings::set_browser_use_binding(enabled)
}

#[tauri::command]
pub async fn get_desktop_use_status(
) -> Result<crate::work::desktop_operator::DesktopOperatorStatus, String> {
    Ok(crate::work::desktop_operator::desktop_operator_manager()
        .status()
        .await)
}

#[tauri::command]
pub async fn refresh_desktop_use_status(
) -> Result<crate::work::desktop_operator::DesktopOperatorStatus, String> {
    Ok(crate::work::desktop_operator::desktop_operator_manager()
        .refresh_status()
        .await)
}

#[tauri::command]
pub fn get_desktop_use_binding() -> Result<bool, String> {
    Ok(crate::work::desktop_operator::is_requested())
}

#[tauri::command]
pub fn set_desktop_use_binding(enabled: bool) -> Result<(), String> {
    crate::storage::profile_bindings::set_desktop_use_binding(enabled)
}

#[tauri::command]
pub async fn request_desktop_use_permissions(
) -> Result<crate::work::desktop_operator::DesktopOperatorStatus, String> {
    crate::work::desktop_operator::desktop_operator_manager()
        .request_permissions()
        .await
}

#[tauri::command]
pub async fn open_desktop_permission_pane(kind: String) -> Result<(), String> {
    crate::work::desktop_operator::desktop_operator_manager()
        .open_permission_pane(&kind)
        .await
}

pub async fn prepare_browser_runtime_impl(
    emitter: std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>,
) -> Result<crate::work::models::WorkBrowserSummary, String> {
    let paths = crate::work::paths::WorkPaths::app();
    tokio::task::spawn_blocking(move || {
        let reporter = |progress| {
            emitter.emit_realtime("browser-runtime-preparation", &progress, None);
        };
        crate::work::browser_operator::runtime::prepare_with_progress(&paths, reporter)?;
        crate::work::browser::get_config_with_paths(&paths)
    })
    .await
    .map_err(|error| format!("Browser Runtime preparation task failed: {error}"))?
}

#[tauri::command]
pub async fn prepare_browser_runtime(
    emitter: tauri::State<'_, std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>>,
) -> Result<crate::work::models::WorkBrowserSummary, String> {
    prepare_browser_runtime_impl(emitter.inner().clone()).await
}

#[tauri::command]
pub async fn get_browser_session(
    run_id: String,
) -> Result<Option<crate::work::models::BrowserSession>, String> {
    Ok(crate::work::browser_operator::browser_session_manager()
        .get_session(&run_id)
        .await)
}

#[tauri::command]
pub async fn list_browser_sessions() -> Result<Vec<crate::work::models::BrowserSession>, String> {
    Ok(crate::work::browser_operator::browser_session_manager()
        .list_sessions()
        .await)
}

fn resolve_browser_actor_run_id(run_id: &str) -> Result<Option<String>, String> {
    let paths = crate::work::paths::WorkPaths::app();
    paths.ensure_layout()?;
    let task_manager = crate::work::tasks::TaskManager::new(paths);
    if let Some(work_run) = task_manager.find_run_by_id(run_id)? {
        return Ok(work_run.session_id);
    }

    // Code sessions and standalone Work sessions use the persisted RunMeta id
    // directly. A WorkRun id is handled above so we never stop the wrong layer.
    Ok(crate::storage::runs::get_run(run_id).map(|_| run_id.to_string()))
}

#[tauri::command]
pub async fn control_browser_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    run_id: String,
    action: String,
) -> Result<crate::work::models::BrowserSession, String> {
    let mgr = crate::work::browser_operator::browser_session_manager();
    match action.as_str() {
        "pause" => mgr.pause_session(&run_id).await,
        "resume" => mgr.resume_session(&run_id).await,
        "stop" => {
            let session = mgr.stop_session(&run_id).await?;
            if let Some(actor_run_id) = resolve_browser_actor_run_id(&run_id)? {
                crate::commands::session_dispatch::stop_session_impl(
                    emitter.inner(),
                    sessions.inner(),
                    spawn_locks.inner(),
                    actor_run_id,
                )
                .await?;
            }
            let _ = crate::work::browser_operator::browser_operator_manager()
                .close_context(&run_id)
                .await;
            Ok(session)
        }
        "takeover_start" => {
            let session = mgr.start_takeover(&run_id).await?;
            if let Err(error) = crate::work::browser_operator::browser_operator_manager()
                .focus_page(&run_id)
                .await
            {
                let _ = mgr.finish_takeover(&run_id).await;
                return Err(format!("无法打开浏览器接管窗口: {error}"));
            }
            Ok(session)
        }
        "takeover_finish" => mgr.finish_takeover(&run_id).await,
        "close" => {
            let session = mgr.get_session(&run_id).await.unwrap_or_else(|| {
                crate::work::models::BrowserSession {
                    session_id: format!("bsess-{}", run_id),
                    run_id: run_id.clone(),
                    mode: "work".to_string(),
                    surface: "managed".to_string(),
                    status: crate::work::models::BrowserSessionStatus::Closed,
                    current_url: None,
                    page_title: None,
                    current_action: None,
                    last_screenshot: None,
                    traces: Vec::new(),
                    last_error: None,
                    is_taking_over: false,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                }
            });
            crate::browser_runtime::close_browser_session(&run_id).await;
            let _ = crate::work::browser_operator::browser_operator_manager()
                .close_context(&run_id)
                .await;
            Ok(mgr.get_session(&run_id).await.unwrap_or(session))
        }
        other => Err(format!(
            "Unsupported browser session control action: {other}"
        )),
    }
}

#[tauri::command]
pub async fn browser_user_interact(
    run_id: String,
    action: String,
    params: serde_json::Value,
) -> Result<crate::work::models::BrowserSession, String> {
    crate::work::browser_operator::browser_operator_manager()
        .user_interact(&run_id, &action, params)
        .await
}

#[tauri::command]
pub async fn get_browser_traces(
    run_id: String,
) -> Result<Vec<crate::work::models::BrowserTraceEntry>, String> {
    let mgr = crate::work::browser_operator::browser_session_manager();
    Ok(mgr
        .get_session(&run_id)
        .await
        .map(|s| s.traces)
        .unwrap_or_default())
}

#[tauri::command]
pub async fn register_embedded_browser(
    payload: crate::work::browser_operator::EmbeddedRegistration,
) -> Result<(), String> {
    crate::work::browser_operator::embedded_registry().register(payload);
    Ok(())
}

#[tauri::command]
pub async fn unregister_embedded_browser(endpoint: String) -> Result<(), String> {
    crate::work::browser_operator::embedded_registry().unregister(&endpoint);
    Ok(())
}

#[tauri::command]
pub async fn list_embedded_browsers(
) -> Result<Vec<crate::work::browser_operator::EmbeddedRegistration>, String> {
    Ok(crate::work::browser_operator::embedded_registry().list())
}

// ── Global MCP Catalog & Binding Commands ──

#[tauri::command]
pub fn list_mcp_catalog(
) -> Result<Vec<crate::storage::pi_profile_bindings::McpCatalogServer>, String> {
    Ok(crate::storage::pi_profile_bindings::read_mcp_catalog())
}

#[tauri::command]
pub fn save_mcp_catalog_server(
    server: crate::storage::pi_profile_bindings::McpCatalogServer,
) -> Result<(), String> {
    log::debug!("[pi_extensions] save_mcp_catalog_server: id={}", server.id);
    crate::storage::pi_profile_bindings::save_mcp_catalog_server(&server)
}

#[tauri::command]
pub fn delete_mcp_catalog_server(server_id: String) -> Result<(), String> {
    log::debug!(
        "[pi_extensions] delete_mcp_catalog_server: id={}",
        server_id
    );
    crate::storage::pi_profile_bindings::delete_mcp_catalog_server(&server_id)
}

#[tauri::command]
pub fn get_mcp_bindings() -> Result<Vec<crate::storage::pi_profile_bindings::McpBinding>, String> {
    log::debug!("[capabilities] get_mcp_bindings: global");
    Ok(crate::storage::pi_profile_bindings::read_mcp_bindings())
}

#[tauri::command]
pub fn set_mcp_binding(
    server_id: String,
    enabled: bool,
    secret_ref: Option<String>,
    clear_secret_ref: Option<bool>,
) -> Result<(), String> {
    log::debug!(
        "[capabilities] set_mcp_binding: server_id={}, enabled={}",
        server_id,
        enabled
    );
    let secret_override = if clear_secret_ref.unwrap_or(false) {
        Some(None)
    } else {
        secret_ref.map(Some)
    };
    crate::storage::pi_profile_bindings::set_mcp_binding(&server_id, enabled, secret_override)?;
    // One global binding feeds both runtime projections.
    if let Err(error) =
        crate::storage::pi_profile_bindings::sync_pi_code_mcp_from_catalog_and_bindings(None)
    {
        log::warn!(
            "[capabilities] set_mcp_binding: Pi Code MCP sync failed: {}",
            error
        );
    }
    let paths = crate::work::paths::WorkPaths::app();
    if let Err(error) = paths
        .ensure_layout()
        .and_then(|_| crate::work::connectors::ensure_config(&paths))
    {
        log::warn!(
            "[capabilities] set_mcp_binding: Work MCP sync failed: {}",
            error
        );
    }
    Ok(())
}

// ── Global Connector Catalog & Binding Commands ──

#[tauri::command]
pub fn list_connector_catalog(
) -> Result<Vec<crate::storage::pi_profile_bindings::ConnectorCatalogItem>, String> {
    log::debug!("[pi_extensions] list_connector_catalog");
    Ok(crate::storage::pi_profile_bindings::read_connector_catalog())
}

#[tauri::command]
pub fn save_connector_catalog_item(
    item: crate::storage::pi_profile_bindings::ConnectorCatalogItem,
) -> Result<(), String> {
    log::debug!(
        "[pi_extensions] save_connector_catalog_item: id={}",
        item.id
    );
    crate::storage::pi_profile_bindings::save_connector_catalog_item(&item)
}

#[tauri::command]
pub fn delete_connector_catalog_item(connector_id: String) -> Result<(), String> {
    log::debug!(
        "[pi_extensions] delete_connector_catalog_item: id={}",
        connector_id
    );
    crate::storage::pi_profile_bindings::delete_connector_catalog_item(&connector_id)
}

#[tauri::command]
pub fn get_connector_bindings(
) -> Result<Vec<crate::storage::pi_profile_bindings::ConnectorBinding>, String> {
    log::debug!("[capabilities] get_connector_bindings: global");
    Ok(crate::storage::pi_profile_bindings::read_connector_bindings())
}

#[tauri::command]
pub fn set_connector_binding(
    connector_id: String,
    connection_id: Option<String>,
    clear_connection_id: Option<bool>,
    enabled: bool,
    permission_profile: Option<String>,
) -> Result<(), String> {
    log::debug!(
        "[capabilities] set_connector_binding: connector_id={}, enabled={}",
        connector_id,
        enabled
    );
    let connection_override = if clear_connection_id.unwrap_or(false) {
        Some(None)
    } else {
        connection_id.map(Some)
    };
    crate::storage::pi_profile_bindings::set_connector_binding(
        &connector_id,
        connection_override,
        enabled,
        permission_profile,
    )
}

// ── Host Secrets Commands ──

#[tauri::command]
pub fn list_host_secret_refs() -> Result<Vec<String>, String> {
    let secrets = crate::storage::pi_profile_bindings::read_host_secrets();
    let mut refs = secrets.into_keys().collect::<Vec<_>>();
    refs.sort();
    Ok(refs)
}

#[tauri::command]
pub fn set_host_secret(
    secret_ref: String,
    secret_map: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    crate::storage::pi_profile_bindings::set_host_secret(&secret_ref, secret_map)
}

#[tauri::command]
pub fn delete_host_secret(secret_ref: String) -> Result<(), String> {
    crate::storage::pi_profile_bindings::delete_host_secret(&secret_ref)
}

#[tauri::command]
pub async fn fetch_pi_packages(
    query: Option<String>,
    sort: Option<String>,
    package_type: Option<String>,
) -> Result<Vec<crate::models::PiPackageItem>, String> {
    crate::storage::pi_packages::fetch_packages(
        query.as_deref(),
        sort.as_deref(),
        package_type.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn install_work_pi_extension(
    package_name: String,
    display_name: Option<String>,
    description: Option<String>,
) -> Result<crate::work::models::WorkResourceSummary, String> {
    crate::work::resources::install_pi_extension_resource(
        &package_name,
        display_name.as_deref(),
        description.as_deref(),
    )
    .await
}

#[tauri::command]
pub fn uninstall_work_pi_extension(id: String) -> Result<(), String> {
    crate::work::resources::uninstall_pi_extension_resource(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_package_entries_map_enabled_state_and_scope() {
        let enabled = pi_package_plugin(&json!("npm:@acme/pi-tools"), "user", None, None).unwrap();
        assert_eq!(enabled.agent.as_deref(), Some("pi"));
        assert_eq!(enabled.plugin_id.as_deref(), Some("npm:@acme/pi-tools"));
        assert_eq!(enabled.enabled, Some(true));
        assert_eq!(enabled.scope.as_deref(), Some("user"));

        let disabled = pi_package_plugin(
            &json!({"source": "git:github.com/acme/pi-tools", "extensions": []}),
            "project",
            Some("/workspace"),
            Some("code"),
        )
        .unwrap();
        assert_eq!(disabled.enabled, Some(false));
        assert_eq!(disabled.project_path.as_deref(), Some("/workspace"));
        assert_eq!(
            disabled.extra.get("profile").and_then(Value::as_str),
            Some("code")
        );
    }

    #[test]
    fn pi_profile_paths_are_agentcabin_owned() {
        let info = get_pi_profile_info("code".to_string()).unwrap();
        assert!(info.profile_dir.ends_with(".agentcabin/profiles/code/pi"));
        assert!(info
            .settings_path
            .ends_with(".agentcabin/profiles/code/pi/settings.json"));
        assert!(info
            .rules_path
            .ends_with(".agentcabin/profiles/code/AGENTS.md"));
    }

    #[test]
    fn pi_package_source_rejects_option_injection() {
        assert!(validate_pi_package_source("--help").is_err());
        assert!(validate_pi_package_source(" npm:@acme/pi-tools ").is_ok());
    }

    #[test]
    fn validate_shared_pi_source_blocks_system_packages_and_native_features() {
        assert!(validate_shared_pi_source("npm:pi-mcp-adapter").is_err());
        assert!(validate_shared_pi_source("pi-mcp-adapter@2.22.0").is_err());
        assert!(validate_shared_pi_source("npm:@gotgenes/pi-permission-system").is_err());
        assert!(validate_shared_pi_source("npm:@narumitw/pi-plan-mode").is_err());
        assert!(validate_shared_pi_source("npm:pi-mono-multi-edit").is_err());
        assert!(validate_shared_pi_source("npm:@narumitw/pi-lsp").is_err());
        assert!(validate_shared_pi_source("npm:@ff-labs/pi-fff").is_ok());
    }
}
