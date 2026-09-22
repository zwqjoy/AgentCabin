//! Agent Plugins 1.0.0 storage, validation, and runtime projection.
//!
//! An Agent Plugin is an installed directory containing plugin.json, an
//! optional skills directory, and an optional mcp.json. Package bytes are
//! immutable from the Capability Center: trust and global enablement live in
//! AgentCabin-owned state outside the package.

use crate::storage::{self, profile_bindings};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

pub const SPEC_VERSION: &str = "1.0.0";
pub const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
pub const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

const PLUGIN_ROOT_TOKEN: &str = concat!("$", "{PLUGIN_ROOT}");
const PLUGIN_DATA_TOKEN: &str = concat!("$", "{PLUGIN_DATA}");
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_MCP_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SKILL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PACKAGE_FILES: usize = 4096;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_COMPONENT_ID_PART: usize = 96;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginAuthor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginSource {
    pub kind: String,
    pub location: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginSkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub has_assets: bool,
    pub plugin_id: String,
    #[serde(default = "default_true")]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginMcpServerSummary {
    pub id: String,
    pub name: String,
    pub original_name: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub env_keys: Vec<String>,
    #[serde(default)]
    pub header_keys: Vec<String>,
    pub plugin_id: String,
    #[serde(default = "default_true")]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkBuddyExpertMemberSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub profession: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<AgentPluginAuthor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AgentPluginSource>,
    #[serde(default)]
    pub skills: Vec<AgentPluginSkillSummary>,
    #[serde(default)]
    pub mcp_servers: Vec<AgentPluginMcpServerSummary>,
    #[serde(default = "default_agent_plugin_format")]
    pub package_format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expert_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    #[serde(default)]
    pub members: Vec<WorkBuddyExpertMemberSummary>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub can_update: bool,
    #[serde(default = "default_true")]
    pub can_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginBinding {
    pub plugin_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginRuntimeSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginRuntimeMcpServer {
    pub id: String,
    pub name: String,
    pub original_name: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub plugin_id: String,
    pub plugin_root: PathBuf,
    pub plugin_data: PathBuf,
}

#[derive(Debug, Clone)]
struct Manifest {
    name: String,
    version: String,
    description: String,
    author: Option<AgentPluginAuthor>,
    homepage: Option<String>,
    repository: Option<String>,
    license: Option<String>,
    package_format: String,
    workbuddy: Option<WorkBuddyManifest>,
}

#[derive(Debug, Clone)]
struct WorkBuddyManifest {
    expert_kind: String,
    display_name: String,
    profession: String,
    lead_agent: String,
    agent_paths: Vec<String>,
    member_agents: Vec<String>,
    mcp_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PluginState {
    #[serde(default)]
    plugin_id: String,
    #[serde(default)]
    source: Option<AgentPluginSource>,
    #[serde(default)]
    trusted: bool,
    #[serde(default)]
    installed_at: String,
}

#[derive(Debug, Clone)]
struct LoadedPlugin {
    summary: AgentPluginSummary,
    runtime_skills: Vec<AgentPluginRuntimeSkill>,
    runtime_mcp_servers: Vec<AgentPluginRuntimeMcpServer>,
}

fn default_true() -> bool {
    true
}

fn default_agent_plugin_format() -> String {
    "agent-plugin".to_string()
}

pub fn agent_plugins_root_with_root(root: &Path) -> PathBuf {
    root.join("agent-plugins")
}

pub fn agent_plugins_root() -> PathBuf {
    agent_plugins_root_with_root(&storage::data_dir())
}

pub fn agent_plugin_packages_dir_with_root(root: &Path) -> PathBuf {
    agent_plugins_root_with_root(root).join("packages")
}

pub fn agent_plugin_data_dir_with_root(root: &Path) -> PathBuf {
    agent_plugins_root_with_root(root).join("data")
}

pub fn agent_plugin_data_path_with_root(root: &Path, plugin_id: &str) -> PathBuf {
    agent_plugin_data_dir_with_root(root).join(plugin_id)
}

pub fn agent_plugin_catalog_dir_with_root(root: &Path) -> PathBuf {
    agent_plugins_root_with_root(root).join("catalog")
}

pub fn agent_plugin_mcp_config_path_with_root(root: &Path, mode: &str) -> Result<PathBuf, String> {
    let mode = profile_bindings::validate_mode(mode)?;
    Ok(profile_bindings::profile_dir_with_root(root, mode)?.join("agent-plugin-mcp.json"))
}

fn package_path_with_root(root: &Path, plugin_id: &str) -> PathBuf {
    agent_plugin_packages_dir_with_root(root).join(plugin_id)
}

fn state_path_with_root(root: &Path, plugin_id: &str) -> PathBuf {
    agent_plugin_catalog_dir_with_root(root).join(format!("{plugin_id}.json"))
}

fn binding_path_with_root(root: &Path) -> PathBuf {
    root.join("agent-plugin-bindings.json")
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn package_manifest_path(root: &Path) -> Option<PathBuf> {
    let workbuddy = root.join(".codebuddy-plugin").join("plugin.json");
    if is_real_file(&workbuddy) {
        return Some(workbuddy);
    }
    // Keep the old path readable only for already-installed state migration;
    // new imports and UI listings require the WorkBuddy package marker above.
    let standard = root.join("plugin.json");
    is_real_file(&standard).then_some(standard)
}

fn contained_in(root: &Path, child: &Path) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    let Ok(child) = fs::canonicalize(child) else {
        return false;
    };
    child == root || child.starts_with(&root)
}

fn is_valid_plugin_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 64
        || name.contains("--")
        || name.contains("..")
        || !name.is_ascii()
    {
        return false;
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    if !bytes[bytes.len() - 1].is_ascii_lowercase() && !bytes[bytes.len() - 1].is_ascii_digit() {
        return false;
    }
    name.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
    })
}

fn validate_plugin_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    if !is_valid_plugin_name(id) {
        return Err(format!("Invalid Agent Plugin id '{id}'"));
    }
    Ok(id.to_string())
}

fn component_segment(value: &str) -> String {
    let mut segment = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while segment.contains("--") {
        segment = segment.replace("--", "-");
    }
    segment = segment
        .trim_matches(|ch| ch == '-' || ch == '.')
        .to_string();
    if segment.is_empty() {
        segment = "component".to_string();
    }
    if segment.len() > MAX_COMPONENT_ID_PART {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        segment.truncate(MAX_COMPONENT_ID_PART.saturating_sub(9));
        segment.push('-');
        segment.push_str(&digest[..8]);
    }
    segment
}

fn component_id(plugin_id: &str, kind: &str, component: &str) -> String {
    format!(
        "agent-plugin--{plugin_id}--{kind}--{}",
        component_segment(component)
    )
}

fn read_json_file(path: &Path, max_bytes: u64, label: &str) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{label}: {error}"))?;
    if metadata.len() > max_bytes {
        return Err(format!("{label} is too large"));
    }
    let content = fs::read_to_string(path).map_err(|error| format!("{label}: {error}"))?;
    serde_json::from_str(&content).map_err(|error| format!("{label} is not valid JSON: {error}"))
}

fn json_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("plugin.json {key} must be a string")),
    }
}

fn parse_author(value: Option<&Value>) -> Result<Option<AgentPluginAuthor>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| "plugin.json author must be an object".to_string())?;
    for key in object.keys() {
        if !matches!(key.as_str(), "name" | "email" | "url") {
            return Err(format!("plugin.json author has unknown field '{key}'"));
        }
    }
    Ok(Some(AgentPluginAuthor {
        name: json_string(object, "name")?,
        email: json_string(object, "email")?,
        url: json_string(object, "url")?,
    }))
}

fn localized_json_text(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.trim().to_string()),
        Some(Value::Object(values)) => ["zh", "zh-CN", "en", "default"]
            .into_iter()
            .filter_map(|key| values.get(key).and_then(Value::as_str))
            .find(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string()),
        _ => None,
    }
}

fn json_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_workbuddy_manifest(object: &Map<String, Value>) -> Result<WorkBuddyManifest, String> {
    let expert_type = object
        .get("expertType")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if object.contains_key("teamInfo") || object.contains_key("members") {
                "team"
            } else {
                "agent"
            }
        });
    if !matches!(expert_type, "agent" | "team") {
        return Err("WorkBuddy plugin.json expertType must be 'agent' or 'team'".into());
    }
    let agent_paths = match object.get("agents") {
        Some(Value::String(path)) if !path.trim().is_empty() => vec![path.trim().to_string()],
        Some(Value::Array(_)) => json_string_array(object.get("agents")),
        _ => Vec::new(),
    };
    if agent_paths.is_empty() {
        return Err("WorkBuddy expert package must declare agents".into());
    }
    let team_info = object.get("teamInfo").and_then(Value::as_object);
    let lead_agent = object
        .get("agentName")
        .and_then(Value::as_str)
        .or_else(|| team_info.and_then(|value| value.get("leadAgent")?.as_str()))
        .unwrap_or_default()
        .trim()
        .to_string();
    let member_agents = json_string_array(team_info.and_then(|value| value.get("memberAgents")));
    let dependencies = object.get("dependencies").and_then(Value::as_object);
    let mcp_declaration = dependencies.and_then(|value| value.get("mcpServers"));
    let mut mcp_paths = match mcp_declaration {
        Some(Value::String(path)) if !path.trim().is_empty() => vec![path.trim().to_string()],
        Some(Value::Array(_)) => json_string_array(mcp_declaration),
        _ => Vec::new(),
    };
    if mcp_paths.is_empty() {
        mcp_paths.push(".mcp.json".into());
    }
    Ok(WorkBuddyManifest {
        expert_kind: if expert_type == "team" {
            "expert-team".into()
        } else {
            "expert".into()
        },
        display_name: localized_json_text(object.get("displayName"))
            .or_else(|| localized_json_text(object.get("title")))
            .or_else(|| localized_json_text(object.get("name_zh")))
            .unwrap_or_default(),
        profession: localized_json_text(object.get("profession")).unwrap_or_default(),
        lead_agent,
        agent_paths,
        member_agents,
        mcp_paths,
    })
}

fn parse_manifest(path: &Path, warnings: &mut Vec<String>) -> Result<Manifest, String> {
    let value = read_json_file(path, MAX_MANIFEST_BYTES, "plugin.json")?;
    let object = value
        .as_object()
        .ok_or_else(|| "plugin.json top level must be an object".to_string())?;
    let is_workbuddy = object.get("$schema").is_none()
        && (object.contains_key("expertType")
            || object.contains_key("agentName")
            || object.contains_key("agents"));
    match object.get("$schema").and_then(Value::as_str) {
        Some(schema) if schema == PLUGIN_SCHEMA => {}
        Some(schema) => {
            return Err(format!(
            "Unsupported plugin.json $schema '{schema}'; Agent Plugins {SPEC_VERSION} is required"
        ))
        }
        None if is_workbuddy => {}
        None => return Err("plugin.json is missing required $schema".to_string()),
    }

    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "plugin.json is missing required name".to_string())?;
    if !is_valid_plugin_name(name) {
        return Err(format!("plugin.json name '{name}' is not valid"));
    }
    for key in object.keys() {
        if is_workbuddy {
            continue;
        }
        if !matches!(
            key.as_str(),
            "$schema"
                | "name"
                | "version"
                | "description"
                | "author"
                | "homepage"
                | "repository"
                | "license"
                | "keywords"
                | "extensions"
        ) {
            warnings.push(format!("plugin.json unknown field '{key}' was ignored"));
        }
    }
    if !is_workbuddy {
        for key in [
            "version",
            "description",
            "homepage",
            "repository",
            "license",
        ] {
            let _ = json_string(object, key)?;
        }
    }
    if let Some(keywords) = object.get("keywords") {
        let values = keywords
            .as_array()
            .ok_or_else(|| "plugin.json keywords must be an array of strings".to_string())?;
        if values.iter().any(|value| !value.is_string()) {
            return Err("plugin.json keywords must be an array of strings".to_string());
        }
    }
    if let Some(extensions) = object.get("extensions") {
        if !extensions.is_object() {
            warnings.push("plugin.json extensions is not an object and was ignored".to_string());
        }
    }

    let workbuddy = is_workbuddy
        .then(|| parse_workbuddy_manifest(object))
        .transpose()?;
    Ok(Manifest {
        name: name.to_string(),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: localized_json_text(object.get("displayDescription"))
            .or_else(|| localized_json_text(object.get("description")))
            .unwrap_or_default(),
        author: match object.get("author") {
            Some(Value::String(name)) if is_workbuddy => Some(AgentPluginAuthor {
                name: Some(name.clone()),
                email: None,
                url: None,
            }),
            value => parse_author(value)?,
        },
        homepage: if is_workbuddy {
            object.get("homepage").and_then(|value| match value {
                Value::String(value) => Some(value.clone()),
                Value::Object(value) => value.get("url")?.as_str().map(str::to_string),
                _ => None,
            })
        } else {
            json_string(object, "homepage")?
        },
        repository: if is_workbuddy {
            localized_json_text(object.get("repository"))
        } else {
            json_string(object, "repository")?
        },
        license: localized_json_text(object.get("license")),
        package_format: if is_workbuddy {
            "workbuddy".into()
        } else {
            "agent-plugin".into()
        },
        workbuddy,
    })
}

fn parse_yaml_scalar(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

fn parse_skill_frontmatter(path: &Path) -> Result<(Option<String>, Option<String>), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SKILL_BYTES {
        return Err("SKILL.md is too large".to_string());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Ok((None, None));
    }
    let mut name = None;
    let mut description = None;
    for line in lines.take(100) {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(parse_yaml_scalar(value));
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(parse_yaml_scalar(value));
        }
    }
    Ok((
        name.filter(|value| !value.is_empty()),
        description.filter(|value| !value.is_empty()),
    ))
}

fn workbuddy_agent_files(root: &Path, raw: &str) -> Vec<PathBuf> {
    let raw = raw.trim().trim_start_matches("./");
    let raw_path = Path::new(raw);
    if raw.is_empty()
        || raw_path.is_absolute()
        || raw_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Vec::new();
    }
    let target_dir = root.join(raw);
    if is_real_dir(&target_dir) && contained_in(root, &target_dir) {
        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(&target_dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    is_real_file(p)
                        && p.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                })
                .collect();
            paths.sort();
            results.extend(paths);
        }
        if !results.is_empty() {
            return results;
        }
    }
    let mut candidates = vec![raw.to_string()];
    if !raw.to_ascii_lowercase().ends_with(".md") {
        candidates.push(format!("{raw}.md"));
        candidates.push(format!("agents/{raw}"));
        candidates.push(format!("agents/{raw}.md"));
        candidates.push(format!("agents/{raw}/AGENT.md"));
        candidates.push(format!("agents/{raw}/agent.md"));
    }
    candidates
        .into_iter()
        .map(|candidate| root.join(candidate))
        .find(|path| is_real_file(path) && contained_in(root, path))
        .into_iter()
        .collect()
}

fn workbuddy_agent_id(path: &Path) -> String {
    parse_skill_frontmatter(path)
        .ok()
        .and_then(|value| value.0)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("expert")
                .to_string()
        })
}

fn workbuddy_agent_profession(path: &Path) -> String {
    let Ok(content) = fs::read_to_string(path) else {
        return String::new();
    };
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for line in lines.take(100) {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("profession:") {
            let value = parse_yaml_scalar(value);
            if !value.is_empty() {
                return value;
            }
        }
    }
    String::new()
}

fn workbuddy_members(
    root: &Path,
    manifest: &WorkBuddyManifest,
) -> Vec<WorkBuddyExpertMemberSummary> {
    let mut members = manifest
        .agent_paths
        .iter()
        .flat_map(|path| workbuddy_agent_files(root, path))
        .map(|path| {
            let id = workbuddy_agent_id(&path);
            let description = parse_skill_frontmatter(&path)
                .ok()
                .and_then(|value| value.1)
                .unwrap_or_default();
            let profession = {
                let value = workbuddy_agent_profession(&path);
                if value.is_empty() {
                    description.clone()
                } else {
                    value
                }
            };
            let is_lead = (!manifest.lead_agent.is_empty() && id == manifest.lead_agent)
                || (manifest.lead_agent.is_empty() && manifest.member_agents.first() != Some(&id));
            WorkBuddyExpertMemberSummary {
                name: id.clone(),
                id,
                profession,
                role: if is_lead {
                    "lead".into()
                } else {
                    "member".into()
                },
            }
        })
        .collect::<Vec<_>>();
    if !members.iter().any(|member| member.role == "lead") {
        if let Some(first) = members.first_mut() {
            first.role = "lead".into();
        }
    }
    members
}

fn materialize_workbuddy_expert_skill(
    root: &Path,
    plugin_id: &str,
    manifest: &WorkBuddyManifest,
) -> Result<(), String> {
    let mut agents = manifest
        .agent_paths
        .iter()
        .flat_map(|raw| workbuddy_agent_files(root, raw))
        .filter_map(|path| {
            fs::read_to_string(&path)
                .ok()
                .map(|content| (path, content))
        })
        .collect::<Vec<_>>();
    if agents.is_empty() {
        return Err("WorkBuddy expert package has no readable agent Markdown".into());
    }
    agents.sort_by_key(|(path, _)| {
        let id = workbuddy_agent_id(path);
        if !manifest.lead_agent.is_empty() && id == manifest.lead_agent {
            0
        } else {
            1
        }
    });
    // DSH's skill registry deliberately accepts only kebab-case names. The
    // AgentCabin component id is namespaced with `--`, which is useful for
    // storage/UI identity but is not a valid DSH skill invocation name. Keep
    // the namespaced id in the host catalog and emit a stable, valid public
    // skill name in the materialized expert document.
    let dsh_skill_name = format!("agentcabin-expert-{}", dsh_skill_segment(plugin_id));
    let title = if manifest.display_name.is_empty() {
        plugin_id
    } else {
        &manifest.display_name
    };
    let mut body = format!(
        "---\nname: \"{dsh_skill_name}\"\ndescription: \"WorkBuddy {}: {}\"\n---\n\n# {}\n\n",
        manifest.expert_kind.replace('-', " "),
        title.replace('"', "'"),
        title
    );
    body.push_str("Activation: this is an opt-in expert role, not a standalone general-purpose skill. Apply the lead agent instructions only when the current user turn explicitly selects this expert or expert team. A previous turn's selection or the package being installed is not activation for the current turn. Otherwise ignore the role instructions below.\n\nFollow the WorkBuddy lead agent instructions below while this expert is selected.\n\n");
    body.push_str(&agents[0].1);
    if manifest.expert_kind == "expert-team" {
        body.push_str("\n\n## Expert team collaboration\n\nUse Work delegation tools for independent member tasks when available. Do not claim a member completed work until its real result is received. The lead owns synthesis and final delivery.\n");
        for (path, content) in agents.iter().skip(1) {
            body.push_str(&format!(
                "\n### Member: {}\n\n{}\n",
                workbuddy_agent_id(path),
                content
            ));
        }
    }
    let directory = root.join(".agentcabin").join("expert");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::write(directory.join("SKILL.md"), body).map_err(|error| error.to_string())
}

/// Convert an Agent Plugin id into the kebab-case grammar accepted by DSH's
/// skill filesystem provider. Plugin ids may contain dots and repeated
/// separators, while DSH rejects both dots and empty name segments.
fn dsh_skill_segment(plugin_id: &str) -> String {
    let mut segment = String::with_capacity(plugin_id.len());
    let mut pending_separator = false;
    for ch in plugin_id.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !segment.is_empty() {
                segment.push('-');
            }
            pending_separator = false;
            segment.push(ch.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }
    }
    segment.trim_matches('-').to_string()
}

fn materialize_workbuddy_mcp(root: &Path, manifest: &WorkBuddyManifest) -> Result<(), String> {
    let mut merged = Map::new();
    for raw in &manifest.mcp_paths {
        let raw = raw.trim().trim_start_matches("./");
        let path = root.join(raw);
        if !is_real_file(&path) || !contained_in(root, &path) {
            continue;
        }
        let value = read_json_file(&path, MAX_MCP_BYTES, "WorkBuddy MCP dependency")?;
        let Some(servers) = value.get("mcpServers").and_then(Value::as_object) else {
            return Err(format!(
                "WorkBuddy MCP dependency '{raw}' is missing mcpServers"
            ));
        };
        for (name, value) in servers {
            let mut entry = value
                .as_object()
                .cloned()
                .ok_or_else(|| format!("WorkBuddy MCP entry '{name}' must be an object"))?;
            let transport = entry
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if entry.contains_key("url") {
                        "streamable-http".into()
                    } else {
                        "stdio".into()
                    }
                });
            let transport = match transport.as_str() {
                "http" | "streamableHttp" => "streamable-http",
                other => other,
            };
            entry.insert("type".into(), Value::String(transport.to_string()));
            if let Some(static_env) = entry.remove("staticEnv") {
                let mut env = entry
                    .remove("env")
                    .and_then(|value| value.as_object().cloned())
                    .unwrap_or_default();
                if let Some(values) = static_env.as_object() {
                    env.extend(values.clone());
                }
                entry.insert("env".into(), Value::Object(env));
            }
            if let Some(static_headers) = entry.remove("staticHeaders") {
                let mut headers = entry
                    .remove("headers")
                    .and_then(|value| value.as_object().cloned())
                    .unwrap_or_default();
                if let Some(values) = static_headers.as_object() {
                    headers.extend(values.clone());
                }
                entry.insert("headers".into(), Value::Object(headers));
            }
            for unsupported in ["timeout", "disabledTools", "preAuth", "failOnStartupError"] {
                entry.remove(unsupported);
            }
            merged.insert(name.clone(), Value::Object(entry));
        }
    }
    if merged.is_empty() {
        return Ok(());
    }
    let value = serde_json::json!({
        "$schema": MCP_SCHEMA,
        "mcpServers": merged,
    });
    fs::write(
        root.join("mcp.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| error.to_string())
}

fn discover_workbuddy_expert_skill(
    root: &Path,
    plugin_id: &str,
    manifest: &WorkBuddyManifest,
    warnings: &mut Vec<String>,
) -> Option<(AgentPluginSkillSummary, AgentPluginRuntimeSkill)> {
    let directory = root.join(".agentcabin").join("expert");
    let skill_md = directory.join("SKILL.md");
    if !is_real_file(&skill_md) || !contained_in(root, &skill_md) {
        warnings.push("WorkBuddy expert runtime Skill is unavailable".into());
        return None;
    }
    let id = component_id(plugin_id, "expert", plugin_id);
    let display_name = if manifest.display_name.is_empty() {
        plugin_id.to_string()
    } else {
        manifest.display_name.clone()
    };
    let summary = AgentPluginSkillSummary {
        id: id.clone(),
        name: display_name,
        description: manifest.profession.clone(),
        path: directory.display().to_string(),
        has_assets: false,
        plugin_id: plugin_id.to_string(),
        readonly: true,
    };
    let runtime = AgentPluginRuntimeSkill {
        id: id.clone(),
        name: id,
        description: manifest.profession.clone(),
        path: directory,
        plugin_id: plugin_id.to_string(),
    };
    Some((summary, runtime))
}

fn discover_skills(
    root: &Path,
    plugin_id: &str,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginSkillSummary, AgentPluginRuntimeSkill)> {
    let skills_dir = root.join("skills");
    if !skills_dir.exists() {
        return Vec::new();
    }
    if !is_real_dir(&skills_dir) || !contained_in(root, &skills_dir) {
        warnings
            .push("skills is not a real directory inside the plugin and was ignored".to_string());
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(&skills_dir) else {
        warnings.push("skills could not be read and was ignored".to_string());
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let skill_dir = entry.path();
        let Ok(meta) = fs::symlink_metadata(&skill_dir) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }
        let Some(directory_name) = skill_dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Ok(files) = fs::read_dir(&skill_dir) else {
            continue;
        };
        let mut has_exact_skill_md = false;
        let mut loose_skill_md = None;
        for file in files.flatten() {
            let file_name = file.file_name().to_string_lossy().into_owned();
            if file_name == "SKILL.md" {
                has_exact_skill_md = true;
            } else if file_name.eq_ignore_ascii_case("skill.md") {
                loose_skill_md = Some(file_name);
            }
        }
        if !has_exact_skill_md {
            if let Some(loose) = loose_skill_md {
                warnings.push(format!(
                    "skill '{directory_name}' contains {loose}; exact SKILL.md is required and it was skipped"
                ));
            }
            continue;
        }
        let skill_md = skill_dir.join("SKILL.md");
        if !is_real_file(&skill_md) || !contained_in(root, &skill_md) {
            warnings.push(format!(
                "skill '{directory_name}' SKILL.md is not a real in-package file"
            ));
            continue;
        }
        let (name, description) = match parse_skill_frontmatter(&skill_md) {
            Ok(values) => values,
            Err(error) => {
                warnings.push(format!(
                    "skill '{directory_name}' could not be read: {error}"
                ));
                continue;
            }
        };
        if name.is_none() && description.is_none() {
            warnings.push(format!(
                "skill '{directory_name}' SKILL.md has no name/description frontmatter and was skipped"
            ));
            continue;
        }
        let id = component_id(plugin_id, "skill", directory_name);
        let has_assets = fs::read_dir(&skill_dir)
            .map(|files| {
                files.flatten().any(|file| {
                    let name = file.file_name().to_string_lossy().into_owned();
                    name != "SKILL.md" && !name.starts_with('.')
                })
            })
            .unwrap_or(false);
        let display_name = name.unwrap_or_else(|| directory_name.to_string());
        let display_description = description.unwrap_or_default();
        let summary = AgentPluginSkillSummary {
            id: id.clone(),
            name: display_name.clone(),
            description: display_description.clone(),
            path: skill_dir.display().to_string(),
            has_assets,
            plugin_id: plugin_id.to_string(),
            readonly: true,
        };
        let runtime = AgentPluginRuntimeSkill {
            id: id.clone(),
            name: id,
            description: display_description,
            path: skill_dir,
            plugin_id: plugin_id.to_string(),
        };
        result.push((summary, runtime));
    }
    result.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    result
}

fn expand_value(value: &str, root: &Path, data: &Path) -> String {
    value
        .replace(PLUGIN_ROOT_TOKEN, &root.display().to_string())
        .replace(PLUGIN_DATA_TOKEN, &data.display().to_string())
}

fn expand_path(value: &str, root: &Path, data: &Path) -> Option<PathBuf> {
    let path = if let Some(rest) = value.strip_prefix(PLUGIN_ROOT_TOKEN) {
        root.join(rest.trim_start_matches('/'))
    } else if let Some(rest) = value.strip_prefix(PLUGIN_DATA_TOKEN) {
        data.join(rest.trim_start_matches('/'))
    } else if let Some(rest) = value.strip_prefix("./") {
        root.join(rest)
    } else {
        return None;
    };
    Some(path)
}

fn valid_http_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if url.scheme() == "https" {
        return true;
    }
    matches!(url.scheme(), "http" | "https") && matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn parse_string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{label} must be an array of strings"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{label} must be an array of strings"))
        })
        .collect()
}

fn parse_string_map(value: Option<&Value>, label: &str) -> Result<HashMap<String, String>, String> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object of strings"))?;
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.clone(), value.to_string()))
                .ok_or_else(|| format!("{label} must be an object of strings"))
        })
        .collect()
}

fn discover_mcp_servers(
    root: &Path,
    plugin_id: &str,
    data_dir: &Path,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginMcpServerSummary, AgentPluginRuntimeMcpServer)> {
    let path = root.join("mcp.json");
    if !path.exists() {
        return Vec::new();
    }
    if !is_real_file(&path) || !contained_in(root, &path) {
        warnings.push("mcp.json is not a real in-package file and was ignored".to_string());
        return Vec::new();
    }
    let value = match read_json_file(&path, MAX_MCP_BYTES, "mcp.json") {
        Ok(value) => value,
        Err(error) => {
            warnings.push(format!("{error}; MCP components were ignored"));
            return Vec::new();
        }
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            warnings.push(
                "mcp.json top level must be an object; MCP components were ignored".to_string(),
            );
            return Vec::new();
        }
    };
    if object.get("$schema").and_then(Value::as_str) != Some(MCP_SCHEMA) {
        warnings.push(format!(
            "mcp.json must declare $schema {MCP_SCHEMA}; MCP components were ignored"
        ));
        return Vec::new();
    }
    let servers = match object.get("mcpServers").and_then(Value::as_object) {
        Some(servers) => servers,
        None => {
            warnings.push(
                "mcp.json must contain an mcpServers object; MCP components were ignored"
                    .to_string(),
            );
            return Vec::new();
        }
    };
    for key in object.keys() {
        if key != "$schema" && key != "mcpServers" {
            warnings.push(format!(
                "mcp.json unknown top-level field '{key}'; MCP components were ignored"
            ));
            return Vec::new();
        }
    }

    let mut result = Vec::new();
    for (original_name, entry) in servers {
        let skip = |reason: String, warnings: &mut Vec<String>| {
            warnings.push(format!(
                "MCP entry '{original_name}' {reason} and was skipped"
            ));
        };
        let Some(entry) = entry.as_object() else {
            skip("is not an object".to_string(), warnings);
            continue;
        };
        let Some(transport) = entry.get("type").and_then(Value::as_str) else {
            skip("is missing type".to_string(), warnings);
            continue;
        };
        let allowed: &[&str] = match transport {
            "stdio" => &["type", "command", "args", "env", "cwd"],
            "streamable-http" | "sse" => &["type", "url", "headers"],
            _ => {
                skip(
                    format!("uses unsupported transport '{transport}'"),
                    warnings,
                );
                continue;
            }
        };
        if let Some(unknown) = entry.keys().find(|key| !allowed.contains(&key.as_str())) {
            skip(format!("contains unsupported field '{unknown}'"), warnings);
            continue;
        }
        let required = if transport == "stdio" {
            "command"
        } else {
            "url"
        };
        if !entry.contains_key(required) {
            skip(format!("is missing required field '{required}'"), warnings);
            continue;
        }
        if transport == "sse" {
            skip(
                "uses legacy sse, which AgentCabin does not implement".to_string(),
                warnings,
            );
            continue;
        }

        let id = component_id(plugin_id, "mcp", original_name);
        if transport == "stdio" {
            let Some(raw_command) = entry.get("command").and_then(Value::as_str) else {
                skip("command must be a non-empty string".to_string(), warnings);
                continue;
            };
            if raw_command.trim().is_empty() {
                skip("command must be a non-empty string".to_string(), warnings);
                continue;
            }
            let mut command = raw_command.to_string();
            if raw_command.starts_with("./") {
                let Some(command_path) = expand_path(raw_command, root, data_dir) else {
                    skip("command path is invalid".to_string(), warnings);
                    continue;
                };
                if !is_real_file(&command_path) || !contained_in(root, &command_path) {
                    skip(
                        "command points outside the plugin or is not a file".to_string(),
                        warnings,
                    );
                    continue;
                }
                command = command_path.display().to_string();
            }
            let args = match parse_string_array(entry.get("args"), "stdio args") {
                Ok(values) => values
                    .iter()
                    .map(|value| expand_value(value, root, data_dir))
                    .collect::<Vec<_>>(),
                Err(error) => {
                    skip(error, warnings);
                    continue;
                }
            };
            let raw_env = match parse_string_map(entry.get("env"), "stdio env") {
                Ok(values) => values,
                Err(error) => {
                    skip(error, warnings);
                    continue;
                }
            };
            if raw_env.keys().any(|key| {
                key.eq_ignore_ascii_case("PLUGIN_ROOT") || key.eq_ignore_ascii_case("PLUGIN_DATA")
            }) {
                skip(
                    "env cannot define PLUGIN_ROOT or PLUGIN_DATA".to_string(),
                    warnings,
                );
                continue;
            }
            let mut env = raw_env
                .into_iter()
                .map(|(key, value)| (key, expand_value(&value, root, data_dir)))
                .collect::<HashMap<_, _>>();
            env.insert("PLUGIN_ROOT".to_string(), root.display().to_string());
            env.insert("PLUGIN_DATA".to_string(), data_dir.display().to_string());

            let cwd = match entry.get("cwd") {
                None => root.to_path_buf(),
                Some(Value::String(value)) => {
                    let Some(path) = expand_path(value, root, data_dir) else {
                        skip(
                            "cwd must start with ./, PLUGIN_ROOT, or PLUGIN_DATA".to_string(),
                            warnings,
                        );
                        continue;
                    };
                    let in_root = contained_in(root, &path);
                    let in_data = contained_in(data_dir, &path);
                    if !is_real_dir(&path) || (!in_root && !in_data) {
                        skip(
                            "cwd is outside the plugin and PLUGIN_DATA directories".to_string(),
                            warnings,
                        );
                        continue;
                    }
                    path
                }
                Some(_) => {
                    skip("cwd must be a string".to_string(), warnings);
                    continue;
                }
            };
            let summary = AgentPluginMcpServerSummary {
                id: id.clone(),
                name: original_name.clone(),
                original_name: original_name.clone(),
                transport: "stdio".to_string(),
                command: Some(command.clone()),
                args: args.clone(),
                cwd: Some(cwd.display().to_string()),
                url: None,
                env_keys: env
                    .keys()
                    .filter(|key| *key != "PLUGIN_ROOT" && *key != "PLUGIN_DATA")
                    .cloned()
                    .collect(),
                header_keys: Vec::new(),
                plugin_id: plugin_id.to_string(),
                readonly: true,
            };
            let runtime = AgentPluginRuntimeMcpServer {
                id: id.clone(),
                name: id,
                original_name: original_name.clone(),
                transport: "stdio".to_string(),
                command: Some(command),
                args,
                cwd: Some(cwd),
                url: None,
                env,
                headers: HashMap::new(),
                plugin_id: plugin_id.to_string(),
                plugin_root: root.to_path_buf(),
                plugin_data: data_dir.to_path_buf(),
            };
            result.push((summary, runtime));
        } else {
            let Some(url) = entry.get("url").and_then(Value::as_str) else {
                skip("url must be a string".to_string(), warnings);
                continue;
            };
            if url.trim().is_empty() || !valid_http_url(url) {
                skip(
                    "url must use https or point to localhost".to_string(),
                    warnings,
                );
                continue;
            }
            let headers = match parse_string_map(entry.get("headers"), "HTTP headers") {
                Ok(values) => values,
                Err(error) => {
                    skip(error, warnings);
                    continue;
                }
            };
            let summary = AgentPluginMcpServerSummary {
                id: id.clone(),
                name: original_name.clone(),
                original_name: original_name.clone(),
                transport: "streamable-http".to_string(),
                command: None,
                args: Vec::new(),
                cwd: None,
                url: Some(url.to_string()),
                env_keys: Vec::new(),
                header_keys: headers.keys().cloned().collect(),
                plugin_id: plugin_id.to_string(),
                readonly: true,
            };
            let runtime = AgentPluginRuntimeMcpServer {
                id: id.clone(),
                name: id,
                original_name: original_name.clone(),
                transport: "streamable-http".to_string(),
                command: None,
                args: Vec::new(),
                cwd: None,
                url: Some(url.to_string()),
                env: HashMap::new(),
                headers,
                plugin_id: plugin_id.to_string(),
                plugin_root: root.to_path_buf(),
                plugin_data: data_dir.to_path_buf(),
            };
            result.push((summary, runtime));
        }
    }
    result.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    result
}

fn read_state_with_root(root: &Path, plugin_id: &str) -> PluginState {
    let path = state_path_with_root(root, plugin_id);
    if !is_real_file(&path) {
        return PluginState {
            plugin_id: plugin_id.to_string(),
            ..PluginState::default()
        };
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<PluginState>(&content).ok())
        .filter(|state| state.plugin_id.is_empty() || state.plugin_id == plugin_id)
        .map(|mut state| {
            state.plugin_id = plugin_id.to_string();
            state
        })
        .unwrap_or_else(|| PluginState {
            plugin_id: plugin_id.to_string(),
            ..PluginState::default()
        })
}

fn write_state_with_root(root: &Path, state: &PluginState) -> Result<(), String> {
    let plugin_id = validate_plugin_id(&state.plugin_id)?;
    let dir = agent_plugin_catalog_dir_with_root(root);
    profile_bindings::ensure_managed_directory(&dir, "Agent Plugin catalog directory")?;
    let content = serde_json::to_string_pretty(state)
        .map_err(|error| format!("Failed to serialize Agent Plugin state: {error}"))?;
    profile_bindings::write_managed_file(
        &dir.join(format!("{plugin_id}.json")),
        format!("{content}\n"),
        "Agent Plugin state",
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BindingFile {
    #[serde(default)]
    bindings: Vec<AgentPluginBinding>,
}

fn read_bindings_with_root(root: &Path) -> Vec<AgentPluginBinding> {
    let path = binding_path_with_root(root);
    if !is_real_file(&path) {
        return Vec::new();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| {
            serde_json::from_str::<BindingFile>(&content)
                .ok()
                .map(|file| file.bindings)
        })
        .unwrap_or_default()
}

fn write_bindings_with_root(root: &Path, bindings: &[AgentPluginBinding]) -> Result<(), String> {
    let path = binding_path_with_root(root);
    let file = BindingFile {
        bindings: bindings.to_vec(),
    };
    let content = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize Agent Plugin bindings: {error}"))?;
    profile_bindings::write_managed_file(&path, format!("{content}\n"), "Agent Plugin bindings")
}

fn is_plugin_enabled_with_root(root: &Path, plugin_id: &str) -> bool {
    if let Some(enabled) = profile_bindings::global_capability_override_with_root(
        root,
        profile_bindings::CAPABILITY_KIND_AGENT_PLUGIN,
        plugin_id,
    ) {
        return enabled;
    }
    read_bindings_with_root(root)
        .into_iter()
        .find(|binding| binding.plugin_id.eq_ignore_ascii_case(plugin_id))
        .map(|binding| binding.enabled)
        .unwrap_or(false)
}

fn load_plugin_with_root(root: &Path, package_dir: &Path, state: &PluginState) -> LoadedPlugin {
    let fallback_id = package_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("invalid-plugin")
        .to_string();
    let mut summary = AgentPluginSummary {
        id: fallback_id.clone(),
        name: fallback_id.clone(),
        version: String::new(),
        description: String::new(),
        author: None,
        homepage: None,
        repository: None,
        license: None,
        source: state.source.clone(),
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        package_format: default_agent_plugin_format(),
        expert_kind: None,
        display_name: None,
        profession: None,
        members: Vec::new(),
        warnings: Vec::new(),
        error: None,
        trusted: state.trusted,
        enabled: is_plugin_enabled_with_root(root, &fallback_id),
        can_update: state
            .source
            .as_ref()
            .is_some_and(|source| source.kind == "github"),
        can_uninstall: true,
    };
    if !is_real_dir(package_dir) {
        summary.error = Some("Plugin package directory is not a real directory".to_string());
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    }
    let Some(manifest_path) = package_manifest_path(package_dir) else {
        summary.error =
            Some("Package is missing plugin.json or .codebuddy-plugin/plugin.json".to_string());
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    };
    let mut warnings = Vec::new();
    let manifest = match parse_manifest(&manifest_path, &mut warnings) {
        Ok(manifest) => manifest,
        Err(error) => {
            summary.error = Some(error);
            summary.warnings = warnings;
            return LoadedPlugin {
                summary,
                runtime_skills: Vec::new(),
                runtime_mcp_servers: Vec::new(),
            };
        }
    };
    if manifest.name != fallback_id {
        summary.error = Some(format!(
            "plugin.json name '{}' does not match installed directory '{}'",
            manifest.name, fallback_id
        ));
        summary.warnings = warnings;
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    }
    let data_dir = agent_plugin_data_path_with_root(root, &manifest.name);
    if let Err(error) =
        profile_bindings::ensure_managed_directory(&data_dir, "Agent Plugin data directory")
    {
        summary.error = Some(error);
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    }
    let mut skills = discover_skills(package_dir, &manifest.name, &mut warnings);
    if let Some(workbuddy) = &manifest.workbuddy {
        if let Some(expert_skill) =
            discover_workbuddy_expert_skill(package_dir, &manifest.name, workbuddy, &mut warnings)
        {
            skills.push(expert_skill);
        }
    }
    let mcp_servers = discover_mcp_servers(package_dir, &manifest.name, &data_dir, &mut warnings);
    summary.name = manifest.name.clone();
    summary.version = manifest.version;
    summary.description = manifest.description;
    summary.author = manifest.author;
    summary.homepage = manifest.homepage;
    summary.repository = manifest.repository;
    summary.license = manifest.license;
    summary.skills = skills.iter().map(|item| item.0.clone()).collect();
    summary.mcp_servers = mcp_servers.iter().map(|item| item.0.clone()).collect();
    summary.package_format = manifest.package_format.clone();
    if let Some(workbuddy) = &manifest.workbuddy {
        summary.expert_kind = Some(workbuddy.expert_kind.clone());
        summary.display_name = Some(if workbuddy.display_name.is_empty() {
            manifest.name.clone()
        } else {
            workbuddy.display_name.clone()
        });
        summary.profession =
            (!workbuddy.profession.is_empty()).then(|| workbuddy.profession.clone());
        summary.members = workbuddy_members(package_dir, workbuddy);
    }
    summary.warnings = warnings;
    summary.enabled = is_plugin_enabled_with_root(root, &summary.id);
    LoadedPlugin {
        summary,
        runtime_skills: skills.into_iter().map(|item| item.1).collect(),
        runtime_mcp_servers: mcp_servers.into_iter().map(|item| item.1).collect(),
    }
}

fn list_loaded_with_root(root: &Path) -> Vec<LoadedPlugin> {
    let packages = agent_plugin_packages_dir_with_root(root);
    if !is_real_dir(&packages) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&packages) else {
        return Vec::new();
    };
    let mut plugins = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !is_real_dir(&path) || !is_valid_plugin_name(&name) {
                return None;
            }
            let state = read_state_with_root(root, &name);
            Some(load_plugin_with_root(root, &path, &state))
        })
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| left.summary.name.cmp(&right.summary.name));
    plugins
}

pub fn list_agent_plugins_with_root(root: &Path) -> Vec<AgentPluginSummary> {
    list_loaded_with_root(root)
        .into_iter()
        .map(|plugin| plugin.summary)
        .collect()
}

pub fn list_agent_plugins() -> Vec<AgentPluginSummary> {
    list_agent_plugins_with_root(&storage::data_dir())
}

pub fn get_agent_plugin_bindings_with_root(root: &Path) -> Result<Vec<AgentPluginBinding>, String> {
    Ok(read_bindings_with_root(root))
}

pub fn get_agent_plugin_bindings() -> Result<Vec<AgentPluginBinding>, String> {
    get_agent_plugin_bindings_with_root(&storage::data_dir())
}

pub fn list_enabled_skills_with_root(root: &Path) -> Vec<AgentPluginRuntimeSkill> {
    list_loaded_with_root(root)
        .into_iter()
        .filter(|plugin| {
            plugin.summary.error.is_none() && plugin.summary.trusted && plugin.summary.enabled
        })
        .flat_map(|plugin| plugin.runtime_skills)
        .collect()
}

pub fn list_enabled_general_skills_with_root(root: &Path) -> Vec<AgentPluginRuntimeSkill> {
    list_loaded_with_root(root)
        .into_iter()
        .filter(|plugin| {
            plugin.summary.error.is_none()
                && plugin.summary.trusted
                && plugin.summary.enabled
                && plugin.summary.expert_kind.is_none()
        })
        .flat_map(|plugin| plugin.runtime_skills)
        .filter(|skill| !skill.id.contains("--expert--") && !skill.id.contains("--expert-team--"))
        .collect()
}

pub fn list_enabled_mcp_servers_with_root(root: &Path) -> Vec<AgentPluginRuntimeMcpServer> {
    list_loaded_with_root(root)
        .into_iter()
        .filter(|plugin| {
            plugin.summary.error.is_none() && plugin.summary.trusted && plugin.summary.enabled
        })
        .flat_map(|plugin| plugin.runtime_mcp_servers)
        .collect()
}

fn is_github_source(source: &str) -> bool {
    Url::parse(source)
        .ok()
        .is_some_and(|url| url.scheme() == "https" && url.host_str() == Some("github.com"))
}

fn parse_github_source(source: &str) -> Result<(String, Option<String>), String> {
    let url = Url::parse(source.trim()).map_err(|error| format!("Invalid GitHub URL: {error}"))?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err("Agent Plugin source must be an https://github.com URL".to_string());
    }
    let segments = url
        .path_segments()
        .ok_or_else(|| "GitHub URL has no repository path".to_string())?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return Err("GitHub source must be owner/repository or a tree subdirectory".to_string());
    }
    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return Err("GitHub source must include owner and repository".to_string());
    }
    let tree = if segments.len() == 2 {
        None
    } else if segments.len() >= 4 && segments[2] == "tree" {
        let branch = segments[3];
        if branch.is_empty() {
            return Err("GitHub tree URL is missing branch".to_string());
        }
        let subpath = segments[4..].join("/");
        Some(if subpath.is_empty() {
            branch.to_string()
        } else {
            format!("{branch}:{subpath}")
        })
    } else {
        return Err("GitHub source must use /tree/<branch>/<subdirectory>".to_string());
    };
    Ok((format!("https://github.com/{owner}/{repo}.git"), tree))
}

fn clone_github_source(source: &str) -> Result<(PathBuf, PathBuf), String> {
    let (git_url, tree) = parse_github_source(source)?;
    let temp = std::env::temp_dir().join(format!("agentcabin-plugin-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp)
        .map_err(|error| format!("Could not create plugin temp directory: {error}"))?;
    let mut clone = Command::new("git");
    clone
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--filter=blob:none")
        .arg("--sparse");
    if let Some(tree) = tree.as_ref() {
        if let Some((branch, _)) = tree.split_once(':') {
            clone.arg("--branch").arg(branch);
        } else {
            clone.arg("--branch").arg(tree);
        }
    }
    let output = clone
        .arg(&git_url)
        .arg(&temp)
        .output()
        .map_err(|error| format!("git clone could not start: {error}"))?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&temp);
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let source_dir = if let Some(tree) = tree {
        let (_, subpath) = tree.split_once(':').unwrap_or((tree.as_str(), ""));
        if !subpath.is_empty() {
            let sparse = Command::new("git")
                .arg("-C")
                .arg(&temp)
                .arg("sparse-checkout")
                .arg("set")
                .arg(subpath)
                .output()
                .map_err(|error| format!("git sparse checkout could not start: {error}"))?;
            if !sparse.status.success() {
                let _ = fs::remove_dir_all(&temp);
                return Err(format!(
                    "git sparse checkout failed: {}",
                    String::from_utf8_lossy(&sparse.stderr).trim()
                ));
            }
            temp.join(subpath)
        } else {
            temp.clone()
        }
    } else {
        let disable = Command::new("git")
            .arg("-C")
            .arg(&temp)
            .arg("sparse-checkout")
            .arg("disable")
            .output()
            .map_err(|error| format!("git sparse checkout could not start: {error}"))?;
        if !disable.status.success() {
            let _ = fs::remove_dir_all(&temp);
            return Err(format!(
                "git sparse checkout failed: {}",
                String::from_utf8_lossy(&disable.stderr).trim()
            ));
        }
        temp.clone()
    };
    Ok((temp, source_dir))
}

fn copy_tree(from: &Path, to: &Path, counters: &mut (usize, u64)) -> Result<(), String> {
    let metadata = fs::symlink_metadata(from).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Plugin package contains a symlink: {}",
            from.display()
        ));
    }
    if metadata.is_dir() {
        profile_bindings::ensure_managed_directory(to, "Agent Plugin staging directory")?;
        for entry in fs::read_dir(from).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" || name == "node_modules" {
                continue;
            }
            copy_tree(&entry.path(), &to.join(name), counters)?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(format!(
            "Plugin package contains unsupported file: {}",
            from.display()
        ));
    }
    if metadata.len() > MAX_PACKAGE_BYTES
        || counters.1.saturating_add(metadata.len()) > MAX_PACKAGE_BYTES
    {
        return Err("Agent Plugin package exceeds the size limit".to_string());
    }
    counters.0 = counters.0.saturating_add(1);
    counters.1 = counters.1.saturating_add(metadata.len());
    if counters.0 > MAX_PACKAGE_FILES {
        return Err("Agent Plugin package contains too many files".to_string());
    }
    fs::copy(from, to).map_err(|error| format!("Could not copy {}: {error}", from.display()))?;
    Ok(())
}

fn activate_plugin_from_dir(
    root: &Path,
    source_dir: &Path,
    source: AgentPluginSource,
) -> Result<AgentPluginSummary, String> {
    if !is_real_dir(source_dir) {
        return Err("Agent Plugin source is not a real directory".to_string());
    }
    let manifest_path = package_manifest_path(source_dir).ok_or_else(|| {
        "Package source is missing plugin.json or .codebuddy-plugin/plugin.json".to_string()
    })?;
    if !contained_in(source_dir, &manifest_path) {
        return Err("Package manifest is outside its source directory".into());
    }
    let mut warnings = Vec::new();
    let manifest = parse_manifest(&manifest_path, &mut warnings)?;
    let plugin_id = validate_plugin_id(&manifest.name)?;
    let packages = agent_plugin_packages_dir_with_root(root);
    profile_bindings::ensure_managed_directory(&packages, "Agent Plugin packages directory")?;
    let data_path = agent_plugin_data_path_with_root(root, &plugin_id);
    profile_bindings::ensure_managed_directory(&data_path, "Agent Plugin data directory")?;
    let staging = packages.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let mut counters = (0usize, 0u64);
    if let Err(error) = copy_tree(source_dir, &staging, &mut counters) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let mut staged_warnings = Vec::new();
    let staged_manifest_path = package_manifest_path(&staging)
        .ok_or_else(|| "Staged package manifest is missing".to_string())?;
    let staged_manifest = match parse_manifest(&staged_manifest_path, &mut staged_warnings) {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    if staged_manifest.name != plugin_id {
        let _ = fs::remove_dir_all(&staging);
        return Err("Staged Agent Plugin manifest name changed during installation".to_string());
    }
    if let Some(workbuddy) = &staged_manifest.workbuddy {
        if let Err(error) = materialize_workbuddy_expert_skill(&staging, &plugin_id, workbuddy) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = materialize_workbuddy_mcp(&staging, workbuddy) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }

    let destination = packages.join(&plugin_id);
    let backup = packages.join(format!(".backup-{}", uuid::Uuid::new_v4()));
    let previous_state_path = state_path_with_root(root, &plugin_id);
    let previous_state_bytes = fs::read(&previous_state_path).ok();
    let previous_state = read_state_with_root(root, &plugin_id);
    if destination.exists() {
        fs::rename(&destination, &backup)
            .map_err(|error| format!("Could not stage existing Agent Plugin: {error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("Could not activate Agent Plugin: {error}"));
    }

    let state = PluginState {
        plugin_id: plugin_id.clone(),
        source: Some(source),
        trusted: previous_state.trusted,
        installed_at: Utc::now().to_rfc3339(),
    };
    if let Err(error) = write_state_with_root(root, &state) {
        let _ = fs::remove_dir_all(&destination);
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        match previous_state_bytes {
            Some(bytes) => {
                let _ = profile_bindings::write_managed_file(
                    &previous_state_path,
                    bytes,
                    "Agent Plugin state rollback",
                );
            }
            None => {
                let _ = fs::remove_file(&previous_state_path);
            }
        }
        return Err(error);
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    sync_runtime_configs_with_root(root)?;
    let state = read_state_with_root(root, &plugin_id);
    Ok(load_plugin_with_root(root, &destination, &state).summary)
}

pub fn install_agent_plugin_with_root(
    root: &Path,
    source: &str,
) -> Result<AgentPluginSummary, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("Agent Plugin source cannot be empty".to_string());
    }
    profile_bindings::ensure_managed_directory(
        &agent_plugins_root_with_root(root),
        "Agent Plugin root",
    )?;
    if is_github_source(source) {
        let (temp, source_dir) = clone_github_source(source)?;
        let result = activate_plugin_from_dir(
            root,
            &source_dir,
            AgentPluginSource {
                kind: "github".to_string(),
                location: source.to_string(),
                installed_at: Utc::now().to_rfc3339(),
            },
        );
        let _ = fs::remove_dir_all(temp);
        result
    } else {
        let source_path = fs::canonicalize(source)
            .map_err(|error| format!("Could not resolve local Agent Plugin source: {error}"))?;
        activate_plugin_from_dir(
            root,
            &source_path,
            AgentPluginSource {
                kind: "local".to_string(),
                location: source_path.display().to_string(),
                installed_at: Utc::now().to_rfc3339(),
            },
        )
    }
}

pub fn install_agent_plugin(source: &str) -> Result<AgentPluginSummary, String> {
    install_agent_plugin_with_root(&storage::data_dir(), source)
}

pub fn update_agent_plugin_with_root(
    root: &Path,
    plugin_id: &str,
) -> Result<AgentPluginSummary, String> {
    let plugin_id = validate_plugin_id(plugin_id)?;
    let state = read_state_with_root(root, &plugin_id);
    let source = state
        .source
        .as_ref()
        .ok_or_else(|| format!("Agent Plugin '{plugin_id}' has no recorded source"))?
        .location
        .clone();
    install_agent_plugin_with_root(root, &source)
}

pub fn update_agent_plugin(plugin_id: &str) -> Result<AgentPluginSummary, String> {
    update_agent_plugin_with_root(&storage::data_dir(), plugin_id)
}

fn remove_binding_with_root(root: &Path, plugin_id: &str) -> Result<(), String> {
    let mut bindings = read_bindings_with_root(root);
    bindings.retain(|binding| !binding.plugin_id.eq_ignore_ascii_case(plugin_id));
    let path = binding_path_with_root(root);
    if bindings.is_empty() {
        if is_real_file(&path) {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    write_bindings_with_root(root, &bindings)
}

pub fn uninstall_agent_plugin_with_root(root: &Path, plugin_id: &str) -> Result<(), String> {
    let plugin_id = validate_plugin_id(plugin_id)?;
    let package = package_path_with_root(root, &plugin_id);
    if !is_real_dir(&package) {
        return Err(format!("Agent Plugin '{plugin_id}' is not installed"));
    }
    fs::remove_dir_all(&package)
        .map_err(|error| format!("Could not uninstall Agent Plugin '{plugin_id}': {error}"))?;
    let state_path = state_path_with_root(root, &plugin_id);
    if is_real_file(&state_path) {
        fs::remove_file(state_path).map_err(|error| error.to_string())?;
    }
    remove_binding_with_root(root, &plugin_id)?;
    profile_bindings::remove_global_capability_binding_with_root(
        root,
        profile_bindings::CAPABILITY_KIND_AGENT_PLUGIN,
        &plugin_id,
    )?;
    sync_runtime_configs_with_root(root)
}

pub fn uninstall_agent_plugin(plugin_id: &str) -> Result<(), String> {
    uninstall_agent_plugin_with_root(&storage::data_dir(), plugin_id)
}

pub fn set_agent_plugin_trust_with_root(
    root: &Path,
    plugin_id: &str,
    trusted: bool,
) -> Result<AgentPluginSummary, String> {
    let plugin_id = validate_plugin_id(plugin_id)?;
    let package = package_path_with_root(root, &plugin_id);
    if !is_real_dir(&package) {
        return Err(format!("Agent Plugin '{plugin_id}' is not installed"));
    }
    let mut state = read_state_with_root(root, &plugin_id);
    state.plugin_id = plugin_id.clone();
    state.trusted = trusted;
    if !trusted {
        profile_bindings::set_global_capability_binding_with_root(
            root,
            profile_bindings::CAPABILITY_KIND_AGENT_PLUGIN,
            &plugin_id,
            false,
        )?;
        let mut bindings = read_bindings_with_root(root);
        for binding in &mut bindings {
            if binding.plugin_id.eq_ignore_ascii_case(&plugin_id) {
                binding.enabled = false;
                binding.disabled_by = Some("trust".to_string());
            }
        }
        if !bindings.is_empty() {
            write_bindings_with_root(root, &bindings)?;
        }
    }
    write_state_with_root(root, &state)?;
    sync_runtime_configs_with_root(root)?;
    Ok(load_plugin_with_root(root, &package, &state).summary)
}

pub fn set_agent_plugin_trust(
    plugin_id: &str,
    trusted: bool,
) -> Result<AgentPluginSummary, String> {
    set_agent_plugin_trust_with_root(&storage::data_dir(), plugin_id, trusted)
}

pub fn set_agent_plugin_binding_with_root(
    root: &Path,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let plugin_id = validate_plugin_id(plugin_id)?;
    let package = package_path_with_root(root, &plugin_id);
    if !is_real_dir(&package) {
        return Err(format!("Agent Plugin '{plugin_id}' is not installed"));
    }
    let state = read_state_with_root(root, &plugin_id);
    if enabled && !state.trusted {
        return Err("Trust the Agent Plugin before enabling it".to_string());
    }
    let mut bindings = read_bindings_with_root(root);
    if let Some(binding) = bindings
        .iter_mut()
        .find(|binding| binding.plugin_id.eq_ignore_ascii_case(&plugin_id))
    {
        binding.enabled = enabled;
        binding.disabled_by = (!enabled).then(|| "user".to_string());
    } else {
        bindings.push(AgentPluginBinding {
            plugin_id: plugin_id.clone(),
            enabled,
            disabled_by: (!enabled).then(|| "user".to_string()),
        });
    }
    write_bindings_with_root(root, &bindings)?;
    profile_bindings::set_global_capability_binding_with_root(
        root,
        profile_bindings::CAPABILITY_KIND_AGENT_PLUGIN,
        &plugin_id,
        enabled,
    )?;
    sync_runtime_configs_with_root(root)
}

pub fn set_agent_plugin_binding(plugin_id: &str, enabled: bool) -> Result<(), String> {
    set_agent_plugin_binding_with_root(&storage::data_dir(), plugin_id, enabled)
}

fn server_value(server: &AgentPluginRuntimeMcpServer) -> Value {
    let mut object = Map::new();
    object.insert(
        "_agentcabinPluginRoot".to_string(),
        Value::String(server.plugin_root.display().to_string()),
    );
    object.insert(
        "_agentcabinPluginData".to_string(),
        Value::String(server.plugin_data.display().to_string()),
    );
    object.insert(
        "transport".to_string(),
        Value::String(server.transport.clone()),
    );
    if let Some(command) = &server.command {
        object.insert("command".to_string(), Value::String(command.clone()));
    }
    if !server.args.is_empty() {
        object.insert(
            "args".to_string(),
            Value::Array(server.args.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(cwd) = &server.cwd {
        object.insert("cwd".to_string(), Value::String(cwd.display().to_string()));
    }
    if let Some(url) = &server.url {
        object.insert("url".to_string(), Value::String(url.clone()));
    }
    if !server.env.is_empty() {
        object.insert(
            "env".to_string(),
            Value::Object(
                server
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            ),
        );
    }
    if !server.headers.is_empty() {
        object.insert(
            "headers".to_string(),
            Value::Object(
                server
                    .headers
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            ),
        );
    }
    Value::Object(object)
}

pub fn sync_runtime_configs_with_root(root: &Path) -> Result<(), String> {
    for mode in ["code", "work"] {
        let path = agent_plugin_mcp_config_path_with_root(root, mode)?;
        let servers = list_enabled_mcp_servers_with_root(root);
        if servers.is_empty() {
            if is_real_file(&path) {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
            continue;
        }
        let mut server_map = Map::new();
        for server in servers {
            server_map.insert(server.id.clone(), server_value(&server));
        }
        let value = serde_json::json!({
            "$schema": MCP_SCHEMA,
            "mcpServers": server_map,
        });
        let content = serde_json::to_string_pretty(&value)
            .map_err(|error| format!("Could not serialize Agent Plugin MCP config: {error}"))?;
        profile_bindings::write_managed_file(
            &path,
            format!("{content}\n"),
            "Agent Plugin MCP config",
        )?;
    }
    Ok(())
}

pub fn sync_runtime_configs() -> Result<(), String> {
    sync_runtime_configs_with_root(&storage::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_plugin(root: &Path, manifest: &str, skill: Option<&str>, mcp: Option<&str>) {
        fs::create_dir_all(root.join("skills/demo")).unwrap();
        fs::write(root.join("plugin.json"), manifest).unwrap();
        if let Some(skill) = skill {
            fs::write(root.join("skills/demo/SKILL.md"), skill).unwrap();
        } else {
            fs::remove_dir_all(root.join("skills")).unwrap();
        }
        if let Some(mcp) = mcp {
            fs::write(root.join("mcp.json"), mcp).unwrap();
        }
    }

    #[test]
    fn parses_namespaced_skill_and_mcp_components() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        write_plugin(
            &source,
            &format!(r#"{{"$schema":"{}","name":"demo-plugin"}}"#, PLUGIN_SCHEMA),
            Some("---\nname: Demo\ndescription: A demo\n---\n"),
            Some(&format!(
                r#"{{"$schema":"{}","mcpServers":{{"server":{{"type":"stdio","command":"echo","args":["{}"]}}}}}}"#,
                MCP_SCHEMA, PLUGIN_ROOT_TOKEN
            )),
        );
        let summary =
            install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
        assert_eq!(
            summary.skills[0].id,
            "agent-plugin--demo-plugin--skill--demo"
        );
        assert_eq!(
            summary.mcp_servers[0].id,
            "agent-plugin--demo-plugin--mcp--server"
        );
        assert!(!summary.trusted);
        assert!(!summary.enabled);
    }

    #[test]
    fn invalid_component_does_not_invalidate_sibling_components() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("skills/valid")).unwrap();
        fs::write(
            source.join("plugin.json"),
            format!(r#"{{"$schema":"{}","name":"demo-plugin"}}"#, PLUGIN_SCHEMA),
        )
        .unwrap();
        fs::write(
            source.join("skills/valid/SKILL.md"),
            "---\ndescription: valid\n---\n",
        )
        .unwrap();
        fs::write(
            source.join("mcp.json"),
            format!(
                r#"{{"$schema":"{}","mcpServers":{{"bad":{{"type":"stdio"}}}}}}"#,
                MCP_SCHEMA
            ),
        )
        .unwrap();
        let summary =
            install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
        assert_eq!(summary.skills.len(), 1);
        assert!(summary.mcp_servers.is_empty());
        assert!(!summary.warnings.is_empty());
        assert!(summary.error.is_none());
    }

    #[test]
    fn trust_and_global_binding_gate_runtime_projection() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("skills/demo")).unwrap();
        fs::write(
            source.join("plugin.json"),
            format!(r#"{{"$schema":"{}","name":"demo-plugin"}}"#, PLUGIN_SCHEMA),
        )
        .unwrap();
        fs::write(
            source.join("skills/demo/SKILL.md"),
            "---\ndescription: demo\n---\n",
        )
        .unwrap();
        let summary =
            install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
        assert!(list_enabled_skills_with_root(temp.path()).is_empty());
        set_agent_plugin_trust_with_root(temp.path(), &summary.id, true).unwrap();
        set_agent_plugin_binding_with_root(temp.path(), &summary.id, true).unwrap();
        let skills = list_enabled_skills_with_root(temp.path());
        assert_eq!(skills.len(), 1);

        set_agent_plugin_binding_with_root(temp.path(), &summary.id, false).unwrap();
        assert!(list_enabled_skills_with_root(temp.path()).is_empty());
    }

    #[test]
    fn imports_workbuddy_expert_team_as_selectable_runtime_skill() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("workbuddy-team");
        fs::create_dir_all(source.join(".codebuddy-plugin")).unwrap();
        fs::create_dir_all(source.join("agents")).unwrap();
        fs::create_dir_all(source.join("skills/research")).unwrap();
        fs::write(
            source.join(".codebuddy-plugin/plugin.json"),
            r#"{
              "name":"market-team","version":"1.2.0","expertType":"team",
              "displayName":{"zh":"市场专家团"},"agentName":"lead",
              "agents":["./agents/lead.md","./agents/researcher.md"],
              "teamInfo":{"leadAgent":"lead","memberAgents":["researcher"]},
              "skills":["./skills/research"]
            }"#,
        )
        .unwrap();
        fs::write(
            source.join("agents/lead.md"),
            "---\nname: lead\ndescription: Lead strategist\n---\nLead the final synthesis.\n",
        )
        .unwrap();
        fs::write(
            source.join("agents/researcher.md"),
            "---\nname: researcher\ndescription: Research specialist\n---\nGather evidence.\n",
        )
        .unwrap();
        fs::write(
            source.join("skills/research/SKILL.md"),
            "---\nname: research\ndescription: Research workflow\n---\nUse primary sources.\n",
        )
        .unwrap();

        let summary =
            install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
        assert_eq!(summary.package_format, "workbuddy");
        assert_eq!(summary.expert_kind.as_deref(), Some("expert-team"));
        assert_eq!(summary.display_name.as_deref(), Some("市场专家团"));
        assert_eq!(summary.members.len(), 2);
        assert_eq!(summary.skills.len(), 2);
        assert!(summary
            .skills
            .iter()
            .any(|skill| skill.id.contains("--expert--")));

        // The host keeps the namespaced component id, while DSH receives a
        // valid kebab-case frontmatter name that its skill provider can load.
        let materialized = temp
            .path()
            .join("agent-plugins/packages/market-team/.agentcabin/expert/SKILL.md");
        let materialized_body = fs::read_to_string(materialized).unwrap();
        assert!(materialized_body.contains("name: \"agentcabin-expert-market-team\""));
        assert!(!materialized_body.contains("name: \"agent-plugin--"));
        assert!(materialized_body.contains("current user turn explicitly selects this expert"));

        set_agent_plugin_trust_with_root(temp.path(), &summary.id, true).unwrap();
        set_agent_plugin_binding_with_root(temp.path(), &summary.id, true).unwrap();
        assert_eq!(list_enabled_skills_with_root(temp.path()).len(), 2);
    }

    #[test]
    fn mcp_http_validation_requires_supported_scheme_and_host() {
        assert!(valid_http_url("https://example.com/mcp"));
        assert!(valid_http_url("http://localhost:8787/mcp"));
        assert!(valid_http_url("https://[::1]:8787/mcp"));
        assert!(!valid_http_url("http://example.com/mcp"));
        assert!(!valid_http_url("ftp://localhost/mcp"));
        assert!(!valid_http_url("https://"));
    }
}
