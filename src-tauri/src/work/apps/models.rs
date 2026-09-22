use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppAuthType {
    OAuth2,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Expired,
    Error,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCatalogItem {
    pub app_id: String,
    pub display_name: String,
    pub icon: String,
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_auth_type")]
    pub auth_type: AppAuthType,
    #[serde(default)]
    pub documentation_url: Option<String>,
}

fn default_auth_type() -> AppAuthType {
    AppAuthType::OAuth2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppAccount {
    pub account_id: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub status: ConnectionStatus,
    pub created_at: String,
    #[serde(default)]
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppConnection {
    pub connection_id: String,
    pub app_id: String,
    pub provider: String,
    pub status: ConnectionStatus,
    #[serde(default)]
    pub accounts: Vec<AppAccount>,
    #[serde(default)]
    pub last_checked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeRequest {
    pub app_id: String,
    #[serde(default)]
    pub redirect_url: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeResponse {
    pub authorization_url: String,
    pub connection_id: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapability {
    pub capability_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub run_id: String,
    pub app_id: String,
    #[serde(default)]
    pub account_id: Option<String>,
    pub token: String,
    pub expires_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAppDefaultAccount {
    pub workspace_id: String,
    pub app_id: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppsConfigFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub connections: Vec<AppConnection>,
    #[serde(default)]
    pub workspace_defaults: Vec<WorkspaceAppDefaultAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSecretsFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub secrets: std::collections::HashMap<String, serde_json::Value>,
}

fn default_version() -> u32 {
    1
}
