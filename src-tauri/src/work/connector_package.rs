//! Generic Connector Package contracts.
//!
//! A package is a product-level connector. MCP, CLI, and Skill are optional
//! runtime projections of the same package. Provider-specific code such as
//! Composio belongs behind the auth/provider boundary and must not leak into
//! this manifest.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::work::apps::models::ConnectionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorRuntimeKind {
    Mcp,
    Cli,
    Skill,
}

/// Well-known lifecycle operations exposed by a Connector Package CLI. A
/// Package may also declare a validated operation name for domain actions;
/// this enum only supplies the default risk/confirmation semantics for the
/// lifecycle names and never becomes an arbitrary command launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectorCliOperationKind {
    Init,
    VersionCheck,
    Auth,
    Status,
    UnAuth,
}

impl ConnectorCliOperationKind {
    pub const ALL: [Self; 5] = [
        Self::Init,
        Self::VersionCheck,
        Self::Auth,
        Self::Status,
        Self::UnAuth,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::VersionCheck => "versionCheck",
            Self::Auth => "auth",
            Self::Status => "status",
            Self::UnAuth => "unAuth",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "init" => Some(Self::Init),
            "versioncheck" | "version_check" | "version-check" => Some(Self::VersionCheck),
            "auth" | "login" => Some(Self::Auth),
            "status" | "whoami" => Some(Self::Status),
            "unauth" | "un_auth" | "un-auth" | "logout" => Some(Self::UnAuth),
            _ => None,
        }
    }

    pub fn default_requires_confirmation(self) -> bool {
        matches!(self, Self::Init | Self::Auth | Self::UnAuth)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCliOperationSpec {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub requires_confirmation: Option<bool>,
}

/// Validated, non-secret CLI projection. This type is safe to serialize into
/// Work Profile because it contains no credential or provider material.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCliDefinition {
    pub package_id: String,
    pub command: String,
    #[serde(default)]
    pub base_args: Vec<String>,
    pub operations: BTreeMap<String, ConnectorCliOperationSpec>,
    #[serde(default)]
    pub redaction_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCliInvocation {
    pub package_id: String,
    pub operation: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub timeout_seconds: u64,
    pub requires_confirmation: bool,
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub redaction_fields: Vec<String>,
    /// Relative directories under the user's HOME that this CLI is allowed to
    /// read and update while running in the Work sandbox.  The Host resolves
    /// these paths; the model never supplies absolute paths.
    #[serde(default)]
    pub home_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorPackageOrigin {
    #[default]
    Builtin,
    Community,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAuthKind {
    #[default]
    None,
    #[serde(rename = "oauth2", alias = "o_auth2")]
    OAuth2,
    ApiKey,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorAuthSpec {
    #[serde(default)]
    pub kind: ConnectorAuthKind,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorAuthField {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPackagePermissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub readable_areas: Vec<String>,
    #[serde(default)]
    pub writable_areas: Vec<String>,
    /// Connector-owned configuration/credential directories relative to the
    /// user's HOME.  These are mounted only for the Connector CLI process.
    #[serde(default)]
    pub home_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPackageManifest {
    pub id: String,
    pub version: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub origin: ConnectorPackageOrigin,
    pub runtimes: Vec<ConnectorRuntimeKind>,
    #[serde(default)]
    pub auth: ConnectorAuthSpec,
    #[serde(default)]
    pub auth_fields: Vec<ConnectorAuthField>,
    #[serde(default)]
    pub mcp_file: Option<String>,
    #[serde(default)]
    pub cli_file: Option<String>,
    #[serde(default)]
    pub skill_dirs: Vec<String>,
    #[serde(default)]
    pub permissions: ConnectorPackagePermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAuthStatus {
    #[default]
    NotRequired,
    NotAuthenticated,
    Pending,
    Authenticated,
    Expired,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorRuntimeStatus {
    #[default]
    NotReady,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPackageState {
    pub package_id: String,
    pub version: String,
    pub installed: bool,
    pub trusted: bool,
    pub enabled: bool,
    #[serde(default)]
    pub auth_status: ConnectorAuthStatus,
    #[serde(default)]
    pub runtime_status: ConnectorRuntimeStatus,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Safe frontend/runtime projection of an installed package. It deliberately
/// contains no absolute filesystem paths and no credential material.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPackageSummary {
    pub manifest: ConnectorPackageManifest,
    pub state: ConnectorPackageState,
}

/// Product-level catalog projection for built-in Connector Packages. A
/// catalog item is available without a local package directory: the bundled
/// Provider owns its runtime bridge, while the Package lifecycle fields keep
/// the UI contract ready for physical manifest migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCatalogItem {
    pub package_id: String,
    pub display_name: String,
    pub description: String,
    pub icon: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub runtimes: Vec<ConnectorRuntimeKind>,
    pub auth: ConnectorAuthSpec,
    pub origin: ConnectorPackageOrigin,
    pub builtin: bool,
    pub installed: bool,
    pub trusted: bool,
    pub enabled: bool,
    pub auth_status: ConnectorAuthStatus,
    pub connection_status: ConnectionStatus,
    pub account_count: usize,
    #[serde(default)]
    pub documentation_url: Option<String>,
}

impl ConnectorPackageState {
    pub fn for_manifest(manifest: &ConnectorPackageManifest) -> Self {
        let auth_status = if manifest.auth.kind == ConnectorAuthKind::None {
            ConnectorAuthStatus::NotRequired
        } else {
            ConnectorAuthStatus::NotAuthenticated
        };
        Self {
            package_id: manifest.id.clone(),
            version: manifest.version.clone(),
            installed: false,
            trusted: false,
            enabled: false,
            auth_status,
            runtime_status: ConnectorRuntimeStatus::NotReady,
            last_error: None,
        }
    }
}

pub fn validate_manifest(manifest: &ConnectorPackageManifest) -> Result<(), String> {
    validate_package_id(&manifest.id)?;
    if manifest.version.trim().is_empty() {
        return Err("Connector Package version cannot be empty".into());
    }
    if manifest.display_name.trim().is_empty() {
        return Err("Connector Package display name cannot be empty".into());
    }
    if manifest.runtimes.is_empty() {
        return Err("Connector Package must declare at least one runtime".into());
    }

    let mut runtimes = BTreeSet::new();
    for runtime in &manifest.runtimes {
        if !runtimes.insert(*runtime as u8) {
            return Err("Connector Package runtimes cannot contain duplicates".into());
        }
    }

    if manifest.runtimes.contains(&ConnectorRuntimeKind::Mcp) {
        validate_relative_entry(
            manifest
                .mcp_file
                .as_deref()
                .ok_or("MCP Connector Package must declare mcpFile")?,
            "mcpFile",
        )?;
    }
    if manifest.runtimes.contains(&ConnectorRuntimeKind::Cli) {
        validate_relative_entry(
            manifest
                .cli_file
                .as_deref()
                .ok_or("CLI Connector Package must declare cliFile")?,
            "cliFile",
        )?;
    }
    if manifest.runtimes.contains(&ConnectorRuntimeKind::Skill) && manifest.skill_dirs.is_empty() {
        return Err("Skill Connector Package must declare at least one skill directory".into());
    }

    if let Some(icon) = manifest.icon.as_deref() {
        validate_relative_entry(icon, "icon")?;
    }
    for skill_dir in &manifest.skill_dirs {
        // WorkBuddy connector packages may put a single SKILL.md at the
        // package root and declare it as `skills: ["SKILL.md"]`. The
        // compatibility loader normalizes that safe package-root directory
        // to `.`.
        if skill_dir != "." {
            validate_relative_entry(skill_dir, "skillDirs")?;
        }
    }
    for home_path in &manifest.permissions.home_paths {
        validate_home_path(home_path)?;
    }
    Ok(())
}

fn validate_home_path(path: &str) -> Result<(), String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path == Path::new(".")
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(
            "Connector Package permissions.homePaths must be non-empty relative directories".into(),
        );
    }

    let normalized = path.to_string_lossy().replace('\\', "/");
    let sensitive = [
        ".ssh",
        ".gnupg",
        ".aws",
        ".config/gcloud",
        ".config/gh",
        "Library/Keychains",
        ".netrc",
        ".bash_history",
        ".zsh_history",
        ".python_history",
        ".node_repl_history",
    ];
    if sensitive
        .iter()
        .any(|blocked| normalized == *blocked || normalized.starts_with(&format!("{blocked}/")))
    {
        return Err(format!(
            "Connector Package permissions.homePaths cannot expose sensitive path '{normalized}'"
        ));
    }
    Ok(())
}

pub fn validate_package_files(
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<(), String> {
    validate_manifest(manifest)?;
    if !root.is_dir() {
        return Err(format!(
            "Connector Package root is not a directory: {}",
            root.display()
        ));
    }

    if let Some(entry) = manifest.mcp_file.as_deref() {
        require_regular_file(root, entry, "mcpFile")?;
    }
    if let Some(entry) = manifest.cli_file.as_deref() {
        require_regular_file(root, entry, "cliFile")?;
    }
    if let Some(entry) = manifest.icon.as_deref() {
        require_regular_file(root, entry, "icon")?;
    }
    for entry in &manifest.skill_dirs {
        require_directory(root, entry, "skillDirs")?;
    }
    Ok(())
}

pub fn validate_package_id(id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed != id {
        return Err("Connector Package id cannot contain surrounding whitespace".into());
    }
    let id = trimmed;
    if id.is_empty() || id.len() > 64 || id == "." || id == ".." {
        return Err("Connector Package id must contain 1-64 characters".into());
    }
    if !id.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || ".-_".contains(character)
    }) {
        return Err(
            "Connector Package id must use lowercase letters, digits, dot, dash, or underscore"
                .into(),
        );
    }
    Ok(())
}

fn validate_relative_entry(entry: &str, field: &str) -> Result<(), String> {
    let path = Path::new(entry.trim());
    if entry.trim().is_empty() || path.is_absolute() {
        return Err(format!("Connector Package {field} must be a relative path"));
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(format!(
                "Connector Package {field} cannot escape its package root"
            ));
        }
    }
    Ok(())
}

fn package_entry(root: &Path, entry: &str, field: &str) -> Result<PathBuf, String> {
    validate_relative_entry(entry, field)?;
    Ok(root.join(entry))
}

fn require_regular_file(root: &Path, entry: &str, field: &str) -> Result<(), String> {
    let path = package_entry(root, entry, field)?;
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("Connector Package {field} is missing: {error}"))?;
    if !metadata.file_type().is_file() {
        return Err(format!("Connector Package {field} must be a regular file"));
    }
    Ok(())
}

fn require_directory(root: &Path, entry: &str, field: &str) -> Result<(), String> {
    let path = if entry == "." {
        root.to_path_buf()
    } else {
        package_entry(root, entry, field)?
    };
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("Connector Package {field} is missing: {error}"))?;
    if !metadata.file_type().is_dir() {
        return Err(format!("Connector Package {field} must be a directory"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn mcp_manifest() -> ConnectorPackageManifest {
        ConnectorPackageManifest {
            id: "agentkey".into(),
            version: "1.0.0".into(),
            display_name: "AgentKey".into(),
            description: "Remote MCP connector".into(),
            icon: Some("icon.svg".into()),
            origin: ConnectorPackageOrigin::Builtin,
            runtimes: vec![ConnectorRuntimeKind::Mcp],
            auth: ConnectorAuthSpec {
                kind: ConnectorAuthKind::OAuth2,
                provider: Some("mcp_oauth".into()),
            },
            auth_fields: Vec::new(),
            mcp_file: Some("mcp.json".into()),
            cli_file: None,
            skill_dirs: Vec::new(),
            permissions: ConnectorPackagePermissions::default(),
        }
    }

    #[test]
    fn validates_mcp_manifest_and_files() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("mcp.json"), "{}").unwrap();
        std::fs::write(temp.path().join("icon.svg"), "<svg />").unwrap();
        let manifest = mcp_manifest();

        validate_package_files(temp.path(), &manifest).unwrap();
        let state = ConnectorPackageState::for_manifest(&manifest);
        assert_eq!(state.auth_status, ConnectorAuthStatus::NotAuthenticated);
        assert!(!state.installed);
    }

    #[test]
    fn rejects_runtime_without_entrypoint() {
        let mut manifest = mcp_manifest();
        manifest.mcp_file = None;
        let error = validate_manifest(&manifest).unwrap_err();
        assert!(error.contains("mcpFile"));
    }

    #[test]
    fn rejects_path_traversal_and_invalid_ids() {
        let mut manifest = mcp_manifest();
        manifest.id = "../gmail".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.id = "..".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.id = " gmail".into();
        assert!(validate_manifest(&manifest).is_err());

        manifest.id = "gmail".into();
        manifest.mcp_file = Some("../mcp.json".into());
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn validates_connector_home_paths_without_allowing_sensitive_roots() {
        let mut manifest = mcp_manifest();
        manifest.permissions.home_paths = vec![
            ".config/example-cli".into(),
            "Library/Application Support/example-cli".into(),
        ];
        validate_manifest(&manifest).unwrap();

        for invalid in [
            "",
            ".",
            "../outside",
            "/absolute/path",
            ".ssh",
            "Library/Keychains",
        ] {
            manifest.permissions.home_paths = vec![invalid.into()];
            assert!(
                validate_manifest(&manifest).is_err(),
                "home path should be rejected: {invalid}"
            );
        }
    }

    #[test]
    fn supports_cli_and_skill_package_state() {
        let manifest = ConnectorPackageManifest {
            id: "dingtalk".into(),
            version: "1.0.0".into(),
            display_name: "钉钉".into(),
            description: String::new(),
            icon: None,
            origin: ConnectorPackageOrigin::Community,
            runtimes: vec![ConnectorRuntimeKind::Cli, ConnectorRuntimeKind::Skill],
            auth: ConnectorAuthSpec {
                kind: ConnectorAuthKind::Cli,
                provider: None,
            },
            auth_fields: Vec::new(),
            mcp_file: None,
            cli_file: Some("cli.json".into()),
            skill_dirs: vec!["skills/dingtalk".into()],
            permissions: ConnectorPackagePermissions::default(),
        };
        let state = ConnectorPackageState::for_manifest(&manifest);
        assert_eq!(state.auth_status, ConnectorAuthStatus::NotAuthenticated);
        validate_manifest(&manifest).unwrap();
    }
}
