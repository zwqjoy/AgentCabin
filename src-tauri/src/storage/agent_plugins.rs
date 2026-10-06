//! Agent Plugins 1.0.0 storage, validation, and runtime projection.
//!
//! An Agent Plugin is an installed directory containing plugin.json, an
//! optional skills directory, and an optional mcp.json. Claude Code packages
//! import .claude-plugin/plugin.json and .mcp.json into the same Host. Package bytes are
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
const CLAUDE_ROOT_TOKEN: &str = "${CLAUDE_PLUGIN_ROOT}";
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginCompatibility {
    pub level: String,
    pub supported: Vec<String>,
    pub detected_unsupported: Vec<String>,
    pub blocked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPluginComponentSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub support_status: String,
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
    #[serde(default)]
    pub compatibility: PluginCompatibility,
    #[serde(default)]
    pub components: Vec<AgentPluginComponentSummary>,
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
struct NormalizedPluginManifest {
    name: String,
    version: String,
    description: String,
    author: Option<AgentPluginAuthor>,
    homepage: Option<String>,
    repository: Option<String>,
    license: Option<String>,
    package_format: String,
    workbuddy: Option<WorkBuddyManifest>,
    claude: Option<Map<String, Value>>,
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
    let claude = root.join(".claude-plugin/plugin.json");
    if is_real_file(&claude) {
        return Some(claude);
    }
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

fn parse_claude_plugin_manifest(
    object: &Map<String, Value>,
) -> Result<NormalizedPluginManifest, String> {
    let name = json_string(object, "name")?.ok_or("Claude plugin manifest requires name")?;
    if !is_valid_plugin_name(&name) {
        return Err("Claude plugin name is invalid".into());
    }
    if let Some(keywords) = object.get("keywords") {
        parse_string_array(Some(keywords), "keywords")?;
    }
    Ok(NormalizedPluginManifest {
        name,
        version: json_string(object, "version")?.unwrap_or_default(),
        description: json_string(object, "description")?.unwrap_or_default(),
        author: parse_author(object.get("author"))?,
        homepage: json_string(object, "homepage")?,
        repository: json_string(object, "repository")?,
        license: json_string(object, "license")?,
        package_format: "claude-code".into(),
        workbuddy: None,
        claude: Some(object.clone()),
    })
}

fn parse_manifest(
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<NormalizedPluginManifest, String> {
    let value = read_json_file(path, MAX_MANIFEST_BYTES, "plugin.json")?;
    let object = value
        .as_object()
        .ok_or_else(|| "plugin.json top level must be an object".to_string())?;
    if path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == ".claude-plugin")
    {
        return parse_claude_plugin_manifest(object);
    }
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
    Ok(NormalizedPluginManifest {
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
        claude: None,
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
    discover_skills_at(root, &root.join("skills"), plugin_id, warnings)
}

fn discover_skills_at(
    root: &Path,
    skills_dir: &Path,
    plugin_id: &str,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginSkillSummary, AgentPluginRuntimeSkill)> {
    if !skills_dir.exists() {
        return Vec::new();
    }
    if !is_real_dir(skills_dir) || !contained_in(root, skills_dir) {
        warnings
            .push("skills is not a real directory inside the plugin and was ignored".to_string());
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(skills_dir) else {
        warnings.push("skills could not be read and was ignored".to_string());
        return Vec::new();
    };
    let mut result = Vec::new();
    let skill_dirs = if is_real_file(&skills_dir.join("SKILL.md")) {
        vec![skills_dir.to_path_buf()]
    } else {
        entries.flatten().map(|entry| entry.path()).collect()
    };
    for skill_dir in skill_dirs {
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
        .replace(CLAUDE_ROOT_TOKEN, &root.display().to_string())
        .replace(PLUGIN_ROOT_TOKEN, &root.display().to_string())
        .replace(PLUGIN_DATA_TOKEN, &data.display().to_string())
}

fn expand_path(value: &str, root: &Path, data: &Path) -> Option<PathBuf> {
    let path = if let Some(rest) = value
        .strip_prefix(PLUGIN_ROOT_TOKEN)
        .or_else(|| value.strip_prefix(CLAUDE_ROOT_TOKEN))
    {
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

fn safe_root_values(value: &Value, root: &Path) -> bool {
    match value {
        Value::String(value) => {
            let normalized = value.replace(CLAUDE_ROOT_TOKEN, PLUGIN_ROOT_TOKEN);
            if normalized
                .replace(PLUGIN_ROOT_TOKEN, "")
                .replace(PLUGIN_DATA_TOKEN, "")
                .contains("${")
            {
                return false;
            }
            for tail in normalized.split(PLUGIN_ROOT_TOKEN).skip(1) {
                let suffix = tail
                    .split([' ', '\"', '\'', ';', ','])
                    .next()
                    .unwrap_or_default();
                if !suffix.is_empty() && !suffix.starts_with('/') {
                    return false;
                }
                let path = Path::new(suffix.trim_start_matches('/'));
                if path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return false;
                }
                let mut candidate = root.join(path);
                while !candidate.exists() {
                    if !candidate.pop() {
                        return false;
                    }
                }
                if !contained_in(root, &candidate) {
                    return false;
                }
            }
            true
        }
        Value::Array(values) => values.iter().all(|v| safe_root_values(v, root)),
        Value::Object(values) => values.values().all(|v| safe_root_values(v, root)),
        _ => true,
    }
}

fn mask_env_placeholders(value: &str) -> Option<String> {
    let mut masked = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        masked.push_str(&rest[..start]);
        let placeholder = &rest[start + 2..];
        let end = placeholder.find('}')?;
        let name = &placeholder[..end];
        if name == "CLAUDE_PLUGIN_ROOT" || name == "PLUGIN_ROOT" || name == "PLUGIN_DATA" {
            masked.push_str("${");
            masked.push_str(name);
            masked.push('}');
        } else {
            let mut chars = name.chars();
            let valid = chars
                .next()
                .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                && chars.all(|c| c == '_' || c.is_ascii_alphanumeric());
            if !valid {
                return None;
            }
            // Keep the value safe for path validation without resolving the
            // Host-owned variable during package import.
            masked.push_str("HOST_ENV_PLACEHOLDER");
        }
        rest = &placeholder[end + 1..];
    }
    masked.push_str(rest);
    Some(masked)
}

fn safe_claude_secret_map(value: Option<&Value>, root: &Path) -> bool {
    let Some(value) = value else {
        return true;
    };
    let Some(values) = value.as_object() else {
        return false;
    };
    values.values().all(|value| {
        let Some(value) = value.as_str() else {
            return false;
        };
        mask_env_placeholders(value)
            .is_some_and(|masked| safe_root_values(&Value::String(masked), root))
    })
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
    discover_mcp_servers_at(
        root,
        &root.join("mcp.json"),
        plugin_id,
        data_dir,
        false,
        warnings,
    )
}

fn discover_mcp_servers_at(
    root: &Path,
    path: &Path,
    plugin_id: &str,
    data_dir: &Path,
    claude: bool,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginMcpServerSummary, AgentPluginRuntimeMcpServer)> {
    if !path.exists() {
        return Vec::new();
    }
    if !is_real_file(path) || !contained_in(root, path) {
        warnings.push("mcp.json is not a real in-package file and was ignored".to_string());
        return Vec::new();
    }
    let value = match read_json_file(path, MAX_MCP_BYTES, "mcp.json") {
        Ok(value) => value,
        Err(error) => {
            warnings.push(format!("{error}; MCP components were ignored"));
            return Vec::new();
        }
    };
    normalize_mcp_servers(root, plugin_id, data_dir, claude, &value, warnings)
}

fn normalize_mcp_servers(
    root: &Path,
    plugin_id: &str,
    data_dir: &Path,
    claude: bool,
    value: &Value,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginMcpServerSummary, AgentPluginRuntimeMcpServer)> {
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            warnings.push(
                "mcp.json top level must be an object; MCP components were ignored".to_string(),
            );
            return Vec::new();
        }
    };
    if !claude && object.get("$schema").and_then(Value::as_str) != Some(MCP_SCHEMA) {
        warnings.push(format!(
            "mcp.json must declare $schema {MCP_SCHEMA}; MCP components were ignored"
        ));
        return Vec::new();
    }
    let servers = if let Some(wrapped) = object.get("mcpServers") {
        let Some(servers) = wrapped.as_object() else {
            warnings.push(
                "mcp.json must contain an mcpServers object; MCP components were ignored"
                    .to_string(),
            );
            return Vec::new();
        };
        for key in object.keys() {
            if key != "$schema" && key != "mcpServers" {
                warnings.push(format!(
                    "mcp.json unknown top-level field '{key}'; MCP components were ignored"
                ));
                return Vec::new();
            }
        }
        servers
    } else if claude {
        // Claude Code's native .mcp.json shape maps server names directly at
        // the top level. Keep $schema metadata out of that server map.
        let servers = object
            .iter()
            .filter(|(key, _)| key.as_str() != "$schema")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>();
        // The owned map lives through normalization below.
        return normalize_mcp_server_map(root, plugin_id, data_dir, claude, &servers, warnings);
    } else {
        warnings.push(
            "mcp.json must contain an mcpServers object; MCP components were ignored".to_string(),
        );
        return Vec::new();
    };

    normalize_mcp_server_map(root, plugin_id, data_dir, claude, servers, warnings)
}

fn normalize_mcp_server_map(
    root: &Path,
    plugin_id: &str,
    data_dir: &Path,
    claude: bool,
    servers: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Vec<(AgentPluginMcpServerSummary, AgentPluginRuntimeMcpServer)> {
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
        if entry.get("type").is_some_and(|value| !value.is_string()) {
            skip("type must be a string".into(), warnings);
            continue;
        }
        let Some(transport) = entry
            .get("type")
            .and_then(Value::as_str)
            .map(|t| {
                if claude && t == "http" {
                    "streamable-http"
                } else {
                    t
                }
            })
            .or_else(|| {
                claude.then_some(if entry.contains_key("command") {
                    "stdio"
                } else {
                    "streamable-http"
                })
            })
        else {
            skip("is missing type".to_string(), warnings);
            continue;
        };
        let allowed: &[&str] = match transport {
            "stdio" => &["type", "command", "args", "env", "cwd"],
            "streamable-http" | "sse" => &["type", "url", "headers"],
            _ => {
                skip("uses unsupported transport".to_string(), warnings);
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

        let mut non_secret_fields = entry.clone();
        non_secret_fields.remove("env");
        non_secret_fields.remove("headers");
        if claude
            && (!safe_root_values(&Value::Object(non_secret_fields), root)
                || !safe_claude_secret_map(entry.get("env"), root)
                || !safe_claude_secret_map(entry.get("headers"), root))
        {
            skip(
                "contains a plugin-root path escape or invalid variable placeholder".into(),
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
            if raw_command.starts_with("./")
                || raw_command.starts_with(CLAUDE_ROOT_TOKEN)
                || raw_command.starts_with(PLUGIN_ROOT_TOKEN)
            {
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
                key.eq_ignore_ascii_case("PLUGIN_ROOT")
                    || key.eq_ignore_ascii_case("PLUGIN_DATA")
                    || key.eq_ignore_ascii_case("CLAUDE_PLUGIN_ROOT")
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
            let headers: HashMap<String, String> =
                match parse_string_map(entry.get("headers"), "HTTP headers") {
                    Ok(values) => values
                        .iter()
                        .map(|(key, value)| (key.clone(), expand_value(value, root, data_dir)))
                        .collect(),
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

// Claude declarations only select in-package files; execution remains in the Host.
fn claude_paths(
    root: &Path,
    manifest: &Map<String, Value>,
    key: &str,
    defaults: &[&str],
    blocked: &mut Vec<String>,
) -> Vec<PathBuf> {
    let mut raw: Vec<String> = defaults.iter().map(|p| p.to_string()).collect();
    if let Some(value) = manifest.get(key) {
        match value {
            Value::String(path) => raw.push(path.clone()),
            Value::Array(values) => {
                for value in values {
                    if let Some(path) = value.as_str() {
                        raw.push(path.into());
                    } else {
                        blocked.push(format!("{key}: component paths must be strings"));
                    }
                }
            }
            Value::Object(_)
                if matches!(key, "hooks" | "mcpServers" | "lspServers" | "commands") => {}
            _ => blocked.push(format!("{key}: unsupported component declaration")),
        }
    }
    let mut paths = Vec::new();
    for raw in raw {
        let path = Path::new(&raw);
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            blocked.push(format!(
                "{key}: path must be relative and cannot contain .."
            ));
            continue;
        }
        let path = root.join(path);
        if !path.exists() {
            if !defaults.contains(&raw.as_str()) {
                blocked.push(format!("{key}: component path is missing"));
            }
            continue;
        }
        if !contained_in(root, &path)
            || fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
        {
            blocked.push(format!(
                "{key}: component path escapes package or is a symlink"
            ));
            continue;
        }
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    paths
}

fn detect_components(
    root: &Path,
    path: &Path,
    plugin: &str,
    kind: &str,
    result: &mut Vec<AgentPluginComponentSummary>,
) {
    if !contained_in(root, path) {
        return;
    }
    if is_real_dir(path) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                detect_components(root, &entry.path(), plugin, kind, result);
            }
        }
    } else if is_real_file(path) && (kind == "hooks" || path.extension().is_some_and(|e| e == "md"))
    {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        if result.iter().any(|c| c.path == relative && c.kind == kind) {
            return;
        }
        result.push(AgentPluginComponentSummary {
            id: component_id(plugin, kind, &relative),
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            kind: kind.into(),
            path: relative,
            support_status: "detected".into(),
        });
    }
}

struct ImportedComponents {
    skills: Vec<(AgentPluginSkillSummary, AgentPluginRuntimeSkill)>,
    mcp_servers: Vec<(AgentPluginMcpServerSummary, AgentPluginRuntimeMcpServer)>,
    components: Vec<AgentPluginComponentSummary>,
    compatibility: PluginCompatibility,
    warnings: Vec<String>,
}

// Package importer only: no Claude CLI, execution runtime, or independent state.
struct ClaudePluginAdapter;
impl ClaudePluginAdapter {
    fn discover(
        package_dir: &Path,
        plugin_id: &str,
        data_dir: &Path,
        claude: &Map<String, Value>,
    ) -> ImportedComponents {
        let mut skills = Vec::new();
        let mut mcp_servers = Vec::new();
        let mut components = Vec::new();
        let mut warnings = Vec::new();
        let mut compatibility = PluginCompatibility::default();
        let mut blocked = Vec::new();

        for path in claude_paths(package_dir, claude, "skills", &["skills"], &mut blocked) {
            skills.extend(discover_skills_at(
                package_dir,
                &path,
                plugin_id,
                &mut blocked,
            ));
        }
        skills.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        skills.dedup_by(|a, b| a.0.id == b.0.id);
        for path in claude_paths(
            package_dir,
            claude,
            "mcpServers",
            &[".mcp.json"],
            &mut blocked,
        ) {
            mcp_servers.extend(discover_mcp_servers_at(
                package_dir,
                &path,
                plugin_id,
                data_dir,
                true,
                &mut blocked,
            ));
        }
        if let Some(Value::Object(servers)) = claude.get("mcpServers") {
            mcp_servers.extend(normalize_mcp_servers(
                package_dir,
                plugin_id,
                data_dir,
                true,
                &serde_json::json!({"mcpServers": servers}),
                &mut blocked,
            ));
        }
        let mut seen = std::collections::HashSet::new();
        mcp_servers.retain(|server| {
            if seen.insert(server.0.id.clone()) {
                true
            } else {
                blocked.push(format!(
                    "MCP '{}': duplicate server identity was ignored",
                    server.0.original_name
                ));
                false
            }
        });
        for (kind, defaults) in [
            ("commands", vec!["commands"]),
            ("agents", vec!["agents"]),
            ("hooks", vec!["hooks/hooks.json", "hooks.json"]),
        ] {
            for path in claude_paths(package_dir, claude, kind, &defaults, &mut blocked) {
                detect_components(package_dir, &path, plugin_id, kind, &mut components);
            }
            if claude.contains_key(kind) {
                compatibility.detected_unsupported.push(kind.into());
            }
        }
        for (kind, defaults) in [
            ("lspServers", vec![".lsp.json"]),
            ("outputStyles", vec!["output-styles"]),
            ("workflows", vec!["workflows"]),
        ] {
            let paths = claude_paths(package_dir, claude, kind, &defaults, &mut blocked);
            if !paths.is_empty() || claude.contains_key(kind) {
                compatibility.detected_unsupported.push(kind.into());
            }
        }
        for key in claude.keys() {
            if matches!(key.as_str(), "lspServers" | "outputStyles" | "workflows") {
                compatibility.detected_unsupported.push(key.clone());
            } else if !matches!(
                key.as_str(),
                "name"
                    | "version"
                    | "description"
                    | "author"
                    | "homepage"
                    | "repository"
                    | "license"
                    | "keywords"
                    | "skills"
                    | "commands"
                    | "agents"
                    | "hooks"
                    | "mcpServers"
            ) {
                warnings.push(format!("Claude manifest unknown field '{key}' was ignored"));
                if !matches!(
                    key.as_str(),
                    "$schema"
                        | "metadata"
                        | "displayName"
                        | "icon"
                        | "documentationUrl"
                        | "supportUrl"
                        | "privacyPolicyUrl"
                        | "termsOfServiceUrl"
                ) {
                    compatibility.detected_unsupported.push(key.clone());
                }
            }
        }
        compatibility
            .detected_unsupported
            .extend(components.iter().map(|c| c.kind.clone()));
        compatibility.detected_unsupported.sort();
        compatibility.detected_unsupported.dedup();
        compatibility.blocked = blocked;

        ImportedComponents {
            skills,
            mcp_servers,
            components,
            compatibility,
            warnings,
        }
    }
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
        compatibility: PluginCompatibility {
            level: "unsupported".into(),
            ..Default::default()
        },
        components: Vec::new(),
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
            Some("Package is missing plugin.json, .claude-plugin/plugin.json, or .codebuddy-plugin/plugin.json".to_string());
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    };
    if manifest_path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == ".claude-plugin")
    {
        summary.package_format = "claude-code".into();
    }
    if !contained_in(package_dir, &manifest_path) {
        summary.error = Some("Package manifest is outside its installed directory".into());
        return LoadedPlugin {
            summary,
            runtime_skills: Vec::new(),
            runtime_mcp_servers: Vec::new(),
        };
    }
    let mut warnings = Vec::new();
    let count = [
        ".claude-plugin/plugin.json",
        ".codebuddy-plugin/plugin.json",
        "plugin.json",
    ]
    .iter()
    .filter(|p| is_real_file(&package_dir.join(p)))
    .count();
    if count > 1 {
        warnings.push(format!(
            "Multiple plugin manifests detected; using {}.",
            manifest_path
                .strip_prefix(package_dir)
                .unwrap_or(&manifest_path)
                .display()
        ));
    }
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
    let mut skills = if manifest.claude.is_some() {
        Vec::new()
    } else {
        discover_skills(package_dir, &manifest.name, &mut warnings)
    };
    if let Some(workbuddy) = &manifest.workbuddy {
        if let Some(expert_skill) =
            discover_workbuddy_expert_skill(package_dir, &manifest.name, workbuddy, &mut warnings)
        {
            skills.push(expert_skill);
        }
    }
    let mcp_servers;
    let mut compatibility = PluginCompatibility::default();
    if let Some(claude) = &manifest.claude {
        let imported =
            ClaudePluginAdapter::discover(package_dir, &manifest.name, &data_dir, claude);
        skills = imported.skills;
        mcp_servers = imported.mcp_servers;
        summary.components = imported.components;
        compatibility = imported.compatibility;
        warnings.extend(imported.warnings);
    } else {
        mcp_servers = discover_mcp_servers(package_dir, &manifest.name, &data_dir, &mut warnings);
    }
    if !skills.is_empty() {
        compatibility.supported.push("Skills".into());
    }
    if !mcp_servers.is_empty() {
        compatibility.supported.push("MCP".into());
    }
    compatibility.level = if compatibility.supported.is_empty() {
        "unsupported"
    } else if !compatibility.detected_unsupported.is_empty() || !compatibility.blocked.is_empty() {
        "partial"
    } else {
        "full"
    }
    .into();
    summary.compatibility = compatibility;
    summary.name = manifest.name.clone();
    summary.version = manifest.version;
    summary.description = manifest.description;
    summary.author = manifest.author;
    summary.homepage = manifest.homepage;
    summary.repository = manifest.repository;
    summary.license = manifest.license;
    summary.skills = skills.iter().map(|item| item.0.clone()).collect();
    summary.mcp_servers = mcp_servers
        .iter()
        .map(|item| {
            let mut summary = item.0.clone();
            if manifest.claude.is_some() {
                summary.command = None;
                summary.args.clear();
                summary.cwd = None;
                summary.url = None;
            }
            summary
        })
        .collect();
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

/// Resolve an explicitly selected expert independently of global plugin enablement.
/// Installation/trust remain host-owned; selection is scoped to a single conversation.
pub fn selected_expert_resources_with_root(
    root: &Path,
    plugin_id: &str,
) -> Result<
    (
        AgentPluginSummary,
        Vec<AgentPluginRuntimeSkill>,
        Vec<AgentPluginRuntimeMcpServer>,
    ),
    String,
> {
    let plugin = list_loaded_with_root(root)
        .into_iter()
        .find(|p| p.summary.id == plugin_id)
        .ok_or_else(|| format!("专家未安装: {plugin_id}"))?;
    if plugin.summary.expert_kind.is_none()
        || !plugin.summary.trusted
        || plugin.summary.error.is_some()
    {
        return Err(format!("专家不可用或尚未信任: {plugin_id}"));
    }
    Ok((
        plugin.summary,
        plugin.runtime_skills,
        plugin.runtime_mcp_servers,
    ))
}

/// Resolve a selected expert's server on the Host, independently of the
/// global enable switch. Selection, installation and trust are checked afresh.
pub(crate) fn selected_expert_mcp_config_with_root(
    root: &Path,
    run_id: &str,
    server_id: &str,
) -> Result<Option<Value>, String> {
    let Some(expert) = crate::storage::session_experts::get_with_root(root, run_id)? else {
        return Ok(None);
    };
    let (_, _, servers) = selected_expert_resources_with_root(root, &expert.id)?;
    Ok(servers
        .iter()
        .find(|server| server.id == server_id)
        .map(server_value))
}

/// Load the actual WorkBuddy Agent instructions and explicitly preloaded Skills.
/// Skill resources/scripts remain on disk and are read only when needed.
pub fn selected_expert_prompt_with_root(root: &Path, plugin_id: &str) -> Result<String, String> {
    selected_expert_agent_prompt_with_root(root, plugin_id, None)
}

pub fn selected_expert_member_prompts_with_root(
    root: &Path,
    plugin_id: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let package = package_path_with_root(root, plugin_id);
    let mut warnings = Vec::new();
    let manifest = parse_manifest(
        &package_manifest_path(&package).ok_or("专家配置缺失")?,
        &mut warnings,
    )?;
    let workbuddy = manifest
        .workbuddy
        .as_ref()
        .ok_or("专家不是 WorkBuddy 格式")?;
    let mut members = std::collections::BTreeMap::new();
    if workbuddy.expert_kind == "expert-team" {
        for member in &workbuddy.member_agents {
            members.insert(
                member.clone(),
                selected_expert_agent_prompt_with_root(root, plugin_id, Some(member))?,
            );
        }
    }
    Ok(members)
}

fn selected_expert_agent_prompt_with_root(
    root: &Path,
    plugin_id: &str,
    member: Option<&str>,
) -> Result<String, String> {
    let (summary, skills, _) = selected_expert_resources_with_root(root, plugin_id)?;
    let package = package_path_with_root(root, plugin_id);
    let manifest_path = package_manifest_path(&package).ok_or("专家配置缺失")?;
    let mut warnings = Vec::new();
    let raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if let Some(connectors) = raw
        .pointer("/dependencies/connectors")
        .and_then(Value::as_array)
    {
        let catalog = profile_bindings::read_connector_catalog_with_root(root);
        let packages = crate::work::connector_package_manager::list_with_paths(
            &crate::work::paths::WorkPaths::new(root.to_path_buf()),
        )?;
        for id in connectors.iter().filter_map(Value::as_str) {
            if !catalog.iter().any(|connector| connector.id == id)
                || !profile_bindings::is_connector_enabled_with_root(root, id)
            {
                return Err(format!(
                    "专家需要连接器 {id}，请先在能力中心完成连接并启用。"
                ));
            }
            if let Some(connector) = packages
                .iter()
                .find(|connector| connector.manifest.id == id)
            {
                use crate::work::connector_package::{ConnectorAuthKind, ConnectorAuthStatus};
                let needs_auth = !matches!(
                    connector.manifest.auth.kind,
                    ConnectorAuthKind::None | ConnectorAuthKind::Cli
                );
                if !connector.state.trusted
                    || !connector.state.enabled
                    || (needs_auth
                        && connector.state.auth_status != ConnectorAuthStatus::Authenticated)
                {
                    return Err(format!(
                        "专家依赖的连接器 {id} 尚未完成授权，请先在能力中心完成连接。"
                    ));
                }
            }
        }
    }
    let manifest = parse_manifest(&manifest_path, &mut warnings)?;
    let workbuddy = manifest
        .workbuddy
        .as_ref()
        .ok_or("专家不是 WorkBuddy 格式")?;
    let mut agents = workbuddy
        .agent_paths
        .iter()
        .flat_map(|raw| workbuddy_agent_files(&package, raw))
        .collect::<Vec<_>>();
    let target = member.unwrap_or(&workbuddy.lead_agent);
    if member.is_some() && !workbuddy.member_agents.iter().any(|id| id == target) {
        return Err("该 Agent 不属于所选专家团".into());
    }
    agents.retain(|file| workbuddy_agent_id(file) == target);
    if agents.is_empty() {
        return Err(format!("专家 Agent 未找到: {target}"));
    }
    let mut prompt = format!(
        "## 当前会话专家: {}\n当前角色配置取代历史轮次的专家配置。\n",
        summary.display_name.as_deref().unwrap_or(&summary.name)
    );
    let mut preload_names = std::collections::HashSet::new();
    for (index, file) in agents.iter().enumerate() {
        if !is_real_file(file) || !contained_in(&package, file) {
            return Err("专家 Agent 文件不在安装包内".into());
        }
        let content = fs::read_to_string(file).map_err(|e| e.to_string())?;
        if content.len() > MAX_SKILL_BYTES as usize {
            return Err("专家 Agent 定义过大".into());
        }
        let (metadata, body) = split_agent_frontmatter(&content)?;
        if let Some(names) = metadata
            .get("skills")
            .and_then(serde_yaml::Value::as_sequence)
        {
            for name in names.iter().filter_map(serde_yaml::Value::as_str) {
                preload_names.insert(name.to_string());
            }
        }
        if index == 0 {
            prompt.push_str(body);
        }
    }
    if member.is_none() && workbuddy.expert_kind == "expert-team" {
        prompt.push_str(&format!("\n可委派的专家团成员: {}。通过 AgentTool 工具将任务交给指定成员；工具会加载该成员的角色和预加载技能，执行真实的独立 Pi 会话。可以并行委派独立任务。主理人负责汇总实际返回的结果，不能把角色模拟当作已执行的成员任务。\n", workbuddy.member_agents.join(", ")));
    }
    for name in preload_names {
        let package_skill = skills.iter().find(|skill| {
            skill.name == name
                || summary
                    .skills
                    .iter()
                    .any(|s| s.id == skill.id && s.name == name)
        });
        let (skill_dir, boundary) = if let Some(skill) = package_skill {
            (skill.path.clone(), package.clone())
        } else {
            let shared = crate::storage::skills::list_skills_with_root(root)
                .into_iter()
                .find(|s| s.name == name && s.enabled)
                .ok_or_else(|| format!("专家声明的预加载技能未找到或未启用: {name}"))?;
            (PathBuf::from(shared.path), root.join("skills"))
        };
        let skill_md = skill_dir.join("SKILL.md");
        if !is_real_file(&skill_md) || !contained_in(&boundary, &skill_md) {
            return Err("预加载技能不在可信技能目录内".into());
        }
        let content = fs::read_to_string(&skill_md).map_err(|e| e.to_string())?;
        if content.len() > MAX_SKILL_BYTES as usize {
            return Err("预加载技能过大".into());
        }
        prompt.push_str(&format!(
            "\n\n### 预加载技能: {name}\n技能目录: {}\n{}\n",
            skill_dir.display(),
            content
        ));
    }
    Ok(prompt)
}

fn split_agent_frontmatter(content: &str) -> Result<(serde_yaml::Value, &str), String> {
    let normalized = content.trim_start_matches('\u{feff}');
    if !normalized.starts_with("---\n") && !normalized.starts_with("---\r\n") {
        return Ok((serde_yaml::Value::Null, normalized));
    }
    let first_newline = normalized.find('\n').unwrap();
    let remainder = &normalized[first_newline + 1..];
    let end = remainder.find("\n---").ok_or("Agent frontmatter 未闭合")?;
    let yaml = serde_yaml::from_str(&remainder[..end])
        .map_err(|e| format!("Agent frontmatter 无效: {e}"))?;
    let body_start = remainder[end + 1..]
        .find('\n')
        .map(|n| end + 2 + n)
        .unwrap_or(remainder.len());
    Ok((yaml, &remainder[body_start..]))
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
        "Package source is missing plugin.json, .claude-plugin/plugin.json, or .codebuddy-plugin/plugin.json".to_string()
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

    #[test]
    fn acceptance_plugin_skill_and_mcp_share_trust_and_binding_lifecycle() {
        let temp = TempDir::new().unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/acceptance/agent-plugin-basic");

        let installed = install_agent_plugin_with_root(temp.path(), fixture.to_str().unwrap())
            .expect("install local acceptance fixture");
        assert_eq!(installed.name, "acceptance-plugin");
        assert_eq!(
            installed.skills.len(),
            1,
            "plugin summary includes its Skill"
        );
        assert_eq!(
            installed.mcp_servers.len(),
            1,
            "plugin summary includes its MCP"
        );
        assert!(!installed.trusted && !installed.enabled);
        assert!(list_agent_plugins_with_root(temp.path())
            .iter()
            .any(|plugin| plugin.id == installed.id));
        assert!(list_enabled_general_skills_with_root(temp.path()).is_empty());
        assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());

        assert!(set_agent_plugin_binding_with_root(temp.path(), &installed.id, true).is_err());
        set_agent_plugin_trust_with_root(temp.path(), &installed.id, true).unwrap();
        assert!(list_enabled_general_skills_with_root(temp.path()).is_empty());
        assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());

        set_agent_plugin_binding_with_root(temp.path(), &installed.id, true).unwrap();
        assert_eq!(list_enabled_general_skills_with_root(temp.path()).len(), 1);
        assert_eq!(list_enabled_mcp_servers_with_root(temp.path()).len(), 1);

        set_agent_plugin_binding_with_root(temp.path(), &installed.id, false).unwrap();
        assert!(list_enabled_general_skills_with_root(temp.path()).is_empty());
        assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());

        set_agent_plugin_binding_with_root(temp.path(), &installed.id, true).unwrap();
        assert_eq!(list_enabled_general_skills_with_root(temp.path()).len(), 1);
        assert_eq!(list_enabled_mcp_servers_with_root(temp.path()).len(), 1);

        uninstall_agent_plugin_with_root(temp.path(), &installed.id).unwrap();
        assert!(list_agent_plugins_with_root(temp.path()).is_empty());
        assert!(list_enabled_general_skills_with_root(temp.path()).is_empty());
        assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());
    }

    #[test]
    fn claude_offline_smoke_matrix() {
        for (name, level, skills, mcp) in [
            ("frontend-design", "full", 1, 0),
            ("skill-creator", "full", 1, 0),
            ("github", "full", 1, 2),
            ("playwright", "full", 0, 1),
            ("feature-dev", "partial", 1, 0),
            ("agents-only", "unsupported", 0, 0),
        ] {
            let temp = TempDir::new().unwrap();
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/claude-plugins")
                .join(name);
            let summary =
                install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
            assert_eq!(summary.package_format, "claude-code");
            assert_eq!(summary.compatibility.level, level, "{name}");
            assert_eq!(summary.skills.len(), skills);
            assert_eq!(summary.mcp_servers.len(), mcp);
            assert!(!summary.trusted && !summary.enabled);
            assert!(!serde_json::to_string(&summary)
                .unwrap()
                .contains("fixture-secret"));
            assert!(list_enabled_skills_with_root(temp.path()).is_empty());
            assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());
            assert!(set_agent_plugin_binding_with_root(temp.path(), name, true).is_err());
            set_agent_plugin_trust_with_root(temp.path(), name, true).unwrap();
            assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());
            set_agent_plugin_binding_with_root(temp.path(), name, true).unwrap();
            assert_eq!(list_enabled_skills_with_root(temp.path()).len(), skills);
            assert_eq!(list_enabled_mcp_servers_with_root(temp.path()).len(), mcp);
            let installed = package_path_with_root(temp.path(), name);
            assert_eq!(
                fs::read(source.join(".claude-plugin/plugin.json")).unwrap(),
                fs::read(installed.join(".claude-plugin/plugin.json")).unwrap()
            );
            if mcp > 0 {
                assert_eq!(
                    fs::read(source.join(".mcp.json")).unwrap(),
                    fs::read(installed.join(".mcp.json")).unwrap()
                );
            }
            set_agent_plugin_trust_with_root(temp.path(), name, false).unwrap();
            assert!(list_enabled_skills_with_root(temp.path()).is_empty());
            assert!(list_enabled_mcp_servers_with_root(temp.path()).is_empty());
        }
    }

    #[test]
    fn claude_env_placeholders_are_preserved_only_in_secret_maps() {
        let temp = TempDir::new().unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/claude-plugins/github");
        let summary =
            install_agent_plugin_with_root(temp.path(), source.to_str().unwrap()).unwrap();
        assert_eq!(summary.mcp_servers.len(), 2);
        let summary_json = serde_json::to_string(&summary).unwrap();
        assert!(!summary_json.contains("GITHUB_PERSONAL_ACCESS_TOKEN"));
        set_agent_plugin_trust_with_root(temp.path(), "github", true).unwrap();
        set_agent_plugin_binding_with_root(temp.path(), "github", true).unwrap();
        let server = list_enabled_mcp_servers_with_root(temp.path())
            .into_iter()
            .find(|server| server.original_name == "github")
            .unwrap();
        assert_eq!(
            server.headers.get("Authorization").map(String::as_str),
            Some("Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}")
        );
        let config = fs::read_to_string(
            agent_plugin_mcp_config_path_with_root(temp.path(), "work").unwrap(),
        )
        .unwrap();
        assert!(config.contains("${GITHUB_PERSONAL_ACCESS_TOKEN}"));
        assert!(!config.contains("fixture-secret"));

        let package = temp.path().join("unsafe");
        fs::create_dir_all(package.join(".claude-plugin")).unwrap();
        fs::write(
            package.join(".claude-plugin/plugin.json"),
            r#"{"name":"unsafe"}"#,
        )
        .unwrap();
        fs::write(
            package.join(".mcp.json"),
            r#"{"bad":{"command":"${GITHUB_PERSONAL_ACCESS_TOKEN}"},"good":{"command":"node","env":{"TOKEN":"${GITHUB_PERSONAL_ACCESS_TOKEN}"}}}"#,
        )
        .unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert_eq!(loaded.runtime_mcp_servers.len(), 1);
        assert_eq!(loaded.runtime_mcp_servers[0].original_name, "good");
    }

    #[test]
    fn claude_custom_paths_diagnostics_and_root_alias() {
        let temp = TempDir::new().unwrap();
        let package = temp.path().join("demo");
        fs::create_dir_all(package.join(".claude-plugin")).unwrap();
        fs::create_dir_all(package.join("custom-skills/demo")).unwrap();
        fs::create_dir_all(package.join("custom-commands")).unwrap();
        fs::create_dir_all(package.join("bin")).unwrap();
        fs::write(package.join("bin/server"), "fixture").unwrap();
        fs::write(
            package.join("custom-skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: fixture\n---\n${CLAUDE_PLUGIN_ROOT}",
        )
        .unwrap();
        fs::write(package.join("custom-commands/test.md"), "test").unwrap();
        fs::write(package.join(".claude-plugin/plugin.json"), r#"{"name":"demo","skills":"./custom-skills","commands":"./custom-commands","agents":"../../outside","lspServers":{}}"#).unwrap();
        fs::write(package.join("plugin.json"), "{}").unwrap();
        fs::write(package.join(".mcp.json"), r#"{"mcpServers":{"local":{"command":"${CLAUDE_PLUGIN_ROOT}/bin/server","args":["${CLAUDE_PLUGIN_ROOT}/bin/server"]}}}"#).unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert_eq!(loaded.summary.skills.len(), 1);
        assert_eq!(loaded.summary.components.len(), 1);
        assert_eq!(loaded.summary.compatibility.level, "partial");
        assert!(!loaded.summary.compatibility.blocked.is_empty());
        assert!(loaded
            .summary
            .warnings
            .iter()
            .any(|w| w.contains("Multiple plugin manifests")));
        assert_eq!(
            loaded.runtime_mcp_servers[0].command.as_deref(),
            package.join("bin/server").to_str()
        );
        assert!(
            fs::read_to_string(package.join("custom-skills/demo/SKILL.md"))
                .unwrap()
                .contains(CLAUDE_ROOT_TOKEN)
        );
        for path in [
            "../../outside",
            "/tmp",
            "${CLAUDE_PLUGIN_ROOT}/../../outside",
        ] {
            fs::write(
                package.join(".mcp.json"),
                serde_json::json!({"mcpServers":{"bad":{"command":path}}}).to_string(),
            )
            .unwrap();
            let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
            if path.starts_with(CLAUDE_ROOT_TOKEN) {
                assert!(loaded.runtime_mcp_servers.is_empty());
            }
        }
        fs::write(package.join(".mcp.json"), "{").unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert!(loaded.runtime_mcp_servers.is_empty());
        assert!(!loaded.summary.compatibility.blocked.is_empty());
        fs::write(package.join(".claude-plugin/plugin.json"), "{}").unwrap();
        assert!(
            load_plugin_with_root(temp.path(), &package, &PluginState::default())
                .summary
                .error
                .is_some()
        );
    }

    #[test]
    fn claude_inline_mcp_and_paths_fail_closed() {
        let temp = TempDir::new().unwrap();
        let package = temp.path().join("demo");
        fs::create_dir_all(package.join(".claude-plugin")).unwrap();
        fs::create_dir_all(package.join("skills/demo")).unwrap();
        fs::write(
            package.join("skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: test\n---\n",
        )
        .unwrap();
        fs::write(package.join(".claude-plugin/plugin.json"), r#"{"name":"demo","mcpServers":{"inline":{"command":"node","env":{"TOKEN":"secret-inline"}}},"hooks":{}}"#).unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert_eq!(loaded.runtime_mcp_servers.len(), 1);
        assert_eq!(loaded.summary.compatibility.level, "partial");
        assert!(!serde_json::to_string(&loaded.summary)
            .unwrap()
            .contains("secret-inline"));
        for raw in ["../outside", "/tmp"] {
            let mut blocked = Vec::new();
            let manifest = serde_json::json!({"commands":raw});
            assert!(claude_paths(
                &package,
                manifest.as_object().unwrap(),
                "commands",
                &[],
                &mut blocked
            )
            .is_empty());
            assert!(!blocked.is_empty());
        }
        for value in [
            "${CLAUDE_PLUGIN_ROOT}/../outside",
            "${CLAUDE_PLUGIN_ROOT}/../../outside",
            "${CLAUDE_PLUGIN_ROOT}suffix/file",
        ] {
            assert!(!safe_root_values(&Value::String(value.into()), &package));
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), package.join("escape")).unwrap();
            assert!(!safe_root_values(
                &Value::String("${CLAUDE_PLUGIN_ROOT}/escape/missing".into()),
                &package
            ));
            let mut blocked = Vec::new();
            let manifest = serde_json::json!({"agents":"./escape"});
            assert!(claude_paths(
                &package,
                manifest.as_object().unwrap(),
                "agents",
                &[],
                &mut blocked
            )
            .is_empty());
            assert!(!blocked.is_empty());
            let dest = temp.path().join("install");
            assert!(install_agent_plugin_with_root(&dest, package.to_str().unwrap()).is_err());
            let external_manifest = temp.path().join("external-manifest");
            fs::create_dir_all(&external_manifest).unwrap();
            fs::write(external_manifest.join("plugin.json"), r#"{"name":"demo"}"#).unwrap();
            fs::remove_dir_all(package.join(".claude-plugin")).unwrap();
            std::os::unix::fs::symlink(&external_manifest, package.join(".claude-plugin")).unwrap();
            let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
            assert!(loaded.summary.error.is_some());
            assert!(loaded.runtime_skills.is_empty() && loaded.runtime_mcp_servers.is_empty());
        }
    }

    #[test]
    fn claude_custom_mcp_paths_and_invalid_siblings() {
        let temp = TempDir::new().unwrap();
        let package = temp.path().join("demo");
        fs::create_dir_all(package.join(".claude-plugin")).unwrap();
        fs::create_dir_all(package.join("config")).unwrap();
        fs::write(
            package.join(".claude-plugin/plugin.json"),
            r#"{"name":"demo","mcpServers":"./config/mcp.json","metadata":{"future":true}}"#,
        )
        .unwrap();
        fs::write(package.join("config/mcp.json"), r#"{"mcpServers":{"good":{"type":"http","url":"https://example.com/mcp","headers":{"Authorization":"host-secret"}},"bad":{"command":"node","type":42},"unknown":{"command":"node","futureRuntime":true}}}"#).unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert_eq!(loaded.runtime_mcp_servers.len(), 1);
        assert_eq!(loaded.runtime_mcp_servers[0].transport, "streamable-http");
        assert_eq!(loaded.summary.compatibility.level, "partial");
        assert_eq!(loaded.summary.compatibility.blocked.len(), 2);
        assert!(!serde_json::to_string(&loaded.summary)
            .unwrap()
            .contains("host-secret"));
        fs::write(
            package.join("config/mcp.json"),
            r#"{"mcpServers":{"good":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
        let loaded = load_plugin_with_root(temp.path(), &package, &PluginState::default());
        assert_eq!(loaded.summary.compatibility.level, "full");
        assert!(loaded.summary.compatibility.detected_unsupported.is_empty());
    }

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
