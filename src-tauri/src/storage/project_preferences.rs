//! AgentCabin-owned preferences shared by every conversation in one Code project.
//!
//! These values are project-scoped, but they are application state rather than
//! source-controlled project input. Keep them under AgentCabin's data directory
//! instead of either the repository or the global user settings document.

use crate::models::ProjectModelPreference;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static PROJECT_PREFERENCES_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectPreferencesFile {
    version: u8,
    cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote_host_name: Option<String>,
    #[serde(default)]
    agents: HashMap<String, ProjectModelPreference>,
    updated_at: String,
}

fn normalize_cwd(cwd: &str) -> Result<String, String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Err("Project cwd is required".into());
    }
    if trimmed.len() > 8192 || trimmed.contains('\0') {
        return Err("Invalid project cwd".into());
    }
    let normalized = trimmed.trim_end_matches(['/', '\\']);
    Ok(if normalized.is_empty() {
        trimmed.to_string()
    } else {
        normalized.to_string()
    })
}

fn normalize_host(remote_host_name: Option<&str>) -> Result<Option<String>, String> {
    let Some(host) = remote_host_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if host.len() > 255 || host.contains('\0') {
        return Err("Invalid remote host name".into());
    }
    Ok(Some(host.to_string()))
}

fn validate_agent(agent: &str) -> Result<&str, String> {
    let agent = agent.trim();
    if agent.is_empty()
        || agent.len() > 64
        || !agent
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Invalid project preference agent".into());
    }
    Ok(agent)
}

fn project_id(cwd: &str, remote_host_name: Option<&str>) -> String {
    let identity = format!("{}\0{}", remote_host_name.unwrap_or("local"), cwd);
    let digest = Sha256::digest(identity.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn preference_path(root: &Path, cwd: &str, remote_host_name: Option<&str>) -> PathBuf {
    root.join("projects")
        .join(project_id(cwd, remote_host_name))
        .join("settings.json")
}

fn read_file(
    root: &Path,
    cwd: &str,
    remote_host_name: Option<&str>,
) -> Result<ProjectPreferencesFile, String> {
    let path = preference_path(root, cwd, remote_host_name);
    match fs::read_to_string(&path) {
        Ok(content) => {
            let stored: ProjectPreferencesFile = serde_json::from_str(&content)
                .map_err(|error| format!("Invalid project settings: {error}"))?;
            if stored.cwd != cwd || stored.remote_host_name.as_deref() != remote_host_name {
                return Err("Project settings identity mismatch".into());
            }
            Ok(stored)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProjectPreferencesFile {
            version: 1,
            cwd: cwd.to_string(),
            remote_host_name: remote_host_name.map(str::to_string),
            agents: HashMap::new(),
            updated_at: crate::models::now_iso(),
        }),
        Err(error) => Err(format!("Failed to read project settings: {error}")),
    }
}

fn write_file(root: &Path, stored: &ProjectPreferencesFile) -> Result<(), String> {
    let path = preference_path(root, &stored.cwd, stored.remote_host_name.as_deref());
    let parent = path
        .parent()
        .ok_or_else(|| "Project settings path has no parent".to_string())?;
    crate::storage::ensure_dir(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(stored).map_err(|error| error.to_string())?;
    let temp = parent.join(format!("settings.json.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, format!("{content}\n")).map_err(|error| format!("write tmp: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("chmod tmp: {error}"))?;
    }
    fs::rename(&temp, &path).map_err(|error| format!("rename: {error}"))
}

pub fn get(
    cwd: &str,
    remote_host_name: Option<&str>,
    agent: &str,
) -> Result<ProjectModelPreference, String> {
    get_with_root(&crate::storage::data_dir(), cwd, remote_host_name, agent)
}

fn get_with_root(
    root: &Path,
    cwd: &str,
    remote_host_name: Option<&str>,
    agent: &str,
) -> Result<ProjectModelPreference, String> {
    let cwd = normalize_cwd(cwd)?;
    let host = normalize_host(remote_host_name)?;
    let agent = validate_agent(agent)?;
    let stored = read_file(root, &cwd, host.as_deref())?;
    Ok(stored.agents.get(agent).cloned().unwrap_or_default())
}

pub fn update(
    cwd: &str,
    remote_host_name: Option<&str>,
    agent: &str,
    patch: Value,
) -> Result<ProjectModelPreference, String> {
    update_with_root(
        &crate::storage::data_dir(),
        cwd,
        remote_host_name,
        agent,
        patch,
    )
}

fn update_with_root(
    root: &Path,
    cwd: &str,
    remote_host_name: Option<&str>,
    agent: &str,
    patch: Value,
) -> Result<ProjectModelPreference, String> {
    let _guard = PROJECT_PREFERENCES_LOCK
        .lock()
        .map_err(|_| "Project settings lock poisoned".to_string())?;
    let cwd = normalize_cwd(cwd)?;
    let host = normalize_host(remote_host_name)?;
    let agent = validate_agent(agent)?.to_string();
    let patch = patch
        .as_object()
        .ok_or_else(|| "Project preference patch must be an object".to_string())?;
    if patch
        .keys()
        .any(|key| !matches!(key.as_str(), "model" | "effort" | "permission_mode"))
    {
        return Err("Project preference patch contains an unsupported field".into());
    }

    let mut stored = read_file(root, &cwd, host.as_deref())?;
    let preference = stored.agents.entry(agent).or_default();
    for (key, value) in patch {
        let target = match key.as_str() {
            "model" => &mut preference.model,
            "effort" => &mut preference.effort,
            "permission_mode" => &mut preference.permission_mode,
            _ => unreachable!(),
        };
        *target = if value.is_null() {
            None
        } else {
            Some(
                value
                    .as_str()
                    .ok_or_else(|| format!("Project preference {key} must be a string or null"))?
                    .to_string(),
            )
        };
    }
    let result = preference.clone();
    stored.updated_at = crate::models::now_iso();
    write_file(root, &stored)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_are_shared_by_project_and_separated_by_agent() {
        let root = tempfile::tempdir().unwrap();
        update_with_root(
            root.path(),
            "/repo/demo/",
            None,
            "pi",
            serde_json::json!({"model": "provider/model", "permission_mode": "accept_edits"}),
        )
        .unwrap();

        let pi = get_with_root(root.path(), "/repo/demo", None, "pi").unwrap();
        assert_eq!(pi.model.as_deref(), Some("provider/model"));
        assert_eq!(pi.permission_mode.as_deref(), Some("accept_edits"));
        assert_eq!(
            get_with_root(root.path(), "/repo/demo", None, "codex")
                .unwrap()
                .model,
            None
        );
        assert_eq!(
            get_with_root(root.path(), "/repo/demo", Some("build-host"), "pi")
                .unwrap()
                .model,
            None
        );
    }

    #[test]
    fn partial_updates_preserve_other_project_fields() {
        let root = tempfile::tempdir().unwrap();
        update_with_root(
            root.path(),
            "/repo/demo",
            None,
            "pi",
            serde_json::json!({"model": "m1", "effort": "high"}),
        )
        .unwrap();
        let updated = update_with_root(
            root.path(),
            "/repo/demo",
            None,
            "pi",
            serde_json::json!({"model": "m2"}),
        )
        .unwrap();
        assert_eq!(updated.model.as_deref(), Some("m2"));
        assert_eq!(updated.effort.as_deref(), Some("high"));
    }
}
