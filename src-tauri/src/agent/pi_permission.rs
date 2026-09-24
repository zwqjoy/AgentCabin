//! AgentCabin integration for `@gotgenes/pi-permission-system`.
//!
//! Pi does not implement Claude Code's `/auto` and `/bypass` prompt commands.
//! Its permission extension reads the mode from its unified `config.json`, so
//! keep that configuration in sync before process start and live mode changes.

use crate::agent::adapter::AdapterSettings;
use crate::agent::claude_stream::resolve_pi_path;
use crate::storage;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const EXTENSION_ID: &str = "pi-permission-system";
const EXTENSION_PACKAGE: &str = "@gotgenes/pi-permission-system";

/// Normalize AgentCabin/Claude-style names to the three Pi modes exposed by
/// the UI. The extension itself continues to evaluate allow/ask/deny rules.
pub(crate) fn normalize_permission_mode(mode: &str) -> &'static str {
    match mode.trim() {
        "accept_edits" | "acceptEdits" | "auto_read" => "accept_edits",
        "auto_approve" | "auto" | "auto_all" | "bypass" | "bypassPermissions" => "auto_approve",
        _ => "guarded",
    }
}

fn agent_dir_from_env(env: &HashMap<String, String>) -> Option<PathBuf> {
    env.get("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| storage::home_dir().map(|home| PathBuf::from(home).join(".pi/agent")))
}

fn pi_package_root() -> Option<PathBuf> {
    let binary = resolve_pi_path();
    let canonical = std::fs::canonicalize(binary).ok()?;
    let mut current = canonical.parent()?;
    for _ in 0..6 {
        if current.join("package.json").is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

fn permission_config_path(agent_dir: &Path) -> PathBuf {
    agent_dir
        .join("extensions")
        .join(EXTENSION_ID)
        .join("config.json")
}

fn extension_entry_from_package(package_dir: &Path) -> Option<PathBuf> {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(package_dir.join("package.json")).ok()?).ok()?;
    let entry = manifest
        .get("pi")?
        .get("extensions")?
        .as_array()?
        .first()?
        .as_str()?
        .trim();
    if entry.is_empty() {
        return None;
    }
    let entry = Path::new(entry);
    let resolved = if entry.is_absolute() {
        entry.to_path_buf()
    } else {
        package_dir.join(entry)
    };
    resolved.is_file().then_some(resolved)
}

fn permission_extension_entry(agent_dir: &Path) -> Option<PathBuf> {
    let installed_package = agent_dir
        .join("npm")
        .join("node_modules")
        .join(EXTENSION_PACKAGE);
    extension_entry_from_package(&installed_package).or_else(|| {
        crate::agent::claude_stream::bundled_pi_package_path(EXTENSION_PACKAGE)
            .and_then(|path| extension_entry_from_package(Path::new(&path)))
    })
}

/// The config directory is also inspected by `pi-subagents` when it decides
/// whether to carry this permission extension into child Pi sessions. Give its
/// package manifest the real bundled entrypoint instead of a config-only marker.
fn sync_extension_package_manifest(config_path: &Path) {
    let Some(extension_dir) = config_path.parent() else {
        return;
    };
    let Some(agent_dir) = extension_dir.parent().and_then(Path::parent) else {
        return;
    };
    let manifest_path = extension_dir.join("package.json");
    if manifest_path.exists() {
        let is_config_marker = std::fs::read(&manifest_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .as_ref()
            .and_then(|manifest| manifest.get("name"))
            .and_then(Value::as_str)
            == Some("pi-permission-system-config");
        if !is_config_marker {
            return;
        }
    }
    let Some(entry) = permission_extension_entry(agent_dir) else {
        return;
    };
    let manifest = serde_json::json!({
        "name": "pi-permission-system-config",
        "private": true,
        "pi": { "extensions": [entry.to_string_lossy().into_owned()] }
    });
    if let Ok(contents) = serde_json::to_vec_pretty(&manifest) {
        let _ = std::fs::write(manifest_path, contents);
    }
}

fn object_or_insert<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("object inserted above")
}

/// Set a mode-owned surface while preserving explicit deny rules from the
/// user's policy. For pattern maps, the mode catch-all is placed first so the
/// extension's documented "last matching rule wins" behavior keeps specific
/// exceptions intact.
fn is_explicit_deny(value: &Value) -> bool {
    value.as_str() == Some("deny")
        || value
            .get("action")
            .and_then(Value::as_str)
            .is_some_and(|action| action == "deny")
}

fn set_mode_surface(permission: &mut Map<String, Value>, surface: &str, action: &str) {
    let Some(value) = permission.get_mut(surface) else {
        permission.insert(surface.to_string(), Value::String(action.to_string()));
        return;
    };

    match value {
        value if is_explicit_deny(value) => {}
        Value::String(current) => *current = action.to_string(),
        Value::Object(patterns) => {
            if patterns.get("*").is_some_and(is_explicit_deny) {
                return;
            }

            let existing = std::mem::take(patterns);
            let mut updated = Map::new();
            updated.insert("*".to_string(), Value::String(action.to_string()));
            for (pattern, rule) in existing {
                if pattern != "*" {
                    updated.insert(pattern, rule);
                }
            }
            *patterns = updated;
        }
        _ => *value = Value::String(action.to_string()),
    }
}

/// Seed only the missing parts of the Claude-like baseline. The extension's
/// own resolver still owns rule matching and precedence; this just makes
/// ordinary in-project reads quiet while keeping writes, commands, external
/// directories, and sensitive paths behind the extension's policy gates.
fn ensure_claude_baseline(permission: &mut Map<String, Value>) {
    for surface in ["read", "grep", "find", "ls"] {
        permission
            .entry(surface.to_string())
            .or_insert_with(|| Value::String("allow".to_string()));
    }

    permission.entry("path".to_string()).or_insert_with(|| {
        let mut paths = Map::new();
        paths.insert("*".to_string(), Value::String("allow".to_string()));
        paths.insert("*.env".to_string(), Value::String("deny".to_string()));
        paths.insert("*.env.*".to_string(), Value::String("deny".to_string()));
        paths.insert(
            "*.env.example".to_string(),
            Value::String("allow".to_string()),
        );
        Value::Object(paths)
    });
}

fn sync_mode_policy(root: &mut Map<String, Value>, mode: &str) {
    let Some(permission) = (match root.get_mut("permission") {
        Some(Value::Object(permission)) => Some(permission),
        Some(_) => None,
        None => Some(object_or_insert(root, "permission")),
    }) else {
        return;
    };

    // Claude's acceptEdits equivalent: write/edit are silent, while bash,
    // MCP, external directories, and explicit path denies remain governed by
    // the extension's own policy layers.
    ensure_claude_baseline(permission);
    let edit_action = if mode == "accept_edits" {
        "allow"
    } else {
        "ask"
    };
    set_mode_surface(permission, "write", edit_action);
    set_mode_surface(permission, "edit", edit_action);
}

fn set_boundary_deny(permission: &mut Map<String, Value>, surface: &str, pattern: &str) {
    let value = permission
        .remove(surface)
        .unwrap_or_else(|| Value::String("ask".to_string()));
    let mut patterns = match value {
        Value::Object(patterns) => patterns,
        Value::String(action) => {
            let mut patterns = Map::new();
            patterns.insert("*".to_string(), Value::String(action));
            patterns
        }
        _ => {
            let mut patterns = Map::new();
            patterns.insert("*".to_string(), Value::String("ask".to_string()));
            patterns
        }
    };
    patterns.insert(pattern.to_string(), Value::String("deny".to_string()));
    permission.insert(surface.to_string(), Value::Object(patterns));
}

fn sync_workspace_boundary(root: &mut Map<String, Value>, workspace_root: &Path) {
    let Some(permission) = (match root.get_mut("permission") {
        Some(Value::Object(permission)) => Some(permission),
        Some(_) => None,
        None => Some(object_or_insert(root, "permission")),
    }) else {
        return;
    };

    let workspace = workspace_root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    for area in ["input", "context"] {
        for suffix in ["/*", "/**"] {
            set_boundary_deny(permission, "write", &format!("{area}{suffix}"));
            set_boundary_deny(permission, "write", &format!("{workspace}/{area}{suffix}"));
            set_boundary_deny(permission, "edit", &format!("{area}{suffix}"));
            set_boundary_deny(permission, "edit", &format!("{workspace}/{area}{suffix}"));
        }
    }

    let external = match permission.get_mut("external_directory") {
        None => Some(object_or_insert(permission, "external_directory")),
        Some(Value::Object(external)) => Some(external),
        Some(_) => None,
    };
    if let Some(external) = external {
        if !external.get("*").is_some_and(is_explicit_deny) {
            // Keep the broad fallback before any explicit access roots. This
            // matters when serde_json is configured with insertion-order maps;
            // the permission extension resolves the last matching rule.
            let existing = std::mem::take(external);
            let mut updated = Map::new();
            updated.insert("*".to_string(), Value::String("ask".to_string()));
            for (pattern, rule) in existing {
                if pattern != "*" {
                    updated.insert(pattern, rule);
                }
            }
            *external = updated;
        }
    }
}

fn set_access_pattern(
    permission: &mut Map<String, Value>,
    surface: &str,
    pattern: &str,
    action: &str,
) {
    let value = permission
        .remove(surface)
        .unwrap_or_else(|| Value::String("ask".to_string()));
    let mut patterns = match value {
        Value::Object(patterns) => patterns,
        Value::String(action) => {
            let mut patterns = Map::new();
            patterns.insert("*".to_string(), Value::String(action));
            patterns
        }
        _ => {
            let mut patterns = Map::new();
            patterns.insert("*".to_string(), Value::String("ask".to_string()));
            patterns
        }
    };
    patterns.insert(pattern.to_string(), Value::String(action.to_string()));
    permission.insert(surface.to_string(), Value::Object(patterns));
}

fn add_external_directory_rule(external: &mut Map<String, Value>, root: &str) {
    let root = root.replace('\\', "/").trim_end_matches('/').to_string();
    for pattern in [root.clone(), format!("{root}/*"), format!("{root}/**")] {
        external.insert(pattern, Value::String("allow".to_string()));
    }
}

fn sync_work_access_roots(
    root: &mut Map<String, Value>,
    access_roots: &[(String, bool)],
    attachment_root: Option<&Path>,
) {
    let Some(permission) = (match root.get_mut("permission") {
        Some(Value::Object(permission)) => Some(permission),
        Some(_) => None,
        None => Some(object_or_insert(root, "permission")),
    }) else {
        return;
    };

    let external_paths: Vec<String> = access_roots
        .iter()
        .map(|(path, _)| path.clone())
        .chain(attachment_root.map(|path| path.to_string_lossy().into_owned()))
        .collect();
    if let Some(Value::Object(external)) = permission.get_mut("external_directory") {
        for path in &external_paths {
            add_external_directory_rule(external, path);
        }
    } else if !external_paths.is_empty() {
        let external = object_or_insert(permission, "external_directory");
        for path in &external_paths {
            add_external_directory_rule(external, path);
        }
    }

    for (path, writable) in access_roots {
        let action = if *writable { "allow" } else { "deny" };
        for suffix in ["/*", "/**"] {
            let pattern = format!("{}{}", path.trim_end_matches('/'), suffix);
            set_access_pattern(permission, "write", &pattern, action);
            set_access_pattern(permission, "edit", &pattern, action);
        }
    }
    if let Some(attachment_root) = attachment_root {
        let path = attachment_root.to_string_lossy();
        for suffix in ["/*", "/**"] {
            let pattern = format!("{}{}", path.trim_end_matches('/'), suffix);
            set_access_pattern(permission, "write", &pattern, "deny");
            set_access_pattern(permission, "edit", &pattern, "deny");
        }
    }
}

fn sync_config_file(path: &Path, mode: Option<&str>) -> Result<(), String> {
    sync_config_file_with_workspace(path, mode, None, &[], None)
}

fn sync_config_file_with_workspace(
    path: &Path,
    mode: Option<&str>,
    workspace_root: Option<&Path>,
    access_roots: &[(String, bool)],
    attachment_root: Option<&Path>,
) -> Result<(), String> {
    let mut root = if path.is_file() {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("Failed to read Pi permission config: {}", error))?;
        match serde_json::from_str::<Value>(&content) {
            Ok(Value::Object(object)) => object,
            Ok(_) => Map::new(),
            Err(error) => {
                return Err(format!(
                    "Invalid Pi permission config {}: {}",
                    path.display(),
                    error
                ));
            }
        }
    } else {
        Map::new()
    };

    let mode = normalize_permission_mode(mode.unwrap_or("accept_edits"));

    // The extension's yolo mode is the official equivalent of AgentCabin's
    // bypass/免审 mode. It auto-approves ask-state checks but still respects
    // explicit deny rules.
    root.insert("yoloMode".to_string(), Value::Bool(mode == "auto_approve"));
    sync_mode_policy(&mut root, mode);
    if workspace_root.is_some() {
        // Apply explicit access first; the Workspace input/context deny rules
        // below must win when a user grants a parent directory.
        sync_work_access_roots(&mut root, access_roots, attachment_root);
    }
    if let Some(workspace_root) = workspace_root {
        sync_workspace_boundary(&mut root, workspace_root);
    }

    // Pi's own package documentation is infrastructure, not project data.
    // The extension auto-allows it for read tools, but bash commands such as
    // `grep` still pass through the external-directory gate. Add only the
    // detected Pi package root and preserve user rules.
    if let Some(package_root) = pi_package_root() {
        let permission = match root.get_mut("permission") {
            None => Some(object_or_insert(&mut root, "permission")),
            Some(Value::Object(permission)) => Some(permission),
            Some(_) => None,
        };
        if let Some(permission) = permission {
            let external = match permission.get_mut("external_directory") {
                None => Some(object_or_insert(permission, "external_directory")),
                Some(Value::Object(external)) => Some(external),
                Some(_) => None,
            };
            if let Some(external) = external {
                let pattern = format!("{}/*", package_root.to_string_lossy());
                external
                    .entry(pattern)
                    .or_insert_with(|| Value::String("allow".to_string()));
            }
        }
    }

    let serialized = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|error| format!("Failed to serialize Pi permission config: {}", error))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!("Failed to create Pi permission config directory: {}", error)
        })?;
        sync_extension_package_manifest(path);
    }
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, format!("{}\n", serialized))
        .map_err(|error| format!("Failed to write Pi permission config: {}", error))?;
    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Failed to install Pi permission config: {}", error));
    }
    Ok(())
}

/// Synchronize the Pi permission extension before a process is spawned.
///
/// This is intentionally best-effort for launch callers: a malformed user
/// config should be reported in logs, but should not prevent Pi from starting.
pub(crate) fn sync_for_spawn(settings: &AdapterSettings, env: &HashMap<String, String>) {
    if !settings.pi_permission_system_enabled {
        return;
    }
    let Some(agent_dir) = agent_dir_from_env(env) else {
        log::warn!("[pi_permission] cannot resolve Pi agent directory");
        return;
    };
    if agent_dir == crate::work::paths::WorkPaths::app().work_profile_dir() {
        return;
    }
    let path = permission_config_path(&agent_dir);
    let workspace_root = settings.pi_workspace_root.as_deref().map(Path::new);
    if let Err(error) = sync_config_file_with_workspace(
        &path,
        settings.permission_mode.as_deref(),
        workspace_root,
        &settings.pi_work_access_roots,
        settings.pi_work_attachment_root.as_deref().map(Path::new),
    ) {
        log::warn!(
            "[pi_permission] failed to sync {}: {}",
            path.display(),
            error
        );
    }
}

/// Synchronize a mode change for the already-running Pi process.
///
/// The permission extension refreshes this file in `before_agent_start`, so an
/// active Pi process picks up the change on its next agent turn without a
/// process restart or a synthetic slash command. The directory must be passed
/// by the actor because Code sessions use an isolated managed Pi profile rather
/// than the host's `~/.pi/agent` directory.
pub(crate) fn sync_for_control(agent_dir: &Path, mode: &str) -> Result<(), String> {
    sync_config_file(&permission_config_path(agent_dir), Some(mode.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn syncs_bypass_without_dropping_existing_config() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("extensions/pi-permission-system/config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"permission":{"bash":{"git status":"allow"}},"debugLog":true}"#,
        )
        .unwrap();

        sync_config_file(&path, Some("bypassPermissions")).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["yoloMode"], Value::Bool(true));
        assert_eq!(value["debugLog"], Value::Bool(true));
        assert_eq!(value["permission"]["bash"]["git status"], "allow");
    }

    #[test]
    fn generated_permission_package_manifest_declares_its_extension_entry() {
        let dir = tempdir().unwrap();
        let agent_dir = dir.path();
        let package_dir = agent_dir.join("npm/node_modules").join(EXTENSION_PACKAGE);
        std::fs::create_dir_all(package_dir.join("dist")).unwrap();
        std::fs::write(package_dir.join("dist/index.js"), "export {};\n").unwrap();
        std::fs::write(
            package_dir.join("package.json"),
            r#"{"pi":{"extensions":["dist/index.js"]}}"#,
        )
        .unwrap();

        let config_path = permission_config_path(agent_dir);
        sync_config_file(&config_path, Some("bypassPermissions")).unwrap();

        let manifest: Value = serde_json::from_slice(
            &std::fs::read(config_path.parent().unwrap().join("package.json")).unwrap(),
        )
        .unwrap();
        let entry = manifest["pi"]["extensions"][0].as_str().unwrap();
        assert!(!entry.trim().is_empty());
        assert_eq!(Path::new(entry), package_dir.join("dist/index.js"));
        assert!(Path::new(entry).is_file());
    }

    #[test]
    fn disables_yolo_for_safe_modes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        sync_config_file(&path, Some("acceptEdits")).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["yoloMode"], Value::Bool(false));
    }

    #[test]
    fn syncs_auto_approve_as_yolo_mode() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        sync_config_file(&path, Some("auto_approve")).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["yoloMode"], Value::Bool(true));
    }

    #[test]
    fn syncs_control_in_the_selected_pi_profile() {
        let dir = tempdir().unwrap();
        sync_for_control(dir.path(), "auto_approve").unwrap();
        let path = dir
            .path()
            .join("extensions/pi-permission-system/config.json");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["yoloMode"], Value::Bool(true));
    }

    #[test]
    fn seeds_claude_like_safe_read_and_sensitive_path_baseline() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        sync_config_file(&path, None).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

        for surface in ["read", "grep", "find", "ls"] {
            assert_eq!(value["permission"][surface], "allow");
        }
        assert_eq!(value["permission"]["path"]["*"], "allow");
        assert_eq!(value["permission"]["path"]["*.env"], "deny");
        assert_eq!(value["permission"]["path"]["*.env.*"], "deny");
        assert_eq!(value["permission"]["path"]["*.env.example"], "allow");
        assert_eq!(value["permission"]["write"], "allow");
        assert_eq!(value["permission"]["edit"], "allow");
    }

    #[test]
    fn edit_mode_allows_write_and_edit_but_keeps_other_surfaces_guarded() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{
                "permission": {
                    "write": "ask",
                    "edit": "ask",
                    "bash": "ask",
                    "path": {"*.env": "deny"}
                }
            }"#,
        )
        .unwrap();

        sync_config_file(&path, Some("accept_edits")).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["yoloMode"], Value::Bool(false));
        assert_eq!(value["permission"]["write"], "allow");
        assert_eq!(value["permission"]["edit"], "allow");
        assert_eq!(value["permission"]["bash"], "ask");
        assert_eq!(value["permission"]["path"]["*.env"], "deny");
    }

    #[test]
    fn guarded_mode_restores_write_and_edit_prompts_without_overriding_deny() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{
                "permission": {
                    "write": "allow",
                    "edit": {"*": "allow", "*.lock": "deny"}
                }
            }"#,
        )
        .unwrap();

        sync_config_file(&path, Some("guarded")).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["permission"]["write"], "ask");
        assert_eq!(value["permission"]["edit"]["*"], "ask");
        assert_eq!(value["permission"]["edit"]["*.lock"], "deny");
    }

    #[test]
    fn work_boundary_denies_input_and_context_writes_and_asks_for_external_paths() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        sync_config_file_with_workspace(&path, Some("accept_edits"), Some(dir.path()), &[], None)
            .unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["permission"]["write"]["input/*"], "deny");
        assert_eq!(value["permission"]["edit"]["context/**"], "deny");
        assert_eq!(value["permission"]["external_directory"]["*"], "ask");
    }

    #[test]
    fn grants_external_access_with_read_only_or_writable_rules() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let external = dir.path().join("external");
        sync_config_file_with_workspace(
            &path,
            Some("accept_edits"),
            Some(dir.path()),
            &[(external.to_string_lossy().into_owned(), false)],
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let external_path = external.to_string_lossy();
        assert_eq!(
            value["permission"]["external_directory"][external_path.as_ref()],
            "allow"
        );
        assert_eq!(
            value["permission"]["write"][format!("{external_path}/*")],
            "deny"
        );

        let path = dir.path().join("writable.json");
        sync_config_file_with_workspace(
            &path,
            Some("guarded"),
            Some(dir.path()),
            &[(external.to_string_lossy().into_owned(), true)],
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            value["permission"]["write"][format!("{external_path}/*")],
            "allow"
        );
    }
}
