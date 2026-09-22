use crate::storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ══════════════════════════════════════════════════════════════════════════════
// Validation Helpers
// ══════════════════════════════════════════════════════════════════════════════

pub fn validate_mode(mode: &str) -> Result<&'static str, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "code" => Ok("code"),
        "work" => Ok("work"),
        other => Err(format!(
            "Invalid agent mode '{}'. Allowed modes are 'code' and 'work'.",
            other
        )),
    }
}

pub fn validate_id(id: &str, entity_name: &str) -> Result<String, String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(format!("{} ID cannot be empty", entity_name));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(format!(
            "{} ID '{}' contains invalid path traversal characters",
            entity_name, trimmed
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "{} ID '{}' contains invalid characters (allowed: alphanumeric, -, _, .)",
            entity_name, trimmed
        ));
    }
    Ok(trimmed.to_string())
}

// Capability installation and enablement are global. Runtime profiles may
// still be different, but no capability state is stored per runtime profile.
pub const CAPABILITY_KIND_SKILL: &str = "skill";
pub const CAPABILITY_KIND_MCP: &str = "mcp";
pub const CAPABILITY_KIND_CONNECTOR: &str = "connector";
pub const CAPABILITY_KIND_AGENT_PLUGIN: &str = "agent-plugin";
pub const CAPABILITY_KIND_WEB_ACCESS: &str = "web-access";
pub const CAPABILITY_KIND_BROWSER_USE: &str = "browser-use";
pub const CAPABILITY_KIND_DESKTOP_USE: &str = "desktop-use";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalCapabilityBinding {
    pub kind: String,
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GlobalCapabilityBindingsFile {
    #[serde(default)]
    bindings: Vec<GlobalCapabilityBinding>,
}

pub fn capability_bindings_path_with_root(root: &Path) -> PathBuf {
    root.join("capability-bindings.json")
}

pub fn capability_bindings_path() -> PathBuf {
    capability_bindings_path_with_root(&storage::data_dir())
}

fn validate_capability_kind(kind: &str) -> Result<&'static str, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        CAPABILITY_KIND_SKILL => Ok(CAPABILITY_KIND_SKILL),
        CAPABILITY_KIND_MCP => Ok(CAPABILITY_KIND_MCP),
        CAPABILITY_KIND_CONNECTOR => Ok(CAPABILITY_KIND_CONNECTOR),
        CAPABILITY_KIND_AGENT_PLUGIN => Ok(CAPABILITY_KIND_AGENT_PLUGIN),
        CAPABILITY_KIND_WEB_ACCESS => Ok(CAPABILITY_KIND_WEB_ACCESS),
        CAPABILITY_KIND_BROWSER_USE => Ok(CAPABILITY_KIND_BROWSER_USE),
        CAPABILITY_KIND_DESKTOP_USE => Ok(CAPABILITY_KIND_DESKTOP_USE),
        other => Err(format!("Unknown capability kind '{other}'")),
    }
}

pub fn read_global_capability_bindings_with_root(root: &Path) -> Vec<GlobalCapabilityBinding> {
    let path = capability_bindings_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return Vec::new();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<GlobalCapabilityBindingsFile>(&content).ok())
        .map(|file| file.bindings)
        .unwrap_or_default()
}

pub fn read_global_capability_bindings() -> Vec<GlobalCapabilityBinding> {
    read_global_capability_bindings_with_root(&storage::data_dir())
}

pub fn global_capability_override_with_root(root: &Path, kind: &str, id: &str) -> Option<bool> {
    let kind = validate_capability_kind(kind).ok()?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    read_global_capability_bindings_with_root(root)
        .into_iter()
        .find(|binding| {
            binding.kind.eq_ignore_ascii_case(kind) && binding.id.eq_ignore_ascii_case(id)
        })
        .map(|binding| binding.enabled)
}

pub fn global_capability_override(kind: &str, id: &str) -> Option<bool> {
    global_capability_override_with_root(&storage::data_dir(), kind, id)
}

pub fn set_global_capability_binding_with_root(
    root: &Path,
    kind: &str,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let kind = validate_capability_kind(kind)?;
    let id = validate_id(id, "Capability")?;
    let path = capability_bindings_path_with_root(root);
    let mut file = GlobalCapabilityBindingsFile {
        bindings: read_global_capability_bindings_with_root(root),
    };
    if let Some(binding) = file.bindings.iter_mut().find(|binding| {
        binding.kind.eq_ignore_ascii_case(kind) && binding.id.eq_ignore_ascii_case(&id)
    }) {
        binding.enabled = enabled;
    } else {
        file.bindings.push(GlobalCapabilityBinding {
            kind: kind.to_string(),
            id,
            enabled,
        });
    }
    file.bindings
        .sort_by(|left, right| left.kind.cmp(&right.kind).then(left.id.cmp(&right.id)));
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize global capability bindings: {error}"))?;
    write_managed_file(
        &path,
        format!("{serialized}\n"),
        "global capability bindings",
    )
}

pub fn set_global_capability_binding(kind: &str, id: &str, enabled: bool) -> Result<(), String> {
    set_global_capability_binding_with_root(&storage::data_dir(), kind, id, enabled)
}

pub fn remove_global_capability_binding_with_root(
    root: &Path,
    kind: &str,
    id: &str,
) -> Result<(), String> {
    let kind = validate_capability_kind(kind)?;
    let id = validate_id(id, "Capability")?;
    let path = capability_bindings_path_with_root(root);
    let mut file = GlobalCapabilityBindingsFile {
        bindings: read_global_capability_bindings_with_root(root),
    };
    file.bindings.retain(|binding| {
        !(binding.kind.eq_ignore_ascii_case(kind) && binding.id.eq_ignore_ascii_case(&id))
    });
    if file.bindings.is_empty() {
        if is_regular_file_without_symlink(&path) {
            fs::remove_file(path)
                .map_err(|error| format!("Failed to remove global capability bindings: {error}"))?;
        }
        return Ok(());
    }
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize global capability bindings: {error}"))?;
    write_managed_file(
        &path,
        format!("{serialized}\n"),
        "global capability bindings",
    )
}

pub fn remove_global_capability_binding(kind: &str, id: &str) -> Result<(), String> {
    remove_global_capability_binding_with_root(&storage::data_dir(), kind, id)
}

// ══════════════════════════════════════════════════════════════════════════════
// 1. Global Skill Bindings & Shared Directory
// ══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillBinding {
    pub skill_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillBindingsFile {
    #[serde(default)]
    pub bindings: Vec<SkillBinding>,
}

pub fn shared_skills_dir_with_root(root: &Path) -> PathBuf {
    root.join("skills")
}

pub fn shared_skills_dir() -> PathBuf {
    shared_skills_dir_with_root(&storage::data_dir())
}

pub fn profile_dir_with_root(root: &Path, mode: &str) -> Result<PathBuf, String> {
    let valid_mode = validate_mode(mode)?;
    Ok(root.join("profiles").join(valid_mode))
}

pub fn profile_dir(mode: &str) -> Result<PathBuf, String> {
    profile_dir_with_root(&storage::data_dir(), mode)
}

/// Persistent Pi configuration for Code mode.
///
/// Code's mode profile owns AgentCabin-level files such as `AGENTS.md`, while
/// Pi owns its settings, npm tree, cache, and other runtime-specific files
/// below this directory. The actual Pi process still receives a separate
/// per-run directory under `runtime/code/pi/<run-id>`.
pub fn pi_code_profile_dir_with_root(root: &Path) -> PathBuf {
    root.join("profiles").join("code").join("pi")
}

pub fn pi_code_profile_dir() -> PathBuf {
    pi_code_profile_dir_with_root(&storage::data_dir())
}

/// Persistent DSH configuration for Code mode.
pub fn dsh_code_profile_dir_with_root(root: &Path) -> PathBuf {
    root.join("profiles").join("code").join("dsh")
}

pub fn dsh_code_profile_dir() -> PathBuf {
    dsh_code_profile_dir_with_root(&storage::data_dir())
}

/// Return the persistent profile used by Pi for a mode.
///
/// Code has a dedicated runtime namespace. Work keeps its existing profile
/// root because its package/resource lifecycle is AgentCabin-owned and is
/// intentionally separate from Code's native Pi extensions.
pub fn pi_profile_dir_with_root(root: &Path, mode: &str) -> Result<PathBuf, String> {
    let valid_mode = validate_mode(mode)?;
    if valid_mode == "code" {
        Ok(pi_code_profile_dir_with_root(root))
    } else {
        profile_dir_with_root(root, valid_mode)
    }
}

pub fn pi_profile_dir(mode: &str) -> Result<PathBuf, String> {
    pi_profile_dir_with_root(&storage::data_dir(), mode)
}

pub fn skill_bindings_path_with_root(root: &Path) -> PathBuf {
    root.join("skill-bindings.json")
}

pub fn skill_bindings_path() -> PathBuf {
    skill_bindings_path_with_root(&storage::data_dir())
}

pub fn read_skill_bindings_with_root(root: &Path) -> SkillBindingsFile {
    let path = skill_bindings_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return SkillBindingsFile::default();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(e) => {
            log::warn!(
                "[pi_profile_bindings] Failed to read {}: {}",
                path.display(),
                e
            );
            SkillBindingsFile::default()
        }
    }
}

pub fn read_skill_bindings() -> SkillBindingsFile {
    read_skill_bindings_with_root(&storage::data_dir())
}

pub fn write_skill_bindings_with_root(root: &Path, file: &SkillBindingsFile) -> Result<(), String> {
    let path = skill_bindings_path_with_root(root);
    let serialized = serde_json::to_string_pretty(file)
        .map_err(|e| format!("Failed to serialize skill bindings: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "skill bindings")
}

pub fn write_skill_bindings(file: &SkillBindingsFile) -> Result<(), String> {
    write_skill_bindings_with_root(&storage::data_dir(), file)
}

pub fn is_skill_enabled_with_root(root: &Path, skill_id: &str) -> bool {
    let normalized = skill_id.trim().to_ascii_lowercase();
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_SKILL, &normalized)
    {
        return enabled;
    }
    read_skill_bindings_with_root(root)
        .bindings
        .into_iter()
        .find(|binding| binding.skill_id.trim().eq_ignore_ascii_case(&normalized))
        .map(|binding| binding.enabled)
        .unwrap_or(true)
}

pub fn is_skill_enabled(skill_id: &str) -> bool {
    is_skill_enabled_with_root(&storage::data_dir(), skill_id)
}

pub fn set_skill_binding_with_root(
    root: &Path,
    skill_id: &str,
    enabled: bool,
    disabled_by: Option<String>,
) -> Result<(), String> {
    let normalized_id = validate_id(skill_id, "Skill")?;
    let mut file = read_skill_bindings_with_root(root);
    if let Some(binding) = file
        .bindings
        .iter_mut()
        .find(|binding| binding.skill_id.trim().eq_ignore_ascii_case(&normalized_id))
    {
        binding.enabled = enabled;
        binding.disabled_by = disabled_by;
    } else {
        file.bindings.push(SkillBinding {
            skill_id: normalized_id.clone(),
            enabled,
            disabled_by,
        });
    }
    write_skill_bindings_with_root(root, &file)?;
    set_global_capability_binding_with_root(root, CAPABILITY_KIND_SKILL, &normalized_id, enabled)
}

pub fn set_skill_binding(
    skill_id: &str,
    enabled: bool,
    disabled_by: Option<String>,
) -> Result<(), String> {
    set_skill_binding_with_root(&storage::data_dir(), skill_id, enabled, disabled_by)
}

pub fn remove_skill_binding_with_root(root: &Path, skill_id: &str) -> Result<(), String> {
    let normalized_id = validate_id(skill_id, "Skill")?;
    let mut file = read_skill_bindings_with_root(root);
    file.bindings
        .retain(|b| !b.skill_id.trim().eq_ignore_ascii_case(&normalized_id));
    write_skill_bindings_with_root(root, &file)?;
    remove_global_capability_binding_with_root(root, CAPABILITY_KIND_SKILL, &normalized_id)
}

pub fn remove_skill_binding(skill_id: &str) -> Result<(), String> {
    remove_skill_binding_with_root(&storage::data_dir(), skill_id)
}

pub fn list_enabled_skill_paths_with_root(root: &Path) -> Vec<PathBuf> {
    let shared_root = shared_skills_dir_with_root(root);
    let Ok(shared_meta) = fs::symlink_metadata(&shared_root) else {
        return Vec::new();
    };
    if !path_has_no_symlink_components(&shared_root)
        || shared_meta.file_type().is_symlink()
        || !shared_meta.is_dir()
    {
        return Vec::new();
    }
    let entries = match fs::read_dir(&shared_root) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(path_meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if path_meta.file_type().is_symlink() {
            continue;
        }
        if path_meta.is_dir() {
            let skill_id = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let skill_file = path.join("SKILL.md");
            let has_regular_skill_file = fs::symlink_metadata(&skill_file)
                .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
                .unwrap_or(false);
            if has_regular_skill_file && is_skill_enabled_with_root(root, skill_id) {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

pub fn list_enabled_skill_paths() -> Vec<PathBuf> {
    list_enabled_skill_paths_with_root(&storage::data_dir())
}

// ══════════════════════════════════════════════════════════════════════════════
// 1a. Shared Web Access Package & Global Binding
// ══════════════════════════════════════════════════════════════════════════════

/// Web Access is installed and configured once. Its enablement is global.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebAccessBinding {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebAccessBindingsFile {
    #[serde(default)]
    pub binding: Option<WebAccessBinding>,
}

pub fn web_access_binding_path_with_root(root: &Path) -> PathBuf {
    root.join("web-access-binding.json")
}

pub fn web_access_binding_path() -> PathBuf {
    web_access_binding_path_with_root(&storage::data_dir())
}

pub fn read_web_access_binding_with_root(root: &Path) -> Option<WebAccessBinding> {
    let path = web_access_binding_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<WebAccessBindingsFile>(&content).ok())
        .and_then(|file| file.binding)
}

pub fn is_web_access_enabled_with_root(root: &Path) -> bool {
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_WEB_ACCESS, "web-access")
    {
        return enabled;
    }
    read_web_access_binding_with_root(root)
        .map(|binding| binding.enabled)
        .unwrap_or(true)
}

pub fn is_web_access_enabled() -> bool {
    is_web_access_enabled_with_root(&storage::data_dir())
}

pub fn set_web_access_binding_with_root(root: &Path, enabled: bool) -> Result<(), String> {
    let path = web_access_binding_path_with_root(root);
    let file = WebAccessBindingsFile {
        binding: Some(WebAccessBinding { enabled }),
    };
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize Web Access binding: {error}"))?;
    write_managed_file(&path, format!("{serialized}\n"), "Web Access binding")?;
    set_global_capability_binding_with_root(root, CAPABILITY_KIND_WEB_ACCESS, "web-access", enabled)
}

pub fn set_web_access_binding(enabled: bool) -> Result<(), String> {
    set_web_access_binding_with_root(&storage::data_dir(), enabled)
}

/// Browser Use controls the shared Electron Chromium capability. Its
/// enablement is global.
pub fn browser_use_binding_path_with_root(root: &Path) -> PathBuf {
    root.join("browser-use-binding.json")
}

pub fn browser_use_binding_path() -> PathBuf {
    browser_use_binding_path_with_root(&storage::data_dir())
}

pub fn read_browser_use_binding_with_root(root: &Path) -> Option<WebAccessBinding> {
    let path = browser_use_binding_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<WebAccessBindingsFile>(&content).ok())
        .and_then(|file| file.binding)
}

pub fn is_browser_use_enabled_with_root(root: &Path) -> bool {
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_BROWSER_USE, "browser-use")
    {
        return enabled;
    }
    read_browser_use_binding_with_root(root)
        .map(|binding| binding.enabled)
        .unwrap_or(true)
}

pub fn is_browser_use_enabled() -> bool {
    is_browser_use_enabled_with_root(&storage::data_dir())
}

pub fn set_browser_use_binding_with_root(root: &Path, enabled: bool) -> Result<(), String> {
    let path = browser_use_binding_path_with_root(root);
    let file = WebAccessBindingsFile {
        binding: Some(WebAccessBinding { enabled }),
    };
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize Browser Use binding: {error}"))?;
    write_managed_file(&path, format!("{serialized}\n"), "Browser Use binding")?;
    set_global_capability_binding_with_root(
        root,
        CAPABILITY_KIND_BROWSER_USE,
        "browser-use",
        enabled,
    )
}

pub fn set_browser_use_binding(enabled: bool) -> Result<(), String> {
    set_browser_use_binding_with_root(&storage::data_dir(), enabled)
}

/// Desktop Use is an app-owned native capability rather than an installable
/// package. An absent binding is deliberately disabled; the development P0
/// environment flag remains a bootstrap override for local smoke tests.
pub fn desktop_use_binding_path_with_root(root: &Path) -> PathBuf {
    root.join("desktop-use-binding.json")
}

pub fn desktop_use_binding_path() -> PathBuf {
    desktop_use_binding_path_with_root(&storage::data_dir())
}

pub fn read_desktop_use_binding_with_root(root: &Path) -> Option<WebAccessBinding> {
    let path = desktop_use_binding_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<WebAccessBindingsFile>(&content).ok())
        .and_then(|file| file.binding)
}

pub fn is_desktop_use_enabled_with_root(root: &Path) -> Option<bool> {
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_DESKTOP_USE, "desktop-use")
    {
        return Some(enabled);
    }
    read_desktop_use_binding_with_root(root).map(|binding| binding.enabled)
}

pub fn is_desktop_use_enabled() -> Option<bool> {
    is_desktop_use_enabled_with_root(&storage::data_dir())
}

pub fn set_desktop_use_binding_with_root(root: &Path, enabled: bool) -> Result<(), String> {
    let path = desktop_use_binding_path_with_root(root);
    let file = WebAccessBindingsFile {
        binding: Some(WebAccessBinding { enabled }),
    };
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize Desktop Use binding: {error}"))?;
    write_managed_file(&path, format!("{serialized}\n"), "Desktop Use binding")?;
    set_global_capability_binding_with_root(
        root,
        CAPABILITY_KIND_DESKTOP_USE,
        "desktop-use",
        enabled,
    )
}

pub fn set_desktop_use_binding(enabled: bool) -> Result<(), String> {
    set_desktop_use_binding_with_root(&storage::data_dir(), enabled)
}

// ══════════════════════════════════════════════════════════════════════════════
// 1b. Shared Pi Extension Catalog & Profile Bindings
// ══════════════════════════════════════════════════════════════════════════════

/// Pi packages are installed once in AgentCabin's shared Pi agent directory.
/// The package bytes and npm dependency tree are shared, while this catalog and
/// the per-profile bindings decide which runtime may load a package.
pub fn shared_pi_agent_dir_with_root(root: &Path) -> PathBuf {
    root.join("pi")
}

pub fn shared_pi_agent_dir() -> PathBuf {
    shared_pi_agent_dir_with_root(&storage::data_dir())
}

pub fn shared_pi_extensions_catalog_dir_with_root(root: &Path) -> PathBuf {
    shared_pi_agent_dir_with_root(root)
        .join("extensions")
        .join("catalog")
}

pub fn shared_pi_extensions_catalog_dir() -> PathBuf {
    shared_pi_extensions_catalog_dir_with_root(&storage::data_dir())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionCatalogItem {
    pub id: String,
    pub source: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub package_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionBinding {
    pub extension_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionBindingsFile {
    #[serde(default)]
    pub bindings: Vec<PiExtensionBinding>,
}

pub fn pi_extension_bindings_path_with_root(root: &Path, mode: &str) -> Result<PathBuf, String> {
    Ok(pi_profile_dir_with_root(root, mode)?.join("pi-extension-bindings.json"))
}

pub fn pi_extension_bindings_path(mode: &str) -> Result<PathBuf, String> {
    pi_extension_bindings_path_with_root(&storage::data_dir(), mode)
}

pub fn read_pi_extension_catalog_with_root(root: &Path) -> Vec<PiExtensionCatalogItem> {
    let dir = shared_pi_extensions_catalog_dir_with_root(root);
    if !is_real_directory_without_symlink(&dir) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut items = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() {
                return None;
            }
            (path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .then(|| fs::read_to_string(path).ok())
                .flatten()
                .and_then(|content| serde_json::from_str::<PiExtensionCatalogItem>(&content).ok())
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    items
}

pub fn read_pi_extension_catalog() -> Vec<PiExtensionCatalogItem> {
    read_pi_extension_catalog_with_root(&storage::data_dir())
}

pub fn save_pi_extension_catalog_item_with_root(
    root: &Path,
    item: &PiExtensionCatalogItem,
) -> Result<(), String> {
    let id = validate_id(&item.id, "Pi Extension")?;
    let dir = shared_pi_extensions_catalog_dir_with_root(root);
    ensure_managed_directory(&dir, "Pi extension catalog directory")?;
    let mut normalized = item.clone();
    normalized.id = id.clone();
    let content = serde_json::to_string_pretty(&normalized)
        .map_err(|error| format!("Failed to serialize Pi extension catalog item: {error}"))?;
    write_managed_file(
        &dir.join(format!("{id}.json")),
        format!("{content}\n"),
        "Pi extension catalog item",
    )
}

pub fn save_pi_extension_catalog_item(item: &PiExtensionCatalogItem) -> Result<(), String> {
    save_pi_extension_catalog_item_with_root(&storage::data_dir(), item)
}

pub fn delete_pi_extension_catalog_item_with_root(
    root: &Path,
    extension_id: &str,
) -> Result<(), String> {
    let id = validate_id(extension_id, "Pi Extension")?;
    let path = shared_pi_extensions_catalog_dir_with_root(root).join(format!("{id}.json"));
    if path.is_file() {
        fs::remove_file(path)
            .map_err(|error| format!("Failed to remove Pi extension catalog item: {error}"))?;
    }
    for mode in ["code", "work"] {
        let _ = remove_pi_extension_binding_with_root(root, mode, &id);
    }
    Ok(())
}

pub fn delete_pi_extension_catalog_item(extension_id: &str) -> Result<(), String> {
    delete_pi_extension_catalog_item_with_root(&storage::data_dir(), extension_id)
}

pub fn read_pi_extension_bindings_with_root(root: &Path, mode: &str) -> Vec<PiExtensionBinding> {
    let Ok(path) = pi_extension_bindings_path_with_root(root, mode) else {
        return Vec::new();
    };
    if !is_regular_file_without_symlink(&path) {
        return Vec::new();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<PiExtensionBindingsFile>(&content).ok())
        .map(|file| file.bindings)
        .unwrap_or_default()
}

pub fn read_pi_extension_bindings(mode: &str) -> Vec<PiExtensionBinding> {
    read_pi_extension_bindings_with_root(&storage::data_dir(), mode)
}

pub fn write_pi_extension_bindings_with_root(
    root: &Path,
    mode: &str,
    bindings: &[PiExtensionBinding],
) -> Result<(), String> {
    let dir = pi_profile_dir_with_root(root, mode)?;
    ensure_managed_directory(&dir, "Pi profile directory")?;
    let path = pi_extension_bindings_path_with_root(root, mode)?;
    let content = serde_json::to_string_pretty(&PiExtensionBindingsFile {
        bindings: bindings.to_vec(),
    })
    .map_err(|error| format!("Failed to serialize Pi extension bindings: {error}"))?;
    write_managed_file(&path, format!("{content}\n"), "Pi extension bindings")
}

pub fn set_pi_extension_binding_with_root(
    root: &Path,
    mode: &str,
    extension_id: &str,
    enabled: bool,
    disabled_by: Option<String>,
) -> Result<(), String> {
    let mode = validate_mode(mode)?;
    let id = validate_id(extension_id, "Pi Extension")?;
    let item = read_pi_extension_catalog_with_root(root)
        .into_iter()
        .find(|item| item.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("Pi extension is not installed: {id}"))?;
    if mode == "work" && is_pi_code_native_extension_source(&item.source) {
        return Err(format!(
            "{} is a Pi Code native feature and cannot be enabled in Work",
            item.name
        ));
    }
    let mut bindings = read_pi_extension_bindings_with_root(root, mode);
    if let Some(binding) = bindings
        .iter_mut()
        .find(|binding| binding.extension_id.eq_ignore_ascii_case(&id))
    {
        binding.enabled = enabled;
        binding.disabled_by = disabled_by;
    } else {
        bindings.push(PiExtensionBinding {
            extension_id: id,
            enabled,
            disabled_by,
        });
    }
    write_pi_extension_bindings_with_root(root, mode, &bindings)
}

pub fn set_pi_extension_binding(
    mode: &str,
    extension_id: &str,
    enabled: bool,
    disabled_by: Option<String>,
) -> Result<(), String> {
    set_pi_extension_binding_with_root(
        &storage::data_dir(),
        mode,
        extension_id,
        enabled,
        disabled_by,
    )
}

pub fn remove_pi_extension_binding_with_root(
    root: &Path,
    mode: &str,
    extension_id: &str,
) -> Result<(), String> {
    let id = validate_id(extension_id, "Pi Extension")?;
    let mut bindings = read_pi_extension_bindings_with_root(root, mode);
    bindings.retain(|binding| !binding.extension_id.eq_ignore_ascii_case(&id));
    write_pi_extension_bindings_with_root(root, mode, &bindings)
}

pub fn is_pi_extension_enabled_with_root(root: &Path, mode: &str, extension_id: &str) -> bool {
    read_pi_extension_bindings_with_root(root, mode)
        .iter()
        .find(|binding| {
            binding
                .extension_id
                .eq_ignore_ascii_case(extension_id.trim())
        })
        .map(|binding| binding.enabled)
        .unwrap_or(false)
}

pub fn list_enabled_pi_extension_sources_with_root(root: &Path, mode: &str) -> Vec<PathBuf> {
    let catalog = read_pi_extension_catalog_with_root(root);
    let canonical_root = fs::canonicalize(root).ok();
    let mut paths = catalog
        .into_iter()
        .filter(|item| {
            is_pi_extension_enabled_with_root(root, mode, &item.id)
                && !(mode.trim().eq_ignore_ascii_case("work")
                    && is_pi_code_native_extension_source(&item.source))
        })
        .map(|item| PathBuf::from(item.package_path))
        .filter(|path| {
            let Some(root) = canonical_root.as_ref() else {
                return false;
            };
            let Ok(metadata) = fs::symlink_metadata(path) else {
                return false;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return false;
            }
            fs::canonicalize(path)
                .map(|canonical| canonical.starts_with(root))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

pub fn is_pi_code_native_extension_source(source: &str) -> bool {
    let normalized = source.trim().to_ascii_lowercase();
    [
        "pi-permission-system",
        "pi-goal",
        "pi-plan-mode",
        "pi9/todo",
        "pi-context-prune",
        "pi-subagents",
        "pi-mono-multi-edit",
        "pi-lsp",
    ]
    .iter()
    .any(|name| normalized.contains(name))
}

pub fn list_enabled_pi_extension_sources(mode: &str) -> Vec<PathBuf> {
    list_enabled_pi_extension_sources_with_root(&storage::data_dir(), mode)
}

// ══════════════════════════════════════════════════════════════════════════════
// 2. Global MCP Catalog & Bindings
// ══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpCatalogServer {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_schema: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers_schema: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpBinding {
    pub server_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpBindingsFile {
    #[serde(default)]
    pub bindings: Vec<McpBinding>,
}

pub fn mcp_catalog_dir_with_root(root: &Path) -> PathBuf {
    root.join("mcp").join("catalog")
}

pub fn mcp_catalog_dir() -> PathBuf {
    mcp_catalog_dir_with_root(&storage::data_dir())
}

pub fn mcp_bindings_path_with_root(root: &Path) -> PathBuf {
    root.join("mcp-bindings.json")
}

pub fn mcp_bindings_path() -> PathBuf {
    mcp_bindings_path_with_root(&storage::data_dir())
}

pub fn read_mcp_catalog_with_root(root: &Path) -> Vec<McpCatalogServer> {
    let dir = mcp_catalog_dir_with_root(root);
    if !is_real_directory_without_symlink(&dir) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut servers = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_regular_json = fs::symlink_metadata(&path)
            .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
            .unwrap_or(false);
        if is_regular_json && path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(server) = serde_json::from_str::<McpCatalogServer>(&content) {
                    servers.push(server);
                }
            }
        }
    }
    servers.sort_by(|a, b| a.id.cmp(&b.id));
    servers
}

pub fn read_mcp_catalog() -> Vec<McpCatalogServer> {
    read_mcp_catalog_with_root(&storage::data_dir())
}

pub fn save_mcp_catalog_server_with_root(
    root: &Path,
    server: &McpCatalogServer,
) -> Result<(), String> {
    let validated_id = validate_id(&server.id, "MCP Server")?;
    let dir = mcp_catalog_dir_with_root(root);
    ensure_managed_directory(&dir, "MCP catalog directory")?;
    let path = dir.join(format!("{}.json", validated_id));
    let mut server_clone = server.clone();
    server_clone.id = validated_id;
    let serialized = serde_json::to_string_pretty(&server_clone)
        .map_err(|e| format!("Failed to serialize MCP catalog server: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "MCP catalog server")
}

pub fn save_mcp_catalog_server(server: &McpCatalogServer) -> Result<(), String> {
    save_mcp_catalog_server_with_root(&storage::data_dir(), server)
}

pub fn delete_mcp_catalog_server_with_root(root: &Path, server_id: &str) -> Result<(), String> {
    let validated_id = validate_id(server_id, "MCP Server")?;
    let path = mcp_catalog_dir_with_root(root).join(format!("{}.json", validated_id));
    if path.is_file() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove MCP catalog server: {}", e))?;
    }
    let _ = remove_mcp_binding_with_root(root, &validated_id);
    Ok(())
}

pub fn delete_mcp_catalog_server(server_id: &str) -> Result<(), String> {
    delete_mcp_catalog_server_with_root(&storage::data_dir(), server_id)
}

pub fn read_mcp_bindings_with_root(root: &Path) -> Vec<McpBinding> {
    let path = mcp_bindings_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<McpBindingsFile>(&content)
            .map(|f| f.bindings)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn read_mcp_bindings() -> Vec<McpBinding> {
    read_mcp_bindings_with_root(&storage::data_dir())
}

pub fn write_mcp_bindings_with_root(root: &Path, bindings: &[McpBinding]) -> Result<(), String> {
    let path = mcp_bindings_path_with_root(root);
    let file = McpBindingsFile {
        bindings: bindings.to_vec(),
    };
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize MCP bindings: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "MCP bindings")
}

pub fn write_mcp_bindings(bindings: &[McpBinding]) -> Result<(), String> {
    write_mcp_bindings_with_root(&storage::data_dir(), bindings)
}

pub fn set_mcp_binding_with_root(
    root: &Path,
    server_id: &str,
    enabled: bool,
    secret_ref: Option<Option<String>>,
) -> Result<(), String> {
    let validated_id = validate_id(server_id, "MCP Server")?;
    let mut bindings = read_mcp_bindings_with_root(root);
    if let Some(binding) = bindings
        .iter_mut()
        .find(|binding| binding.server_id.trim().eq_ignore_ascii_case(&validated_id))
    {
        binding.enabled = enabled;
        if let Some(ref_val) = &secret_ref {
            binding.secret_ref = ref_val.clone();
        }
    } else {
        bindings.push(McpBinding {
            server_id: validated_id.clone(),
            enabled,
            secret_ref: secret_ref.flatten(),
            args: Vec::new(),
            env: HashMap::new(),
        });
    }
    write_mcp_bindings_with_root(root, &bindings)?;
    set_global_capability_binding_with_root(root, CAPABILITY_KIND_MCP, &validated_id, enabled)
}

pub fn set_mcp_binding(
    server_id: &str,
    enabled: bool,
    secret_ref: Option<Option<String>>,
) -> Result<(), String> {
    set_mcp_binding_with_root(&storage::data_dir(), server_id, enabled, secret_ref)
}

pub fn remove_mcp_binding_with_root(root: &Path, server_id: &str) -> Result<(), String> {
    let validated_id = validate_id(server_id, "MCP Server")?;
    let mut bindings = read_mcp_bindings_with_root(root);
    bindings.retain(|b| !b.server_id.trim().eq_ignore_ascii_case(&validated_id));
    write_mcp_bindings_with_root(root, &bindings)?;
    remove_global_capability_binding_with_root(root, CAPABILITY_KIND_MCP, &validated_id)
}

pub fn remove_mcp_binding(server_id: &str) -> Result<(), String> {
    remove_mcp_binding_with_root(&storage::data_dir(), server_id)
}

pub fn is_mcp_enabled_with_root(root: &Path, server_id: &str) -> bool {
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_MCP, server_id)
    {
        return enabled;
    }
    read_mcp_bindings_with_root(root)
        .into_iter()
        .find(|binding| {
            binding
                .server_id
                .trim()
                .eq_ignore_ascii_case(server_id.trim())
        })
        .map(|binding| binding.enabled)
        .unwrap_or_else(|| {
            read_mcp_catalog_with_root(root)
                .iter()
                .any(|server| server.id.eq_ignore_ascii_case(server_id.trim()))
        })
}

pub fn is_mcp_enabled(server_id: &str) -> bool {
    is_mcp_enabled_with_root(&storage::data_dir(), server_id)
}

// ══════════════════════════════════════════════════════════════════════════════
// 3. Global Connector Catalog & Bindings
// ══════════════════════════════════════════════════════════════════════════════

fn default_permission_profile() -> String {
    "interactive".to_string()
}

fn default_connector_version() -> String {
    "1.0.0".to_string()
}

fn default_connector_auth_kind() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCatalogItem {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_connector_version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default = "default_connector_auth_kind")]
    pub auth_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorBinding {
    pub connector_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    pub enabled: bool,
    #[serde(default = "default_permission_profile")]
    pub permission_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorBindingsFile {
    #[serde(default)]
    pub bindings: Vec<ConnectorBinding>,
}

pub fn connectors_catalog_dir_with_root(root: &Path) -> PathBuf {
    root.join("connectors").join("catalog")
}

pub fn connectors_catalog_dir() -> PathBuf {
    connectors_catalog_dir_with_root(&storage::data_dir())
}

pub fn read_connector_catalog_with_root(root: &Path) -> Vec<ConnectorCatalogItem> {
    let dir = connectors_catalog_dir_with_root(root);
    if !is_real_directory_without_symlink(&dir) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut connectors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(path_meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if path_meta.file_type().is_symlink() {
            continue;
        }
        let manifest_path = if path_meta.is_dir() {
            path.join("manifest.json")
        } else if path_meta.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            path
        } else {
            continue;
        };
        let manifest_is_regular_file = fs::symlink_metadata(&manifest_path)
            .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
            .unwrap_or(false);
        if manifest_is_regular_file {
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(item) = serde_json::from_str::<ConnectorCatalogItem>(&content) {
                    connectors.push(item);
                }
            }
        }
    }
    connectors.sort_by(|a, b| a.id.cmp(&b.id));
    connectors
}

pub fn read_connector_catalog() -> Vec<ConnectorCatalogItem> {
    read_connector_catalog_with_root(&storage::data_dir())
}

pub fn save_connector_catalog_item_with_root(
    root: &Path,
    item: &ConnectorCatalogItem,
) -> Result<(), String> {
    let validated_id = validate_id(&item.id, "Connector")?;
    let dir = connectors_catalog_dir_with_root(root).join(&validated_id);
    ensure_managed_directory(&dir, "connector catalog directory")?;
    let path = dir.join("manifest.json");
    let mut clone = item.clone();
    clone.id = validated_id;
    let serialized = serde_json::to_string_pretty(&clone)
        .map_err(|e| format!("Failed to serialize connector catalog item: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "connector catalog item")
}

pub fn save_connector_catalog_item(item: &ConnectorCatalogItem) -> Result<(), String> {
    save_connector_catalog_item_with_root(&storage::data_dir(), item)
}

pub fn delete_connector_catalog_item_with_root(
    root: &Path,
    connector_id: &str,
) -> Result<(), String> {
    let validated_id = validate_id(connector_id, "Connector")?;
    let dir = connectors_catalog_dir_with_root(root).join(&validated_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("Failed to remove connector dir: {}", e))?;
    }
    let json_file = connectors_catalog_dir_with_root(root).join(format!("{}.json", validated_id));
    if json_file.is_file() {
        let _ = fs::remove_file(&json_file);
    }
    let _ = remove_connector_binding_with_root(root, &validated_id);
    Ok(())
}

pub fn remove_connector_binding_with_root(root: &Path, connector_id: &str) -> Result<(), String> {
    let validated_id = validate_id(connector_id, "Connector")?;
    let mut bindings = read_connector_bindings_with_root(root);
    bindings.retain(|b| !b.connector_id.trim().eq_ignore_ascii_case(&validated_id));
    write_connector_bindings_with_root(root, &bindings)?;
    remove_global_capability_binding_with_root(root, CAPABILITY_KIND_CONNECTOR, &validated_id)
}

pub fn remove_connector_binding(connector_id: &str) -> Result<(), String> {
    remove_connector_binding_with_root(&storage::data_dir(), connector_id)
}

pub fn delete_connector_catalog_item(connector_id: &str) -> Result<(), String> {
    delete_connector_catalog_item_with_root(&storage::data_dir(), connector_id)
}

pub fn connector_bindings_path_with_root(root: &Path) -> PathBuf {
    root.join("connector-bindings.json")
}

pub fn connector_bindings_path() -> PathBuf {
    connector_bindings_path_with_root(&storage::data_dir())
}

pub fn read_connector_bindings_with_root(root: &Path) -> Vec<ConnectorBinding> {
    let path = connector_bindings_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<ConnectorBindingsFile>(&content)
            .map(|f| f.bindings)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn read_connector_bindings() -> Vec<ConnectorBinding> {
    read_connector_bindings_with_root(&storage::data_dir())
}

pub fn write_connector_bindings_with_root(
    root: &Path,
    bindings: &[ConnectorBinding],
) -> Result<(), String> {
    let path = connector_bindings_path_with_root(root);
    let file = ConnectorBindingsFile {
        bindings: bindings.to_vec(),
    };
    let serialized = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize connector bindings: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "connector bindings")
}

pub fn write_connector_bindings(bindings: &[ConnectorBinding]) -> Result<(), String> {
    write_connector_bindings_with_root(&storage::data_dir(), bindings)
}

pub fn set_connector_binding_with_root(
    root: &Path,
    connector_id: &str,
    connection_id: Option<Option<String>>,
    enabled: bool,
    permission_profile: Option<String>,
) -> Result<(), String> {
    let validated_id = validate_id(connector_id, "Connector")?;
    let mut bindings = read_connector_bindings_with_root(root);
    if let Some(binding) = bindings.iter_mut().find(|binding| {
        binding
            .connector_id
            .trim()
            .eq_ignore_ascii_case(&validated_id)
    }) {
        binding.enabled = enabled;
        if let Some(connection) = &connection_id {
            binding.connection_id = connection.clone();
        }
        if let Some(profile) = &permission_profile {
            binding.permission_profile = profile.clone();
        }
    } else {
        bindings.push(ConnectorBinding {
            connector_id: validated_id.clone(),
            connection_id: connection_id.flatten(),
            enabled,
            permission_profile: permission_profile.unwrap_or_else(default_permission_profile),
        });
    }
    write_connector_bindings_with_root(root, &bindings)?;
    set_global_capability_binding_with_root(root, CAPABILITY_KIND_CONNECTOR, &validated_id, enabled)
}

pub fn set_connector_binding(
    connector_id: &str,
    connection_id: Option<Option<String>>,
    enabled: bool,
    permission_profile: Option<String>,
) -> Result<(), String> {
    set_connector_binding_with_root(
        &storage::data_dir(),
        connector_id,
        connection_id,
        enabled,
        permission_profile,
    )
}

pub fn is_connector_enabled_with_root(root: &Path, connector_id: &str) -> bool {
    if let Some(enabled) =
        global_capability_override_with_root(root, CAPABILITY_KIND_CONNECTOR, connector_id)
    {
        return enabled;
    }
    read_connector_bindings_with_root(root)
        .into_iter()
        .find(|binding| {
            binding
                .connector_id
                .trim()
                .eq_ignore_ascii_case(connector_id.trim())
        })
        .map(|binding| binding.enabled)
        .unwrap_or_else(|| {
            read_connector_catalog_with_root(root)
                .iter()
                .any(|item| item.id.eq_ignore_ascii_case(connector_id.trim()))
        })
}

pub fn is_connector_enabled(connector_id: &str) -> bool {
    is_connector_enabled_with_root(&storage::data_dir(), connector_id)
}

#[derive(Debug, Clone)]
pub struct ProjectedMcpServer {
    pub server_id: String,
    pub enabled: bool,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub secret_env: HashMap<String, String>,
    pub secret_headers: HashMap<String, String>,
}

pub fn project_mcp_bindings_from_catalog(
    root: &Path,
) -> (Vec<ProjectedMcpServer>, Vec<McpCatalogServer>) {
    let bindings = read_mcp_bindings_with_root(root);
    let catalog = read_mcp_catalog_with_root(root);
    let mut projected = Vec::new();

    // The catalog is the global inventory and is enabled by default. The
    // global Capability Center binding is the source of truth for both modes.
    for item in &catalog {
        let binding = bindings
            .iter()
            .find(|binding| binding.server_id.eq_ignore_ascii_case(&item.id));

        let mut secret_env = HashMap::new();
        let mut secret_headers = HashMap::new();
        if let Some(secret_ref) = binding.and_then(|binding| binding.secret_ref.as_deref()) {
            if let Some(secret_dict) = get_host_secret_with_root(root, secret_ref) {
                for (key, value) in secret_dict {
                    if key.eq_ignore_ascii_case("authorization")
                        || key.eq_ignore_ascii_case("token")
                        || key.eq_ignore_ascii_case("apiKey")
                        || key.to_lowercase().contains("header")
                    {
                        secret_headers.insert(key, value);
                    } else {
                        secret_env.insert(key, value);
                    }
                }
            }
        }

        let args = binding
            .filter(|binding| !binding.args.is_empty())
            .map(|binding| binding.args.clone())
            .unwrap_or_else(|| item.args.clone());
        let mut env = item.env_schema.clone();
        if let Some(binding) = binding {
            env.extend(binding.env.clone());
        }

        projected.push(ProjectedMcpServer {
            server_id: item.id.clone(),
            enabled: is_mcp_enabled_with_root(root, &item.id),
            transport: item.transport.clone(),
            command: item.command.clone(),
            args,
            url: item.url.clone(),
            env,
            headers: item.headers_schema.clone(),
            secret_env,
            secret_headers,
        });
    }

    (projected, catalog)
}

pub fn sync_pi_code_mcp_from_catalog_and_bindings_with_root(
    root: &Path,
    _cwd: Option<&str>,
) -> Result<(), String> {
    migrate_legacy_pi_code_profile_with_root(root)?;
    let (projected_servers, catalog) = project_mcp_bindings_from_catalog(root);

    let code_dir = pi_code_profile_dir_with_root(root);
    let config_path = code_dir.join("mcp.json");

    let mut root_val: serde_json::Value = if is_regular_file_without_symlink(&config_path) {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({ "mcpServers": {} }))
    } else {
        serde_json::json!({ "mcpServers": {} })
    };

    if let Ok(meta) = fs::symlink_metadata(&config_path) {
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Refusing to write Pi MCP config through symlink: {}",
                config_path.display()
            ));
        }
    }

    if !root_val.is_object() {
        root_val = serde_json::json!({ "mcpServers": {} });
    }

    let servers_map = root_val
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| "mcpServers must be an object".to_string())?;

    for server in &projected_servers {
        if !server.enabled {
            servers_map.remove(&server.server_id);
            continue;
        }

        let mut server_obj = serde_json::Map::new();
        if server.transport == "stdio" {
            if let Some(cmd) = &server.command {
                server_obj.insert("command".into(), serde_json::Value::String(cmd.clone()));
            }
            server_obj.insert(
                "args".into(),
                serde_json::Value::Array(
                    server
                        .args
                        .iter()
                        .map(|a| serde_json::Value::String(a.clone()))
                        .collect(),
                ),
            );

            let mut env_map = serde_json::Map::new();
            for (k, v) in &server.env {
                env_map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            for (k, v) in &server.secret_env {
                env_map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            if !env_map.is_empty() {
                server_obj.insert("env".into(), serde_json::Value::Object(env_map));
            }
        } else if server.transport == "sse"
            || server.transport == "http"
            || server.transport == "streamable-http"
        {
            server_obj.insert(
                "transport".into(),
                serde_json::Value::String(server.transport.clone()),
            );
            if let Some(url) = &server.url {
                server_obj.insert("url".into(), serde_json::Value::String(url.clone()));
            }

            let mut headers_map = serde_json::Map::new();
            for (k, v) in &server.headers {
                headers_map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            for (k, v) in &server.secret_headers {
                headers_map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            if !headers_map.is_empty() {
                server_obj.insert("headers".into(), serde_json::Value::Object(headers_map));
            }
        }

        servers_map.insert(
            server.server_id.clone(),
            serde_json::Value::Object(server_obj),
        );
    }

    // Purge: remove any entry that was previously synced from the catalog
    // but is no longer present in the current catalog or its binding is disabled/missing.
    let catalog_ids: Vec<String> = catalog.iter().map(|s| s.id.clone()).collect();
    let enabled_ids: Vec<String> = projected_servers
        .iter()
        .filter(|b| b.enabled)
        .filter(|b| {
            catalog_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&b.server_id))
        })
        .map(|b| b.server_id.clone())
        .collect();
    let stale_keys: Vec<String> = servers_map
        .keys()
        .filter(|k| {
            catalog_ids.iter().any(|id| id.eq_ignore_ascii_case(k))
                && !enabled_ids.iter().any(|id| id.eq_ignore_ascii_case(k))
        })
        .cloned()
        .collect();
    for key in stale_keys {
        servers_map.remove(&key);
    }

    if let Some(parent) = config_path.parent() {
        ensure_managed_directory(parent, "Pi MCP config parent")?;
    }
    let serialized = serde_json::to_string_pretty(&root_val).map_err(|e| e.to_string())?;
    write_managed_file(&config_path, format!("{}\n", serialized), "Pi MCP config")?;
    Ok(())
}

pub fn sync_pi_code_mcp_from_catalog_and_bindings(cwd: Option<&str>) -> Result<(), String> {
    sync_pi_code_mcp_from_catalog_and_bindings_with_root(&storage::data_dir(), cwd)
}

// ══════════════════════════════════════════════════════════════════════════════
// 4. Host Secrets Storage (Token, API Key, Refresh Token)
// ══════════════════════════════════════════════════════════════════════════════

pub fn host_secrets_dir_with_root(root: &Path) -> PathBuf {
    root.join("host-secrets")
}

pub fn host_secrets_dir() -> PathBuf {
    host_secrets_dir_with_root(&storage::data_dir())
}

pub fn host_secrets_path_with_root(root: &Path) -> PathBuf {
    host_secrets_dir_with_root(root).join("secrets.json")
}

pub fn host_secrets_path() -> PathBuf {
    host_secrets_path_with_root(&storage::data_dir())
}

pub fn read_host_secrets_with_root(root: &Path) -> HashMap<String, HashMap<String, String>> {
    let path = host_secrets_path_with_root(root);
    if !is_regular_file_without_symlink(&path) {
        return HashMap::new();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(e) => {
            log::warn!("[pi_profile_bindings] failed to read host secrets: {}", e);
            HashMap::new()
        }
    }
}

fn is_regular_file_without_symlink(path: &Path) -> bool {
    path_has_no_symlink_components(path)
        && fs::symlink_metadata(path)
            .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
            .unwrap_or(false)
}

pub(crate) fn is_real_directory_without_symlink(path: &Path) -> bool {
    path_has_no_symlink_components(path)
        && fs::symlink_metadata(path)
            .map(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            .unwrap_or(false)
}

fn path_has_no_symlink_components(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_symlink() && !is_allowed_system_alias(&current) =>
            {
                return false;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return false,
        }
    }
    true
}

pub(crate) fn ensure_managed_directory(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("Managed {label} path cannot be empty."));
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    if is_allowed_system_alias(&current) {
                        continue;
                    }
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
                if !metadata.is_dir() {
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        format!(
                            "Managed {label} {} was created concurrently; retry the operation.",
                            current.display()
                        )
                    } else {
                        format!(
                            "Failed to create managed {label} {}: {error}",
                            current.display()
                        )
                    }
                })?;
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    format!(
                        "Failed to inspect managed {label} {} after creation: {error}",
                        current.display()
                    )
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
            }
            Err(error) => {
                return Err(format!(
                    "Failed to inspect managed {label} {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

fn is_allowed_system_alias(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        if path == Path::new("/var") || path == Path::new("/tmp") {
            return fs::canonicalize(path)
                .map(|canonical| {
                    (path == Path::new("/var") && canonical == Path::new("/private/var"))
                        || (path == Path::new("/tmp") && canonical == Path::new("/private/tmp"))
                })
                .unwrap_or(false);
        }
    }
    false
}

pub(crate) fn write_managed_file(
    path: &Path,
    contents: impl AsRef<[u8]>,
    label: &str,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Managed {label} path has no parent: {}", path.display()))?;
    ensure_managed_directory(parent, &format!("{label} parent"))?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Strict isolation error: managed {label} '{}' is not a regular file.",
                path.display()
            ));
        }
    }
    if !path_has_no_symlink_components(path) {
        return Err(format!(
            "Strict isolation error: managed {label} path '{}' contains a symlink.",
            path.display()
        ));
    }

    fs::write(path, contents).map_err(|error| {
        format!(
            "Failed to write managed {label} {}: {error}",
            path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

pub fn read_host_secrets() -> HashMap<String, HashMap<String, String>> {
    read_host_secrets_with_root(&storage::data_dir())
}

pub fn write_host_secrets_with_root(
    root: &Path,
    secrets: &HashMap<String, HashMap<String, String>>,
) -> Result<(), String> {
    let dir = host_secrets_dir_with_root(root);
    ensure_managed_directory(&dir, "host-secrets directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    let path = host_secrets_path_with_root(root);
    let serialized = serde_json::to_string_pretty(secrets)
        .map_err(|e| format!("Failed to serialize host secrets: {}", e))?;
    write_managed_file(&path, format!("{}\n", serialized), "host secrets file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn write_host_secrets(
    secrets: &HashMap<String, HashMap<String, String>>,
) -> Result<(), String> {
    write_host_secrets_with_root(&storage::data_dir(), secrets)
}

pub fn get_host_secret_with_root(root: &Path, secret_ref: &str) -> Option<HashMap<String, String>> {
    let secrets = read_host_secrets_with_root(root);
    secrets.get(secret_ref).cloned()
}

pub fn get_host_secret(secret_ref: &str) -> Option<HashMap<String, String>> {
    get_host_secret_with_root(&storage::data_dir(), secret_ref)
}

pub fn set_host_secret_with_root(
    root: &Path,
    secret_ref: &str,
    secret_map: HashMap<String, String>,
) -> Result<(), String> {
    let validated_ref = validate_id(secret_ref, "Host Secret Ref")?;
    let mut secrets = read_host_secrets_with_root(root);
    secrets.insert(validated_ref, secret_map);
    write_host_secrets_with_root(root, &secrets)
}

pub fn set_host_secret(
    secret_ref: &str,
    secret_map: HashMap<String, String>,
) -> Result<(), String> {
    set_host_secret_with_root(&storage::data_dir(), secret_ref, secret_map)
}

pub fn delete_host_secret_with_root(root: &Path, secret_ref: &str) -> Result<(), String> {
    let mut secrets = read_host_secrets_with_root(root);
    secrets.remove(secret_ref.trim());
    write_host_secrets_with_root(root, &secrets)
}

pub fn delete_host_secret(secret_ref: &str) -> Result<(), String> {
    delete_host_secret_with_root(&storage::data_dir(), secret_ref)
}

// ══════════════════════════════════════════════════════════════════════════════
// 5. Legacy Migration
// ══════════════════════════════════════════════════════════════════════════════

/// Entries that earlier AgentCabin versions placed directly in
/// `profiles/code` while that directory also acted as Pi's `HOME`.
///
/// Keep this list deliberately narrow: `AGENTS.md`, capability bindings, and
/// other mode-level AgentCabin data must remain in `profiles/code`.
const LEGACY_CODE_PI_ENTRIES: &[&str] = &[
    "settings.json",
    "npm",
    "node_modules",
    "npm-cache",
    "cache",
    "mcp-oauth",
    "mcp.json",
    "pi-extension-bindings.json",
    "extensions",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "sessions",
    "auth.json",
    "models.json",
    "themes",
];

static MIGRATION_CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MIGRATION_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

/// Move legacy Code Pi state into `profiles/code/pi` without overwriting an
/// existing destination. If a destination already exists with different file
/// contents, the legacy entry is intentionally left untouched so recovery
/// remains possible and startup does not destroy user data. Identical regular
/// files are redundant and are removed after their contents are verified.
pub fn migrate_legacy_pi_code_profile_if_needed() -> Result<(), String> {
    if MIGRATION_CHECKED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    migrate_legacy_pi_code_profile_with_root(&storage::data_dir())
}

pub fn migrate_legacy_pi_code_profile_with_root(root: &Path) -> Result<(), String> {
    let migration_lock = MIGRATION_LOCK.get_or_init(|| std::sync::Mutex::new(()));
    let _guard = migration_lock
        .lock()
        .map_err(|_| "Code Pi migration lock poisoned".to_string())?;
    let is_default_root = root == storage::data_dir();
    if is_default_root && MIGRATION_CHECKED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }

    let result = migrate_legacy_pi_code_profile_impl(root);
    if is_default_root && result.is_ok() {
        MIGRATION_CHECKED.store(true, std::sync::atomic::Ordering::Release);
    }
    result
}

fn migrate_legacy_pi_code_profile_impl(root: &Path) -> Result<(), String> {
    let legacy_root = profile_dir_with_root(root, "code")?;
    let target_root = pi_code_profile_dir_with_root(root);

    let legacy_metadata = match fs::symlink_metadata(&legacy_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to inspect legacy Code Pi profile {}: {error}",
                legacy_root.display()
            ));
        }
    };
    if legacy_metadata.file_type().is_symlink() || !legacy_metadata.is_dir() {
        return Err(format!(
            "Legacy Code Pi profile is not a real directory: {}",
            legacy_root.display()
        ));
    }
    if !path_has_no_symlink_components(&legacy_root) {
        return Err(format!(
            "Refusing to migrate legacy Code Pi profile through a symlink: {}",
            legacy_root.display()
        ));
    }
    match fs::symlink_metadata(&target_root) {
        Ok(target_metadata) => {
            if target_metadata.file_type().is_symlink() || !target_metadata.is_dir() {
                return Err(format!(
                    "Code Pi profile destination is not a real directory: {}",
                    target_root.display()
                ));
            }
            if !path_has_no_symlink_components(&target_root) {
                return Err(format!(
                    "Refusing to migrate Code Pi profile through a symlink: {}",
                    target_root.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Failed to inspect Code Pi profile destination {}: {error}",
                target_root.display()
            ));
        }
    }

    let mut target_created = false;
    for entry_name in LEGACY_CODE_PI_ENTRIES {
        let source = legacy_root.join(entry_name);
        let source_metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect legacy Code Pi entry {}: {error}",
                    source.display()
                ));
            }
        };
        if source_metadata.file_type().is_symlink()
            || (!source_metadata.is_file() && !source_metadata.is_dir())
        {
            return Err(format!(
                "Refusing to migrate non-regular legacy Code Pi entry: {}",
                source.display()
            ));
        }

        let target = target_root.join(entry_name);
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Refusing to migrate Code Pi entry through a symlink: {}",
                        target.display()
                    ));
                }
                if !metadata.is_file() && !metadata.is_dir() {
                    return Err(format!(
                        "Code Pi destination is not a regular file or directory: {}",
                        target.display()
                    ));
                }
                let identical_files = if metadata.is_file() && source_metadata.is_file() {
                    match (fs::read(&source), fs::read(&target)) {
                        (Ok(source_content), Ok(target_content)) => {
                            source_content == target_content
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                if identical_files {
                    fs::remove_file(&source).map_err(|error| {
                        format!(
                            "Failed to remove duplicate legacy Code Pi entry {}: {error}",
                            source.display()
                        )
                    })?;
                    continue;
                }
                log::warn!(
                    "[pi_profile_bindings] keeping legacy Code Pi entry {} because destination {} already exists",
                    source.display(),
                    target.display()
                );
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to inspect Code Pi destination {}: {error}",
                    target.display()
                ));
            }
        }

        if !target_created {
            ensure_managed_directory(&target_root, "Code Pi profile directory")?;
            target_created = true;
        }
        fs::rename(&source, &target).map_err(|error| {
            format!(
                "Failed to migrate legacy Code Pi entry {} to {}: {error}",
                source.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

/// Migrate legacy skills from ~/.pi/agent/skills and ~/.agentcabin/profiles/work/skills
/// into ~/.agentcabin/skills without destructive data loss.
pub fn migrate_legacy_skills_if_needed() -> Result<(), String> {
    migrate_legacy_skills_with_root(&storage::data_dir())
}

pub fn migrate_legacy_skills_with_root(root: &Path) -> Result<(), String> {
    let target_root = shared_skills_dir_with_root(root);
    ensure_managed_directory(&target_root, "shared skills root")?;

    // 1. Migrate Pi legacy user skills: ~/.pi/agent/skills
    if let Some(home) = storage::home_dir() {
        let legacy_pi_skills = Path::new(&home).join(".pi").join("agent").join("skills");
        if legacy_pi_skills.is_dir() {
            if let Ok(entries) = fs::read_dir(&legacy_pi_skills) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let dest = target_root.join(&name);
                    if !dest.exists() && path.is_dir() && path.join("SKILL.md").is_file() {
                        if let Err(e) = copy_dir_all(&path, &dest) {
                            log::warn!(
                                "[pi_profile_bindings] failed to copy legacy Pi skill {}: {}",
                                path.display(),
                                e
                            );
                        } else if let Err(e) = set_skill_binding_with_root(root, &name, true, None)
                        {
                            log::warn!(
                                "[pi_profile_bindings] failed to set skill binding for {}: {}",
                                name,
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    // 2. Migrate Work legacy profile skills: profiles/work/skills
    let legacy_work_skills = root.join("profiles").join("work").join("skills");
    if legacy_work_skills.is_dir() {
        if let Ok(entries) = fs::read_dir(&legacy_work_skills) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let dest = target_root.join(&name);
                if !dest.exists() && path.is_dir() && path.join("SKILL.md").is_file() {
                    if let Err(e) = copy_dir_all(&path, &dest) {
                        log::warn!(
                            "[pi_profile_bindings] failed to copy legacy Work skill {}: {}",
                            path.display(),
                            e
                        );
                    } else if let Err(e) = set_skill_binding_with_root(root, &name, true, None) {
                        log::warn!(
                            "[pi_profile_bindings] failed to set skill binding for {}: {}",
                            name,
                            e
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_and_id_validation() {
        assert_eq!(validate_mode("code").unwrap(), "code");
        assert_eq!(validate_mode("WORK").unwrap(), "work");
        assert!(validate_mode("invalid").is_err());

        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(
            pi_code_profile_dir_with_root(tmp.path()),
            tmp.path().join("profiles/code/pi")
        );
        assert_eq!(
            pi_profile_dir_with_root(tmp.path(), "code").unwrap(),
            tmp.path().join("profiles/code/pi")
        );
        assert_eq!(
            pi_profile_dir_with_root(tmp.path(), "work").unwrap(),
            tmp.path().join("profiles/work")
        );
        assert_eq!(
            pi_extension_bindings_path_with_root(tmp.path(), "code").unwrap(),
            tmp.path()
                .join("profiles/code/pi/pi-extension-bindings.json")
        );
        assert_eq!(
            pi_extension_bindings_path_with_root(tmp.path(), "work").unwrap(),
            tmp.path().join("profiles/work/pi-extension-bindings.json")
        );

        assert_eq!(
            validate_id("my-skill_1.0", "Skill").unwrap(),
            "my-skill_1.0"
        );
        assert!(validate_id("../hack", "Skill").is_err());
        assert!(validate_id("a/b", "Skill").is_err());
        assert!(validate_id("", "Skill").is_err());
    }

    #[test]
    fn test_skill_bindings_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sample = SkillBindingsFile {
            bindings: vec![
                SkillBinding {
                    skill_id: "pdf".into(),
                    enabled: true,
                    disabled_by: None,
                },
                SkillBinding {
                    skill_id: "coding".into(),
                    enabled: false,
                    disabled_by: Some("user".into()),
                },
            ],
        };
        write_skill_bindings_with_root(tmp.path(), &sample).unwrap();

        let bindings_file = skill_bindings_path_with_root(tmp.path());
        let read_back = read_skill_bindings_with_root(tmp.path());
        assert_eq!(read_back.bindings.len(), 2);
        assert!(read_back.bindings[0].enabled);
        assert!(!read_back.bindings[1].enabled);
        assert!(bindings_file.is_file());
        assert!(!tmp
            .path()
            .join("profiles/code/skill-bindings.json")
            .exists());
        assert!(!tmp
            .path()
            .join("profiles/work/skill-bindings.json")
            .exists());
    }

    #[test]
    fn shared_pi_resources_use_one_global_capability_binding() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Skills default to enabled in global catalog
        assert!(is_skill_enabled_with_root(tmp.path(), "research"));

        // A switch from either UI surface updates the shared capability state.
        set_skill_binding_with_root(tmp.path(), "research", false, None).unwrap();
        assert!(!is_skill_enabled_with_root(tmp.path(), "research"));
        assert_eq!(
            global_capability_override_with_root(tmp.path(), CAPABILITY_KIND_SKILL, "research"),
            Some(false)
        );

        set_skill_binding_with_root(tmp.path(), "research", true, None).unwrap();
        assert!(is_skill_enabled_with_root(tmp.path(), "research"));
        assert!(!tmp
            .path()
            .join("profiles/code/skill-bindings.json")
            .exists());
        assert!(!tmp
            .path()
            .join("profiles/work/skill-bindings.json")
            .exists());

        let safe_path = tmp.path().join("pi").join("safe-extension");
        fs::create_dir_all(&safe_path).unwrap();
        let safe = PiExtensionCatalogItem {
            id: "safe-extension".into(),
            source: "npm:@acme/safe-extension".into(),
            name: "Safe Extension".into(),
            description: None,
            version: Some("1.0.0".into()),
            package_path: safe_path.to_string_lossy().into_owned(),
        };
        save_pi_extension_catalog_item_with_root(tmp.path(), &safe).unwrap();

        set_pi_extension_binding_with_root(tmp.path(), "code", &safe.id, true, None).unwrap();
        set_pi_extension_binding_with_root(tmp.path(), "work", &safe.id, false, None).unwrap();
        assert_eq!(
            list_enabled_pi_extension_sources_with_root(tmp.path(), "code"),
            vec![safe_path]
        );
        assert!(list_enabled_pi_extension_sources_with_root(tmp.path(), "work").is_empty());

        let native_path = tmp.path().join("pi").join("native-feature");
        fs::create_dir_all(&native_path).unwrap();
        let native = PiExtensionCatalogItem {
            id: "native-feature".into(),
            source: "npm:@gotgenes/pi-permission-system".into(),
            name: "Permission system".into(),
            description: None,
            version: None,
            package_path: native_path.to_string_lossy().into_owned(),
        };
        save_pi_extension_catalog_item_with_root(tmp.path(), &native).unwrap();
        set_pi_extension_binding_with_root(tmp.path(), "code", &native.id, true, None).unwrap();
        assert!(
            set_pi_extension_binding_with_root(tmp.path(), "work", &native.id, true, None).is_err()
        );
        assert!(list_enabled_pi_extension_sources_with_root(tmp.path(), "work").is_empty());
    }

    #[test]
    fn test_mcp_and_connector_bindings() {
        let tmp = tempfile::TempDir::new().unwrap();

        let mcp_sample = vec![McpBinding {
            server_id: "notion".into(),
            enabled: true,
            secret_ref: Some("notion-main".into()),
            args: vec![],
            env: HashMap::new(),
        }];
        write_mcp_bindings_with_root(tmp.path(), &mcp_sample).unwrap();
        let mcp_file = mcp_bindings_path_with_root(tmp.path());

        let connector_sample = vec![ConnectorBinding {
            connector_id: "feishu".into(),
            connection_id: Some("feishu-main".into()),
            enabled: true,
            permission_profile: "interactive".into(),
        }];
        write_connector_bindings_with_root(tmp.path(), &connector_sample).unwrap();
        let connector_file = connector_bindings_path_with_root(tmp.path());

        let read_mcp: McpBindingsFile =
            serde_json::from_str(&fs::read_to_string(&mcp_file).unwrap()).unwrap();
        assert_eq!(read_mcp.bindings[0].server_id, "notion");
        assert!(read_mcp.bindings[0].enabled);

        let read_conn: ConnectorBindingsFile =
            serde_json::from_str(&fs::read_to_string(&connector_file).unwrap()).unwrap();
        assert_eq!(read_conn.bindings[0].connector_id, "feishu");
        assert_eq!(read_conn.bindings[0].permission_profile, "interactive");
    }

    #[test]
    fn test_connector_catalog_crud() {
        let tmp = tempfile::TempDir::new().unwrap();
        let item = ConnectorCatalogItem {
            id: "github".into(),
            name: "GitHub Connector".into(),
            description: Some("Interact with GitHub API".into()),
            version: "1.2.0".into(),
            author: Some("AgentCabin".into()),
            homepage: None,
            auth_kind: "apiKey".into(),
            entrypoint: None,
            skills: vec!["git-ops".into()],
            mcp_server: Some("github-mcp".into()),
            cli: None,
        };

        save_connector_catalog_item_with_root(tmp.path(), &item).unwrap();
        let catalog = read_connector_catalog_with_root(tmp.path());
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].id, "github");
        assert_eq!(catalog[0].auth_kind, "apiKey");

        set_connector_binding_with_root(
            tmp.path(),
            "github",
            Some(Some("github-conn-1".into())),
            true,
            Some("readonly".into()),
        )
        .unwrap();
        let bindings = read_connector_bindings_with_root(tmp.path());
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].connection_id.as_deref(), Some("github-conn-1"));

        // Clear connection ID
        set_connector_binding_with_root(tmp.path(), "github", Some(None), true, None).unwrap();
        let bindings = read_connector_bindings_with_root(tmp.path());
        assert_eq!(bindings[0].connection_id, None);

        delete_connector_catalog_item_with_root(tmp.path(), "github").unwrap();
        let catalog_after = read_connector_catalog_with_root(tmp.path());
        assert!(catalog_after.is_empty());
        let bindings_after = read_connector_bindings_with_root(tmp.path());
        assert!(bindings_after.is_empty());
    }

    #[test]
    fn test_mcp_catalog_and_secret_projection() {
        let tmp = tempfile::TempDir::new().unwrap();
        let server = McpCatalogServer {
            id: "weather".into(),
            name: "Weather Server".into(),
            description: None,
            transport: "http".into(),
            command: None,
            args: vec![],
            url: Some("https://api.weather.com/mcp".into()),
            env_schema: HashMap::new(),
            headers_schema: HashMap::from([("X-Api-Key".into(), "KEY_PLACEHOLDER".into())]),
        };
        save_mcp_catalog_server_with_root(tmp.path(), &server).unwrap();

        let mut secret_map = HashMap::new();
        secret_map.insert("X-Api-Key".into(), "secret-12345".into());
        set_host_secret_with_root(tmp.path(), "weather-secret", secret_map).unwrap();

        set_mcp_binding_with_root(
            tmp.path(),
            "weather",
            true,
            Some(Some("weather-secret".into())),
        )
        .unwrap();

        let retrieved_secret = get_host_secret_with_root(tmp.path(), "weather-secret").unwrap();
        assert_eq!(retrieved_secret.get("X-Api-Key").unwrap(), "secret-12345");

        // Clear secret ref
        set_mcp_binding_with_root(tmp.path(), "weather", true, Some(None)).unwrap();
        let bindings = read_mcp_bindings_with_root(tmp.path());
        assert_eq!(bindings[0].secret_ref, None);
    }

    #[test]
    fn global_capability_catalog_is_shared_without_positive_bindings() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mcp = McpCatalogServer {
            id: "shared-mcp".into(),
            name: "Shared MCP".into(),
            description: None,
            transport: "stdio".into(),
            command: Some("shared-mcp".into()),
            args: vec!["--serve".into()],
            url: None,
            env_schema: HashMap::from([("MODE".into(), "catalog".into())]),
            headers_schema: HashMap::new(),
        };
        save_mcp_catalog_server_with_root(tmp.path(), &mcp).unwrap();

        let connector_dir = connectors_catalog_dir_with_root(tmp.path()).join("shared-connector");
        fs::create_dir_all(&connector_dir).unwrap();
        fs::write(
            connector_dir.join("manifest.json"),
            serde_json::to_string(&ConnectorCatalogItem {
                id: "shared-connector".into(),
                name: "Shared Connector".into(),
                description: None,
                version: "1.0.0".into(),
                author: None,
                homepage: None,
                auth_kind: "none".into(),
                entrypoint: None,
                skills: Vec::new(),
                mcp_server: None,
                cli: None,
            })
            .unwrap(),
        )
        .unwrap();

        let (projected, _) = project_mcp_bindings_from_catalog(tmp.path());
        assert_eq!(projected.len(), 1);
        assert!(projected[0].enabled);
        assert!(is_mcp_enabled_with_root(tmp.path(), "shared-mcp"));
        assert!(is_connector_enabled_with_root(
            tmp.path(),
            "shared-connector"
        ));

        set_mcp_binding_with_root(tmp.path(), "shared-mcp", false, None).unwrap();
        assert!(!project_mcp_bindings_from_catalog(tmp.path())
            .0
            .iter()
            .any(|server| server.enabled));

        set_connector_binding_with_root(tmp.path(), "shared-connector", None, false, None).unwrap();
        assert!(!is_connector_enabled_with_root(
            tmp.path(),
            "shared-connector"
        ));
    }

    #[test]
    fn browser_use_binding_is_global_and_default_enabled() {
        let tmp = tempfile::TempDir::new().unwrap();

        assert!(is_browser_use_enabled_with_root(tmp.path()));

        set_browser_use_binding_with_root(tmp.path(), false).unwrap();
        assert!(!is_browser_use_enabled_with_root(tmp.path()));

        assert!(browser_use_binding_path_with_root(tmp.path()).is_file());
        assert!(!tmp
            .path()
            .join("profiles/code/browser-use-binding.json")
            .exists());
        assert!(!tmp
            .path()
            .join("profiles/work/browser-use-binding.json")
            .exists());
    }

    #[test]
    fn desktop_use_binding_is_global_and_opt_in() {
        let tmp = tempfile::TempDir::new().unwrap();

        assert_eq!(is_desktop_use_enabled_with_root(tmp.path()), None);

        set_desktop_use_binding_with_root(tmp.path(), true).unwrap();
        assert_eq!(is_desktop_use_enabled_with_root(tmp.path()), Some(true));

        set_desktop_use_binding_with_root(tmp.path(), false).unwrap();
        assert_eq!(is_desktop_use_enabled_with_root(tmp.path()), Some(false));
        assert!(desktop_use_binding_path_with_root(tmp.path()).is_file());
        assert!(!tmp
            .path()
            .join("profiles/code/desktop-use-binding.json")
            .exists());
        assert!(!tmp
            .path()
            .join("profiles/work/desktop-use-binding.json")
            .exists());
    }

    #[test]
    fn test_legacy_skill_migration() {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_legacy_skill = tmp
            .path()
            .join("profiles")
            .join("work")
            .join("skills")
            .join("calc");
        fs::create_dir_all(&work_legacy_skill).unwrap();
        fs::write(
            work_legacy_skill.join("SKILL.md"),
            "---\nname: Calculator\n---\n# Calc",
        )
        .unwrap();

        migrate_legacy_skills_with_root(tmp.path()).unwrap();

        let migrated_skill = tmp.path().join("skills").join("calc").join("SKILL.md");
        assert!(migrated_skill.is_file());
        let bindings = read_skill_bindings_with_root(tmp.path());
        assert_eq!(bindings.bindings.len(), 1);
        assert_eq!(bindings.bindings[0].skill_id, "calc");
        assert!(bindings.bindings[0].enabled);
    }

    #[test]
    fn pi_code_mcp_projection_uses_the_pi_profile_directory() {
        let tmp = tempfile::TempDir::new().unwrap();

        sync_pi_code_mcp_from_catalog_and_bindings_with_root(tmp.path(), None).unwrap();

        assert!(tmp.path().join("profiles/code/pi/mcp.json").is_file());
        assert!(!tmp.path().join("profiles/code/mcp.json").exists());
    }

    #[test]
    fn migrates_legacy_code_pi_entries_without_moving_mode_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let code_root = tmp.path().join("profiles/code");
        fs::create_dir_all(code_root.join("npm/node_modules")).unwrap();
        fs::create_dir_all(code_root.join("cache")).unwrap();
        fs::write(code_root.join("settings.json"), "{\"packages\":[]}").unwrap();
        fs::write(code_root.join("mcp.json"), "{\"mcpServers\":{}}").unwrap();
        fs::write(
            code_root.join("pi-extension-bindings.json"),
            "{\"bindings\":[]}",
        )
        .unwrap();
        fs::write(code_root.join("AGENTS.md"), "# Code rules").unwrap();
        fs::write(code_root.join("skill-bindings.json"), "legacy").unwrap();

        migrate_legacy_pi_code_profile_with_root(tmp.path()).unwrap();

        let pi_root = code_root.join("pi");
        assert_eq!(
            fs::read_to_string(pi_root.join("settings.json")).unwrap(),
            "{\"packages\":[]}"
        );
        assert!(pi_root.join("npm/node_modules").is_dir());
        assert!(pi_root.join("cache").is_dir());
        assert!(pi_root.join("mcp.json").is_file());
        assert!(pi_root.join("pi-extension-bindings.json").is_file());
        assert!(code_root.join("AGENTS.md").is_file());
        assert!(code_root.join("skill-bindings.json").is_file());
        assert!(!code_root.join("settings.json").exists());
        assert!(!code_root.join("npm").exists());
    }

    #[test]
    fn legacy_code_pi_migration_preserves_conflicting_destinations() {
        let tmp = tempfile::TempDir::new().unwrap();
        let code_root = tmp.path().join("profiles/code");
        let pi_root = code_root.join("pi");
        fs::create_dir_all(&pi_root).unwrap();
        fs::write(code_root.join("settings.json"), "legacy").unwrap();
        fs::write(pi_root.join("settings.json"), "current").unwrap();

        migrate_legacy_pi_code_profile_with_root(tmp.path()).unwrap();

        assert_eq!(
            fs::read_to_string(code_root.join("settings.json")).unwrap(),
            "legacy"
        );
        assert_eq!(
            fs::read_to_string(pi_root.join("settings.json")).unwrap(),
            "current"
        );
    }

    #[test]
    fn legacy_code_pi_migration_removes_identical_file_duplicates() {
        let tmp = tempfile::TempDir::new().unwrap();
        let code_root = tmp.path().join("profiles/code");
        let pi_root = code_root.join("pi");
        fs::create_dir_all(&pi_root).unwrap();
        fs::write(code_root.join("settings.json"), "same").unwrap();
        fs::write(pi_root.join("settings.json"), "same").unwrap();

        migrate_legacy_pi_code_profile_with_root(tmp.path()).unwrap();

        assert!(!code_root.join("settings.json").exists());
        assert_eq!(
            fs::read_to_string(pi_root.join("settings.json")).unwrap(),
            "same"
        );
    }
}
