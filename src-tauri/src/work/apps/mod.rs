pub mod composio;
pub mod error;
pub mod lark_cli;
pub mod models;
pub mod provider;
pub mod storage;

use crate::work::apps::models::{
    AppAccount, AppCatalogItem, AppConnection, AuthorizeResponse, ConnectionStatus,
};
use crate::work::apps::provider::get_default_provider;
use crate::work::paths::WorkPaths;
use chrono::Utc;

pub async fn list_catalog() -> Result<Vec<AppCatalogItem>, String> {
    let provider = get_default_provider();
    provider.catalog().await.map_err(|e| e.to_string())
}

pub fn list_connections() -> Result<Vec<AppConnection>, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout().map_err(|e| e.to_string())?;
    storage::list_connections(&paths).map_err(|e| e.to_string())
}

pub fn get_connection(app_id: &str) -> Result<Option<AppConnection>, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout().map_err(|e| e.to_string())?;
    storage::get_connection(&paths, app_id).map_err(|e| e.to_string())
}

pub async fn authorize(
    app_id: &str,
    redirect_uri: Option<&str>,
    user_id: Option<&str>,
) -> Result<AuthorizeResponse, String> {
    let paths = WorkPaths::app();
    authorize_with_paths(&paths, app_id, redirect_uri, user_id).await
}

/// Start authorization against an explicit Work path. The path-aware variant
/// is used by the internal bridge so tests and isolated Work profiles never
/// accidentally write state into the process-wide default profile.
pub async fn authorize_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    redirect_uri: Option<&str>,
    user_id: Option<&str>,
) -> Result<AuthorizeResponse, String> {
    paths.ensure_layout().map_err(|e| e.to_string())?;
    let provider = crate::work::apps::provider::resolve_provider(paths);
    let response = provider
        .authorize(app_id, redirect_uri, user_id)
        .await
        .map_err(|e| e.to_string())?;

    let normalized_app_id = app_id.trim().to_lowercase();
    let previous = storage::get_connection(paths, &normalized_app_id).map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let connection = AppConnection {
        connection_id: response.connection_id.clone(),
        app_id: normalized_app_id,
        provider: provider.provider_id().to_string(),
        status: ConnectionStatus::Pending,
        accounts: previous
            .as_ref()
            .map(|connection| connection.accounts.clone())
            .unwrap_or_default(),
        last_checked_at: previous
            .as_ref()
            .and_then(|connection| connection.last_checked_at.clone()),
        created_at: previous
            .as_ref()
            .map(|connection| connection.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    storage::save_connection(paths, &connection).map_err(|e| e.to_string())?;
    Ok(response)
}

pub async fn check_status(
    app_id: &str,
    connection_id: Option<&str>,
) -> Result<ConnectionStatus, String> {
    let paths = WorkPaths::app();
    check_status_with_paths(&paths, app_id, connection_id).await
}

pub async fn check_status_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    connection_id: Option<&str>,
) -> Result<ConnectionStatus, String> {
    paths.ensure_layout().map_err(|e| e.to_string())?;

    let normalized_app_id = app_id.trim().to_lowercase();
    if normalized_app_id == "feishu" || normalized_app_id == "lark" {
        if let Ok(identity) = lark_cli::verify_lark_auth(paths) {
            let _ = lark_cli::persist_feishu_connection(paths, &identity, None, None)?;
            return Ok(ConnectionStatus::Connected);
        }
        return Ok(storage::get_connection(paths, "feishu")
            .map_err(|e| e.to_string())?
            .map(|connection| connection.status)
            .unwrap_or(ConnectionStatus::Disconnected));
    }

    let Some(mut connection) =
        storage::get_connection(paths, &normalized_app_id).map_err(|e| e.to_string())?
    else {
        return Ok(ConnectionStatus::Disconnected);
    };
    let provider =
        crate::work::apps::provider::resolve_provider_for(paths, Some(&connection.provider))
            .map_err(|e| e.to_string())?;
    let provider_connection_id = connection_id.unwrap_or(&connection.connection_id);
    let status = provider
        .check_status(provider_connection_id)
        .await
        .map_err(|e| e.to_string())?;
    let remote_accounts = provider
        .list_accounts(&normalized_app_id)
        .await
        .map_err(|e| e.to_string())?;
    let accounts = merge_accounts(&connection.accounts, remote_accounts);
    let now = Utc::now().to_rfc3339();
    connection.status = status;
    connection.accounts = accounts;
    connection.last_checked_at = Some(now.clone());
    connection.updated_at = now;
    storage::save_connection(paths, &connection).map_err(|e| e.to_string())?;
    Ok(status)
}

fn merge_accounts(existing: &[AppAccount], remote: Vec<AppAccount>) -> Vec<AppAccount> {
    remote
        .into_iter()
        .map(|mut account| {
            if let Some(previous) = existing
                .iter()
                .find(|candidate| candidate.account_id == account.account_id)
            {
                account.alias = previous.alias.clone();
                account.last_used_at = previous.last_used_at.clone();
            }
            account
        })
        .collect()
}

pub fn connect_native(
    app_id: &str,
    token: &str,
    account_alias: Option<&str>,
    account_email: Option<&str>,
) -> Result<AppConnection, String> {
    let paths = WorkPaths::app();
    connect_native_with_paths(&paths, app_id, token, account_alias, account_email)
}

pub fn connect_native_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    token: &str,
    account_alias: Option<&str>,
    account_email: Option<&str>,
) -> Result<AppConnection, String> {
    paths.ensure_layout().map_err(|e| e.to_string())?;
    let normalized_app_id = app_id.trim().to_lowercase();
    let now = Utc::now().to_rfc3339();
    let account_id = format!("acc_{}_{}", normalized_app_id, uuid::Uuid::new_v4());

    let mut secret_val = serde_json::json!({
        "token": token,
        "app_id": normalized_app_id,
        "account_id": account_id,
        "created_at": now
    });
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(token) {
        if let Some(obj) = parsed.as_object() {
            if let Some(map) = secret_val.as_object_mut() {
                for (k, v) in obj {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // Save secret
    let secret_key = format!("{normalized_app_id}:{account_id}");
    storage::save_secret(paths, &secret_key, secret_val.clone()).map_err(|e| e.to_string())?;

    // Also save default secret key for single-account lookup
    let _ = storage::save_secret(paths, &normalized_app_id, secret_val.clone());

    let previous = storage::get_connection(paths, &normalized_app_id).map_err(|e| e.to_string())?;
    let mut accounts = previous.map(|c| c.accounts).unwrap_or_default();

    let new_account = AppAccount {
        account_id: account_id.clone(),
        alias: account_alias.map(|s| s.to_string()),
        display_name: account_alias
            .map(|s| s.to_string())
            .or_else(|| account_email.map(|s| s.to_string()))
            .or_else(|| Some(format!("{normalized_app_id} 账号"))),
        email: account_email.map(|s| s.to_string()),
        status: ConnectionStatus::Connected,
        created_at: now.clone(),
        last_used_at: Some(now.clone()),
    };

    if let Some(existing_idx) = accounts.iter().position(|a| {
        (account_email.is_some() && a.email.as_deref() == account_email)
            || (account_alias.is_some() && a.alias.as_deref() == account_alias)
    }) {
        accounts[existing_idx] = new_account;
    } else if accounts.len() <= 1 {
        accounts = vec![new_account];
    } else {
        accounts.push(new_account);
    }

    let connection = AppConnection {
        connection_id: format!("conn_{normalized_app_id}"),
        app_id: normalized_app_id.clone(),
        provider: "native".into(),
        status: ConnectionStatus::Connected,
        accounts,
        last_checked_at: Some(now.clone()),
        created_at: now.clone(),
        updated_at: now,
    };

    storage::save_connection(paths, &connection).map_err(|e| e.to_string())?;
    let _ = crate::work::connector_package_manager::sync_auth_status_from_app_with_paths(
        paths,
        &normalized_app_id,
        ConnectionStatus::Connected,
    );

    if normalized_app_id == "feishu" {
        if let Some(obj) = secret_val.as_object() {
            let app_id_str = obj.get("app_id").and_then(|v| v.as_str()).unwrap_or("");
            let app_secret_str = obj.get("app_secret").and_then(|v| v.as_str()).unwrap_or("");
            if !app_id_str.is_empty() && !app_secret_str.is_empty() {
                let _ = lark_cli::configure_lark_cli_with_paths(paths, app_id_str, app_secret_str);
            }
        }
        let _ = lark_cli::mount_lark_skills_to_profile(paths);
    }

    Ok(connection)
}

pub async fn disconnect(app_id: &str, account_id: Option<&str>) -> Result<(), String> {
    let paths = WorkPaths::app();
    disconnect_with_paths(&paths, app_id, account_id).await
}

pub async fn disconnect_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    account_id: Option<&str>,
) -> Result<(), String> {
    paths.ensure_layout().map_err(|e| e.to_string())?;

    if (app_id.trim().eq_ignore_ascii_case("feishu") || app_id.trim().eq_ignore_ascii_case("lark"))
        && account_id.is_none()
    {
        let _ = lark_cli::logout_with_paths(paths);
        storage::remove_connection(paths, "feishu").map_err(|e| e.to_string())?;
        let _ = crate::work::connector_package_manager::sync_auth_status_from_app_with_paths(
            paths,
            "feishu",
            ConnectionStatus::Disconnected,
        );
        return Ok(());
    }

    let provider = if let Some(connection) =
        storage::get_connection(paths, app_id).map_err(|e| e.to_string())?
    {
        crate::work::apps::provider::resolve_provider_for(paths, Some(&connection.provider))
            .map_err(|e| e.to_string())?
    } else {
        get_default_provider()
    };
    provider
        .disconnect(app_id, account_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(acc_id) = account_id {
        storage::remove_account(paths, app_id, acc_id).map_err(|e| e.to_string())?;
    } else {
        storage::remove_connection(paths, app_id).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn get_provider_config() -> Result<composio::ComposioConfigInfo, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout().map_err(|e| e.to_string())?;
    let has_key = composio::get_composio_api_key(&paths).is_some();
    Ok(composio::ComposioConfigInfo {
        has_api_key: has_key,
        base_url: composio::DEFAULT_COMPOSIO_BASE_URL.to_string(),
    })
}

pub fn save_provider_config(api_key: &str) -> Result<(), String> {
    let paths = WorkPaths::app();
    paths.ensure_layout().map_err(|e| e.to_string())?;
    composio::save_composio_api_key(&paths, api_key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::apps::models::AppAccount;
    use tempfile::TempDir;

    fn setup_test_paths() -> (TempDir, WorkPaths) {
        let temp_dir = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp_dir.path().to_path_buf());
        paths.ensure_layout().unwrap();
        (temp_dir, paths)
    }

    #[tokio::test]
    async fn test_catalog_retrieval() {
        let provider = get_default_provider();
        let catalog = provider.catalog().await.unwrap();
        assert_eq!(catalog.len(), 7);
        assert!(catalog.iter().any(|item| item.app_id == "feishu"));
        assert!(catalog.iter().any(|item| item.app_id == "gmail"));
        assert!(catalog.iter().any(|item| item.app_id == "github"));
        assert!(catalog.iter().any(|item| item.app_id == "slack"));
        assert!(catalog.iter().any(|item| item.app_id == "notion"));
        assert!(catalog.iter().any(|item| item.app_id == "googlecalendar"));
        assert!(catalog.iter().any(|item| item.app_id == "googledrive"));
    }

    #[test]
    fn test_storage_connections_crud() {
        let (_tmp, paths) = setup_test_paths();

        let conn = AppConnection {
            connection_id: "conn_gmail_123".into(),
            app_id: "gmail".into(),
            provider: "native".into(),
            status: ConnectionStatus::Connected,
            accounts: vec![
                AppAccount {
                    account_id: "acc_1".into(),
                    alias: Some("工作账号".into()),
                    display_name: Some("work@example.com".into()),
                    email: Some("work@example.com".into()),
                    status: ConnectionStatus::Connected,
                    created_at: "2026-08-22T00:00:00Z".into(),
                    last_used_at: None,
                },
                AppAccount {
                    account_id: "acc_2".into(),
                    alias: Some("个人账号".into()),
                    display_name: Some("personal@example.com".into()),
                    email: Some("personal@example.com".into()),
                    status: ConnectionStatus::Connected,
                    created_at: "2026-08-22T00:00:00Z".into(),
                    last_used_at: None,
                },
            ],
            last_checked_at: Some("2026-08-22T00:00:00Z".into()),
            created_at: "2026-08-22T00:00:00Z".into(),
            updated_at: "2026-08-22T00:00:00Z".into(),
        };

        storage::save_connection(&paths, &conn).unwrap();
        let loaded = storage::get_connection(&paths, "gmail").unwrap().unwrap();
        assert_eq!(loaded.connection_id, "conn_gmail_123");
        assert_eq!(loaded.accounts.len(), 2);

        // Test workspace default account
        storage::set_workspace_default_account(&paths, "ws_1", "gmail", "acc_1").unwrap();
        let def = storage::get_workspace_default_account(&paths, "ws_1", "gmail").unwrap();
        assert_eq!(def, Some("acc_1".into()));

        // Test remove single account
        storage::remove_account(&paths, "gmail", "acc_1").unwrap();
        let loaded_after = storage::get_connection(&paths, "gmail").unwrap().unwrap();
        assert_eq!(loaded_after.accounts.len(), 1);
        assert_eq!(loaded_after.accounts[0].account_id, "acc_2");
        // Workspace default account was removed because it was acc_1
        let def_after = storage::get_workspace_default_account(&paths, "ws_1", "gmail").unwrap();
        assert_eq!(def_after, None);

        // Test remove entire connection
        storage::remove_connection(&paths, "gmail").unwrap();
        let loaded_none = storage::get_connection(&paths, "gmail").unwrap();
        assert!(loaded_none.is_none());
    }

    #[test]
    fn test_storage_secrets_security() {
        let (_tmp, paths) = setup_test_paths();

        let secret_payload = serde_json::json!({
            "access_token": "secret_oauth_token_12345",
            "refresh_token": "secret_refresh_token_67890"
        });

        storage::save_secret(&paths, "gmail:acc_1", secret_payload.clone()).unwrap();
        let loaded_secret = storage::get_secret(&paths, "gmail:acc_1").unwrap().unwrap();
        assert_eq!(loaded_secret["access_token"], "secret_oauth_token_12345");

        // Verify file mode 0600 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(paths.work_apps_secrets_path()).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        storage::remove_secret(&paths, "gmail:acc_1").unwrap();
        let none_secret = storage::get_secret(&paths, "gmail:acc_1").unwrap();
        assert!(none_secret.is_none());
    }
}
