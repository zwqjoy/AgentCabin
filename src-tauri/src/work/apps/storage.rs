use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::work::apps::error::AppError;
use crate::work::apps::models::{
    AppConnection, AppSecretsFile, AppsConfigFile, WorkspaceAppDefaultAccount,
};
use crate::work::paths::WorkPaths;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024; // 1 MB
const MAX_SECRETS_BYTES: u64 = 1024 * 1024; // 1 MB

fn atomic_write_json<T: Serialize>(path: &Path, value: &T, secret: bool) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::Storage(format!("Path has no parent directory: {}", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;

    let serialized = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Storage(format!("Failed to serialize JSON: {e}")))?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{serialized}\n"))
        .map_err(|e| AppError::Storage(format!("Failed to write temporary file: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if secret { 0o600 } else { 0o644 };
        if let Err(e) = fs::set_permissions(&temporary, fs::Permissions::from_mode(mode)) {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::Storage(format!(
                "Failed to set file permissions: {e}"
            )));
        }
    }

    if let Err(e) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::Storage(format!(
            "Failed to rename temporary file to destination: {e}"
        )));
    }

    Ok(())
}

pub fn read_config(paths: &WorkPaths) -> Result<AppsConfigFile, AppError> {
    let path = paths.work_apps_config_path();
    if !path.is_file() {
        return Ok(AppsConfigFile::default());
    }
    let metadata = fs::metadata(&path).map_err(|e| AppError::Storage(e.to_string()))?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(AppError::Storage("Apps config file is too large".into()));
    }
    let content = fs::read_to_string(&path).map_err(|e| AppError::Storage(e.to_string()))?;
    let config: AppsConfigFile = serde_json::from_str(&content)
        .map_err(|e| AppError::Storage(format!("Failed to parse apps config: {e}")))?;
    Ok(config)
}

pub fn write_config(paths: &WorkPaths, config: &AppsConfigFile) -> Result<(), AppError> {
    atomic_write_json(&paths.work_apps_config_path(), config, false)
}

pub fn read_secrets(paths: &WorkPaths) -> Result<AppSecretsFile, AppError> {
    let path = paths.work_apps_secrets_path();
    if !path.is_file() {
        let legacy_path = paths.legacy_work_apps_secrets_path();
        if !legacy_path.is_file() {
            return Ok(AppSecretsFile::default());
        }

        let migrated = read_secrets_file(&legacy_path)?;
        write_secrets(paths, &migrated)?;
        let _ = fs::remove_file(&legacy_path);
        return Ok(migrated);
    }
    read_secrets_file(&path)
}

fn read_secrets_file(path: &Path) -> Result<AppSecretsFile, AppError> {
    let metadata = fs::metadata(path).map_err(|e| AppError::Storage(e.to_string()))?;
    if metadata.len() > MAX_SECRETS_BYTES {
        return Err(AppError::Storage("App secrets file is too large".into()));
    }
    let content = fs::read_to_string(path).map_err(|e| AppError::Storage(e.to_string()))?;
    let secrets: AppSecretsFile = serde_json::from_str(&content)
        .map_err(|e| AppError::Storage(format!("Failed to parse app secrets: {e}")))?;
    Ok(secrets)
}

pub fn write_secrets(paths: &WorkPaths, secrets: &AppSecretsFile) -> Result<(), AppError> {
    atomic_write_json(&paths.work_apps_secrets_path(), secrets, true)
}

pub fn list_connections(paths: &WorkPaths) -> Result<Vec<AppConnection>, AppError> {
    let config = read_config(paths)?;
    let valid_connections: Vec<AppConnection> = config
        .connections
        .into_iter()
        .filter(|c| {
            c.provider.eq_ignore_ascii_case("native") || c.provider.eq_ignore_ascii_case("composio")
        })
        .collect();
    Ok(valid_connections)
}

fn matches_app_id(stored: &str, target: &str) -> bool {
    if stored.eq_ignore_ascii_case(target) {
        return true;
    }
    let s = stored.to_ascii_lowercase().replace(['-', '_'], "");
    let t = target.to_ascii_lowercase().replace(['-', '_'], "");
    if s == t {
        return true;
    }
    if (s == "feishu" || s == "lark" || s == "larksuite")
        && (t == "feishu" || t == "lark" || t == "larksuite")
    {
        return true;
    }
    false
}

pub fn get_connection(paths: &WorkPaths, app_id: &str) -> Result<Option<AppConnection>, AppError> {
    let connections = list_connections(paths)?;
    Ok(connections
        .into_iter()
        .find(|c| matches_app_id(&c.app_id, app_id)))
}

pub fn save_connection(paths: &WorkPaths, conn: &AppConnection) -> Result<(), AppError> {
    let mut config = read_config(paths)?;
    if let Some(idx) = config
        .connections
        .iter()
        .position(|c| c.app_id.eq_ignore_ascii_case(&conn.app_id))
    {
        config.connections[idx] = conn.clone();
    } else {
        config.connections.push(conn.clone());
    }
    write_config(paths, &config)
}

pub fn remove_connection(paths: &WorkPaths, app_id: &str) -> Result<(), AppError> {
    let mut config = read_config(paths)?;
    config
        .connections
        .retain(|c| !c.app_id.eq_ignore_ascii_case(app_id));
    config
        .workspace_defaults
        .retain(|d| !d.app_id.eq_ignore_ascii_case(app_id));
    write_config(paths, &config)?;

    // Also remove any related secrets
    let mut secrets = read_secrets(paths)?;
    let keys_to_remove: Vec<String> = secrets
        .secrets
        .keys()
        .filter(|k| k.starts_with(&format!("{app_id}:")) || *k == app_id)
        .cloned()
        .collect();
    for k in keys_to_remove {
        secrets.secrets.remove(&k);
    }
    write_secrets(paths, &secrets)?;
    Ok(())
}

pub fn remove_account(paths: &WorkPaths, app_id: &str, account_id: &str) -> Result<(), AppError> {
    let mut config = read_config(paths)?;
    let mut modified = false;
    if let Some(conn) = config
        .connections
        .iter_mut()
        .find(|c| c.app_id.eq_ignore_ascii_case(app_id))
    {
        conn.accounts.retain(|acc| acc.account_id != account_id);
        modified = true;
    }
    if modified {
        config
            .workspace_defaults
            .retain(|d| !(d.app_id.eq_ignore_ascii_case(app_id) && d.account_id == account_id));
        write_config(paths, &config)?;

        // Remove secret for this account
        let secret_key = format!("{app_id}:{account_id}");
        let mut secrets = read_secrets(paths)?;
        if secrets.secrets.remove(&secret_key).is_some() {
            write_secrets(paths, &secrets)?;
        }
    }
    Ok(())
}

pub fn get_workspace_default_account(
    paths: &WorkPaths,
    workspace_id: &str,
    app_id: &str,
) -> Result<Option<String>, AppError> {
    let config = read_config(paths)?;
    Ok(config
        .workspace_defaults
        .into_iter()
        .find(|d| d.workspace_id == workspace_id && d.app_id.eq_ignore_ascii_case(app_id))
        .map(|d| d.account_id))
}

pub fn set_workspace_default_account(
    paths: &WorkPaths,
    workspace_id: &str,
    app_id: &str,
    account_id: &str,
) -> Result<(), AppError> {
    let mut config = read_config(paths)?;
    if let Some(idx) = config
        .workspace_defaults
        .iter()
        .position(|d| d.workspace_id == workspace_id && d.app_id.eq_ignore_ascii_case(app_id))
    {
        config.workspace_defaults[idx].account_id = account_id.to_string();
    } else {
        config.workspace_defaults.push(WorkspaceAppDefaultAccount {
            workspace_id: workspace_id.to_string(),
            app_id: app_id.to_string(),
            account_id: account_id.to_string(),
        });
    }
    write_config(paths, &config)
}

pub fn get_secret(paths: &WorkPaths, key: &str) -> Result<Option<serde_json::Value>, AppError> {
    let secrets = read_secrets(paths)?;
    if let Some(v) = secrets.secrets.get(key) {
        return Ok(Some(v.clone()));
    }
    if key.eq_ignore_ascii_case("lark") {
        if let Some(v) = secrets.secrets.get("feishu") {
            return Ok(Some(v.clone()));
        }
    } else if key.eq_ignore_ascii_case("feishu") {
        if let Some(v) = secrets.secrets.get("lark") {
            return Ok(Some(v.clone()));
        }
    }
    Ok(None)
}

pub fn save_secret(paths: &WorkPaths, key: &str, value: serde_json::Value) -> Result<(), AppError> {
    let mut secrets = read_secrets(paths)?;
    secrets.secrets.insert(key.to_string(), value);
    write_secrets(paths, &secrets)
}

pub fn remove_secret(paths: &WorkPaths, key: &str) -> Result<(), AppError> {
    let mut secrets = read_secrets(paths)?;
    if secrets.secrets.remove(key).is_some() {
        write_secrets(paths, &secrets)?;
    }
    Ok(())
}
