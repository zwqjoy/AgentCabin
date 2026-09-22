use crate::storage::{self, agent_plugins};
use crate::work::connector_package_manager;
use crate::work::paths::WorkPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TworkSyncReport {
    pub found_twork_dir: Option<String>,
    pub imported_experts: Vec<String>,
    pub skipped_experts: Vec<String>,
    pub imported_connectors: Vec<String>,
    pub skipped_connectors: Vec<String>,
    pub errors: Vec<String>,
}

pub fn locate_twork_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TWORK_DIR") {
        let p = PathBuf::from(dir.trim());
        if p.exists() {
            return Some(p);
        }
    }

    let fixed = PathBuf::from("/Users/cengwenqi/个人/Apps/T-Work");
    if fixed.exists() {
        return Some(fixed);
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(parent) = cwd.parent() {
            let sibling = parent.join("T-Work");
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }

    None
}

pub fn sync_from_twork_if_present() -> Result<TworkSyncReport, String> {
    let Some(twork_root) = locate_twork_dir() else {
        return Ok(TworkSyncReport::default());
    };

    let mut report = TworkSyncReport {
        found_twork_dir: Some(twork_root.display().to_string()),
        ..Default::default()
    };

    let data_dir = storage::data_dir();
    let paths = WorkPaths::app();
    let _ = paths.ensure_layout();

    // 1. Sync experts from twork/experts
    let experts_dir = twork_root.join("experts");
    if experts_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&experts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let plugin_json = path.join(".codebuddy-plugin").join("plugin.json");
                if !plugin_json.is_file() {
                    continue;
                }

                let plugin_id = match fs::read_to_string(&plugin_json) {
                    Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(val) => val
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            }),
                        Err(_) => path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    },
                    Err(_) => path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                };

                let installed_dest =
                    agent_plugins::agent_plugin_packages_dir_with_root(&data_dir).join(&plugin_id);
                if installed_dest.exists() {
                    report.skipped_experts.push(plugin_id);
                    continue;
                }

                log::info!(
                    "[twork/sync] Installing expert/team: {plugin_id} from {}",
                    path.display()
                );
                match agent_plugins::install_agent_plugin_with_root(
                    &data_dir,
                    &path.to_string_lossy(),
                ) {
                    Ok(_) => {
                        let _ = agent_plugins::set_agent_plugin_trust_with_root(
                            &data_dir, &plugin_id, true,
                        );
                        let _ = agent_plugins::set_agent_plugin_binding_with_root(
                            &data_dir, &plugin_id, true,
                        );
                        report.imported_experts.push(plugin_id);
                    }
                    Err(err) => {
                        log::error!("[twork/sync] Failed to install expert {plugin_id}: {err}");
                        report
                            .errors
                            .push(format!("Failed to install expert {plugin_id}: {err}"));
                    }
                }
            }
        }
    }

    // 2. Sync connectors from twork/connectors
    let connectors_dir = twork_root.join("connectors");
    if connectors_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&connectors_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let meta_json = path.join("connector-meta.json");
                if !meta_json.is_file() {
                    continue;
                }

                let pkg_id = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let installed_dest = paths.work_connector_packages_dir().join(&pkg_id);
                if installed_dest.exists() {
                    report.skipped_connectors.push(pkg_id);
                    continue;
                }

                log::info!(
                    "[twork/sync] Installing connector: {pkg_id} from {}",
                    path.display()
                );
                match connector_package_manager::install_with_paths(&paths, &path) {
                    Ok(summary) => {
                        let _ = connector_package_manager::set_trusted_with_paths(
                            &paths,
                            &summary.manifest.id,
                            true,
                        );
                        let _ = connector_package_manager::set_enabled_with_paths(
                            &paths,
                            &summary.manifest.id,
                            true,
                        );
                        report.imported_connectors.push(summary.manifest.id);
                    }
                    Err(err) => {
                        log::error!("[twork/sync] Failed to install connector {pkg_id}: {err}");
                        report
                            .errors
                            .push(format!("Failed to install connector {pkg_id}: {err}"));
                    }
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_twork_import() {
        let report = sync_from_twork_if_present().expect("Sync should run without panic");
        println!("T-Work Sync Report: {:?}", report);
        assert!(report.found_twork_dir.is_some());
    }
}
