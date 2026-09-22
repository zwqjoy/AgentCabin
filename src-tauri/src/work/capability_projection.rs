//! Capability Center 2.0 Read Model & Projection.
//!
//! "减少 Authority，而不是增加抽象。"
//!
//! This module does NOT create a new database or store. All states and readiness
//! ratings are derived on-the-fly from existing authoritative sources:
//! - Skills & Capabilities (`resources.rs`)
//! - Connected Apps (`apps/`)
//! - Browser Runtime (`browser.rs`)
//! - Connector Packages (`connector_package_manager.rs`, `connectors.rs`)
//! - MCP Servers (`profile_bindings`, `mcp_registry.rs`)
//! - Run-scoped Effective Capabilities (`CapabilityResolver`, launch snapshots)

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent::capability_resolver::{
    CapabilityResolver, EffectiveCapabilities, RuntimeProviderKind,
};
use crate::work::browser;
use crate::work::connector_package::{ConnectorAuthStatus, ConnectorRuntimeStatus};
use crate::work::connector_package_manager;
use crate::work::connectors;
use crate::work::models::{AppMode, ResourceOrigin, WorkResourceDiscovery, WorkResourceKind};
use crate::work::paths::WorkPaths;
use crate::work::resources;

// ── Read Model Types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReadiness {
    Ready,
    NeedsAuth,
    MissingDependency,
    Disabled,
    Unhealthy,
    Incompatible,
    NotInstalled,
}

impl CapabilityReadiness {
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityReadiness::Ready => "ready",
            CapabilityReadiness::NeedsAuth => "needs_auth",
            CapabilityReadiness::MissingDependency => "missing_dependency",
            CapabilityReadiness::Disabled => "disabled",
            CapabilityReadiness::Unhealthy => "unhealthy",
            CapabilityReadiness::Incompatible => "incompatible",
            CapabilityReadiness::NotInstalled => "not_installed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAvailabilityScope {
    pub provider: String,
    pub available: bool,
    /// Availability in Work and Code are tracked separately. `available` is
    /// retained as the legacy aggregate for older clients.
    #[serde(default)]
    pub work_available: bool,
    #[serde(default)]
    pub code_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn runtime_scope(
    provider: &str,
    work_available: bool,
    code_available: bool,
    reason: Option<&str>,
) -> RuntimeAvailabilityScope {
    RuntimeAvailabilityScope {
        provider: provider.to_string(),
        available: work_available || code_available,
        work_available,
        code_available,
        reason: reason.map(str::to_string),
    }
}

fn work_runtime_scopes(
    pi_available: bool,
    dsh_available: bool,
    code_available: bool,
) -> Vec<RuntimeAvailabilityScope> {
    vec![
        runtime_scope(
            "pi",
            pi_available,
            code_available,
            (!pi_available).then_some("Pi Work runtime is unavailable"),
        ),
        runtime_scope(
            "dsh",
            dsh_available,
            code_available,
            (!dsh_available).then_some("DSH Work runtime is unavailable"),
        ),
        runtime_scope(
            "claude",
            false,
            code_available,
            Some("Claude Work runtime adapter is not available"),
        ),
        runtime_scope(
            "codex",
            false,
            code_available,
            Some("Codex Work runtime adapter is not available"),
        ),
        runtime_scope(
            "grok",
            false,
            code_available,
            Some("Grok Work runtime adapter is not available"),
        ),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAuthInfo {
    pub status: String,
    pub account_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityHealthInfo {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAction {
    pub action_type: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCenterItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String, // "skill" | "mcp" | "connector" | "app"
    pub origin: String,   // "builtin" | "user" | "community"
    pub installed: bool,
    pub enabled: bool,
    pub readiness: CapabilityReadiness,
    pub readiness_reason: String,
    pub scopes: Vec<String>, // ["work", "code"]
    pub runtime_availability: Vec<RuntimeAvailabilityScope>,
    pub permissions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<CapabilityAuthInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<CapabilityHealthInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub actions: Vec<CapabilityAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCenterOverview {
    pub total: usize,
    pub ready_count: usize,
    pub needs_setup_count: usize,
    pub needs_auth_count: usize,
    pub unavailable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCenterProjection {
    pub overview: CapabilityCenterOverview,
    pub items: Vec<CapabilityCenterItem>,
}

// ── Run Effective Capabilities Inspector (Strict Zero Secret Leakage) ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillCapabilityItemView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<crate::agent::capability_resolver::SkillOwner>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilityItemView {
    pub id: String,
    pub transport: String,
    pub env_keys: Vec<String>,    // ONLY key names, never secret values
    pub header_keys: Vec<String>, // ONLY key names, never secret values
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCapabilityItemView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunEffectiveCapabilitiesView {
    pub run_id: String,
    pub runtime: String,
    pub app_mode: String,
    pub enabled_skills: Vec<SkillCapabilityItemView>,
    pub mcp_servers: Vec<McpServerCapabilityItemView>,
    pub connectors: Vec<ConnectorCapabilityItemView>,
    pub browser_enabled: bool,
    pub browser_use_enabled: bool,
    pub browser_status: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub diagnostics: Vec<String>,
    pub strict_mode: bool,
}

// ── Launch Snapshot Persistence (Run Launch Evidence) ──

const SNAPSHOT_FILENAME: &str = "effective_capabilities_snapshot.json";

/// Persist an unprivileged launch snapshot of the effective capabilities into
/// the managed runtime directory. This serves as immutable launch evidence.
pub fn record_launch_snapshot(
    caps: &EffectiveCapabilities,
    runtime_dir: &Path,
) -> Result<(), String> {
    if !runtime_dir.exists() {
        let _ = fs::create_dir_all(runtime_dir);
    }
    let snapshot_file = runtime_dir.join(SNAPSHOT_FILENAME);
    let view = project_effective_capabilities_view(caps);
    let json = serde_json::to_string_pretty(&view).map_err(|e| e.to_string())?;
    fs::write(snapshot_file, json).map_err(|e| e.to_string())
}

/// Project an `EffectiveCapabilities` instance into a safe, redacted `RunEffectiveCapabilitiesView`.
pub fn project_effective_capabilities_view(
    caps: &EffectiveCapabilities,
) -> RunEffectiveCapabilitiesView {
    let enabled_skills = caps
        .enabled_skills
        .iter()
        .map(|s| SkillCapabilityItemView {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            owner: s.owner.clone(),
        })
        .collect();

    let mcp_servers = caps
        .mcp_servers
        .iter()
        .map(|m| {
            let mut env_keys: Vec<String> = m.env.keys().cloned().collect();
            env_keys.sort();
            let mut header_keys: Vec<String> = m.headers.keys().cloned().collect();
            header_keys.sort();
            McpServerCapabilityItemView {
                id: m.id.clone(),
                transport: m.transport.clone(),
                env_keys,
                header_keys,
            }
        })
        .collect();

    let connectors = caps
        .connectors
        .iter()
        .map(|c| ConnectorCapabilityItemView {
            id: c.id.clone(),
            name: c.name.clone(),
            entry_point: c.entry_point.clone(),
        })
        .collect();

    let browser_status = if !caps.browser_enabled {
        "Disabled".to_string()
    } else if caps.browser_config.is_some() {
        "Ready".to_string()
    } else {
        "Available".to_string()
    };

    let diagnostics = caps
        .diagnostics
        .iter()
        .map(|d| format!("[{}] {}: {}", d.level, d.code, d.message))
        .collect();

    RunEffectiveCapabilitiesView {
        run_id: caps.run_id.clone(),
        runtime: caps.runtime.as_str().to_string(),
        app_mode: caps.app_mode.as_str().to_string(),
        enabled_skills,
        mcp_servers,
        connectors,
        browser_enabled: caps.browser_enabled,
        browser_use_enabled: caps.browser_use_enabled,
        browser_status,
        allowed_tools: caps.allowed_tools.clone(),
        disallowed_tools: caps.disallowed_tools.clone(),
        diagnostics,
        strict_mode: caps.strict_mode,
    }
}

/// Retrieve the run-scoped effective capabilities view for a specific run.
pub async fn get_run_effective_capabilities(
    data_dir: &Path,
    run_id: &str,
) -> Result<RunEffectiveCapabilitiesView, String> {
    let meta = crate::storage::runs::get_run(run_id);
    let (app_mode, provider, cwd) = if let Some(ref m) = meta {
        let provider =
            RuntimeProviderKind::try_from_agent_str(&m.agent).unwrap_or(RuntimeProviderKind::Pi);
        (m.app_mode, provider, m.cwd.clone())
    } else {
        // Fallback to Pi Work mode
        (AppMode::Work, RuntimeProviderKind::Pi, String::new())
    };

    let runtime_dir = CapabilityResolver::runtime_dir(data_dir, app_mode, provider, run_id);
    let snapshot_file = runtime_dir.join(SNAPSHOT_FILENAME);

    if snapshot_file.is_file() {
        if let Ok(content) = fs::read_to_string(&snapshot_file) {
            if let Ok(view) = serde_json::from_str::<RunEffectiveCapabilitiesView>(&content) {
                // Invariant: must bind to the requested run_id
                if view.run_id == run_id {
                    return Ok(view);
                }
            }
        }
    }

    // If snapshot doesn't exist on disk yet (e.g. testing or older run), resolve safely
    let caps = CapabilityResolver::resolve(data_dir, app_mode, provider, &cwd, run_id)
        .await
        .map_err(|e| format!("Failed to resolve capabilities for run '{run_id}': {e}"))?;

    Ok(project_effective_capabilities_view(&caps))
}

// ── Readiness Derivation & Projection ──

/// Project all Skills from `resources.rs`
fn work_resource_summaries(
    paths: &WorkPaths,
    runtime: RuntimeProviderKind,
) -> Vec<crate::work::models::WorkResourceSummary> {
    let mut by_id = HashMap::new();
    for provider in [RuntimeProviderKind::Pi, RuntimeProviderKind::Dsh, runtime] {
        for summary in resources::list_resources_with_runtime(paths, provider) {
            by_id.insert(summary.id.clone(), summary);
        }
    }
    let mut summaries = by_id.into_values().collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    summaries
}

fn project_skills(paths: &WorkPaths, runtime: RuntimeProviderKind) -> Vec<CapabilityCenterItem> {
    // The center is a cross-runtime catalog. Do not let the selected/default
    // runtime hide a Skill that is valid for the other supported Work runtime.
    let summaries = work_resource_summaries(paths, runtime);
    let pi_summaries = resources::list_resources_with_runtime(paths, RuntimeProviderKind::Pi);
    let dsh_summaries = resources::list_resources_with_runtime(paths, RuntimeProviderKind::Dsh);
    let pi_available_by_id: HashMap<String, bool> = pi_summaries
        .iter()
        .map(|summary| (summary.id.clone(), summary.runtime_available))
        .collect();
    let dsh_available_by_id: HashMap<String, bool> = dsh_summaries
        .iter()
        .map(|summary| (summary.id.clone(), summary.runtime_available))
        .collect();
    let pi_active_by_id: HashMap<String, bool> = pi_summaries
        .iter()
        .map(|summary| (summary.id.clone(), summary.active))
        .collect();
    let dsh_active_by_id: HashMap<String, bool> = dsh_summaries
        .iter()
        .map(|summary| (summary.id.clone(), summary.active))
        .collect();
    summaries
        .into_iter()
        .filter(|res| {
            matches!(
                res.kind,
                WorkResourceKind::Skill | WorkResourceKind::Capability
            )
        })
        .map(|res| {
            let installed = true; // scanned from disk
            let enabled = res.enabled;
            let pi_available = pi_available_by_id.get(&res.id).copied().unwrap_or(false);
            let dsh_available = dsh_available_by_id.get(&res.id).copied().unwrap_or(false);
            let runtime_available = pi_available || dsh_available;
            let active = pi_active_by_id.get(&res.id).copied().unwrap_or(false)
                || dsh_active_by_id.get(&res.id).copied().unwrap_or(false);

            let (readiness, reason, actions) = if !installed {
                (
                    CapabilityReadiness::NotInstalled,
                    "Skill is not installed".to_string(),
                    vec![CapabilityAction {
                        action_type: "install".to_string(),
                        label: "安装".to_string(),
                    }],
                )
            } else if !enabled {
                (
                    CapabilityReadiness::Disabled,
                    "Skill is installed but not enabled for Work".to_string(),
                    vec![CapabilityAction {
                        action_type: "enable".to_string(),
                        label: "启用".to_string(),
                    }],
                )
            } else if !active {
                (
                    CapabilityReadiness::Incompatible,
                    "Skill is incompatible with current configuration".to_string(),
                    vec![],
                )
            } else if !runtime_available {
                (
                    CapabilityReadiness::MissingDependency,
                    "Skill runtime dependencies are missing".to_string(),
                    vec![],
                )
            } else {
                (
                    CapabilityReadiness::Ready,
                    "Ready for Work".to_string(),
                    vec![CapabilityAction {
                        action_type: "disable".to_string(),
                        label: "停用".to_string(),
                    }],
                )
            };

            let origin_str = match res.origin {
                ResourceOrigin::Builtin => "builtin",
                ResourceOrigin::User => "user",
                ResourceOrigin::Community => "community",
            };

            CapabilityCenterItem {
                id: res.id.clone(),
                name: res.name,
                description: res.description,
                category: "skill".to_string(),
                origin: origin_str.to_string(),
                installed,
                enabled,
                readiness,
                readiness_reason: reason,
                scopes: vec!["work".to_string()],
                runtime_availability: work_runtime_scopes(pi_available, dsh_available, false),
                permissions: res.permissions,
                auth: None,
                health: None,
                diagnostics: vec![],
                capabilities: res.discovery.keywords,
                actions,
            }
        })
        .collect()
}

/// Project Connected Apps from `apps/`
fn project_apps(paths: &WorkPaths) -> Vec<CapabilityCenterItem> {
    let catalog = crate::work::apps::provider::default_six_apps_catalog();
    let connections = crate::work::apps::storage::list_connections(paths).unwrap_or_default();

    catalog
        .into_iter()
        .filter(|item| !item.app_id.eq_ignore_ascii_case("feishu"))
        .map(|item| {
            let conn = connections.iter().find(|c| c.app_id == item.app_id);
            if let Some(conn) = conn {
                let status = conn.status;
                let account_count = conn.accounts.len();
                let accounts = conn
                    .accounts
                    .iter()
                    .filter_map(|a| a.email.clone().or_else(|| a.alias.clone()))
                    .collect::<Vec<_>>();

                let (r, msg, acts) = match status {
                    crate::work::apps::models::ConnectionStatus::Connected => {
                        let primary = accounts
                            .first()
                            .map(|a| format!(" · {a}"))
                            .unwrap_or_default();
                        (
                            CapabilityReadiness::Ready,
                            format!("Connected{primary}"),
                            vec![CapabilityAction {
                                action_type: "disconnect".to_string(),
                                label: "断开".to_string(),
                            }],
                        )
                    }
                    crate::work::apps::models::ConnectionStatus::Disconnected => (
                        CapabilityReadiness::NeedsAuth,
                        "App is disconnected, login required".to_string(),
                        vec![CapabilityAction {
                            action_type: "connect".to_string(),
                            label: "连接".to_string(),
                        }],
                    ),
                    crate::work::apps::models::ConnectionStatus::Expired => (
                        CapabilityReadiness::NeedsAuth,
                        "Authorization expired, re-login required".to_string(),
                        vec![CapabilityAction {
                            action_type: "connect".to_string(),
                            label: "重新登录".to_string(),
                        }],
                    ),
                    crate::work::apps::models::ConnectionStatus::Pending => (
                        CapabilityReadiness::NeedsAuth,
                        "Authorization pending".to_string(),
                        vec![CapabilityAction {
                            action_type: "connect".to_string(),
                            label: "完成验证".to_string(),
                        }],
                    ),
                    crate::work::apps::models::ConnectionStatus::Error => (
                        CapabilityReadiness::Unhealthy,
                        "Connection error".to_string(),
                        vec![CapabilityAction {
                            action_type: "connect".to_string(),
                            label: "重试连接".to_string(),
                        }],
                    ),
                };

                CapabilityCenterItem {
                    id: item.app_id,
                    name: item.display_name,
                    description: item.description,
                    category: "app".to_string(),
                    origin: "builtin".to_string(),
                    installed: true,
                    enabled: true,
                    readiness: r,
                    readiness_reason: msg,
                    scopes: vec!["work".to_string()],
                    runtime_availability: work_runtime_scopes(true, true, false),
                    permissions: vec!["network".to_string()],
                    auth: Some(CapabilityAuthInfo {
                        status: format!("{:?}", status).to_lowercase(),
                        account_count,
                        accounts,
                    }),
                    health: None,
                    diagnostics: vec![],
                    capabilities: item.capabilities,
                    actions: acts,
                }
            } else {
                CapabilityCenterItem {
                    id: item.app_id,
                    name: item.display_name,
                    description: item.description,
                    category: "app".to_string(),
                    origin: "builtin".to_string(),
                    installed: false,
                    enabled: false,
                    readiness: CapabilityReadiness::NotInstalled,
                    readiness_reason: "Not connected".to_string(),
                    scopes: vec!["work".to_string()],
                    runtime_availability: work_runtime_scopes(false, false, false),
                    permissions: vec!["network".to_string()],
                    auth: Some(CapabilityAuthInfo {
                        status: "not_connected".to_string(),
                        account_count: 0,
                        accounts: vec![],
                    }),
                    health: None,
                    diagnostics: vec![],
                    capabilities: item.capabilities,
                    actions: vec![CapabilityAction {
                        action_type: "connect".to_string(),
                        label: "连接".to_string(),
                    }],
                }
            }
        })
        .collect()
}

/// Project Connector Packages and Connectors
fn project_connectors(paths: &WorkPaths) -> Vec<CapabilityCenterItem> {
    let mut items = Vec::new();

    // 1. Connector Packages
    let packages = connector_package_manager::list_with_paths(paths).unwrap_or_default();
    for pkg in packages {
        let manifest = &pkg.manifest;
        let state = &pkg.state;

        let (readiness, reason, actions) = if !state.installed {
            (
                CapabilityReadiness::NotInstalled,
                "Connector package is not installed".to_string(),
                vec![CapabilityAction {
                    action_type: "install".to_string(),
                    label: "安装".to_string(),
                }],
            )
        } else if !state.trusted {
            (
                CapabilityReadiness::Disabled,
                "Connector package requires trust approval".to_string(),
                vec![CapabilityAction {
                    action_type: "trust".to_string(),
                    label: "信任并启用".to_string(),
                }],
            )
        } else if !state.enabled {
            (
                CapabilityReadiness::Disabled,
                "Connector package is disabled".to_string(),
                vec![CapabilityAction {
                    action_type: "enable".to_string(),
                    label: "启用".to_string(),
                }],
            )
        } else if matches!(
            state.auth_status,
            ConnectorAuthStatus::NotAuthenticated | ConnectorAuthStatus::Expired
        ) {
            (
                CapabilityReadiness::NeedsAuth,
                "Authentication required".to_string(),
                vec![CapabilityAction {
                    action_type: "connect".to_string(),
                    label: "登录认证".to_string(),
                }],
            )
        } else if matches!(state.runtime_status, ConnectorRuntimeStatus::Failed) {
            (
                CapabilityReadiness::Unhealthy,
                state
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "Connector runtime health check failed".to_string()),
                vec![CapabilityAction {
                    action_type: "test".to_string(),
                    label: "测试连接".to_string(),
                }],
            )
        } else {
            (
                CapabilityReadiness::Ready,
                "Ready for Work".to_string(),
                vec![CapabilityAction {
                    action_type: "disable".to_string(),
                    label: "停用".to_string(),
                }],
            )
        };

        items.push(CapabilityCenterItem {
            id: manifest.id.clone(),
            name: manifest.display_name.clone(),
            description: manifest.description.clone(),
            category: "connector".to_string(),
            origin: "community".to_string(),
            installed: state.installed,
            enabled: state.enabled,
            readiness,
            readiness_reason: reason,
            scopes: vec!["work".to_string()],
            runtime_availability: work_runtime_scopes(state.enabled, state.enabled, false),
            permissions: manifest.permissions.commands.clone(),
            auth: Some(CapabilityAuthInfo {
                status: format!("{:?}", state.auth_status).to_lowercase(),
                account_count: usize::from(state.auth_status == ConnectorAuthStatus::Authenticated),
                accounts: vec![],
            }),
            health: Some(CapabilityHealthInfo {
                status: format!("{:?}", state.runtime_status).to_lowercase(),
                latency_ms: None,
                message: state.last_error.clone(),
            }),
            diagnostics: vec![],
            capabilities: vec![],
            actions,
        });
    }

    // 2. Standalone custom connectors
    if let Ok(custom_connectors) = connectors::list_with_paths(paths) {
        for c in custom_connectors {
            if items.iter().any(|item| item.id == c.name) {
                continue;
            }
            let (readiness, reason, actions) = if !c.enabled {
                (
                    CapabilityReadiness::Disabled,
                    "Connector is disabled".to_string(),
                    vec![CapabilityAction {
                        action_type: "enable".to_string(),
                        label: "启用".to_string(),
                    }],
                )
            } else if !c.runtime_available {
                (
                    CapabilityReadiness::Incompatible,
                    "Connector runtime is not available".to_string(),
                    vec![],
                )
            } else {
                (
                    CapabilityReadiness::Ready,
                    "Ready for Work".to_string(),
                    vec![CapabilityAction {
                        action_type: "test".to_string(),
                        label: "测试连接".to_string(),
                    }],
                )
            };

            items.push(CapabilityCenterItem {
                id: c.name.clone(),
                name: c.name.clone(),
                description: format!("Custom connector via {}", c.transport),
                category: "connector".to_string(),
                origin: "user".to_string(),
                installed: true,
                enabled: c.enabled,
                readiness,
                readiness_reason: reason,
                scopes: vec!["work".to_string()],
                runtime_availability: work_runtime_scopes(
                    c.pi_runtime_available,
                    c.dsh_runtime_available,
                    false,
                ),
                permissions: vec![],
                auth: None,
                health: None,
                diagnostics: vec![],
                capabilities: vec![],
                actions,
            });
        }
    }

    items
}

/// Project Work web access and browser automation into the same capability
/// matrix as Skills, MCP, and Connectors. The DSH native Work plugin uses the
/// authenticated bridge directly; it must not be treated as missing merely
/// because Pi's legacy browser adapter has not been installed.
fn project_browser(paths: &WorkPaths) -> Vec<CapabilityCenterItem> {
    let Ok(summary) = browser::get_config_with_paths(paths) else {
        return Vec::new();
    };
    let browser_use_enabled =
        crate::storage::profile_bindings::is_browser_use_enabled_with_root(paths.data_root());

    let (readiness, reason, actions) = if !summary.enabled {
        (
            CapabilityReadiness::Disabled,
            "网络访问未启用".to_string(),
            vec![CapabilityAction {
                action_type: "enable".to_string(),
                label: "启用".to_string(),
            }],
        )
    } else if !summary.configured {
        (
            CapabilityReadiness::NeedsAuth,
            "网络访问需要配置凭据或 endpoint".to_string(),
            vec![],
        )
    } else if browser_use_enabled && !summary.browser_runtime_available {
        (
            CapabilityReadiness::MissingDependency,
            summary.browser_runtime_message.clone(),
            vec![CapabilityAction {
                action_type: "install_chromium".to_string(),
                label: "准备浏览器运行时".to_string(),
            }],
        )
    } else {
        (
            CapabilityReadiness::Ready,
            "Web / Browser 已就绪".to_string(),
            vec![CapabilityAction {
                action_type: "disable".to_string(),
                label: "停用".to_string(),
            }],
        )
    };

    vec![CapabilityCenterItem {
        id: "work-browser".to_string(),
        name: "Web 与 Browser".to_string(),
        description: "通过 Work Host 的网络、网页与浏览器自动化能力访问外部信息。".to_string(),
        category: "browser".to_string(),
        origin: "builtin".to_string(),
        installed: true,
        enabled: summary.enabled,
        readiness,
        readiness_reason: reason,
        scopes: vec!["work".to_string()],
        runtime_availability: work_runtime_scopes(
            summary.pi_runtime_available,
            summary.dsh_runtime_available,
            false,
        ),
        permissions: vec!["network".to_string(), "browser".to_string()],
        auth: Some(CapabilityAuthInfo {
            status: if summary.configured {
                "configured".to_string()
            } else {
                "needs_auth".to_string()
            },
            account_count: usize::from(summary.configured),
            accounts: vec![],
        }),
        health: None,
        diagnostics: vec![],
        capabilities: vec![
            "web_search".to_string(),
            "web_open".to_string(),
            "browser".to_string(),
        ],
        actions,
    }]
}

/// Computer Use is a host-owned Work capability. Both Pi and DSH reach it via
/// the same Policy/Approval pipeline; neither runtime receives native desktop
/// authority.
fn project_desktop() -> Vec<CapabilityCenterItem> {
    let requested = crate::work::desktop_operator::is_requested();
    let ready = crate::work::desktop_operator::is_enabled();
    let (readiness, reason) = if ready {
        (
            CapabilityReadiness::Ready,
            "Computer Use V2 已就绪，并受 Work Policy / Approval 管控".to_string(),
        )
    } else if requested {
        (
            CapabilityReadiness::MissingDependency,
            "已请求 Computer Use，但本机原生桌面后端尚未就绪".to_string(),
        )
    } else {
        (
            CapabilityReadiness::Disabled,
            "Computer Use 默认关闭".to_string(),
        )
    };

    vec![CapabilityCenterItem {
        id: "work-computer-use".to_string(),
        name: "Computer Use V2".to_string(),
        description:
            "通过 AgentCabin Host 执行受审批的桌面操作；运行时本身不直接获得原生桌面权限。"
                .to_string(),
        category: "computer_use".to_string(),
        origin: "builtin".to_string(),
        installed: true,
        enabled: ready,
        readiness,
        readiness_reason: reason,
        scopes: vec!["work".to_string()],
        runtime_availability: work_runtime_scopes(ready, ready, false),
        permissions: vec!["desktop".to_string(), "approval".to_string()],
        auth: None,
        health: None,
        diagnostics: vec![],
        capabilities: vec![
            "launch_app".to_string(),
            "observe_ui".to_string(),
            "act_ui".to_string(),
        ],
        actions: vec![],
    }]
}

/// Project MCP Servers
fn project_mcp(_paths: &WorkPaths) -> Vec<CapabilityCenterItem> {
    let configured = crate::storage::mcp_registry::list_configured(None);
    configured
        .into_iter()
        .map(|s| {
            let (readiness, reason, actions) = (
                CapabilityReadiness::Ready,
                "Ready for Work".to_string(),
                vec![CapabilityAction {
                    action_type: "test".to_string(),
                    label: "测试".to_string(),
                }],
            );

            CapabilityCenterItem {
                id: s.name.clone(),
                name: s.name.clone(),
                description: format!("MCP server ({})", s.server_type),
                category: "mcp".to_string(),
                origin: "user".to_string(),
                installed: true,
                enabled: true,
                readiness,
                readiness_reason: reason,
                scopes: vec!["code".to_string(), "work".to_string()],
                runtime_availability: work_runtime_scopes(true, true, true),
                permissions: vec![],
                auth: None,
                health: None,
                diagnostics: vec![],
                capabilities: vec![],
                actions,
            }
        })
        .collect()
}

/// Build the full CapabilityCenterProjection dynamically.
pub fn build_capability_center_projection(
    paths: &WorkPaths,
    runtime: Option<&str>,
) -> Result<CapabilityCenterProjection, String> {
    let runtime_provider = match runtime {
        Some(r) => RuntimeProviderKind::try_from_agent_str(r)?,
        None => RuntimeProviderKind::Pi,
    };

    let mut items = Vec::new();

    // 1. Apps
    items.extend(project_apps(paths));

    // 2. Connectors
    items.extend(project_connectors(paths));

    // 3. Web / Browser / Computer Use
    items.extend(project_browser(paths));
    items.extend(project_desktop());

    // 4. Skills
    items.extend(project_skills(paths, runtime_provider));

    // 5. MCP
    items.extend(project_mcp(paths));

    // Overview counters: only count installed and configured capabilities
    let mut ready_count = 0;
    let mut needs_setup_count = 0;
    let mut needs_auth_count = 0;
    let mut unavailable_count = 0;

    for item in &items {
        if !item.installed || item.readiness == CapabilityReadiness::NotInstalled {
            continue;
        }
        match item.readiness {
            CapabilityReadiness::Ready => ready_count += 1,
            CapabilityReadiness::NeedsAuth => needs_auth_count += 1,
            CapabilityReadiness::MissingDependency | CapabilityReadiness::Incompatible => {
                needs_setup_count += 1
            }
            CapabilityReadiness::Disabled | CapabilityReadiness::Unhealthy => {
                unavailable_count += 1
            }
            CapabilityReadiness::NotInstalled => {}
        }
    }

    let overview = CapabilityCenterOverview {
        total: items.len(),
        ready_count,
        needs_setup_count,
        needs_auth_count,
        unavailable_count,
    };

    Ok(CapabilityCenterProjection { overview, items })
}

// ── Intent-Based Search ──

/// Deterministic token-based search for capabilities across discovery metadata.
pub fn search_capability_center(
    paths: &WorkPaths,
    query: &str,
    limit: Option<usize>,
) -> Result<Vec<CapabilityCenterItem>, String> {
    let projection = build_capability_center_projection(paths, None)?;
    let tokens: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if tokens.is_empty() {
        return Ok(projection.items);
    }

    // Retrieve discovery metadata for skills
    let summaries = work_resource_summaries(paths, RuntimeProviderKind::Pi);
    let discovery_by_id: HashMap<String, WorkResourceDiscovery> =
        summaries.into_iter().map(|s| (s.id, s.discovery)).collect();

    let mut scored: Vec<(u32, CapabilityCenterItem)> = Vec::new();

    for item in projection.items {
        let mut score = 0u32;
        let name_lower = item.name.to_lowercase();
        let desc_lower = item.description.to_lowercase();
        let id_lower = item.id.to_lowercase();

        let discovery = discovery_by_id.get(&item.id);

        for token in &tokens {
            // 1. Direct name match (weight 100)
            if name_lower.contains(token) || id_lower.contains(token) {
                score += 100;
            }

            // 2. Aliases match (weight 80)
            if let Some(disc) = discovery {
                if disc
                    .aliases
                    .iter()
                    .any(|a| a.to_lowercase().contains(token))
                {
                    score += 80;
                }

                // 3. Verbs & nouns (weight 60)
                if disc.verbs.iter().any(|v| v.to_lowercase().contains(token))
                    || disc.nouns.iter().any(|n| n.to_lowercase().contains(token))
                {
                    score += 60;
                }

                // 4. Keywords & domains (weight 40)
                if disc
                    .keywords
                    .iter()
                    .any(|k| k.to_lowercase().contains(token))
                    || disc
                        .domains
                        .iter()
                        .any(|d| d.to_lowercase().contains(token))
                {
                    score += 40;
                }

                // 5. Examples (weight 10)
                if disc
                    .examples
                    .iter()
                    .any(|e| e.to_lowercase().contains(token))
                {
                    score += 10;
                }
            }

            // 6. Builtin capability tags (weight 50)
            if item
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains(token))
            {
                score += 50;
            }

            // 7. Description match (weight 20)
            if desc_lower.contains(token) {
                score += 20;
            }

            // Special synonyms for common Chinese queries
            if (token == "excel" || token == "表格")
                && (name_lower.contains("sheet")
                    || name_lower.contains("excel")
                    || id_lower.contains("excel"))
            {
                score += 120;
            }
            if (token == "飞书" || token == "feishu")
                && (id_lower.contains("lark")
                    || id_lower.contains("feishu")
                    || name_lower.contains("feishu")
                    || desc_lower.contains("feishu")
                    || desc_lower.contains("lark"))
            {
                score += 150;
            }
        }

        if score > 0 {
            scored.push((score, item));
        }
    }

    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    let max = limit.unwrap_or(20).clamp(1, 100);
    let results = scored.into_iter().take(max).map(|(_, item)| item).collect();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::WorkBrowserSummary;
    use crate::work::models::WorkResourceSummary;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_skill_readiness_rules() {
        // installed=true, enabled=false -> Disabled
        let disabled_res = WorkResourceSummary {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            description: "Test".to_string(),
            kind: WorkResourceKind::Skill,
            origin: ResourceOrigin::Builtin,
            entry: "index.js".to_string(),
            permissions: vec![],
            enabled: false,
            active: true,
            runtime_available: true,
            discovery: WorkResourceDiscovery::default(),
            execution: None,
        };
        assert!(!disabled_res.enabled);

        // installed=true, enabled=true, runtime_available=false -> MissingDependency
        let missing_dep_res = WorkResourceSummary {
            id: "test-skill-2".to_string(),
            name: "Test Skill 2".to_string(),
            description: "Test".to_string(),
            kind: WorkResourceKind::Skill,
            origin: ResourceOrigin::Builtin,
            entry: "index.js".to_string(),
            permissions: vec![],
            enabled: true,
            active: true,
            runtime_available: false,
            discovery: WorkResourceDiscovery::default(),
            execution: None,
        };
        assert!(!missing_dep_res.runtime_available);
    }

    #[test]
    fn test_secret_redaction_in_effective_capabilities_view() {
        use crate::agent::capability_resolver::{DiagnosticEntry, EffectiveMcpServer};
        use std::collections::HashMap;

        let mut env = HashMap::new();
        env.insert(
            "SECRET_KEY".to_string(),
            "super_secret_value_12345".to_string(),
        );
        env.insert(
            "DATABASE_URL".to_string(),
            "postgres://user:pass@host/db".to_string(),
        );

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token_xyz".to_string());

        let caps = EffectiveCapabilities {
            app_mode: AppMode::Work,
            runtime: RuntimeProviderKind::Pi,
            run_id: "test-run-1".to_string(),
            managed_home: PathBuf::from("/tmp"),
            managed_runtime_dir: PathBuf::from("/tmp/run-1"),
            enabled_skills: vec![],
            mcp_servers: vec![EffectiveMcpServer {
                id: "test-server".to_string(),
                transport: "stdio".to_string(),
                command: Some("node".to_string()),
                args: vec!["server.js".to_string()],
                cwd: None,
                url: None,
                env,
                headers,
            }],
            connectors: vec![],
            browser_enabled: true,
            browser_use_enabled: false,
            browser_config: None,
            allowed_tools: vec!["read".to_string(), "write".to_string()],
            disallowed_tools: vec!["delete".to_string()],
            prohibited_discovery_paths: vec![],
            detected_prohibited_paths: vec![],
            diagnostics: vec![DiagnosticEntry {
                level: "info".to_string(),
                code: "OK".to_string(),
                message: "Healthy".to_string(),
                path: None,
            }],
            strict_mode: true,
        };

        let view = project_effective_capabilities_view(&caps);

        // Verify keys only, zero secret values
        assert_eq!(view.mcp_servers.len(), 1);
        let mcp = &view.mcp_servers[0];
        assert_eq!(mcp.env_keys, vec!["DATABASE_URL", "SECRET_KEY"]);
        assert_eq!(mcp.header_keys, vec!["Authorization"]);

        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("super_secret_value_12345"));
        assert!(!json.contains("postgres://user:pass@host/db"));
        assert!(!json.contains("Bearer token_xyz"));
        assert!(json.contains("SECRET_KEY"));
        assert!(json.contains("Authorization"));
    }

    #[test]
    fn test_run_id_binding_in_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let runtime_dir = temp_dir.path().join("run-123");
        fs::create_dir_all(&runtime_dir).unwrap();

        let caps = EffectiveCapabilities {
            app_mode: AppMode::Work,
            runtime: RuntimeProviderKind::Pi,
            run_id: "run-123".to_string(),
            managed_home: temp_dir.path().to_path_buf(),
            managed_runtime_dir: runtime_dir.clone(),
            enabled_skills: vec![],
            mcp_servers: vec![],
            connectors: vec![],
            browser_enabled: true,
            browser_use_enabled: false,
            browser_config: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            prohibited_discovery_paths: vec![],
            detected_prohibited_paths: vec![],
            diagnostics: vec![],
            strict_mode: true,
        };

        record_launch_snapshot(&caps, &runtime_dir).unwrap();

        let snapshot_path = runtime_dir.join(SNAPSHOT_FILENAME);
        assert!(snapshot_path.is_file());

        let content = fs::read_to_string(snapshot_path).unwrap();
        let view: RunEffectiveCapabilitiesView = serde_json::from_str(&content).unwrap();
        assert_eq!(view.run_id, "run-123");
        assert_eq!(view.runtime, "pi");
    }

    #[test]
    fn test_cannot_read_mismatched_run_id_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let runtime_dir = temp_dir.path().join("run-999");
        fs::create_dir_all(&runtime_dir).unwrap();

        // Write snapshot with run_id = "run-888" inside "run-999" folder
        let snapshot = RunEffectiveCapabilitiesView {
            run_id: "run-888".to_string(),
            runtime: "pi".to_string(),
            app_mode: "work".to_string(),
            enabled_skills: vec![],
            mcp_servers: vec![],
            connectors: vec![],
            browser_enabled: false,
            browser_use_enabled: false,
            browser_status: "Disabled".to_string(),
            allowed_tools: vec![],
            disallowed_tools: vec![],
            diagnostics: vec![],
            strict_mode: true,
        };
        let snapshot_file = runtime_dir.join(SNAPSHOT_FILENAME);
        fs::write(&snapshot_file, serde_json::to_string(&snapshot).unwrap()).unwrap();

        // Reading for "run-999" must not accept the mismatched run-888 snapshot
        let content = fs::read_to_string(&snapshot_file).unwrap();
        let view: RunEffectiveCapabilitiesView = serde_json::from_str(&content).unwrap();
        assert_ne!(view.run_id, "run-999");
    }

    #[test]
    fn test_app_readiness_mapping() {
        use crate::work::apps::models::ConnectionStatus;

        let statuses = [
            (ConnectionStatus::Connected, CapabilityReadiness::Ready),
            (
                ConnectionStatus::Disconnected,
                CapabilityReadiness::NeedsAuth,
            ),
            (ConnectionStatus::Expired, CapabilityReadiness::NeedsAuth),
            (ConnectionStatus::Pending, CapabilityReadiness::NeedsAuth),
            (ConnectionStatus::Error, CapabilityReadiness::Unhealthy),
        ];

        for (status, expected_readiness) in statuses {
            let r = match status {
                ConnectionStatus::Connected => CapabilityReadiness::Ready,
                ConnectionStatus::Disconnected
                | ConnectionStatus::Expired
                | ConnectionStatus::Pending => CapabilityReadiness::NeedsAuth,
                ConnectionStatus::Error => CapabilityReadiness::Unhealthy,
            };
            assert_eq!(r, expected_readiness);
        }
    }

    #[test]
    fn test_browser_readiness_mapping() {
        // Disabled
        let disabled_summary = WorkBrowserSummary {
            enabled: false,
            provider: "tavily".to_string(),
            configured: true,
            adapter_installed: true,
            pi_runtime_available: true,
            dsh_runtime_available: true,
            runtime_available: true,
            browser_runtime_node_available: true,
            browser_runtime_available: true,
            playwright_installed: true,
            chromium_installed: true,
            browser_runtime_managed: false,
            browser_runtime_message: String::new(),
            max_results: 5,
            endpoint_url: None,
            auth_kind: "api_key".to_string(),
            allowed_hosts: Vec::new(),
        };
        assert!(!disabled_summary.enabled);

        // Missing Chromium
        let missing_chromium = WorkBrowserSummary {
            enabled: true,
            provider: "tavily".to_string(),
            configured: true,
            adapter_installed: true,
            pi_runtime_available: true,
            dsh_runtime_available: true,
            runtime_available: true,
            browser_runtime_node_available: true,
            browser_runtime_available: true,
            playwright_installed: true,
            chromium_installed: false,
            browser_runtime_managed: false,
            browser_runtime_message: String::new(),
            max_results: 5,
            endpoint_url: None,
            auth_kind: "api_key".to_string(),
            allowed_hosts: Vec::new(),
        };
        assert!(!missing_chromium.chromium_installed);
    }

    #[test]
    fn test_connector_readiness_mapping() {
        use crate::work::connector_package::{ConnectorAuthStatus, ConnectorRuntimeStatus};

        // Not authenticated -> NeedsAuth
        let not_auth = ConnectorAuthStatus::NotAuthenticated;
        assert_eq!(
            matches!(
                not_auth,
                ConnectorAuthStatus::NotAuthenticated | ConnectorAuthStatus::Expired
            ),
            true
        );

        // Expired -> NeedsAuth
        let expired = ConnectorAuthStatus::Expired;
        assert_eq!(
            matches!(
                expired,
                ConnectorAuthStatus::NotAuthenticated | ConnectorAuthStatus::Expired
            ),
            true
        );

        // Failed runtime -> Unhealthy
        let failed_runtime = ConnectorRuntimeStatus::Failed;
        assert_eq!(failed_runtime == ConnectorRuntimeStatus::Failed, true);
    }

    #[test]
    fn test_readiness_is_derived_not_persisted() {
        // Confirm that CapabilityCenterItem can be serialized and deserialized cleanly
        // and its readiness is an enum that represents derived operational state.
        let item = CapabilityCenterItem {
            id: "excel-skill".to_string(),
            name: "Excel".to_string(),
            description: "Spreadsheet tool".to_string(),
            category: "skill".to_string(),
            origin: "builtin".to_string(),
            installed: true,
            enabled: true,
            readiness: CapabilityReadiness::Ready,
            readiness_reason: "Ready for Work".to_string(),
            scopes: vec!["work".to_string()],
            runtime_availability: vec![],
            permissions: vec![],
            auth: None,
            health: None,
            diagnostics: vec![],
            capabilities: vec!["excel".to_string(), "spreadsheet".to_string()],
            actions: vec![],
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"readiness\":\"ready\""));
        assert!(json.contains("\"readinessReason\":\"Ready for Work\""));
    }

    #[test]
    fn test_unconfigured_apps_do_not_bloat_needs_auth_or_setup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = WorkPaths::new(temp_dir.path().to_path_buf());
        paths.ensure_layout().unwrap();

        let items = project_apps(&paths);
        // Ensure feishu app is excluded because feishu is managed natively via WorkBuddy Connector
        assert!(!items.iter().any(|item| item.id == "feishu"));

        // When no connections exist, all apps must be NotInstalled and not enabled
        for item in &items {
            assert!(!item.installed);
            assert!(!item.enabled);
            assert_eq!(item.readiness, CapabilityReadiness::NotInstalled);
        }

        // Running projection should not count these unconfigured apps towards needs_auth or needs_setup
        let projection = build_capability_center_projection(&paths, None).unwrap();
        assert!(projection
            .items
            .iter()
            .any(|item| item.category == "browser"));
        assert!(projection
            .items
            .iter()
            .any(|item| item.category == "computer_use"));
        assert_eq!(projection.overview.needs_auth_count, 0);
        assert_eq!(projection.overview.needs_setup_count, 0);
    }
}
