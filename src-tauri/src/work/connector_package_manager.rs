//! Host-side lifecycle for Connector Packages.
//!
//! This module owns package discovery and lifecycle state only. It does not
//! execute package code. Runtime adapters consume an enabled, trusted package
//! in a later phase, which keeps installation and execution as separate trust
//! boundaries.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;
use uuid::Uuid;

use crate::work::apps::models::ConnectionStatus;
use crate::work::cli_runtime;
use crate::work::connector_package::{
    validate_manifest, validate_package_files, validate_package_id, ConnectorAuthField,
    ConnectorAuthKind, ConnectorAuthSpec, ConnectorAuthStatus, ConnectorCatalogItem,
    ConnectorCliDefinition, ConnectorCliInvocation, ConnectorCliOperationKind,
    ConnectorCliOperationSpec, ConnectorPackageManifest, ConnectorPackageOrigin,
    ConnectorPackageState, ConnectorPackageSummary, ConnectorRuntimeKind, ConnectorRuntimeStatus,
};
use crate::work::executor::{WorkExecutionResult, WorkExecutionStatus};
use crate::work::models::ToolRiskClass;
use crate::work::paths::WorkPaths;

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_MCP_CONFIG_BYTES: u64 = 256 * 1024;
const MAX_CLI_CONFIG_BYTES: u64 = 256 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const STATE_VERSION: u32 = 1;
const MAX_MCP_SERVERS_PER_PACKAGE: usize = 32;
const MAX_MCP_ARGS: usize = 100;
const MAX_MCP_ARG_CHARS: usize = 4_000;
const MAX_CLI_ARGS: usize = 64;
const MAX_CLI_ARG_CHARS: usize = 8_192;
const MAX_CLI_TIMEOUT_SECONDS: u64 = 600;
const PACKAGE_MCP_SERVER_PREFIX: &str = "__agentcabin_package__";
const DEFAULT_CLI_REDACTION_FIELDS: &[&str] = &[
    "token",
    "secret",
    "password",
    "authorization",
    "api_key",
    "access_token",
    "refresh_token",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectorPackageMcpRuntime {
    pub projected_servers: usize,
}

impl ConnectorPackageMcpRuntime {
    pub fn enabled(self) -> bool {
        self.projected_servers > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectorPackageCliRuntime {
    pub projected_connectors: usize,
}

impl ConnectorPackageCliRuntime {
    pub fn enabled(self) -> bool {
        self.projected_connectors > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConnectorPackageStateFile {
    #[serde(default = "default_state_version")]
    version: u32,
    #[serde(default)]
    packages: Vec<ConnectorPackageState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConnectorPackageSecretsFile {
    #[serde(default = "default_state_version")]
    version: u32,
    #[serde(default)]
    packages: BTreeMap<String, BTreeMap<String, String>>,
}

fn default_state_version() -> u32 {
    STATE_VERSION
}

/// List installed WorkBuddy Connector Packages. Directories without
/// connector-meta.json are intentionally ignored.
pub fn list() -> Result<Vec<ConnectorPackageSummary>, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    list_with_paths(&paths)
}

pub fn list_with_paths(paths: &WorkPaths) -> Result<Vec<ConnectorPackageSummary>, String> {
    let packages_dir = ensure_packages_dir(paths)?;
    let states = read_states(paths)?;
    let mut summaries = Vec::new();

    for entry in fs::read_dir(&packages_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package directory cannot be a symlink: {}",
                path.display()
            ));
        }
        if !metadata.is_dir() {
            continue;
        }
        let workbuddy_manifest_path = path.join("connector-meta.json");
        if !path_exists_without_following_symlink(&workbuddy_manifest_path)? {
            continue;
        }

        let manifest = read_and_validate_manifest(&path)?;
        summaries.push(summary_for(paths, &manifest, states.packages.as_slice()));
    }

    summaries.sort_by(|left, right| {
        left.manifest
            .display_name
            .cmp(&right.manifest.display_name)
            .then_with(|| left.manifest.id.cmp(&right.manifest.id))
    });
    Ok(summaries)
}

/// Provision the built-in Feishu package inside the current Work profile.
///
/// The package manifest is intentionally materialized under `connectors/`
/// instead of being only a catalog record. That makes the same trust,
/// enablement, Skill projection, and CLI invocation code serve Feishu as it
/// serves community packages. The npm runtime itself lives in the separate
/// AgentCabin-owned `binaries/node/cli-connector-packages/` directory.
pub fn ensure_builtin_feishu_package(paths: &WorkPaths) -> Result<ConnectorPackageSummary, String> {
    let packages_dir = ensure_packages_dir(paths)?;
    let destination = packages_dir.join("feishu");
    if let Ok(metadata) = fs::symlink_metadata(&destination) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("内置飞书 Connector Package 路径不安全".into());
        }
        let meta_path = destination.join("connector-meta.json");
        if !meta_path.is_file() {
            let legacy_path = destination.join("connector.json");
            if legacy_path.is_file() {
                let legacy: Value = serde_json::from_str(
                    &fs::read_to_string(&legacy_path).map_err(|error| error.to_string())?,
                )
                .map_err(|error| format!("旧版飞书连接器 manifest 无效: {error}"))?;
                let legacy_id = legacy
                    .get("id")
                    .or_else(|| legacy.get("source"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !legacy_id.eq_ignore_ascii_case("feishu") {
                    return Err(
                        "connectors/feishu 已被非内置 Package 占用，请先移除后再启用内置飞书连接器"
                            .into(),
                    );
                }
                fs::remove_file(&legacy_path).map_err(|error| error.to_string())?;
            }
        } else if let Ok(content) = fs::read_to_string(&meta_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                let id = val
                    .get("source")
                    .or_else(|| val.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !id.is_empty() && !id.eq_ignore_ascii_case("feishu") {
                    return Err(
                        "connectors/feishu 已被非内置 Package 占用，请先移除后再启用内置飞书连接器"
                            .into(),
                    );
                }
            }
        }
    } else {
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    }

    let skills_dir = destination.join("skills");
    fs::create_dir_all(&skills_dir).map_err(|error| error.to_string())?;
    fs::write(
        destination.join("connector-meta.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "source": "feishu",
            "name": "Feishu / Lark",
            "name_zh": "飞书",
            "description": "Work with Feishu/Lark through the application-managed CLI.",
            "description_zh": "通过 AgentCabin 应用级 CLI 使用飞书/Lark 能力。",
            "version": "1.0.0",
            "type": "cli",
            "auth_mode": "cli",
            "protected": true,
            "skills": ["skills"]
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        destination.join("cli.json"),
        crate::work::apps::lark_cli::FEISHU_CLI_JSON,
    )
    .map_err(|error| error.to_string())?;
    for (slug, _description, content) in crate::work::apps::lark_cli::builtin_skill_files() {
        let skill_dir = skills_dir.join(slug);
        fs::create_dir_all(&skill_dir).map_err(|error| error.to_string())?;
        fs::write(skill_dir.join("SKILL.md"), content).map_err(|error| error.to_string())?;
    }

    let manifest = read_and_validate_manifest(&destination)?;
    let mut states = read_states(paths)?;
    let previous_enabled = states
        .packages
        .iter()
        .find(|state| state.package_id == manifest.id && state.version == manifest.version)
        .map(|state| state.enabled);
    let global_enabled = crate::storage::profile_bindings::global_capability_override_with_root(
        paths.data_root(),
        crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
        &manifest.id,
    );
    let result_state = {
        let state = state_mut(&mut states, &manifest);
        state.installed = true;
        state.trusted = true;
        state.enabled = global_enabled.or(previous_enabled).unwrap_or(true);
        state.clone()
    };
    write_states(paths, &states)?;
    Ok(ConnectorPackageSummary {
        manifest,
        state: result_state,
    })
}

/// Return the bundled Connector catalog used by the Marketplace. These
/// entries are product-level registry records today; their Provider bridge is
/// Composio-compatible and their physical Package manifests can be migrated
/// in later without changing the catalog/UI contract.
pub fn list_builtin_catalog() -> Result<Vec<ConnectorCatalogItem>, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    list_builtin_catalog_with_paths(&paths)
}

fn requires_credential_auth(manifest: &ConnectorPackageManifest) -> bool {
    if manifest.auth.kind == ConnectorAuthKind::None || manifest.auth.kind == ConnectorAuthKind::Cli
    {
        return false;
    }
    if manifest.auth.kind == ConnectorAuthKind::ApiKey {
        return true;
    }
    if manifest.runtimes.contains(&ConnectorRuntimeKind::Mcp) {
        return true;
    }
    false
}

pub fn list_builtin_catalog_with_paths(
    paths: &WorkPaths,
) -> Result<Vec<ConnectorCatalogItem>, String> {
    let feishu_package = ensure_builtin_feishu_package(paths)?;
    let connections =
        crate::work::apps::storage::list_connections(paths).map_err(|error| error.to_string())?;
    let mut items = crate::work::apps::provider::default_six_apps_catalog()
        .into_iter()
        .map(|app| {
            let connection = connections
                .iter()
                .find(|candidate| candidate.app_id.eq_ignore_ascii_case(&app.app_id));
            let connection_status = connection
                .map(|candidate| candidate.status)
                .unwrap_or(ConnectionStatus::Disconnected);
            let auth_status = match connection_status {
                ConnectionStatus::Connected => ConnectorAuthStatus::Authenticated,
                ConnectionStatus::Pending => ConnectorAuthStatus::Pending,
                ConnectionStatus::Expired => ConnectorAuthStatus::Expired,
                ConnectionStatus::Error => ConnectorAuthStatus::Error,
                ConnectionStatus::Disconnected => ConnectorAuthStatus::NotAuthenticated,
            };

            let (runtimes, auth_kind, trusted, enabled, installed) = if app.app_id == "feishu" {
                (
                    feishu_package.manifest.runtimes.clone(),
                    feishu_package.manifest.auth.kind,
                    feishu_package.state.trusted,
                    feishu_package.state.enabled,
                    feishu_package.state.installed,
                )
            } else {
                let auth_kind = match app.auth_type {
                    crate::work::apps::models::AppAuthType::ApiKey => ConnectorAuthKind::ApiKey,
                    crate::work::apps::models::AppAuthType::OAuth2 => ConnectorAuthKind::OAuth2,
                };
                let enabled = if auth_status == ConnectorAuthStatus::Authenticated {
                    crate::storage::profile_bindings::global_capability_override_with_root(
                        paths.data_root(),
                        crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
                        &app.app_id,
                    )
                    .unwrap_or(true)
                } else {
                    false
                };
                (
                    vec![ConnectorRuntimeKind::Mcp],
                    auth_kind,
                    true,
                    enabled,
                    true,
                )
            };

            ConnectorCatalogItem {
                package_id: app.app_id,
                display_name: app.display_name,
                description: app.description,
                icon: app.icon,
                categories: app.categories,
                capabilities: app.capabilities,
                runtimes,
                auth: ConnectorAuthSpec {
                    kind: auth_kind,
                    provider: None,
                },
                origin: ConnectorPackageOrigin::Builtin,
                builtin: true,
                installed,
                trusted,
                enabled,
                auth_status,
                connection_status,
                account_count: connection
                    .map(|candidate| candidate.accounts.len())
                    .unwrap_or(0),
                documentation_url: app.documentation_url,
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.package_id.cmp(&right.package_id))
    });
    Ok(items)
}

/// Return one installed Package using the same safe projection exposed by the
/// package list command. This is also the lookup boundary used by auth flows;
/// callers never need to construct a package path from user input.
pub fn get_with_paths(
    paths: &WorkPaths,
    package_id: &str,
) -> Result<ConnectorPackageSummary, String> {
    let (manifest, _root) = find_installed(paths, package_id)?;
    let states = read_states(paths)?;
    Ok(summary_for(paths, &manifest, &states.packages))
}

/// Mirror a Connected Apps status into the generic Connector Package state.
/// Packages that are not installed are deliberately ignored so the legacy
/// Apps catalog remains independent from the new Package catalog.
pub fn sync_auth_status_from_app_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    status: ConnectionStatus,
) -> Result<(), String> {
    let Ok((manifest, _root)) = find_installed(paths, app_id) else {
        return Ok(());
    };
    let next_status = match status {
        ConnectionStatus::Connected => ConnectorAuthStatus::Authenticated,
        ConnectionStatus::Pending => ConnectorAuthStatus::Pending,
        ConnectionStatus::Expired => ConnectorAuthStatus::Expired,
        ConnectionStatus::Disconnected => ConnectorAuthStatus::NotAuthenticated,
        ConnectionStatus::Error => ConnectorAuthStatus::Error,
    };
    if manifest.auth.kind == ConnectorAuthKind::None {
        return Ok(());
    }
    set_auth_status_with_paths(paths, &manifest.id, next_status).map(|_| ())
}

/// Update only the non-secret Package auth state. OAuth tokens, API keys and
/// provider credentials remain owned by the existing Apps/Host secret layer.
pub fn set_auth_status_with_paths(
    paths: &WorkPaths,
    package_id: &str,
    auth_status: ConnectorAuthStatus,
) -> Result<ConnectorPackageSummary, String> {
    let (manifest, _root) = find_installed(paths, package_id)?;
    let mut states = read_states(paths)?;
    let state = state_mut(&mut states, &manifest);
    if manifest.auth.kind == ConnectorAuthKind::None {
        state.auth_status = ConnectorAuthStatus::NotRequired;
    } else {
        state.auth_status = auth_status;
    }
    if auth_status != ConnectorAuthStatus::Error {
        state.last_error = None;
    }
    let result_state = state.clone();
    write_states(paths, &states)?;
    Ok(ConnectorPackageSummary {
        manifest,
        state: result_state,
    })
}

/// Store values declared by a WorkBuddy token-schema.json. Values are kept in
/// the Host-only Work profile and substituted only when Rust opens the MCP
/// connection; the generated package config retains ${KEY} placeholders.
pub fn configure_token_auth(
    package_id: &str,
    values: BTreeMap<String, String>,
) -> Result<ConnectorPackageSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    configure_token_auth_with_paths(&paths, package_id, values)
}

pub fn configure_token_auth_with_paths(
    paths: &WorkPaths,
    package_id: &str,
    values: BTreeMap<String, String>,
) -> Result<ConnectorPackageSummary, String> {
    let (manifest, _root) = find_installed(paths, package_id)?;
    if manifest.auth.kind != ConnectorAuthKind::ApiKey || manifest.auth_fields.is_empty() {
        return Err(format!(
            "WorkBuddy connector '{}' does not declare token-schema.json fields",
            manifest.id
        ));
    }
    let declared = manifest
        .auth_fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(key) = values.keys().find(|key| !declared.contains(key.as_str())) {
        return Err(format!(
            "Unknown WorkBuddy connector credential field '{key}'"
        ));
    }
    let mut normalized = BTreeMap::new();
    for field in &manifest.auth_fields {
        let value = values
            .get(&field.key)
            .map(|value| value.trim())
            .unwrap_or("");
        if field.required && value.is_empty() {
            return Err(format!("{} is required", field.label));
        }
        if value.chars().count() > 16_384 || value.chars().any(char::is_control) {
            return Err(format!(
                "{} contains an invalid credential value",
                field.label
            ));
        }
        if !value.is_empty() {
            normalized.insert(field.key.clone(), value.to_string());
        }
    }
    let mut secrets = read_package_secrets(paths)?;
    secrets.packages.insert(manifest.id.clone(), normalized);
    write_package_secrets(paths, &secrets)?;
    let summary =
        set_auth_status_with_paths(paths, &manifest.id, ConnectorAuthStatus::Authenticated)?;
    if summary.state.trusted {
        let _ = set_enabled_with_paths(paths, &manifest.id, true);
        return get_with_paths(paths, &manifest.id);
    }
    Ok(summary)
}

/// Resolve WorkBuddy ${KEY} placeholders in a Host-only runtime projection.
/// Callers must never serialize the returned value to the frontend or Pi.
pub fn resolve_runtime_secrets(paths: &WorkPaths, root: &mut Value) -> Result<(), String> {
    let secrets = read_package_secrets(paths)?;
    let Some(root_object) = root.as_object_mut() else {
        return Ok(());
    };
    let servers = if root_object.contains_key("mcpServers") {
        root_object.get_mut("mcpServers")
    } else {
        root_object.get_mut("mcp_servers")
    };
    let Some(servers) = servers.and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (name, config) in servers {
        let Some(remainder) = name.strip_prefix(PACKAGE_MCP_SERVER_PREFIX) else {
            continue;
        };
        let Some((package_id, _)) = remainder.split_once("__") else {
            continue;
        };
        let values = secrets
            .packages
            .get(package_id)
            .cloned()
            .unwrap_or_default();
        replace_workbuddy_placeholders(config, &values, package_id)?;
    }
    Ok(())
}

/// Validate a package directory without installing or executing it.
pub fn validate_directory(source: &Path) -> Result<ConnectorPackageManifest, String> {
    let source = canonical_package_source(source)?;
    read_and_validate_manifest(&source)
}

/// Copy and validate a local package into the Work Profile. New packages are
/// installed untrusted and disabled; enabling requires an explicit trust step.
pub fn install(source: &Path) -> Result<ConnectorPackageSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    install_with_paths(&paths, source)
}

pub fn install_with_paths(
    paths: &WorkPaths,
    source: &Path,
) -> Result<ConnectorPackageSummary, String> {
    let source = canonical_package_source(source)?;
    let manifest = read_and_validate_manifest(&source)?;
    let packages_dir = ensure_packages_dir(paths)?;
    let destination = packages_dir.join(&manifest.id);

    if source == destination {
        return Err("Connector Package is already installed at the target path".into());
    }
    if path_exists_without_following_symlink(&destination)? {
        return Err(format!(
            "Connector Package '{}' is already installed",
            manifest.id
        ));
    }

    let mut total_bytes = 0;
    if let Err(error) = copy_package_tree(&source, &destination, &mut total_bytes) {
        if destination.exists() {
            let _ = fs::remove_dir_all(&destination);
        }
        return Err(error);
    }

    // Re-validate the copied tree so a future copy implementation cannot
    // weaken the source validation boundary.
    if let Err(error) = read_and_validate_manifest(&destination) {
        let _ = fs::remove_dir_all(&destination);
        return Err(error);
    }

    let mut states = read_states(paths)?;
    let mut state = ConnectorPackageState::for_manifest(&manifest);
    state.installed = true;
    states
        .packages
        .retain(|item| item.package_id != manifest.id);
    states.packages.push(state.clone());
    if let Err(error) = write_states(paths, &states) {
        let _ = fs::remove_dir_all(&destination);
        return Err(error);
    }

    Ok(ConnectorPackageSummary { manifest, state })
}

/// Project trusted and enabled Package MCP servers into a separate generated
/// config. The legacy user-managed `mcp.json` is never rewritten here. The
/// generated config is read by the Work MCP adapter inside the Pi session.
pub fn sync_mcp_runtime(paths: &WorkPaths) -> Result<ConnectorPackageMcpRuntime, String> {
    let packages_dir = ensure_packages_dir(paths)?;
    let mut states = read_states(paths)?;
    let mut generated_servers = Map::new();
    let mut state_changed = false;

    for entry in fs::read_dir(&packages_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let root = entry.path();
        let metadata = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package directory cannot be a symlink: {}",
                root.display()
            ));
        }
        if !metadata.is_dir() {
            continue;
        }
        let manifest_path = root.join("connector-meta.json");
        if !path_exists_without_following_symlink(&manifest_path)? {
            continue;
        }

        let manifest = read_and_validate_manifest(&root)?;
        if !manifest.runtimes.contains(&ConnectorRuntimeKind::Mcp) {
            continue;
        }
        let package_state = effective_package_state(
            paths,
            &manifest,
            read_state_for_manifest(&states.packages, &manifest),
        );
        if !package_state.trusted || !package_state.enabled {
            state_changed |= set_runtime_state(
                &mut states,
                &manifest,
                ConnectorRuntimeStatus::NotReady,
                None,
            );
            continue;
        }

        let projection = if manifest.auth.kind != ConnectorAuthKind::None
            && package_state.auth_status != ConnectorAuthStatus::Authenticated
        {
            Err("Connector Package 需要先完成认证，暂未加载 MCP runtime".to_string())
        } else {
            project_package_mcp_servers(&root, &manifest)
        };

        match projection {
            Ok(servers) => {
                for (name, config) in servers {
                    generated_servers.insert(name, config);
                }
                state_changed |=
                    set_runtime_state(&mut states, &manifest, ConnectorRuntimeStatus::Ready, None);
            }
            Err(error) => {
                let status = if manifest.auth.kind != ConnectorAuthKind::None
                    && package_state.auth_status != ConnectorAuthStatus::Authenticated
                {
                    ConnectorRuntimeStatus::NotReady
                } else {
                    ConnectorRuntimeStatus::Failed
                };
                state_changed |= set_runtime_state(&mut states, &manifest, status, Some(error));
            }
        }
    }

    write_package_mcp_config(paths, &generated_servers)?;
    if state_changed {
        write_states(paths, &states)?;
    }
    Ok(ConnectorPackageMcpRuntime {
        projected_servers: generated_servers.len(),
    })
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawConnectorCliConfig {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    base_args: Vec<String>,
    #[serde(default)]
    operations: Map<String, Value>,
    #[serde(default)]
    init: Option<Value>,
    #[serde(default)]
    version_check: Option<Value>,
    #[serde(default)]
    auth: Option<Value>,
    #[serde(default)]
    status: Option<Value>,
    #[serde(default)]
    un_auth: Option<Value>,
    /// Explicit AgentCabin form:
    /// { "packageManager": "npm", "package": "@scope/cli", "version": "1.2.3" }
    #[serde(default)]
    install: Option<RawCliInstallSpec>,
    #[serde(default)]
    redaction_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCliInstallSpec {
    #[serde(default = "default_npm_package_manager")]
    package_manager: String,
    #[serde(alias = "packageName")]
    package: String,
    #[serde(default)]
    version: Option<String>,
}

fn default_npm_package_manager() -> String {
    "npm".into()
}

fn read_raw_cli_config(
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<RawConnectorCliConfig, String> {
    let relative = manifest
        .cli_file
        .as_deref()
        .ok_or_else(|| "CLI Connector Package must declare cliFile".to_string())?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Package cliFile must be a regular file".into());
    }
    if metadata.len() > MAX_CLI_CONFIG_BYTES {
        return Err("Package cliFile is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| format!("invalid Package cliFile: {error}"))
}

/// Validate and normalize a Package CLI definition. The returned definition
/// is a static, non-secret catalog that can be passed to the Work tool layer.
pub fn read_cli_definition(
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<ConnectorCliDefinition, String> {
    let raw = read_raw_cli_config(root, manifest)?;
    normalize_cli_definition(manifest, raw)
}

/// Read the optional application-global CLI install declaration.
///
/// AgentCabin packages should use the explicit `install` object. For
/// compatibility with WorkBuddy marketplace packages, a platform-specific
/// `init` string of the form `npm install -g <package>` is also accepted and
/// translated to the same AgentCabin-owned npm prefix.
fn read_cli_install_spec(
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<Option<String>, String> {
    let raw = read_raw_cli_config(root, manifest)?;
    if let Some(spec) = raw.install {
        return Ok(Some(normalize_cli_install_spec(
            &spec.package_manager,
            &spec.package,
            spec.version.as_deref(),
        )?));
    }

    let Some(init) = raw.init else {
        return Ok(None);
    };
    let command = match init {
        Value::String(value) => Some(value),
        Value::Object(object) => {
            let platform = match std::env::consts::OS {
                "macos" => "darwin",
                "windows" => "win32",
                other => other,
            };
            object
                .get(platform)
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        _ => None,
    };
    let Some(command) = command else {
        return Ok(None);
    };
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.len() == 4
        && tokens[0] == "npm"
        && matches!(tokens[1], "install" | "i")
        && matches!(tokens[2], "-g" | "--global")
    {
        validate_managed_package_spec(tokens[3])?;
        return Ok(Some(tokens[3].to_string()));
    }
    Err("Package cli.json init 只允许使用无 shell 的 npm install -g <package> 形式".into())
}

fn normalize_cli_install_spec(
    package_manager: &str,
    package: &str,
    version: Option<&str>,
) -> Result<String, String> {
    if !package_manager.trim().eq_ignore_ascii_case("npm") {
        return Err(format!(
            "Package CLI install only supports npm, got '{package_manager}'"
        ));
    }
    let package = package.trim();
    let spec = match version.map(str::trim).filter(|value| !value.is_empty()) {
        Some(version) => format!("{package}@{version}"),
        None => package.to_string(),
    };
    validate_managed_package_spec(&spec)?;
    Ok(spec)
}

fn validate_managed_package_spec(spec: &str) -> Result<(), String> {
    crate::work::cli_runtime::validate_package_spec(spec)
}

fn normalize_cli_definition(
    manifest: &ConnectorPackageManifest,
    raw: RawConnectorCliConfig,
) -> Result<ConnectorCliDefinition, String> {
    let command = raw
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Package cliFile requires command".to_string())?;
    validate_cli_command(command, &manifest.permissions.commands)?;

    let base_args = parse_cli_args(
        raw.args
            .into_iter()
            .chain(raw.base_args)
            .collect::<Vec<_>>(),
        "cliFile args",
    )?;

    let mut raw_operations = raw.operations;
    let direct_operations = [
        (ConnectorCliOperationKind::Init, raw.init),
        (ConnectorCliOperationKind::VersionCheck, raw.version_check),
        (ConnectorCliOperationKind::Auth, raw.auth),
        (ConnectorCliOperationKind::Status, raw.status),
        (ConnectorCliOperationKind::UnAuth, raw.un_auth),
    ];
    for (kind, value) in direct_operations {
        if let Some(value) = value {
            raw_operations.insert(kind.as_str().to_string(), value);
        }
    }
    if raw_operations.is_empty() {
        return Err("Package cliFile must declare at least one CLI operation".into());
    }

    let mut operations = BTreeMap::new();
    for (name, value) in raw_operations {
        let key = normalize_cli_operation_key(&name)?;
        let spec = normalize_cli_operation(ConnectorCliOperationKind::parse(&key), value)?;
        operations.insert(key, spec);
    }

    let redaction_fields = normalize_redaction_fields(raw.redaction_fields)?;
    Ok(ConnectorCliDefinition {
        package_id: manifest.id.clone(),
        command: command.to_string(),
        base_args,
        operations,
        redaction_fields,
    })
}

fn normalize_cli_operation(
    kind: Option<ConnectorCliOperationKind>,
    value: Value,
) -> Result<ConnectorCliOperationSpec, String> {
    let mut spec = match value {
        Value::Null => ConnectorCliOperationSpec::default(),
        Value::Array(args) => ConnectorCliOperationSpec {
            args: args
                .into_iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        "Package CLI operation args must contain strings only".to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            ..ConnectorCliOperationSpec::default()
        },
        Value::Object(object) => serde_json::from_value(Value::Object(object))
            .map_err(|error| format!("invalid Package CLI operation: {error}"))?,
        _ => return Err("Package CLI operation must be an object or an args array".into()),
    };
    spec.args = parse_cli_args(spec.args, "CLI operation args")?;
    if let Some(timeout) = spec.timeout_seconds {
        if !(1..=MAX_CLI_TIMEOUT_SECONDS).contains(&timeout) {
            return Err(format!(
                "Package CLI operation timeout must be between 1 and {MAX_CLI_TIMEOUT_SECONDS} seconds"
            ));
        }
    }
    if let Some(cwd) = spec.cwd.as_deref() {
        validate_cli_cwd(cwd)?;
    }
    if let Some(output) = spec.output.as_deref() {
        if !matches!(output.trim().to_ascii_lowercase().as_str(), "json" | "text") {
            return Err("Package CLI operation output must be 'json' or 'text'".into());
        }
    } else {
        spec.output = Some("json".into());
    }
    if spec.requires_confirmation.is_none() {
        spec.requires_confirmation = Some(
            kind.map(|kind| kind.default_requires_confirmation())
                .unwrap_or(true),
        );
    }
    Ok(spec)
}

fn normalize_cli_operation_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err("Package CLI operation name is empty, too long, or invalid".into());
    }
    if let Some(kind) = ConnectorCliOperationKind::parse(value) {
        return Ok(kind.as_str().to_string());
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(format!("Unsupported Package CLI operation '{value}'"));
    }
    Ok(value.to_string())
}

fn cli_operation_can_run_without_auth(kind: Option<&ConnectorCliOperationKind>) -> bool {
    matches!(
        kind,
        Some(
            ConnectorCliOperationKind::Init
                | ConnectorCliOperationKind::Auth
                | ConnectorCliOperationKind::Status
                | ConnectorCliOperationKind::VersionCheck
                | ConnectorCliOperationKind::UnAuth
        )
    )
}

fn parse_cli_args(args: Vec<String>, field: &str) -> Result<Vec<String>, String> {
    if args.len() > MAX_CLI_ARGS {
        return Err(format!(
            "{field} cannot contain more than {MAX_CLI_ARGS} items"
        ));
    }
    for arg in &args {
        if arg.chars().count() > MAX_CLI_ARG_CHARS || arg.contains('\0') {
            return Err(format!(
                "{field} contains an argument that is too long or invalid"
            ));
        }
        if arg.chars().any(char::is_control) {
            return Err(format!("{field} cannot contain control characters"));
        }
        if looks_like_secret_argument(arg) {
            return Err(format!(
                "{field} cannot contain inline credential arguments"
            ));
        }
    }
    Ok(args)
}

fn validate_cli_command(command: &str, allowed_commands: &[String]) -> Result<(), String> {
    if command.contains('\0')
        || command.chars().any(char::is_control)
        || command.starts_with('-')
        || command
            .chars()
            .any(|character| ";&|<>".contains(character) || character == '\u{60}')
    {
        return Err("Package CLI command contains unsafe shell syntax".into());
    }
    if !allowed_commands.iter().any(|allowed| {
        let allowed = allowed.trim();
        allowed == "*"
            || allowed == command
            || Path::new(command)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| allowed == name)
    }) {
        return Err(format!(
            "Package CLI command '{command}' is not declared in permissions.commands"
        ));
    }
    Ok(())
}

fn validate_cli_cwd(cwd: &str) -> Result<(), String> {
    let path = Path::new(cwd.trim());
    if cwd.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Package CLI cwd must be a relative Work Workspace path".into());
    }
    Ok(())
}

fn normalize_redaction_fields(fields: Vec<String>) -> Result<Vec<String>, String> {
    let fields = if fields.is_empty() {
        DEFAULT_CLI_REDACTION_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect()
    } else {
        fields
    };
    if fields.len() > 32 {
        return Err("Package CLI redactionFields cannot contain more than 32 items".into());
    }
    let mut normalized = Vec::new();
    for field in fields {
        let field = field.trim().to_ascii_lowercase();
        if field.is_empty() || field.len() > 64 || field.chars().any(char::is_control) {
            return Err("Package CLI redactionFields contains an invalid field".into());
        }
        if !normalized.contains(&field) {
            normalized.push(field);
        }
    }
    Ok(normalized)
}

fn looks_like_secret_argument(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token=",
        "secret=",
        "password=",
        "api_key=",
        "apikey=",
        "access_token=",
        "authorization=",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Project validated, trusted and enabled Package CLI definitions into a
/// generated catalog. The catalog is non-secret and is consumed only by the
/// Work Host/tool boundary.
pub fn sync_cli_runtime(paths: &WorkPaths) -> Result<ConnectorPackageCliRuntime, String> {
    ensure_builtin_feishu_package(paths)?;
    let packages_dir = ensure_packages_dir(paths)?;
    let mut states = read_states(paths)?;
    let mut projected = Vec::new();
    let mut state_changed = false;

    for entry in fs::read_dir(&packages_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let root = entry.path();
        let metadata = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package directory cannot be a symlink: {}",
                root.display()
            ));
        }
        if !metadata.is_dir()
            || !path_exists_without_following_symlink(&root.join("connector-meta.json"))?
        {
            continue;
        }
        let manifest = read_and_validate_manifest(&root)?;
        if !manifest.runtimes.contains(&ConnectorRuntimeKind::Cli) {
            continue;
        }
        let package_state = effective_package_state(
            paths,
            &manifest,
            read_state_for_manifest(&states.packages, &manifest),
        );
        if !package_state.trusted || !package_state.enabled {
            state_changed |= set_runtime_state(
                &mut states,
                &manifest,
                ConnectorRuntimeStatus::NotReady,
                None,
            );
            continue;
        }

        match read_cli_definition(&root, &manifest) {
            Ok(definition) => {
                projected.push(serde_json::json!({
                    "packageId": manifest.id,
                    "authStatus": package_state.auth_status,
                    "definition": definition,
                }));
                state_changed |=
                    set_runtime_state(&mut states, &manifest, ConnectorRuntimeStatus::Ready, None);
            }
            Err(error) => {
                state_changed |= set_runtime_state(
                    &mut states,
                    &manifest,
                    ConnectorRuntimeStatus::Failed,
                    Some(error),
                );
            }
        }
    }

    write_package_cli_config(paths, &projected)?;
    if state_changed {
        write_states(paths, &states)?;
    }
    Ok(ConnectorPackageCliRuntime {
        projected_connectors: projected.len(),
    })
}

/// Install a Package-declared CLI into the AgentCabin application prefix.
///
/// This runs only after the user explicitly trusts and enables the Package.
/// Packages without an install declaration may intentionally use an existing
/// host CLI; their command still goes through the normal Work CLI pipeline.
fn ensure_managed_cli_runtime(
    paths: &WorkPaths,
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<(), String> {
    let Some(package_spec) = read_cli_install_spec(root, manifest)? else {
        return Ok(());
    };
    let definition = read_cli_definition(root, manifest)?;
    if cli_runtime::managed_command_exists(paths, &definition.command) {
        return Ok(());
    }
    cli_runtime::install_npm_package(paths, &package_spec)?;
    if !cli_runtime::managed_command_exists(paths, &definition.command) {
        return Err(format!(
            "npm 包 '{package_spec}' 已安装，但未找到 CLI 命令 '{}' 的应用级 bin",
            definition.command
        ));
    }
    Ok(())
}

fn write_package_cli_config(paths: &WorkPaths, connectors: &[Value]) -> Result<(), String> {
    let path = paths.work_connector_cli_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Connector Package CLI config has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "connectors": connectors,
    }))
    .map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".connector-package-cli.{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

/// Return Skill directories from trusted, enabled Packages. A package can
/// declare a single skill directory or a directory containing direct child
/// skills; both forms are normalized to directories containing SKILL.md.
pub fn package_skill_sources(paths: &WorkPaths) -> Result<Vec<String>, String> {
    let packages_dir = ensure_packages_dir(paths)?;
    let mut states = read_states(paths)?;
    let mut sources = Vec::new();
    let mut state_changed = false;

    for entry in fs::read_dir(&packages_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let root = entry.path();
        let metadata = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package directory cannot be a symlink: {}",
                root.display()
            ));
        }
        if !metadata.is_dir()
            || !path_exists_without_following_symlink(&root.join("connector-meta.json"))?
        {
            continue;
        }
        let manifest = read_and_validate_manifest(&root)?;
        if !manifest.runtimes.contains(&ConnectorRuntimeKind::Skill) {
            continue;
        }
        let package_state = effective_package_state(
            paths,
            &manifest,
            read_state_for_manifest(&states.packages, &manifest),
        );
        if !package_state.trusted || !package_state.enabled {
            continue;
        }
        if requires_credential_auth(&manifest)
            && package_state.auth_status != ConnectorAuthStatus::Authenticated
        {
            continue;
        }

        let mut package_sources = Vec::new();
        for relative in &manifest.skill_dirs {
            let directory = root.join(relative);
            if directory.join("SKILL.md").is_file() {
                let skill_id = directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if crate::storage::profile_bindings::is_skill_enabled_with_root(
                    paths.data_root(),
                    skill_id,
                ) {
                    package_sources.push(directory);
                }
                continue;
            }
            for child in fs::read_dir(&directory).map_err(|error| error.to_string())? {
                let child = child.map_err(|error| error.to_string())?.path();
                let child_metadata =
                    fs::symlink_metadata(&child).map_err(|error| error.to_string())?;
                if child_metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Connector Package Skill directory cannot contain a symlink: {}",
                        child.display()
                    ));
                }
                if child_metadata.is_dir()
                    && child.join("SKILL.md").is_file()
                    && crate::storage::profile_bindings::is_skill_enabled_with_root(
                        paths.data_root(),
                        child
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or_default(),
                    )
                {
                    package_sources.push(child);
                }
            }
        }

        if package_sources.is_empty() {
            state_changed |= set_runtime_state(
                &mut states,
                &manifest,
                ConnectorRuntimeStatus::Failed,
                Some("Skill Connector Package does not contain a readable SKILL.md".into()),
            );
            continue;
        }
        for source in package_sources {
            push_unique_path(&mut sources, source);
        }
    }
    if state_changed {
        write_states(paths, &states)?;
    }
    sources.sort();
    Ok(sources
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

/// Resolve a fixed Package CLI operation and append only caller-provided argv
/// items. The executable and static operation arguments always come from the
/// validated Package definition.
pub fn resolve_cli_invocation(
    paths: &WorkPaths,
    package_id: &str,
    operation: &str,
    extra_args: &[String],
) -> Result<ConnectorCliInvocation, String> {
    if package_id.trim().eq_ignore_ascii_case("feishu") {
        ensure_builtin_feishu_package(paths)?;
    }
    let (manifest, root) = find_installed(paths, package_id)?;
    if !manifest.runtimes.contains(&ConnectorRuntimeKind::Cli) {
        return Err(format!(
            "Connector Package '{}' has no CLI runtime",
            manifest.id
        ));
    }
    let states = read_states(paths)?;
    let state = effective_package_state(
        paths,
        &manifest,
        read_state_for_manifest(&states.packages, &manifest),
    );
    if !state.trusted || !state.enabled {
        return Err(format!(
            "Connector Package '{}' must be trusted and enabled before CLI use",
            manifest.id
        ));
    }
    let key = normalize_cli_operation_key(operation)?;
    let kind = ConnectorCliOperationKind::parse(&key);
    let definition = read_cli_definition(&root, &manifest)?;
    let spec = definition
        .operations
        .get(&key)
        .ok_or_else(|| format!("CLI operation '{key}' is not declared by '{}'", manifest.id))?;

    if manifest.auth.kind != ConnectorAuthKind::None
        && state.auth_status != ConnectorAuthStatus::Authenticated
        && !cli_operation_can_run_without_auth(kind.as_ref())
    {
        return Err(format!(
            "Connector Package '{}' requires authentication before '{key}'",
            manifest.id
        ));
    }

    let mut args = definition.base_args.clone();
    args.extend(spec.args.clone());
    args.extend(parse_cli_args(extra_args.to_vec(), "CLI invocation args")?);
    if args.len() > MAX_CLI_ARGS {
        return Err(format!(
            "CLI invocation cannot contain more than {MAX_CLI_ARGS} arguments"
        ));
    }
    let command = if manifest.id.eq_ignore_ascii_case("feishu") {
        if !crate::work::apps::lark_cli::private_lark_cli_exists(paths) {
            return Err("AgentCabin 应用级 lark-cli 尚未安装，请先安装飞书连接器运行时".into());
        }
        crate::work::apps::lark_cli::private_lark_cli_path(paths)
            .to_string_lossy()
            .into_owned()
    } else {
        definition.command
    };
    let cwd = spec.cwd.clone().unwrap_or_else(|| "scratch".into());
    Ok(ConnectorCliInvocation {
        package_id: manifest.id,
        operation: key,
        command,
        args,
        cwd,
        timeout_seconds: spec.timeout_seconds.unwrap_or(120),
        requires_confirmation: spec.requires_confirmation.unwrap_or_else(|| {
            kind.map(|kind| kind.default_requires_confirmation())
                .unwrap_or(true)
        }),
        output: spec.output.clone().unwrap_or_else(|| "json".into()),
        redaction_fields: definition.redaction_fields,
        home_paths: manifest.permissions.home_paths,
    })
}

/// Whether a CLI operation needs a Host-managed OAuth connection before it
/// can be retried. CLI-native auth is intentionally left to the Package's
/// declared `auth` operation; this helper only creates the automatic OAuth
/// connection card that the Host can actually complete.
pub fn cli_requires_host_auth(paths: &WorkPaths, arguments: &Value) -> Result<bool, String> {
    let package_id = arguments
        .get("package_id")
        .or_else(|| arguments.get("packageId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let operation = arguments
        .get("operation")
        .or_else(|| arguments.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let Ok((manifest, _root)) = find_installed(paths, package_id) else {
        return Ok(false);
    };
    if manifest.auth.kind != ConnectorAuthKind::OAuth2 {
        return Ok(false);
    }
    let key = normalize_cli_operation_key(operation)?;
    if cli_operation_can_run_without_auth(ConnectorCliOperationKind::parse(&key).as_ref()) {
        return Ok(false);
    }
    let states = read_states(paths)?;
    let state = effective_package_state(
        paths,
        &manifest,
        read_state_for_manifest(&states.packages, &manifest),
    );
    if !state.trusted || !state.enabled {
        return Ok(false);
    }
    Ok(state.auth_status != ConnectorAuthStatus::Authenticated)
}

/// Read the Package-declared confirmation bit for a CLI operation. The
/// pipeline uses this in addition to the coarse risk class so a Package can
/// require confirmation for an otherwise read-only status check.
pub fn cli_requires_confirmation(paths: &WorkPaths, arguments: &Value) -> Result<bool, String> {
    let package_id = arguments
        .get("package_id")
        .or_else(|| arguments.get("packageId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let operation = arguments
        .get("operation")
        .or_else(|| arguments.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let extra_args = arguments
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let Ok(invocation) = resolve_cli_invocation(paths, package_id, operation, &extra_args) else {
        return Ok(false);
    };
    Ok(invocation.requires_confirmation)
}

pub fn cli_operation_risk(arguments: &Value) -> ToolRiskClass {
    let package_id = arguments
        .get("package_id")
        .or_else(|| arguments.get("packageId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let operation = arguments
        .get("operation")
        .or_else(|| arguments.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if package_id.eq_ignore_ascii_case("feishu")
        && (operation.eq_ignore_ascii_case("configShow")
            || operation.eq_ignore_ascii_case("searchUser"))
    {
        return ToolRiskClass::Read;
    }
    match arguments
        .get("operation")
        .or_else(|| arguments.get("action"))
        .and_then(Value::as_str)
        .and_then(ConnectorCliOperationKind::parse)
    {
        Some(ConnectorCliOperationKind::Status | ConnectorCliOperationKind::VersionCheck) => {
            ToolRiskClass::Read
        }
        Some(ConnectorCliOperationKind::Init) => ToolRiskClass::Exec,
        Some(ConnectorCliOperationKind::Auth | ConnectorCliOperationKind::UnAuth) => {
            ToolRiskClass::External
        }
        None => ToolRiskClass::External,
    }
}

pub fn record_cli_result(
    paths: &WorkPaths,
    invocation: &ConnectorCliInvocation,
    result: &WorkExecutionResult,
) -> Result<(), String> {
    if result.status != WorkExecutionStatus::Success {
        return Ok(());
    }
    let (manifest, _root) = find_installed(paths, &invocation.package_id)?;
    if manifest.auth.kind == ConnectorAuthKind::None {
        return Ok(());
    }
    let mut states = read_states(paths)?;
    let next_status = match ConnectorCliOperationKind::parse(&invocation.operation) {
        Some(ConnectorCliOperationKind::Auth) => Some(ConnectorAuthStatus::Authenticated),
        Some(ConnectorCliOperationKind::UnAuth) => Some(ConnectorAuthStatus::NotAuthenticated),
        Some(ConnectorCliOperationKind::Status) => parse_cli_auth_status(&result.stdout),
        _ => None,
    };
    let Some(next_status) = next_status else {
        return Ok(());
    };
    let state = state_mut(&mut states, &manifest);
    state.auth_status = next_status;
    state.last_error = None;
    write_states(paths, &states)
}

fn parse_cli_auth_status(stdout: &str) -> Option<ConnectorAuthStatus> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        if value.get("ok").and_then(Value::as_bool) == Some(true)
            && value
                .get("identity")
                .and_then(Value::as_str)
                .is_some_and(|identity| identity.eq_ignore_ascii_case("user"))
        {
            return Some(ConnectorAuthStatus::Authenticated);
        }
        if let Some(authenticated) = find_boolean_field(
            &value,
            &["authenticated", "isauthenticated", "loggedin", "logged_in"],
        ) {
            return Some(if authenticated {
                ConnectorAuthStatus::Authenticated
            } else {
                ConnectorAuthStatus::NotAuthenticated
            });
        }
    }
    let lower = stdout.to_ascii_lowercase();
    if [
        "not authenticated",
        "not logged in",
        "logged out",
        "unauthorized",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Some(ConnectorAuthStatus::NotAuthenticated);
    }
    if ["authenticated", "logged in", "authorized"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Some(ConnectorAuthStatus::Authenticated);
    }
    None
}

fn find_boolean_field(value: &Value, fields: &[&str]) -> Option<bool> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if fields.iter().any(|field| key.eq_ignore_ascii_case(field)) {
                    if let Some(result) = value.as_bool() {
                        return Some(result);
                    }
                }
                if let Some(result) = find_boolean_field(value, fields) {
                    return Some(result);
                }
            }
            None
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_boolean_field(value, fields)),
        _ => None,
    }
}

/// Redact common credential-shaped JSON fields and key/value lines before a
/// CLI result reaches the Pi transcript or runtime ledger.
pub fn redact_cli_output(value: &str, fields: &[String]) -> String {
    let fields = if fields.is_empty() {
        DEFAULT_CLI_REDACTION_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    } else {
        fields.to_vec()
    };
    if let Ok(value) = serde_json::from_str::<Value>(value) {
        let redacted = redact_json_value(value, &fields);
        if let Ok(serialized) = serde_json::to_string_pretty(&redacted) {
            return serialized;
        }
    }

    value
        .lines()
        .map(|line| redact_cli_line(line, &fields))
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_json_value(value: Value, fields: &[String]) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let sensitive = fields.iter().any(|field| key.eq_ignore_ascii_case(field));
                    (
                        key,
                        if sensitive {
                            Value::String("***redacted***".into())
                        } else {
                            redact_json_value(value, fields)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| redact_json_value(value, fields))
                .collect(),
        ),
        other => other,
    }
}

fn redact_cli_line(line: &str, fields: &[String]) -> String {
    let lower = line.to_ascii_lowercase();
    if let Some(index) = lower.find("bearer ") {
        return format!("{}Bearer ***redacted***", &line[..index]);
    }
    for field in fields {
        let Some(index) = lower.find(field) else {
            continue;
        };
        let suffix = &line[index + field.len()..];
        let Some(separator) = suffix.find([':', '=']) else {
            continue;
        };
        let end = index + field.len() + separator + 1;
        return format!("{} ***redacted***", &line[..end]);
    }
    line.to_string()
}

fn project_package_mcp_servers(
    root: &Path,
    manifest: &ConnectorPackageManifest,
) -> Result<Map<String, Value>, String> {
    let relative = manifest
        .mcp_file
        .as_deref()
        .ok_or_else(|| "MCP Connector Package must declare mcpFile".to_string())?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Package mcpFile must be a regular file".into());
    }
    if metadata.len() > MAX_MCP_CONFIG_BYTES {
        return Err("Package mcpFile is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let raw: Value = serde_json::from_str(&content)
        .map_err(|error| format!("invalid Package mcpFile: {error}"))?;
    let raw_object = raw
        .as_object()
        .ok_or_else(|| "Package mcpFile must be a JSON object".to_string())?;

    let source_servers = if let Some(value) = raw_object
        .get("mcpServers")
        .or_else(|| raw_object.get("mcp_servers"))
    {
        value
            .as_object()
            .ok_or_else(|| "Package mcpFile mcpServers must be an object".to_string())?
            .iter()
            .map(|(name, config)| (name.clone(), config.clone()))
            .collect::<Vec<_>>()
    } else {
        vec![("default".to_string(), raw)]
    };

    if source_servers.is_empty() {
        return Err("Package mcpFile must declare at least one MCP server".into());
    }
    if source_servers.len() > MAX_MCP_SERVERS_PER_PACKAGE {
        return Err(format!(
            "Package mcpFile cannot declare more than {MAX_MCP_SERVERS_PER_PACKAGE} servers"
        ));
    }

    let mut projected = Map::new();
    for (source_name, config) in source_servers {
        let source_name = normalize_mcp_server_name(&source_name)?;
        let config = validate_and_sanitize_mcp_server(manifest, &config)?;
        let generated_name = format!("{PACKAGE_MCP_SERVER_PREFIX}{}__{source_name}", manifest.id);
        projected.insert(generated_name, config);
    }
    Ok(projected)
}

fn validate_and_sanitize_mcp_server(
    manifest: &ConnectorPackageManifest,
    config: &Value,
) -> Result<Value, String> {
    let object = config
        .as_object()
        .ok_or_else(|| "Package MCP server configuration must be an object".to_string())?;
    validate_workbuddy_placeholders(config, &manifest.auth_fields)?;
    validate_string_map(object.get("env"), "env")?;
    validate_string_map(object.get("headers"), "headers")?;
    if object
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("Package MCP server cannot be disabled while its Package is enabled".into());
    }

    let transport = match object.get("type").and_then(Value::as_str) {
        Some("http") | Some("streamable-http") | Some("streamableHttp") => "streamable-http",
        Some("sse") => "sse",
        Some("stdio") => "stdio",
        Some(other) => return Err(format!("Unsupported Package MCP transport: {other}")),
        None if object.get("url").is_some() => "streamable-http",
        None => "stdio",
    };

    match transport {
        "stdio" => {
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Package stdio MCP server requires command".to_string())?;
            if !command_permission_allowed(&manifest.permissions.commands, command) {
                return Err(format!(
                    "Package MCP command '{command}' is not declared in permissions.commands"
                ));
            }
            validate_mcp_args(object)?;
        }
        "streamable-http" | "sse" => {
            let raw_url = object
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Package HTTP MCP server requires url".to_string())?;
            validate_package_mcp_url(raw_url, &manifest.permissions.network)?;
        }
        _ => unreachable!(),
    }

    let mut sanitized = object.clone();
    // WorkBuddy separates package-owned constants from user credentials.
    // Static values are safe to project as ordinary MCP env/headers, while
    // dynamic env/headers above remain forbidden at this trust boundary.
    if let Some(value) = sanitized.remove("staticEnv") {
        validate_string_map(Some(&value), "staticEnv")?;
        merge_string_map(&mut sanitized, "env", value)?;
    }
    if let Some(value) = sanitized.remove("staticHeaders") {
        validate_string_map(Some(&value), "staticHeaders")?;
        merge_string_map(&mut sanitized, "headers", value)?;
    }
    sanitized.remove("timeout");
    sanitized.remove("disabledTools");
    sanitized.remove("preAuth");
    sanitized.insert("type".into(), Value::String(transport.to_string()));
    sanitized.remove("disabled");
    Ok(Value::Object(sanitized))
}

fn validate_string_map(value: Option<&Value>, field: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let values = value
        .as_object()
        .ok_or_else(|| format!("Package MCP {field} must be an object"))?;
    if values.values().any(|value| !value.is_string()) {
        return Err(format!("Package MCP {field} values must be strings"));
    }
    Ok(())
}

fn merge_string_map(
    target: &mut Map<String, Value>,
    field: &str,
    overlay: Value,
) -> Result<(), String> {
    let mut merged = target
        .remove(field)
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| format!("Package MCP {field} must be an object"))
        })
        .transpose()?
        .unwrap_or_default();
    for (key, value) in overlay.as_object().cloned().unwrap_or_default() {
        merged.insert(key, value);
    }
    target.insert(field.into(), Value::Object(merged));
    Ok(())
}

fn validate_workbuddy_placeholders(
    value: &Value,
    fields: &[ConnectorAuthField],
) -> Result<(), String> {
    let declared = fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut used = std::collections::BTreeSet::new();
    collect_workbuddy_placeholders(value, &mut used)?;
    if let Some(key) = used.iter().find(|key| !declared.contains(key.as_str())) {
        return Err(format!(
            "WorkBuddy MCP references ${{{key}}}, but token-schema.json does not declare it"
        ));
    }
    Ok(())
}

fn collect_workbuddy_placeholders(
    value: &Value,
    result: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    match value {
        Value::String(value) => {
            let mut rest = value.as_str();
            while let Some(start) = rest.find("${") {
                let after = &rest[start + 2..];
                let Some(end) = after.find('}') else {
                    return Err("WorkBuddy MCP contains an unterminated ${KEY} placeholder".into());
                };
                let key = &after[..end];
                if !valid_workbuddy_placeholder_key(key) {
                    return Err(format!("Invalid WorkBuddy MCP placeholder '${{{key}}}'"));
                }
                result.insert(key.to_string());
                rest = &after[end + 1..];
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_workbuddy_placeholders(value, result)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_workbuddy_placeholders(value, result)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_mcp_args(object: &Map<String, Value>) -> Result<(), String> {
    let Some(args) = object.get("args") else {
        return Ok(());
    };
    let args = args
        .as_array()
        .ok_or_else(|| "Package MCP args must be an array".to_string())?;
    if args.len() > MAX_MCP_ARGS {
        return Err(format!(
            "Package MCP args cannot contain more than {MAX_MCP_ARGS} items"
        ));
    }
    for arg in args {
        let value = arg
            .as_str()
            .ok_or_else(|| "Package MCP args must contain strings only".to_string())?;
        if value.chars().count() > MAX_MCP_ARG_CHARS {
            return Err("Package MCP argument is too long".into());
        }
    }
    Ok(())
}

fn validate_package_mcp_url(raw_url: &str, allowed_networks: &[String]) -> Result<(), String> {
    let url = Url::parse(raw_url).map_err(|_| "Package MCP url is invalid".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Package MCP url must use http or https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Package MCP url cannot contain inline credentials".into());
    }
    for (key, value) in url.query_pairs() {
        let key = key.to_ascii_lowercase();
        if ["token", "key", "secret", "password", "auth", "credential"]
            .iter()
            .any(|sensitive| key.contains(sensitive))
            && !(value.starts_with("${") && value.ends_with('}'))
        {
            return Err("Package MCP url cannot contain credential query parameters".into());
        }
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Package MCP url must contain a host".to_string())?;
    if !network_permission_allowed(allowed_networks, host) {
        return Err(format!(
            "Package MCP host '{host}' is not declared in permissions.network"
        ));
    }
    Ok(())
}

fn command_permission_allowed(allowed_commands: &[String], command: &str) -> bool {
    allowed_commands.iter().any(|allowed| {
        let allowed = allowed.trim();
        allowed == "*" || allowed == command
    })
}

fn network_permission_allowed(allowed_networks: &[String], host: &str) -> bool {
    allowed_networks.iter().any(|allowed| {
        let allowed = allowed.trim();
        if allowed == "*" {
            return true;
        }
        let candidate = Url::parse(allowed)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_else(|| allowed.trim_start_matches("*.").to_string());
        if allowed.starts_with("*.") {
            host != candidate && host.ends_with(&format!(".{candidate}"))
        } else {
            host.eq_ignore_ascii_case(&candidate)
        }
    })
}

fn normalize_mcp_server_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 80
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(
            "Package MCP server names must use ASCII letters, digits, dot, dash, or underscore"
                .into(),
        );
    }
    Ok(name.to_string())
}

fn set_runtime_state(
    states: &mut ConnectorPackageStateFile,
    manifest: &ConnectorPackageManifest,
    status: ConnectorRuntimeStatus,
    last_error: Option<String>,
) -> bool {
    let state = state_mut(states, manifest);
    if state.runtime_status == status && state.last_error == last_error {
        return false;
    }
    state.runtime_status = status;
    state.last_error = last_error;
    true
}

fn write_package_mcp_config(paths: &WorkPaths, servers: &Map<String, Value>) -> Result<(), String> {
    let path = paths.work_connector_mcp_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Connector Package MCP config has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": servers,
    }))
    .map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".connector-package-mcp.{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

/// Mark a package trusted or untrusted. Trust is independent of enablement;
/// revoking trust also disables the package immediately.
pub fn set_trusted(package_id: &str, trusted: bool) -> Result<ConnectorPackageSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    set_trusted_with_paths(&paths, package_id, trusted)
}

pub fn set_trusted_with_paths(
    paths: &WorkPaths,
    package_id: &str,
    trusted: bool,
) -> Result<ConnectorPackageSummary, String> {
    let (manifest, _root) = find_installed(paths, package_id)?;
    let mut states = read_states(paths)?;
    let state = state_mut(&mut states, &manifest);
    state.trusted = trusted;
    if !trusted {
        state.enabled = false;
        state.runtime_status = ConnectorRuntimeStatus::NotReady;
    }
    state.last_error = None;
    let result_state = state.clone();
    write_states(paths, &states)?;
    if !trusted {
        crate::storage::profile_bindings::set_global_capability_binding_with_root(
            paths.data_root(),
            crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
            &manifest.id,
            false,
        )?;
    }
    Ok(ConnectorPackageSummary {
        manifest: manifest.clone(),
        state: effective_package_state(paths, &manifest, result_state),
    })
}

/// Enable or disable a package. A package must be trusted before it may be
/// enabled. If its CLI declares an npm install spec, enabling performs the
/// explicit, application-scoped install before recording enablement.
pub fn set_enabled(package_id: &str, enabled: bool) -> Result<ConnectorPackageSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    set_enabled_with_paths(&paths, package_id, enabled)
}

pub fn set_enabled_with_paths(
    paths: &WorkPaths,
    package_id: &str,
    enabled: bool,
) -> Result<ConnectorPackageSummary, String> {
    let (manifest, root) = find_installed(paths, package_id)?;
    let mut states = read_states(paths)?;
    let state = state_mut(&mut states, &manifest);
    if enabled && !state.trusted {
        return Err(format!(
            "Connector Package '{}' must be trusted before it can be enabled",
            manifest.id
        ));
    }
    if enabled
        && requires_credential_auth(&manifest)
        && state.auth_status != ConnectorAuthStatus::Authenticated
    {
        return Err(format!(
            "连接器“{}”尚未配置凭据，请先完成认证后再启用",
            manifest.display_name
        ));
    }
    if enabled && manifest.runtimes.contains(&ConnectorRuntimeKind::Cli) {
        if let Err(error) = ensure_managed_cli_runtime(paths, &root, &manifest) {
            state.enabled = false;
            state.runtime_status = ConnectorRuntimeStatus::Failed;
            state.last_error = Some(error.clone());
            write_states(paths, &states)?;
            let _ = crate::storage::profile_bindings::set_global_capability_binding_with_root(
                paths.data_root(),
                crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
                &manifest.id,
                false,
            );
            return Err(error);
        }
    }
    state.enabled = enabled;
    state.runtime_status = ConnectorRuntimeStatus::NotReady;
    state.last_error = None;
    let result_state = state.clone();
    write_states(paths, &states)?;
    crate::storage::profile_bindings::set_global_capability_binding_with_root(
        paths.data_root(),
        crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
        &manifest.id,
        enabled,
    )?;
    Ok(ConnectorPackageSummary {
        manifest: manifest.clone(),
        state: effective_package_state(paths, &manifest, result_state),
    })
}

/// Uninstall one package directory and remove its lifecycle state. The id is
/// validated before it is used to construct a deletion target.
pub fn uninstall(package_id: &str) -> Result<(), String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    uninstall_with_paths(&paths, package_id)
}

pub fn uninstall_with_paths(paths: &WorkPaths, package_id: &str) -> Result<(), String> {
    validate_package_id(package_id)?;
    let packages_dir = ensure_packages_dir(paths)?;
    let target = packages_dir.join(package_id.trim());
    let metadata = fs::symlink_metadata(&target).map_err(|error| {
        format!(
            "Connector Package '{}' is not installed: {error}",
            package_id
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Connector Package '{}' has an unsafe installation path",
            package_id
        ));
    }
    fs::remove_dir_all(&target).map_err(|error| error.to_string())?;

    let mut states = read_states(paths)?;
    states
        .packages
        .retain(|state| state.package_id != package_id.trim());
    write_states(paths, &states)?;
    let mut secrets = read_package_secrets(paths)?;
    if secrets.packages.remove(package_id.trim()).is_some() {
        write_package_secrets(paths, &secrets)?;
    }
    crate::storage::profile_bindings::remove_global_capability_binding_with_root(
        paths.data_root(),
        crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
        package_id.trim(),
    )
}

fn ensure_packages_dir(paths: &WorkPaths) -> Result<PathBuf, String> {
    let directory = paths.work_connector_packages_dir();
    if let Ok(metadata) = fs::symlink_metadata(&directory) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Connector Package root is not a safe directory: {}",
                directory.display()
            ));
        }
    } else {
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    }
    Ok(directory)
}

fn canonical_package_source(source: &Path) -> Result<PathBuf, String> {
    let canonical = source
        .canonicalize()
        .map_err(|error| format!("Connector Package source is not readable: {error}"))?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Connector Package source must be a regular directory".into());
    }
    reject_symlinks(&canonical)?;
    Ok(canonical)
}

fn read_and_validate_manifest(root: &Path) -> Result<ConnectorPackageManifest, String> {
    read_and_validate_workbuddy_manifest(root)
}

fn workbuddy_text(object: &Map<String, Value>, keys: &[&str]) -> String {
    for key in keys {
        match object.get(*key) {
            Some(Value::String(value)) if !value.trim().is_empty() => return value.trim().into(),
            Some(Value::Object(values)) => {
                for locale in ["zh", "zh-CN", "en", "default"] {
                    if let Some(value) = values.get(locale).and_then(Value::as_str) {
                        if !value.trim().is_empty() {
                            return value.trim().into();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    String::new()
}

fn workbuddy_skill_dirs(root: &Path, object: &Map<String, Value>) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(items) = object.get("skills").and_then(Value::as_array) {
        for item in items.iter().filter_map(Value::as_str) {
            let item = item.trim().trim_start_matches("./");
            if item.is_empty() {
                continue;
            }
            let path = root.join(item);
            let directory = if path.is_file() {
                path.parent().unwrap_or(root)
            } else {
                path.as_path()
            };
            if directory.join("SKILL.md").is_file() {
                let relative = directory.strip_prefix(root).unwrap_or(directory);
                let value = if relative.as_os_str().is_empty() {
                    ".".to_string()
                } else {
                    relative.to_string_lossy().replace('\\', "/")
                };
                if !result.contains(&value) {
                    result.push(value);
                }
            }
        }
    }
    if result.is_empty() {
        let skills = root.join("skills");
        if skills.join("SKILL.md").is_file() {
            result.push("skills".into());
        } else if let Ok(entries) = fs::read_dir(&skills) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").is_file() {
                    result.push(format!("skills/{}", entry.file_name().to_string_lossy()));
                }
            }
        }
    }
    result.sort();
    result.dedup();
    result
}

fn read_and_validate_workbuddy_manifest(root: &Path) -> Result<ConnectorPackageManifest, String> {
    let meta_path = root.join("connector-meta.json");
    let metadata = fs::symlink_metadata(&meta_path)
        .map_err(|error| format!("WorkBuddy connector-meta.json is missing: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("WorkBuddy connector-meta.json must be a regular file".into());
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err("WorkBuddy connector-meta.json is too large".into());
    }
    let value: Value =
        serde_json::from_str(&fs::read_to_string(&meta_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid WorkBuddy connector-meta.json: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "WorkBuddy connector-meta.json must be an object".to_string())?;
    let id = object
        .get("source")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    validate_package_id(&id)?;
    let package_type = object.get("type").and_then(Value::as_str).unwrap_or("mcp");
    if !matches!(package_type, "mcp" | "cli") {
        return Err(format!(
            "Unsupported WorkBuddy connector type '{package_type}'; MCP and CLI connectors are supported"
        ));
    }
    let runtime_file = if package_type == "mcp" {
        "mcp.json"
    } else {
        "cli.json"
    };
    let runtime_path = root.join(runtime_file);
    if !runtime_path.is_file() {
        return Err(format!(
            "WorkBuddy connector package is missing {runtime_file}"
        ));
    }
    let runtime_value: Value = serde_json::from_str(
        &fs::read_to_string(&runtime_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("invalid WorkBuddy {runtime_file}: {error}"))?;
    let mut network = Vec::new();
    let mut commands = Vec::new();
    let server_values = runtime_value
        .get("mcpServers")
        .and_then(Value::as_object)
        .map(|servers| servers.values().collect::<Vec<_>>())
        .unwrap_or_else(|| vec![&runtime_value]);
    for server in server_values.into_iter().filter_map(Value::as_object) {
        if let Some(url) = server.get("url").and_then(Value::as_str) {
            if let Ok(url) = Url::parse(url) {
                if let Some(host) = url.host_str() {
                    network.push(host.to_string());
                }
            }
        }
        if let Some(command) = server.get("command").and_then(Value::as_str) {
            if !command.trim().is_empty() {
                commands.push(command.trim().to_string());
            }
        }
    }
    if package_type == "cli" {
        if let Some(command) = runtime_value.get("command").and_then(Value::as_str) {
            if !command.trim().is_empty() {
                commands.push(command.trim().to_string());
            }
        }
    }
    network.sort();
    network.dedup();
    commands.sort();
    commands.dedup();
    let home_paths = if id == "feishu" {
        vec![
            ".lark-cli".to_string(),
            "Library/Application Support/lark-cli".to_string(),
        ]
    } else {
        Vec::new()
    };
    let skill_dirs = workbuddy_skill_dirs(root, object);
    let mut runtimes = vec![if package_type == "mcp" {
        ConnectorRuntimeKind::Mcp
    } else {
        ConnectorRuntimeKind::Cli
    }];
    if !skill_dirs.is_empty() {
        runtimes.push(ConnectorRuntimeKind::Skill);
    }
    let auth_fields = read_workbuddy_token_schema(root)?;
    let auth_mode = object
        .get("auth_mode")
        .or_else(|| object.get("authMode"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let auth = ConnectorAuthSpec {
        kind: if auth_mode == "oauth" || auth_mode == "oauth2" {
            ConnectorAuthKind::OAuth2
        } else if auth_mode == "cli" {
            ConnectorAuthKind::Cli
        } else if !auth_fields.is_empty() {
            ConnectorAuthKind::ApiKey
        } else {
            ConnectorAuthKind::None
        },
        provider: (auth_mode == "oauth" || auth_mode == "oauth2").then(|| "mcp_oauth".to_string()),
    };
    let icon = ["icon.svg", "icon.png", "icon.jpg", "icon.jpeg"]
        .into_iter()
        .find(|name| root.join(name).is_file())
        .map(str::to_string);
    let manifest = ConnectorPackageManifest {
        id: id.clone(),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("1.0.0")
            .to_string(),
        display_name: {
            let value = workbuddy_text(object, &["name_map", "name_zh", "name", "name_en"]);
            if value.is_empty() {
                id
            } else {
                value
            }
        },
        description: workbuddy_text(
            object,
            &[
                "description_map",
                "description_zh",
                "description",
                "description_en",
            ],
        ),
        icon,
        origin: if object.get("protected").and_then(Value::as_bool) == Some(true) {
            ConnectorPackageOrigin::Builtin
        } else {
            ConnectorPackageOrigin::Local
        },
        runtimes,
        auth,
        auth_fields,
        mcp_file: (package_type == "mcp").then(|| "mcp.json".into()),
        cli_file: (package_type == "cli").then(|| "cli.json".into()),
        skill_dirs,
        permissions: crate::work::connector_package::ConnectorPackagePermissions {
            network,
            commands,
            home_paths,
            ..Default::default()
        },
    };
    validate_manifest(&manifest)?;
    validate_package_files(root, &manifest)?;
    // Validate the actual MCP entries now so an invalid WorkBuddy package is
    // rejected during import rather than only on the next Work launch.
    if package_type == "mcp" {
        project_package_mcp_servers(root, &manifest)?;
    } else {
        read_cli_definition(root, &manifest)?;
    }
    reject_symlinks(root)?;
    Ok(manifest)
}

fn read_workbuddy_token_schema(root: &Path) -> Result<Vec<ConnectorAuthField>, String> {
    let path = root.join("token-schema.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("WorkBuddy token-schema.json must be a regular file".into());
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err("WorkBuddy token-schema.json is too large".into());
    }
    let value: Value =
        serde_json::from_str(&fs::read_to_string(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid WorkBuddy token-schema.json: {error}"))?;
    let fields = value
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| "WorkBuddy token-schema.json fields must be an array".to_string())?;
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for raw in fields {
        let field = raw
            .as_object()
            .ok_or_else(|| "WorkBuddy token-schema.json fields must be objects".to_string())?;
        let key = field
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !valid_workbuddy_placeholder_key(key) || !seen.insert(key.to_string()) {
            return Err(format!(
                "Invalid or duplicate WorkBuddy credential field '{key}'"
            ));
        }
        let field_type = field
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("password")
            .trim()
            .to_ascii_lowercase();
        if !matches!(field_type.as_str(), "text" | "password") {
            return Err(format!(
                "Unsupported WorkBuddy credential field type '{field_type}'"
            ));
        }
        let localized = |zh: &str, plain: &str, en: &str| {
            field
                .get(zh)
                .or_else(|| field.get(plain))
                .or_else(|| field.get(en))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let label = localized("label_zh", "label", "label_en");
        result.push(ConnectorAuthField {
            key: key.to_string(),
            label: if label.is_empty() {
                key.to_string()
            } else {
                label
            },
            field_type,
            required: field
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            placeholder: localized("placeholder_zh", "placeholder", "placeholder_en"),
            description: localized("description_zh", "description", "description_en"),
        });
    }
    if result.is_empty() {
        return Err("WorkBuddy token-schema.json must declare at least one field".into());
    }
    Ok(result)
}

fn valid_workbuddy_placeholder_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|value| value.is_ascii_alphabetic())
        && chars.all(|value| value.is_ascii_alphanumeric() || value == '_')
}

fn reject_symlinks(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Connector Package cannot contain symlinks: {}",
            root.display()
        ));
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let entry_metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if entry_metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package cannot contain symlinks: {}",
                path.display()
            ));
        }
        if entry_metadata.is_dir() {
            reject_symlinks(&path)?;
        } else if !entry_metadata.is_file() {
            return Err(format!(
                "Connector Package contains an unsupported filesystem entry: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn copy_package_tree(
    source: &Path,
    destination: &Path,
    total_bytes: &mut u64,
) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Connector Package cannot contain symlinks: {}",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            copy_package_tree(&source_path, &destination_path, total_bytes)?;
        } else if metadata.is_file() {
            *total_bytes = total_bytes.saturating_add(metadata.len());
            if *total_bytes > MAX_PACKAGE_BYTES {
                return Err(format!(
                    "Connector Package exceeds the {} MB size limit",
                    MAX_PACKAGE_BYTES / 1024 / 1024
                ));
            }
            fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        } else {
            return Err(format!(
                "Connector Package contains an unsupported filesystem entry: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn path_exists_without_following_symlink(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn read_states(paths: &WorkPaths) -> Result<ConnectorPackageStateFile, String> {
    let path = paths.work_connector_states_path();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ConnectorPackageStateFile {
                version: STATE_VERSION,
                packages: Vec::new(),
            });
        }
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err("connector-states.json cannot be a symlink".into());
    }
    if !metadata.is_file() {
        return Err("connector-states.json must be a regular file".into());
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err("connector-states.json is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut states: ConnectorPackageStateFile = serde_json::from_str(&content)
        .map_err(|error| format!("invalid connector-states.json: {error}"))?;
    states.version = STATE_VERSION;
    Ok(states)
}

fn write_states(paths: &WorkPaths, states: &ConnectorPackageStateFile) -> Result<(), String> {
    let path = paths.work_connector_states_path();
    let parent = path
        .parent()
        .ok_or_else(|| "connector-states.json has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(states).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".connector-states.{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn read_package_secrets(paths: &WorkPaths) -> Result<ConnectorPackageSecretsFile, String> {
    let path = paths.work_connector_secrets_path();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ConnectorPackageSecretsFile {
                version: STATE_VERSION,
                packages: BTreeMap::new(),
            });
        }
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("connector-package-secrets.json must be a regular file".into());
    }
    if metadata.len() > MAX_MCP_CONFIG_BYTES {
        return Err("connector-package-secrets.json is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut secrets: ConnectorPackageSecretsFile = serde_json::from_str(&content)
        .map_err(|error| format!("invalid connector-package-secrets.json: {error}"))?;
    secrets.version = STATE_VERSION;
    Ok(secrets)
}

fn write_package_secrets(
    paths: &WorkPaths,
    secrets: &ConnectorPackageSecretsFile,
) -> Result<(), String> {
    let path = paths.work_connector_secrets_path();
    let parent = path
        .parent()
        .ok_or_else(|| "connector-package-secrets.json has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(secrets).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".connector-package-secrets.{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn replace_workbuddy_placeholders(
    value: &mut Value,
    secrets: &BTreeMap<String, String>,
    package_id: &str,
) -> Result<(), String> {
    match value {
        Value::String(value) => {
            let mut used = std::collections::BTreeSet::new();
            collect_workbuddy_placeholders(&Value::String(value.clone()), &mut used)?;
            for key in used {
                let secret = secrets.get(&key).ok_or_else(|| {
                    format!("WorkBuddy connector '{package_id}' credential '{key}' is missing")
                })?;
                *value = value.replace(&format!("${{{key}}}"), secret);
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_workbuddy_placeholders(value, secrets, package_id)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_workbuddy_placeholders(value, secrets, package_id)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn read_state_for_manifest(
    states: &[ConnectorPackageState],
    manifest: &ConnectorPackageManifest,
) -> ConnectorPackageState {
    if let Some(state) = states
        .iter()
        .find(|state| state.package_id == manifest.id && state.version == manifest.version)
    {
        let mut state = state.clone();
        state.installed = true;
        return state;
    }
    let mut state = ConnectorPackageState::for_manifest(manifest);
    state.installed = true;
    state
}

fn effective_package_state(
    paths: &WorkPaths,
    manifest: &ConnectorPackageManifest,
    mut state: ConnectorPackageState,
) -> ConnectorPackageState {
    // Trust remains an independent safety gate. The global Capability Center
    // switch controls enablement only after a package is trusted.
    if !state.trusted
        || (requires_credential_auth(manifest)
            && state.auth_status != ConnectorAuthStatus::Authenticated)
    {
        state.enabled = false;
    } else if let Some(enabled) =
        crate::storage::profile_bindings::global_capability_override_with_root(
            paths.data_root(),
            crate::storage::profile_bindings::CAPABILITY_KIND_CONNECTOR,
            &manifest.id,
        )
    {
        state.enabled = enabled;
    }
    state
}

fn summary_for(
    paths: &WorkPaths,
    manifest: &ConnectorPackageManifest,
    states: &[ConnectorPackageState],
) -> ConnectorPackageSummary {
    ConnectorPackageSummary {
        manifest: manifest.clone(),
        state: effective_package_state(paths, manifest, read_state_for_manifest(states, manifest)),
    }
}

fn state_mut<'a>(
    states: &'a mut ConnectorPackageStateFile,
    manifest: &ConnectorPackageManifest,
) -> &'a mut ConnectorPackageState {
    if let Some(index) = states
        .packages
        .iter()
        .position(|state| state.package_id == manifest.id)
    {
        let state = &mut states.packages[index];
        if state.version != manifest.version {
            *state = ConnectorPackageState::for_manifest(manifest);
            state.installed = true;
        }
        return state;
    }
    let mut state = ConnectorPackageState::for_manifest(manifest);
    state.installed = true;
    states.packages.push(state);
    states.packages.last_mut().expect("state was just inserted")
}

fn find_installed(
    paths: &WorkPaths,
    package_id: &str,
) -> Result<(ConnectorPackageManifest, PathBuf), String> {
    validate_package_id(package_id)?;
    let root = ensure_packages_dir(paths)?.join(package_id.trim());
    let metadata = fs::symlink_metadata(&root).map_err(|error| {
        format!(
            "Connector Package '{}' is not installed: {error}",
            package_id
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Connector Package '{}' has an unsafe installation path",
            package_id
        ));
    }
    let manifest = read_and_validate_manifest(&root)?;
    if manifest.id != package_id.trim() {
        return Err("Connector Package id does not match its installation directory".into());
    }
    Ok((manifest, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::apps::models::{AppAccount, AppConnection};
    use crate::work::connector_package::{ConnectorPackageOrigin, ConnectorRuntimeKind};
    use tempfile::TempDir;

    fn setup_paths() -> (TempDir, WorkPaths) {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        (temp, paths)
    }

    #[test]
    fn builtin_catalog_projects_composio_apps_and_connection_state() {
        let (_temp, paths) = setup_paths();
        crate::work::apps::storage::save_connection(
            &paths,
            &AppConnection {
                connection_id: "conn_gmail_123".into(),
                app_id: "gmail".into(),
                provider: "native".into(),
                status: ConnectionStatus::Connected,
                accounts: vec![AppAccount {
                    account_id: "account_1".into(),
                    alias: Some("工作账号".into()),
                    display_name: Some("work@example.com".into()),
                    email: Some("work@example.com".into()),
                    status: ConnectionStatus::Connected,
                    created_at: "2026-08-22T00:00:00Z".into(),
                    last_used_at: None,
                }],
                last_checked_at: Some("2026-08-22T00:00:00Z".into()),
                created_at: "2026-08-22T00:00:00Z".into(),
                updated_at: "2026-08-22T00:00:00Z".into(),
            },
        )
        .unwrap();

        let catalog = list_builtin_catalog_with_paths(&paths).unwrap();
        assert_eq!(catalog.len(), 7);

        let gmail = catalog
            .iter()
            .find(|item| item.package_id == "gmail")
            .unwrap();
        assert!(gmail.builtin && gmail.installed && gmail.trusted && gmail.enabled);
        assert_eq!(gmail.origin, ConnectorPackageOrigin::Builtin);
        assert_eq!(gmail.connection_status, ConnectionStatus::Connected);
        assert_eq!(gmail.auth_status, ConnectorAuthStatus::Authenticated);
        assert_eq!(gmail.account_count, 1);
        assert_eq!(gmail.runtimes, vec![ConnectorRuntimeKind::Mcp]);
        assert_eq!(gmail.auth.kind, ConnectorAuthKind::OAuth2);
        assert_eq!(gmail.auth.provider.as_deref(), None);
    }

    #[test]
    fn migrates_legacy_builtin_feishu_manifest_to_workbuddy_meta() {
        let (_temp, paths) = setup_paths();
        let destination = paths.work_connector_packages_dir().join("feishu");
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            destination.join("connector.json"),
            r#"{"id":"feishu","version":"1.0.0","displayName":"Feishu / Lark"}"#,
        )
        .unwrap();
        ensure_builtin_feishu_package(&paths).unwrap();
        assert!(destination.join("connector-meta.json").is_file());
        assert!(!destination.join("connector.json").exists());
    }

    fn write_source_package(root: &Path) {
        fs::create_dir_all(root.join("skills/example")).unwrap();
        fs::write(
            root.join("connector-meta.json"),
            r#"{"source":"example.connector","name":"Example Connector","version":"1.0.0","type":"mcp","skills":["skills/example"]}"#,
        )
        .unwrap();
        fs::write(
            root.join("mcp.json"),
            r#"{"mcpServers":{"example":{"type":"stdio","command":"printf"}}}"#,
        )
        .unwrap();
        fs::write(root.join("skills/example/SKILL.md"), "# Example\n").unwrap();
    }

    fn write_http_source_package(root: &Path) {
        fs::write(
            root.join("connector-meta.json"),
            r#"{"source":"remote.connector","name":"Remote Connector","version":"1.0.0","type":"mcp"}"#,
        )
        .unwrap();
        fs::write(
            root.join("mcp.json"),
            r#"{"mcpServers":{"default":{"type":"streamable-http","url":"https://example.test/mcp","staticHeaders":{"X-Request-Source":"workbuddy"}}}}"#,
        )
        .unwrap();
    }

    fn write_token_source_package(root: &Path) {
        fs::write(
            root.join("connector-meta.json"),
            r#"{"source":"token.connector","name":"Token Connector","version":"1.0.0","type":"mcp","auth_mode":"token"}"#,
        )
        .unwrap();
        fs::write(
            root.join("token-schema.json"),
            r#"{"fields":[{"key":"API_KEY","label":"API Key","type":"password","required":true}]}"#,
        )
        .unwrap();
        fs::write(
            root.join("mcp.json"),
            r#"{"mcpServers":{"default":{"type":"streamableHttp","url":"https://example.test/mcp?key=${API_KEY}"}}}"#,
        )
        .unwrap();
    }

    fn write_cli_source_package(root: &Path) {
        fs::write(
            root.join("connector-meta.json"),
            r#"{"source":"cli.connector","name":"CLI Connector","version":"1.0.0","type":"cli","auth_mode":"cli"}"#,
        )
        .unwrap();
        fs::write(
            root.join("cli.json"),
            r#"{
              "command": "printf",
              "operations": {
                "status": { "args": ["authenticated"], "output": "text" },
                "auth": { "args": ["authenticated"] },
                "unAuth": { "args": ["logged out"] },
                "sync": { "args": ["sync"], "requiresConfirmation": false }
              }
            }"#,
        )
        .unwrap();
    }

    #[test]
    fn parses_explicit_and_workbuddy_style_application_cli_install_specs() {
        let source = TempDir::new().unwrap();
        write_cli_source_package(source.path());
        let manifest = read_and_validate_manifest(source.path()).unwrap();

        let mut cli: Value =
            serde_json::from_str(&fs::read_to_string(source.path().join("cli.json")).unwrap())
                .unwrap();
        let platform = match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            other => other,
        };
        cli["install"] = serde_json::json!({
            "packageManager": "npm",
            "package": "@example/demo",
            "version": "1.2.3"
        });
        fs::write(
            source.path().join("cli.json"),
            serde_json::to_vec_pretty(&cli).unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_cli_install_spec(source.path(), &manifest).unwrap(),
            Some("@example/demo@1.2.3".into())
        );

        cli["install"] = Value::Null;
        let mut init = Map::new();
        init.insert(
            platform.into(),
            Value::String("npm install -g @example/workbuddy-cli".into()),
        );
        cli["init"] = Value::Object(init);
        fs::write(
            source.path().join("cli.json"),
            serde_json::to_vec_pretty(&cli).unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_cli_install_spec(source.path(), &manifest).unwrap(),
            Some("@example/workbuddy-cli".into())
        );
    }

    #[test]
    fn package_lifecycle_requires_trust_and_persists_state() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_source_package(source.path());

        let installed = install_with_paths(&paths, source.path()).unwrap();
        assert!(installed.state.installed);
        assert!(!installed.state.trusted);
        assert!(!installed.state.enabled);
        assert!(set_enabled_with_paths(&paths, &installed.manifest.id, true).is_err());

        let trusted = set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();
        assert!(trusted.state.trusted);
        let enabled = set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap();
        assert!(enabled.state.enabled);

        let listed = list_with_paths(&paths).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].state.enabled);
        assert!(listed[0].state.trusted);

        let disabled = set_enabled_with_paths(&paths, &installed.manifest.id, false).unwrap();
        assert!(!disabled.state.enabled);
        set_trusted_with_paths(&paths, &installed.manifest.id, false).unwrap();
        assert!(!list_with_paths(&paths).unwrap()[0].state.trusted);

        uninstall_with_paths(&paths, &installed.manifest.id).unwrap();
        assert!(list_with_paths(&paths).unwrap().is_empty());
    }

    #[test]
    fn projects_trusted_enabled_mcp_package_into_runtime_config() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_http_source_package(source.path());

        let installed = install_with_paths(&paths, source.path()).unwrap();
        assert_eq!(sync_mcp_runtime(&paths).unwrap().projected_servers, 0);
        set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();
        set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap();

        let runtime = sync_mcp_runtime(&paths).unwrap();
        assert_eq!(runtime.projected_servers, 1);
        let generated: Value = serde_json::from_str(
            &fs::read_to_string(paths.work_connector_mcp_config_path()).unwrap(),
        )
        .unwrap();
        let servers = generated["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(
            servers["__agentcabin_package__remote.connector__default"]["url"],
            "https://example.test/mcp"
        );
        assert_eq!(
            servers["__agentcabin_package__remote.connector__default"]["headers"]
                ["X-Request-Source"],
            "workbuddy"
        );
        assert_eq!(
            list_with_paths(&paths).unwrap()[0].state.runtime_status,
            ConnectorRuntimeStatus::Ready
        );

        set_enabled_with_paths(&paths, &installed.manifest.id, false).unwrap();
        assert_eq!(sync_mcp_runtime(&paths).unwrap().projected_servers, 0);
        let generated: Value = serde_json::from_str(
            &fs::read_to_string(paths.work_connector_mcp_config_path()).unwrap(),
        )
        .unwrap();
        assert!(generated["mcpServers"].as_object().unwrap().is_empty());
    }

    #[test]
    fn workbuddy_token_schema_is_resolved_only_for_host_runtime() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_token_source_package(source.path());

        let installed = install_with_paths(&paths, source.path()).unwrap();
        assert_eq!(installed.manifest.auth.kind, ConnectorAuthKind::ApiKey);
        assert_eq!(installed.manifest.auth_fields[0].key, "API_KEY");
        set_trusted_with_paths(&paths, "token.connector", true).unwrap();
        assert!(set_enabled_with_paths(&paths, "token.connector", true).is_err());
        configure_token_auth_with_paths(
            &paths,
            "token.connector",
            BTreeMap::from([("API_KEY".into(), "secret-value".into())]),
        )
        .unwrap();
        set_enabled_with_paths(&paths, "token.connector", true).unwrap();
        sync_mcp_runtime(&paths).unwrap();

        let persisted = fs::read_to_string(paths.work_connector_mcp_config_path()).unwrap();
        assert!(persisted.contains("${API_KEY}"));
        assert!(!persisted.contains("secret-value"));
        let mut runtime: Value = serde_json::from_str(&persisted).unwrap();
        resolve_runtime_secrets(&paths, &mut runtime).unwrap();
        assert!(runtime.to_string().contains("secret-value"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(paths.work_connector_secrets_path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn projects_cli_package_and_resolves_fixed_operation() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_cli_source_package(source.path());

        let installed = install_with_paths(&paths, source.path()).unwrap();
        // The built-in Feishu CLI package is provisioned as part of every Work
        // runtime projection, so the custom package is not the only entry.
        assert_eq!(sync_cli_runtime(&paths).unwrap().projected_connectors, 1);
        set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();
        set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap();

        let runtime = sync_cli_runtime(&paths).unwrap();
        assert_eq!(runtime.projected_connectors, 2);
        let generated: Value = serde_json::from_str(
            &fs::read_to_string(paths.work_connector_cli_config_path()).unwrap(),
        )
        .unwrap();
        let connectors = generated["connectors"].as_array().unwrap();
        assert_eq!(connectors.len(), 2);
        let custom = connectors
            .iter()
            .find(|connector| connector["packageId"] == "cli.connector")
            .unwrap();
        assert_eq!(custom["definition"]["command"], "printf");

        let invocation =
            resolve_cli_invocation(&paths, "cli.connector", "status", &["--json".into()]).unwrap();
        assert_eq!(invocation.operation, "status");
        assert_eq!(invocation.command, "printf");
        assert_eq!(invocation.args, vec!["authenticated", "--json"]);
        assert_eq!(invocation.output, "text");
        assert_eq!(
            cli_operation_risk(&serde_json::json!({"operation": "status"})),
            ToolRiskClass::Read
        );

        set_auth_status_with_paths(&paths, "cli.connector", ConnectorAuthStatus::Authenticated)
            .unwrap();
        let custom = resolve_cli_invocation(&paths, "cli.connector", "sync", &[]).unwrap();
        assert_eq!(custom.args, vec!["sync"]);
        assert!(!custom.requires_confirmation);

        let redacted = redact_cli_output(
            r#"{"access_token":"secret-value","authenticated":true}"#,
            &[],
        );
        assert!(!redacted.contains("secret-value"));
        assert!(redacted.contains("***redacted***"));
    }

    #[test]
    fn feishu_cli_resolves_to_agentcabin_private_executable() {
        let (_temp, paths) = setup_paths();
        let private_cli = crate::work::apps::lark_cli::private_lark_cli_path(&paths);
        fs::create_dir_all(private_cli.parent().unwrap()).unwrap();
        fs::write(&private_cli, "private lark-cli fixture").unwrap();
        ensure_builtin_feishu_package(&paths).unwrap();
        set_auth_status_with_paths(&paths, "feishu", ConnectorAuthStatus::Authenticated).unwrap();

        let invocation = resolve_cli_invocation(
            &paths,
            "feishu",
            "sendMessage",
            &[
                "--as".into(),
                "bot".into(),
                "--user-id".into(),
                "ou_test".into(),
                "--text".into(),
                "hello".into(),
            ],
        )
        .unwrap();
        assert_eq!(invocation.command, private_cli.to_string_lossy());
        assert!(invocation.command.contains("cli-connector-packages"));
        assert_eq!(
            invocation.home_paths,
            vec![
                ".lark-cli".to_string(),
                "Library/Application Support/lark-cli".to_string()
            ]
        );
        assert_eq!(
            invocation.args,
            vec![
                "im",
                "+messages-send",
                "--as",
                "bot",
                "--user-id",
                "ou_test",
                "--text",
                "hello"
            ]
        );
    }

    #[test]
    fn oauth_cli_custom_operation_requests_host_auth_until_authenticated() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_cli_source_package(source.path());
        let manifest_path = source.path().join("connector-meta.json");
        let mut manifest: Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["auth_mode"] = Value::String("oauth".into());
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let installed = install_with_paths(&paths, source.path()).unwrap();
        set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();
        set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap();
        assert!(cli_requires_host_auth(
            &paths,
            &serde_json::json!({"package_id": "cli.connector", "operation": "sync"})
        )
        .unwrap());

        set_auth_status_with_paths(&paths, "cli.connector", ConnectorAuthStatus::Authenticated)
            .unwrap();
        assert!(!cli_requires_host_auth(
            &paths,
            &serde_json::json!({"package_id": "cli.connector", "operation": "sync"})
        )
        .unwrap());
    }

    #[test]
    fn package_skill_sources_only_include_trusted_enabled_skills() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_source_package(source.path());
        let installed = install_with_paths(&paths, source.path()).unwrap();

        assert!(package_skill_sources(&paths).unwrap().is_empty());
        set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();
        set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap();
        let sources = package_skill_sources(&paths).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].ends_with("skills/example"));
    }

    #[test]
    fn unauthenticated_connector_package_skills_and_mcp_are_not_injected() {
        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        fs::create_dir_all(source.path().join("skills/geo")).unwrap();
        fs::write(
            source.path().join("connector-meta.json"),
            r#"{"source":"geo.connector","name":"Geo Connector","version":"1.0.0","type":"mcp","auth_mode":"token","skills":["skills/geo"]}"#,
        )
        .unwrap();
        fs::write(
            source.path().join("token-schema.json"),
            r#"{"fields":[{"key":"API_KEY","label":"API Key","type":"password","required":true}]}"#,
        )
        .unwrap();
        fs::write(
            source.path().join("mcp.json"),
            r#"{"mcpServers":{"geo":{"type":"stdio","command":"printf"}}}"#,
        )
        .unwrap();
        fs::write(source.path().join("skills/geo/SKILL.md"), "# Geo Skill\n").unwrap();

        let installed = install_with_paths(&paths, source.path()).unwrap();
        set_trusted_with_paths(&paths, &installed.manifest.id, true).unwrap();

        // 1. 未配置凭据时，无法启用
        let enable_err = set_enabled_with_paths(&paths, &installed.manifest.id, true).unwrap_err();
        assert!(enable_err.contains("尚未配置凭据"));

        // 2. 技能未注入
        assert!(package_skill_sources(&paths).unwrap().is_empty());

        // 3. MCP 未加载
        let mcp_runtime = sync_mcp_runtime(&paths).unwrap();
        assert_eq!(mcp_runtime.projected_servers, 0);

        // 4. 配置凭据（自动激活并启用）
        configure_token_auth_with_paths(
            &paths,
            &installed.manifest.id,
            BTreeMap::from([("API_KEY".into(), "test-key".into())]),
        )
        .unwrap();

        // 5. 认证后已启用，技能和 MCP 正常注入
        let sources = package_skill_sources(&paths).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].ends_with("skills/geo"));

        let mcp_runtime = sync_mcp_runtime(&paths).unwrap();
        assert_eq!(mcp_runtime.projected_servers, 1);
    }

    #[test]
    fn legacy_connector_resource_directories_are_ignored() {
        let (_temp, paths) = setup_paths();
        let legacy = paths.work_connector_packages_dir().join("legacy-mcp");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("manifest.json"), "{}\n").unwrap();
        assert!(list_with_paths(&paths).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn package_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let (_temp, paths) = setup_paths();
        let source = TempDir::new().unwrap();
        write_source_package(source.path());
        symlink(
            source.path().join("mcp.json"),
            source.path().join("linked.json"),
        )
        .unwrap();
        assert!(install_with_paths(&paths, source.path()).is_err());
    }
}
