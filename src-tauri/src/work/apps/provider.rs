use async_trait::async_trait;
use std::sync::Arc;

use crate::work::apps::error::AppError;
use crate::work::apps::models::{AppAccount, AppCatalogItem, AuthorizeResponse, ConnectionStatus};
use crate::work::paths::WorkPaths;
use serde_json::Value;

pub const NATIVE_PROVIDER_ID: &str = "native";
pub const COMPOSIO_PROVIDER_ID: &str = "composio";

#[async_trait]
pub trait ConnectedAppProvider: Send + Sync {
    /// Unique provider identifier (e.g. "native").
    fn provider_id(&self) -> &'static str;

    /// Retrieve available applications in this provider's catalog.
    async fn catalog(&self) -> Result<Vec<AppCatalogItem>, AppError>;

    /// Initiate OAuth / authorization flow for an app.
    async fn authorize(
        &self,
        app_id: &str,
        redirect_uri: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<AuthorizeResponse, AppError>;

    /// Query current status for a given connection.
    async fn check_status(&self, connection_id: &str) -> Result<ConnectionStatus, AppError>;

    /// List all connected accounts for an app.
    async fn list_accounts(&self, app_id: &str) -> Result<Vec<AppAccount>, AppError>;

    /// List the real MCP tools exposed by the provider session.
    async fn list_tools(&self, app_id: Option<&str>) -> Result<Vec<Value>, AppError> {
        let _ = app_id;
        Err(AppError::Provider(
            "Connected app provider does not implement tool discovery".into(),
        ))
    }

    /// Disconnect and revoke access for an app or specific account.
    async fn disconnect(&self, app_id: &str, account_id: Option<&str>) -> Result<(), AppError>;

    /// Execute a real provider-side tool call.
    async fn call_tool(
        &self,
        app_id: &str,
        tool_name: &str,
        arguments: &Value,
        account_id: Option<&str>,
    ) -> Result<Value, AppError> {
        let _ = (app_id, tool_name, arguments, account_id);
        Err(AppError::Provider(
            "Connected app provider does not implement tool execution".into(),
        ))
    }
}

/// Fallback / mock catalog provider used when no remote provider is configured yet.
pub struct StaticCatalogProvider {
    provider_id: &'static str,
    items: Vec<AppCatalogItem>,
}

impl StaticCatalogProvider {
    pub fn new(provider_id: &'static str, items: Vec<AppCatalogItem>) -> Self {
        Self { provider_id, items }
    }

    pub fn default_six_apps() -> Self {
        Self::new(NATIVE_PROVIDER_ID, default_six_apps_catalog())
    }
}

pub fn default_six_apps_catalog() -> Vec<AppCatalogItem> {
    use crate::work::apps::models::AppAuthType;
    vec![
        AppCatalogItem {
            app_id: "feishu".into(),
            display_name: "飞书 (Feishu / Lark)".into(),
            icon: "feishu".into(),
            description:
                "通过命令行管理飞书/Lark 全产品能力：即时通讯、邮件、日历、云文档、多维表格（Base）、知识库等"
                    .into(),
            categories: vec![
                "communication".into(),
                "messaging".into(),
                "productivity".into(),
                "notes".into(),
            ],
            capabilities: vec![
                "send_message".into(),
                "manage_base".into(),
                "read_document".into(),
                "manage_calendar".into(),
            ],
            auth_type: AppAuthType::ApiKey,
            documentation_url: Some("https://open.feishu.cn/page/cli".into()),
        },
        AppCatalogItem {
            app_id: "github".into(),
            display_name: "GitHub".into(),
            icon: "github".into(),
            description:
                "Interact with GitHub repositories, issues, pull requests, and releases via native PAT / CLI"
                    .into(),
            categories: vec!["development".into(), "code".into()],
            capabilities: vec![
                "list_repos".into(),
                "create_issue".into(),
                "manage_pull_requests".into(),
                "search_code".into(),
            ],
            auth_type: AppAuthType::ApiKey,
            documentation_url: Some("https://github.com/settings/tokens".into()),
        },
        AppCatalogItem {
            app_id: "notion".into(),
            display_name: "Notion".into(),
            icon: "notion".into(),
            description:
                "Search, create, and update pages and databases in Notion workspaces via Internal Token"
                    .into(),
            categories: vec!["productivity".into(), "notes".into()],
            capabilities: vec![
                "search_pages".into(),
                "create_page".into(),
                "query_database".into(),
            ],
            auth_type: AppAuthType::ApiKey,
            documentation_url: Some("https://www.notion.so/my-integrations".into()),
        },
        AppCatalogItem {
            app_id: "slack".into(),
            display_name: "Slack".into(),
            icon: "slack".into(),
            description:
                "Send messages, monitor channels, and search Slack conversations via Bot Token"
                    .into(),
            categories: vec!["communication".into(), "messaging".into()],
            capabilities: vec![
                "send_message".into(),
                "list_channels".into(),
                "read_history".into(),
            ],
            auth_type: AppAuthType::ApiKey,
            documentation_url: Some("https://api.slack.com/apps".into()),
        },
        AppCatalogItem {
            app_id: "gmail".into(),
            display_name: "Gmail".into(),
            icon: "gmail".into(),
            description: "Read, search, draft, and send emails with your Google account".into(),
            categories: vec!["communication".into(), "email".into()],
            capabilities: vec![
                "read_emails".into(),
                "send_emails".into(),
                "search_threads".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://workspace.google.com/products/gmail/".into()),
        },
        AppCatalogItem {
            app_id: "googlecalendar".into(),
            display_name: "Google Calendar".into(),
            icon: "googlecalendar".into(),
            description: "Access and schedule events, manage calendars and reminders".into(),
            categories: vec!["productivity".into(), "calendar".into()],
            capabilities: vec![
                "list_events".into(),
                "create_event".into(),
                "update_event".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://workspace.google.com/products/calendar/".into()),
        },
        AppCatalogItem {
            app_id: "googledrive".into(),
            display_name: "Google Drive".into(),
            icon: "googledrive".into(),
            description: "Search, upload, download, and organize files in Google Drive".into(),
            categories: vec!["storage".into(), "files".into()],
            capabilities: vec![
                "search_files".into(),
                "read_file".into(),
                "upload_file".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://workspace.google.com/products/drive/".into()),
        },
    ]
}

#[async_trait]
impl ConnectedAppProvider for StaticCatalogProvider {
    fn provider_id(&self) -> &'static str {
        self.provider_id
    }

    async fn catalog(&self) -> Result<Vec<AppCatalogItem>, AppError> {
        Ok(self.items.clone())
    }

    async fn authorize(
        &self,
        app_id: &str,
        _redirect_uri: Option<&str>,
        _user_id: Option<&str>,
    ) -> Result<AuthorizeResponse, AppError> {
        let app = self
            .items
            .iter()
            .find(|item| item.app_id.eq_ignore_ascii_case(app_id))
            .ok_or_else(|| AppError::NotFound(format!("App '{app_id}' not found in catalog")))?;

        let connection_id = format!("conn_{}_{}", app.app_id, uuid::Uuid::new_v4());
        Ok(AuthorizeResponse {
            authorization_url: format!(
                "https://connect.example.com/oauth/authorize?app={}",
                app.app_id
            ),
            connection_id,
            expires_at: None,
        })
    }

    async fn check_status(&self, _connection_id: &str) -> Result<ConnectionStatus, AppError> {
        Ok(ConnectionStatus::Connected)
    }

    async fn list_accounts(&self, _app_id: &str) -> Result<Vec<AppAccount>, AppError> {
        Ok(Vec::new())
    }

    async fn disconnect(&self, _app_id: &str, _account_id: Option<&str>) -> Result<(), AppError> {
        Ok(())
    }
}

/// Native App Provider managing 100% local native credentials and MCP/CLI execution.
pub struct NativeAppProvider {
    paths: WorkPaths,
    catalog: Vec<AppCatalogItem>,
}

impl NativeAppProvider {
    pub fn new(paths: WorkPaths) -> Self {
        Self {
            paths,
            catalog: default_six_apps_catalog(),
        }
    }
}

#[async_trait]
impl ConnectedAppProvider for NativeAppProvider {
    fn provider_id(&self) -> &'static str {
        NATIVE_PROVIDER_ID
    }

    async fn catalog(&self) -> Result<Vec<AppCatalogItem>, AppError> {
        Ok(self.catalog.clone())
    }

    async fn authorize(
        &self,
        app_id: &str,
        redirect_uri: Option<&str>,
        _user_id: Option<&str>,
    ) -> Result<AuthorizeResponse, AppError> {
        let app = self
            .catalog
            .iter()
            .find(|item| item.app_id.eq_ignore_ascii_case(app_id))
            .ok_or_else(|| {
                AppError::NotFound(format!("App '{app_id}' not found in native catalog"))
            })?;

        let connection_id = format!("conn_{}_{}", app.app_id, uuid::Uuid::new_v4());
        let authorization_url = redirect_uri
            .map(|u| u.to_string())
            .unwrap_or_else(|| format!("http://127.0.0.1:18234/oauth/callback?app={}", app.app_id));

        Ok(AuthorizeResponse {
            authorization_url,
            connection_id,
            expires_at: None,
        })
    }

    async fn check_status(&self, connection_id: &str) -> Result<ConnectionStatus, AppError> {
        let connections = crate::work::apps::storage::list_connections(&self.paths)?;
        if let Some(conn) = connections
            .into_iter()
            .find(|c| c.connection_id == connection_id || connection_id.contains(&c.app_id))
        {
            Ok(conn.status)
        } else {
            Ok(ConnectionStatus::Disconnected)
        }
    }

    async fn list_accounts(&self, app_id: &str) -> Result<Vec<AppAccount>, AppError> {
        if let Some(conn) = crate::work::apps::storage::get_connection(&self.paths, app_id)? {
            Ok(conn.accounts)
        } else {
            Ok(Vec::new())
        }
    }

    async fn list_tools(&self, _app_id: Option<&str>) -> Result<Vec<Value>, AppError> {
        // Native app execution is intentionally not advertised as a remote
        // MCP catalog. Concrete provider calls still return an explicit
        // unsupported-operation error from the Host pipeline.
        Ok(Vec::new())
    }

    async fn disconnect(&self, app_id: &str, account_id: Option<&str>) -> Result<(), AppError> {
        if let Some(acc_id) = account_id {
            crate::work::apps::storage::remove_account(&self.paths, app_id, acc_id)?;
        } else {
            crate::work::apps::storage::remove_connection(&self.paths, app_id)?;
        }
        Ok(())
    }
}

/// Resolve a provider from the product-level provider id stored in a
/// connection/package record.
pub fn resolve_provider_for(
    paths: &WorkPaths,
    provider_id: Option<&str>,
) -> Result<Arc<dyn ConnectedAppProvider>, AppError> {
    let normalized = provider_id
        .unwrap_or(NATIVE_PROVIDER_ID)
        .trim()
        .to_lowercase();
    match normalized.as_str() {
        NATIVE_PROVIDER_ID | "" => Ok(Arc::new(NativeAppProvider::new(paths.clone()))),
        COMPOSIO_PROVIDER_ID => Ok(Arc::new(
            crate::work::apps::composio::ComposioAppProvider::new(
                crate::work::apps::composio::get_composio_api_key(paths),
                None,
                paths.clone(),
            ),
        )),
        _ => Err(AppError::NotFound(format!(
            "Unsupported connected app provider '{normalized}'"
        ))),
    }
}

/// Resolve the default provider for catalog/authorize entry points.
pub fn resolve_provider(paths: &WorkPaths) -> Arc<dyn ConnectedAppProvider> {
    resolve_provider_for(paths, None)
        .expect("default native connected app provider must be registered")
}

pub fn get_default_provider() -> Arc<dyn ConnectedAppProvider> {
    let paths = WorkPaths::app();
    resolve_provider(&paths)
}
