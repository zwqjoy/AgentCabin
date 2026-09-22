use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use url::Url;

use crate::work::apps::error::AppError;
use crate::work::apps::models::{
    AppAccount, AppAuthType, AppCatalogItem, AuthorizeResponse, ConnectionStatus,
};
use crate::work::apps::provider::ConnectedAppProvider;
use crate::work::apps::storage;
use crate::work::paths::WorkPaths;

pub const COMPOSIO_PROVIDER_ID: &str = "composio";
pub const DEFAULT_COMPOSIO_BASE_URL: &str = "https://backend.composio.dev/api/v3.1";
pub const COMPOSIO_API_KEY_SECRET_NAME: &str = "composio:api_key";
const COMPOSIO_SESSION_SECRET_NAME: &str = "composio:session";
const REQUEST_TIMEOUT_SECS: u64 = 15;
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Validates authorization URLs returned from provider before passing to UI.
/// Must be a Composio HTTPS URL to prevent redirecting the user to an
/// arbitrary host or leaking the provider flow to a local/custom endpoint.
pub fn validate_auth_url(raw_url: &str) -> Result<String, AppError> {
    let parsed = Url::parse(raw_url.trim())
        .map_err(|e| AppError::Security(format!("Invalid authorization URL format: {e}")))?;

    if parsed.scheme() != "https" {
        return Err(AppError::Security(
            "Authorization URL must use secure HTTPS scheme".into(),
        ));
    }

    let Some(host) = parsed.host_str() else {
        return Err(AppError::Security(
            "Authorization URL must contain a valid host".into(),
        ));
    };
    if !is_composio_host(host) {
        return Err(AppError::Security(
            "Authorization URL must be hosted by Composio".into(),
        ));
    }

    Ok(parsed.to_string())
}

fn is_composio_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("composio.dev")
        || host.to_ascii_lowercase().ends_with(".composio.dev")
}

fn validate_mcp_url(raw_url: &str) -> Result<String, AppError> {
    let parsed = Url::parse(raw_url.trim())
        .map_err(|e| AppError::Security(format!("Invalid Composio MCP URL format: {e}")))?;
    let Some(host) = parsed.host_str() else {
        return Err(AppError::Security(
            "Composio MCP URL must contain a host".into(),
        ));
    };
    if parsed.scheme() != "https" || !is_composio_host(host) {
        return Err(AppError::Security(
            "Composio MCP endpoint must use HTTPS on a Composio host".into(),
        ));
    }
    Ok(parsed.to_string())
}

/// Map Composio status string to standard ConnectionStatus.
pub fn map_composio_status(status_str: &str) -> ConnectionStatus {
    match status_str.to_ascii_uppercase().as_str() {
        "ACTIVE" | "CONNECTED" | "SUCCESS" => ConnectionStatus::Connected,
        "INITIATED" | "PENDING" | "INITIALIZING" => ConnectionStatus::Pending,
        "EXPIRED" | "TIMEOUT" => ConnectionStatus::Expired,
        "FAILED" | "ERROR" | "CANCELLED" | "INACTIVE" => ConnectionStatus::Error,
        _ => ConnectionStatus::Disconnected,
    }
}

pub fn default_six_app_catalog() -> Vec<AppCatalogItem> {
    vec![
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
        AppCatalogItem {
            app_id: "slack".into(),
            display_name: "Slack".into(),
            icon: "slack".into(),
            description: "Send messages, monitor channels, and search Slack conversations".into(),
            categories: vec!["communication".into(), "messaging".into()],
            capabilities: vec![
                "send_message".into(),
                "list_channels".into(),
                "read_history".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://slack.com/".into()),
        },
        AppCatalogItem {
            app_id: "github".into(),
            display_name: "GitHub".into(),
            icon: "github".into(),
            description: "Interact with GitHub repositories, issues, pull requests, and releases"
                .into(),
            categories: vec!["development".into(), "code".into()],
            capabilities: vec![
                "list_repos".into(),
                "create_issue".into(),
                "manage_pull_requests".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://github.com/".into()),
        },
        AppCatalogItem {
            app_id: "notion".into(),
            display_name: "Notion".into(),
            icon: "notion".into(),
            description: "Search, create, and update pages and databases in Notion workspaces"
                .into(),
            categories: vec!["productivity".into(), "notes".into()],
            capabilities: vec![
                "search_pages".into(),
                "create_page".into(),
                "query_database".into(),
            ],
            auth_type: AppAuthType::OAuth2,
            documentation_url: Some("https://notion.so/".into()),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioAuthConfigItem {
    #[serde(default)]
    id: String,
    #[serde(rename = "auth_scheme", default)]
    auth_scheme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioAuthConfigsResponse {
    #[serde(default)]
    items: Vec<ComposioAuthConfigItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioLinkRequest {
    auth_config_id: String,
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioLinkResponse {
    #[serde(rename = "redirect_url", default)]
    redirect_url: Option<String>,
    #[serde(rename = "redirectUrl", default)]
    redirect_url_camel: Option<String>,
    #[serde(rename = "url", default)]
    url: Option<String>,
    #[serde(rename = "connected_account_id", default)]
    connected_account_id: Option<String>,
    #[serde(rename = "id", default)]
    id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioAccountV3 {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "user_id", default)]
    user_id: Option<String>,
    #[serde(rename = "user_properties", default)]
    user_properties: Option<serde_json::Value>,
    #[serde(rename = "created_at", default)]
    created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioListAccountsResponseV3 {
    #[serde(default)]
    items: Vec<ComposioAccountV3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComposioSession {
    session_id: String,
    user_id: String,
    mcp_url: String,
    #[serde(default)]
    mcp_session_id: Option<String>,
}

pub struct ComposioAppProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
    catalog: Vec<AppCatalogItem>,
    paths: WorkPaths,
}

impl ComposioAppProvider {
    pub fn new(api_key: Option<String>, base_url: Option<String>, paths: WorkPaths) -> Self {
        let base_url = base_url.unwrap_or_else(|| DEFAULT_COMPOSIO_BASE_URL.to_string());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        let catalog = default_six_app_catalog();
        Self {
            api_key,
            base_url,
            client,
            catalog,
            paths,
        }
    }

    fn headers(&self) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = &self.api_key {
            let trimmed = key.trim();
            let key_val = HeaderValue::from_str(trimmed).map_err(|_| {
                AppError::Validation("Invalid characters in Composio API key".into())
            })?;
            headers.insert("x-api-key", key_val.clone());
            headers.insert("X-API-KEY", key_val.clone());
            headers.insert("x-consumer-api-key", key_val);
        }
        Ok(headers)
    }

    fn sanitize_error(err: reqwest::Error) -> AppError {
        let msg = err.to_string();
        AppError::Provider(format!("Composio HTTP request failed: {msg}"))
    }

    async fn resolve_auth_config_id(&self, toolkit_slug: &str) -> Result<String, AppError> {
        let list_url = format!(
            "{}/auth_configs?toolkit_slugs={}",
            self.base_url, toolkit_slug
        );
        let response = self
            .client
            .get(&list_url)
            .headers(self.headers()?)
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if response.status().is_success() {
            if let Ok(parsed) = response.json::<ComposioAuthConfigsResponse>().await {
                if let Some(first) = parsed.items.into_iter().next() {
                    if !first.id.trim().is_empty() {
                        return Ok(first.id);
                    }
                }
            }
        }

        // Create default OAuth2 auth config if not existing
        let create_url = format!("{}/auth_configs", self.base_url);
        let create_body = serde_json::json!({
            "toolkit": { "slug": toolkit_slug },
            "auth_config": { "auth_scheme": "OAUTH2" }
        });

        let create_res = self
            .client
            .post(&create_url)
            .headers(self.headers()?)
            .json(&create_body)
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if !create_res.status().is_success() {
            let status = create_res.status();
            let body = create_res.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!(
                "Composio auth config creation error ({status}): {body}"
            )));
        }

        let created_json: serde_json::Value = create_res
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("Failed to parse created auth config: {e}")))?;

        let id = created_json
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Provider("Composio did not return auth config ID".into()))?;

        Ok(id.to_string())
    }

    fn require_api_key(&self) -> Result<&str, AppError> {
        let key = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::Unauthorized(
                    "未配置 Composio API Key。请在「能力中心 -> 应用连接」配置 Composio API Key 或设置 COMPOSIO_API_KEY 环境变量。".into(),
                )
            })?;
        Ok(key)
    }

    fn load_session(&self) -> Result<Option<ComposioSession>, AppError> {
        let Some(value) = storage::get_secret(&self.paths, COMPOSIO_SESSION_SECRET_NAME)? else {
            return Ok(None);
        };
        let session: ComposioSession = serde_json::from_value(value).map_err(|error| {
            AppError::Storage(format!("Invalid stored Composio session: {error}"))
        })?;
        if session.session_id.trim().is_empty() || session.user_id.trim().is_empty() {
            return Err(AppError::Security(
                "Stored Composio session is incomplete".into(),
            ));
        }
        let mcp_url = validate_mcp_url(&session.mcp_url)?;
        Ok(Some(ComposioSession { mcp_url, ..session }))
    }

    fn save_session(&self, session: &ComposioSession) -> Result<(), AppError> {
        storage::save_secret(
            &self.paths,
            COMPOSIO_SESSION_SECRET_NAME,
            serde_json::to_value(session).map_err(|error| {
                AppError::Storage(format!("Failed to serialize Composio session: {error}"))
            })?,
        )
    }

    async fn ensure_session(
        &self,
        requested_user_id: Option<&str>,
    ) -> Result<ComposioSession, AppError> {
        self.require_api_key()?;
        let requested_user_id = requested_user_id
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(session) = self.load_session()? {
            if requested_user_id.is_none() || requested_user_id == Some(session.user_id.as_str()) {
                return Ok(session);
            }
        }

        let user_id = requested_user_id
            .map(str::to_string)
            .or_else(|| {
                self.load_session()
                    .ok()
                    .flatten()
                    .map(|session| session.user_id)
            })
            .unwrap_or_else(|| format!("agentcabin_{}", uuid::Uuid::new_v4()));
        if user_id.len() > 128
            || !user_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".:_-".contains(character))
        {
            return Err(AppError::Validation(
                "Composio user ID contains unsupported characters".into(),
            ));
        }

        let response = self
            .client
            .post(format!("{}/tool_router/session", self.base_url))
            .headers(self.headers()?)
            .json(&serde_json::json!({
                "user_id": user_id,
                "mcp": true,
                "manage_connections": {
                    "enable": true,
                    "enable_wait_for_connections": true,
                    "enable_connection_removal": true
                },
                "multi_account": {
                    "enable": true,
                    "max_accounts_per_toolkit": 5,
                    "require_explicit_selection": true
                }
            }))
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!(
                "Composio session creation failed ({status}): {}",
                truncate_provider_body(&body)
            )));
        }

        let body: Value = response.json().await.map_err(|error| {
            AppError::Provider(format!("Failed to parse Composio session: {error}"))
        })?;
        let session_id = body
            .get("session_id")
            .or_else(|| body.get("sessionId"))
            .or_else(|| body.get("id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::Provider("Composio did not return a session ID".into()))?;
        let mcp_url = body
            .get("mcp")
            .and_then(|mcp| mcp.get("url"))
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Provider("Composio did not return an MCP URL".into()))?;
        let session = ComposioSession {
            session_id: session_id.to_string(),
            user_id,
            mcp_url: validate_mcp_url(mcp_url)?,
            mcp_session_id: None,
        };
        self.save_session(&session)?;
        Ok(session)
    }

    async fn post_mcp_rpc(
        &self,
        url: &str,
        headers: &HeaderMap,
        message: &Value,
        session_id: Option<&str>,
    ) -> Result<(Option<Value>, Option<String>), AppError> {
        let mut request = self.client.post(url).headers(headers.clone()).json(message);
        if let Some(session_id) = session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        let response = request.send().await.map_err(Self::sanitize_error)?;
        let response_session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .or_else(|| session_id.map(str::to_string));
        let status = response.status();
        let body = response.text().await.map_err(Self::sanitize_error)?;
        if !status.is_success() {
            return Err(AppError::Provider(format!(
                "Composio MCP returned HTTP {}: {}",
                status,
                truncate_provider_body(&body)
            )));
        }
        if body.trim().is_empty() {
            return Ok((None, response_session_id));
        }
        let value = parse_mcp_body(&body)?;
        if let Some(error) = value.get("error") {
            return Err(AppError::Provider(format!(
                "Composio MCP protocol error: {}",
                truncate_provider_body(&error.to_string())
            )));
        }
        Ok((value.get("result").cloned(), response_session_id))
    }

    fn mcp_headers(&self) -> Result<HeaderMap, AppError> {
        let mut headers = self.headers()?;
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        Ok(headers)
    }

    async fn ensure_mcp_session(
        &self,
        requested_user_id: Option<&str>,
    ) -> Result<ComposioSession, AppError> {
        let mut session = self.ensure_session(requested_user_id).await?;
        if session.mcp_session_id.is_some() {
            return Ok(session);
        }

        let initialize = serde_json::json!({
            "jsonrpc": "2.0",
            "id": format!("agentcabin-init-{}", uuid::Uuid::new_v4()),
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "agentcabin-work",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        let (result, response_session_id) = self
            .post_mcp_rpc(&session.mcp_url, &self.mcp_headers()?, &initialize, None)
            .await?;
        if result.is_none() {
            return Err(AppError::Provider(
                "Composio MCP initialize returned no result".into(),
            ));
        }

        session.mcp_session_id = response_session_id;
        self.save_session(&session)?;

        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let (_, notification_session_id) = self
            .post_mcp_rpc(
                &session.mcp_url,
                &self.mcp_headers()?,
                &initialized,
                session.mcp_session_id.as_deref(),
            )
            .await?;
        if notification_session_id.is_some() {
            session.mcp_session_id = notification_session_id;
            self.save_session(&session)?;
        }

        Ok(session)
    }

    async fn call_mcp_method(&self, method: &str, params: Value) -> Result<Value, AppError> {
        let mut session = self.ensure_mcp_session(None).await?;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": format!("agentcabin-{}-{}", method.replace('/', "-"), uuid::Uuid::new_v4()),
            "method": method,
            "params": params,
        });
        let current_session_id = session.mcp_session_id.clone();
        let (result, response_session_id) = self
            .post_mcp_rpc(
                &session.mcp_url,
                &self.mcp_headers()?,
                &request,
                current_session_id.as_deref(),
            )
            .await?;
        if let Some(new_session_id) = response_session_id {
            if session.mcp_session_id.as_deref() != Some(new_session_id.as_str()) {
                session.mcp_session_id = Some(new_session_id);
                self.save_session(&session)?;
            }
        }
        result.ok_or_else(|| {
            AppError::Provider(format!("Composio MCP method '{method}' returned no result"))
        })
    }

    fn validate_catalog_app(&self, app_id: Option<&str>) -> Result<(), AppError> {
        if let Some(app_id) = app_id {
            let normalized = app_id.trim().to_lowercase();
            if !self.catalog.iter().any(|item| item.app_id == normalized) {
                return Err(AppError::NotFound(format!(
                    "App '{app_id}' not found in Composio catalog"
                )));
            }
        }
        Ok(())
    }
}

fn truncate_provider_body(body: &str) -> String {
    body.trim().chars().take(500).collect()
}

fn parse_mcp_body(body: &str) -> Result<Value, AppError> {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        return Ok(value);
    }
    for line in body.lines().rev() {
        let candidate = line.trim().strip_prefix("data:").unwrap_or(line).trim();
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Ok(value);
        }
    }
    Err(AppError::Provider(
        "Composio MCP returned an unreadable response".into(),
    ))
}

#[async_trait]
impl ConnectedAppProvider for ComposioAppProvider {
    fn provider_id(&self) -> &'static str {
        COMPOSIO_PROVIDER_ID
    }

    async fn catalog(&self) -> Result<Vec<AppCatalogItem>, AppError> {
        Ok(self.catalog.clone())
    }

    async fn list_tools(&self, app_id: Option<&str>) -> Result<Vec<Value>, AppError> {
        self.require_api_key()?;
        self.validate_catalog_app(app_id)?;
        let result = self
            .call_mcp_method("tools/list", serde_json::json!({}))
            .await?;
        Ok(result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    async fn authorize(
        &self,
        app_id: &str,
        redirect_uri: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<AuthorizeResponse, AppError> {
        let normalized_app = app_id.trim().to_lowercase();
        let _ = self
            .catalog
            .iter()
            .find(|item| item.app_id == normalized_app)
            .ok_or_else(|| AppError::NotFound(format!("App '{app_id}' not found in catalog")))?;

        let Some(api_key) = &self.api_key else {
            return Err(AppError::Unauthorized(
                "未配置 Composio API Key。请在「能力中心 -> 应用连接」配置 Composio API Key 或设置 COMPOSIO_API_KEY 环境变量后发起授权。".into(),
            ));
        };

        if api_key.trim().is_empty() {
            return Err(AppError::Unauthorized("Composio API key is empty".into()));
        }

        // 1. Try session-based link flow (OpenMausBot mechanism: POST /tool_router/session + POST /tool_router/session/{id}/link)
        let session_url = format!("{}/tool_router/session", self.base_url);
        let session_res = self
            .client
            .post(&session_url)
            .headers(self.headers()?)
            .json(&serde_json::json!({
                "user_id": user_id.unwrap_or("default"),
                "manage_connections": {
                    "enable": true,
                    "enable_wait_for_connections": true,
                    "enable_connection_removal": true
                }
            }))
            .send()
            .await;

        if let Ok(res) = session_res {
            if res.status().is_success() {
                if let Ok(sess_json) = res.json::<serde_json::Value>().await {
                    if let Some(session_id) = sess_json
                        .get("session_id")
                        .or_else(|| sess_json.get("sessionId"))
                        .or_else(|| sess_json.get("id"))
                        .and_then(|v| v.as_str())
                    {
                        let link_url =
                            format!("{}/tool_router/session/{}/link", self.base_url, session_id);
                        let mut link_body = serde_json::json!({ "toolkit": normalized_app });
                        if let Some(redir) = redirect_uri {
                            link_body["callback_url"] = serde_json::json!(redir);
                        }

                        let link_res = self
                            .client
                            .post(&link_url)
                            .headers(self.headers()?)
                            .json(&link_body)
                            .send()
                            .await;

                        if let Ok(l_res) = link_res {
                            if l_res.status().is_success() {
                                if let Ok(l_json) = l_res.json::<serde_json::Value>().await {
                                    if let Some(raw_url) = l_json
                                        .get("redirect_url")
                                        .or_else(|| l_json.get("redirectUrl"))
                                        .or_else(|| l_json.get("url"))
                                        .and_then(|v| v.as_str())
                                    {
                                        let safe_auth_url = validate_auth_url(raw_url)?;
                                        let connection_id = l_json
                                            .get("connected_account_id")
                                            .or_else(|| l_json.get("id"))
                                            .and_then(|v| v.as_str())
                                            .map(ToString::to_string)
                                            .unwrap_or_else(|| {
                                                format!(
                                                    "conn_{}_{}",
                                                    normalized_app,
                                                    uuid::Uuid::new_v4()
                                                )
                                            });

                                        return Ok(AuthorizeResponse {
                                            authorization_url: safe_auth_url,
                                            connection_id,
                                            expires_at: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Fallback to auth_config + connected_accounts/link flow
        let auth_config_id = self.resolve_auth_config_id(&normalized_app).await?;

        let link_url = format!("{}/connected_accounts/link", self.base_url);
        let link_req = ComposioLinkRequest {
            auth_config_id,
            user_id: user_id.unwrap_or("default").to_string(),
            callback_url: redirect_uri.map(ToString::to_string),
        };

        let response = self
            .client
            .post(&link_url)
            .headers(self.headers()?)
            .json(&link_req)
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!(
                "Composio API error ({status}): {body}"
            )));
        }

        let parsed: ComposioLinkResponse = response
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("Failed to parse Composio response: {e}")))?;

        let raw_auth_url = parsed
            .redirect_url
            .or(parsed.redirect_url_camel)
            .or(parsed.url)
            .ok_or_else(|| {
                AppError::Provider("Composio did not return an authorization URL".into())
            })?;

        let connection_id = parsed
            .connected_account_id
            .or(parsed.id)
            .unwrap_or_else(|| format!("conn_{}_{}", normalized_app, uuid::Uuid::new_v4()));

        let safe_auth_url = validate_auth_url(&raw_auth_url)?;

        Ok(AuthorizeResponse {
            authorization_url: safe_auth_url,
            connection_id,
            expires_at: None,
        })
    }

    async fn check_status(&self, connection_id: &str) -> Result<ConnectionStatus, AppError> {
        let Some(api_key) = &self.api_key else {
            return Ok(ConnectionStatus::Disconnected);
        };

        if api_key.trim().is_empty() {
            return Ok(ConnectionStatus::Disconnected);
        }

        let url = format!("{}/connected_accounts/{}", self.base_url, connection_id);
        let response = self
            .client
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(ConnectionStatus::Disconnected);
        }

        if !response.status().is_success() {
            let status = response.status();
            return Err(AppError::Provider(format!(
                "Failed to check connection status from Composio ({status})"
            )));
        }

        let account: ComposioAccountV3 = response
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("Failed to parse account response: {e}")))?;

        let status_str = account.status.unwrap_or_default();
        Ok(map_composio_status(&status_str))
    }

    async fn call_tool(
        &self,
        app_id: &str,
        tool_name: &str,
        arguments: &Value,
        account_id: Option<&str>,
    ) -> Result<Value, AppError> {
        self.require_api_key()?;
        self.validate_catalog_app(Some(app_id))?;
        let tool_name = tool_name.trim();
        if tool_name.is_empty() {
            return Err(AppError::Validation("MCP tool name cannot be empty".into()));
        }

        let mut call_arguments = arguments
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::Validation("MCP tool arguments must be an object".into()))?;
        if let Some(account_id) = account_id.map(str::trim).filter(|value| !value.is_empty()) {
            // Composio's multi-account MCP contract uses the account field for
            // explicit account selection. Preserve an explicit caller value.
            call_arguments
                .entry("account".to_string())
                .or_insert_with(|| Value::String(account_id.to_string()));
        }

        let result = self
            .call_mcp_method(
                "tools/call",
                serde_json::json!({
                    "name": tool_name,
                    "arguments": Value::Object(call_arguments),
                }),
            )
            .await?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(AppError::Provider(format!(
                "Composio MCP tool '{tool_name}' returned an error result"
            )));
        }
        Ok(result)
    }

    async fn list_accounts(&self, app_id: &str) -> Result<Vec<AppAccount>, AppError> {
        let normalized_app = app_id.trim().to_lowercase();
        let Some(api_key) = &self.api_key else {
            return Ok(Vec::new());
        };

        if api_key.trim().is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/connected_accounts?toolkit_slugs={}",
            self.base_url, normalized_app
        );
        let response = self
            .client
            .get(&url)
            .headers(self.headers()?)
            .send()
            .await
            .map_err(Self::sanitize_error)?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(AppError::Provider(format!(
                "Failed to list accounts from Composio ({status})"
            )));
        }

        let list: ComposioListAccountsResponseV3 = response
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("Failed to parse account list: {e}")))?;

        let accounts = list
            .items
            .into_iter()
            .filter_map(|acc| {
                let id = acc.id?;
                let status_str = acc.status.unwrap_or_default();
                let email = acc
                    .user_properties
                    .as_ref()
                    .and_then(|p| p.get("email").and_then(|e| e.as_str()))
                    .map(ToString::to_string);
                let display_name = acc
                    .user_properties
                    .as_ref()
                    .and_then(|p| {
                        p.get("name")
                            .or_else(|| p.get("username"))
                            .and_then(|n| n.as_str())
                    })
                    .map(ToString::to_string)
                    .or_else(|| email.clone());

                Some(AppAccount {
                    account_id: id,
                    alias: None,
                    display_name,
                    email,
                    status: map_composio_status(&status_str),
                    created_at: acc
                        .created_at
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                    last_used_at: None,
                })
            })
            .collect();

        Ok(accounts)
    }

    async fn disconnect(&self, app_id: &str, account_id: Option<&str>) -> Result<(), AppError> {
        let Some(api_key) = &self.api_key else {
            return Ok(());
        };

        if api_key.trim().is_empty() {
            return Ok(());
        }

        if let Some(acc_id) = account_id {
            let url = format!("{}/connected_accounts/{}", self.base_url, acc_id);
            let response = self
                .client
                .delete(&url)
                .headers(self.headers()?)
                .send()
                .await
                .map_err(Self::sanitize_error)?;

            if !response.status().is_success()
                && response.status() != reqwest::StatusCode::NOT_FOUND
            {
                let status = response.status();
                return Err(AppError::Provider(format!(
                    "Failed to delete account on Composio ({status})"
                )));
            }
        } else {
            // Disconnect all accounts for this app
            let accounts = self.list_accounts(app_id).await?;
            for acc in accounts {
                let url = format!("{}/connected_accounts/{}", self.base_url, acc.account_id);
                let _ = self
                    .client
                    .delete(&url)
                    .headers(self.headers()?)
                    .send()
                    .await;
            }
        }

        Ok(())
    }
}

pub fn get_composio_api_key(paths: &WorkPaths) -> Option<String> {
    if let Ok(Some(val)) = storage::get_secret(paths, COMPOSIO_API_KEY_SECRET_NAME) {
        if let Some(s) = val.as_str() {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    std::env::var("COMPOSIO_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioConfigInfo {
    #[serde(rename = "hasApiKey")]
    pub has_api_key: bool,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
}

pub fn save_composio_api_key(paths: &WorkPaths, key: &str) -> Result<(), AppError> {
    storage::save_secret(
        paths,
        COMPOSIO_API_KEY_SECRET_NAME,
        serde_json::json!(key.trim()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_auth_url_security() {
        // Valid HTTPS URL
        let valid = "https://backend.composio.dev/auth/start?id=123";
        assert_eq!(validate_auth_url(valid).unwrap(), valid);

        // Reject HTTP
        let http_url = "http://backend.composio.dev/auth/start";
        assert!(validate_auth_url(http_url).is_err());

        // Reject javascript scheme
        let js_url = "javascript:alert(1)";
        assert!(validate_auth_url(js_url).is_err());

        // Reject file scheme
        let file_url = "file:///etc/passwd";
        assert!(validate_auth_url(file_url).is_err());

        // Reject malformed URL
        let bad_url = "not a url";
        assert!(validate_auth_url(bad_url).is_err());
    }

    #[test]
    fn test_map_composio_status() {
        assert_eq!(map_composio_status("ACTIVE"), ConnectionStatus::Connected);
        assert_eq!(
            map_composio_status("CONNECTED"),
            ConnectionStatus::Connected
        );
        assert_eq!(map_composio_status("INITIATED"), ConnectionStatus::Pending);
        assert_eq!(map_composio_status("PENDING"), ConnectionStatus::Pending);
        assert_eq!(map_composio_status("EXPIRED"), ConnectionStatus::Expired);
        assert_eq!(map_composio_status("TIMEOUT"), ConnectionStatus::Expired);
        assert_eq!(map_composio_status("FAILED"), ConnectionStatus::Error);
        assert_eq!(
            map_composio_status("UNKNOWN_STATE"),
            ConnectionStatus::Disconnected
        );
    }

    #[tokio::test]
    async fn test_composio_provider_mock_flow() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let provider = ComposioAppProvider::new(None, None, paths);
        let catalog = provider.catalog().await.unwrap();
        assert_eq!(catalog.len(), 6);

        // Test authorize flow without API key -> returns error
        let auth_res = provider.authorize("gmail", None, None).await;
        assert!(auth_res.is_err());

        // Missing provider credentials must fail closed.
        let status = provider.check_status("conn_123").await.unwrap();
        assert_eq!(status, ConnectionStatus::Disconnected);

        // Disconnect without provider credentials remains idempotent.
        let dis = provider.disconnect("gmail", None).await;
        assert!(dis.is_ok());
    }
}
