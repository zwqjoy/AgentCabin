use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::time::timeout;
use url::Url;

use crate::agent::claude_stream::augmented_path;
use crate::process_ext::HideConsole;
use crate::work::connectors;
use crate::work::models::{WorkConnectorHealth, WorkConnectorHealthStatus};
use crate::work::paths::WorkPaths;
use crate::work::sandbox::{ExecutionCommand, WorkSandboxLauncher};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_TOOL_NAMES: usize = 100;
const MAX_RPC_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const RESERVED_WORK_ENV_KEYS: &[&str] = &[
    "HOME",
    "PATH",
    "TMPDIR",
    "TMP",
    "TEMP",
    "PI_CODING_AGENT_DIR",
    "CLAUDECODE",
    "npm_config_cache",
    "NPM_CONFIG_CACHE",
];

pub async fn install_adapter() -> Result<String, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    install_adapter_with_paths(&paths).await
}

pub async fn ensure_adapter() -> Result<(), String> {
    let paths = WorkPaths::app();
    ensure_adapter_for_paths(&paths).await
}

pub async fn ensure_adapter_for_paths(paths: &WorkPaths) -> Result<(), String> {
    paths.ensure_layout()?;
    connectors::ensure_config(paths)?;
    let package_mcp_runtime = crate::work::connector_package_manager::sync_mcp_runtime(paths)?;
    let agent_plugin_mcp_enabled =
        !crate::storage::agent_plugins::list_enabled_mcp_servers_with_root(paths.data_root())
            .is_empty();
    if (!connectors::has_enabled(paths)?
        && !package_mcp_runtime.enabled()
        && !agent_plugin_mcp_enabled)
        || connectors::is_adapter_installed(paths)
    {
        return Ok(());
    }
    install_adapter_with_paths(paths).await.map(|_| ())
}

async fn install_adapter_with_paths(paths: &WorkPaths) -> Result<String, String> {
    connectors::ensure_config(paths)?;
    crate::work::system_packages::ensure_common_system_package(
        paths,
        crate::work::system_packages::PI_MCP_ADAPTER_PACKAGE_NAME,
        crate::work::system_packages::PI_MCP_ADAPTER_VERSION,
        crate::work::system_packages::PI_MCP_ADAPTER_SOURCE,
    )
    .await?;
    Ok("Pi MCP adapter 已安装，下一次 Pi Work 会话会加载它".into())
}

pub async fn test(name: &str) -> Result<WorkConnectorHealth, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    test_with_paths(&paths, name).await
}

pub async fn test_with_paths(paths: &WorkPaths, name: &str) -> Result<WorkConnectorHealth, String> {
    let started = Instant::now();
    let checked_at = Utc::now().to_rfc3339();
    let (name, config) = read_connector(paths, name)?;
    if config
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(health(
            name,
            WorkConnectorHealthStatus::Disabled,
            "连接器已停用".into(),
            checked_at,
            started.elapsed(),
            Vec::new(),
        ));
    }

    let result = match connector_transport(&config) {
        "stdio" => probe_stdio(paths, &config).await,
        "streamable-http" => probe_streamable_http(&config, None).await,
        "sse" => probe_sse(&config, None).await,
        _ => Err("不支持的 MCP 连接器传输类型".into()),
    };
    match result {
        Ok(tool_names) => Ok(health(
            name,
            WorkConnectorHealthStatus::Healthy,
            format!("连接成功，发现 {} 个工具", tool_names.len()),
            checked_at,
            started.elapsed(),
            tool_names,
        )),
        Err(message) => Ok(health(
            name,
            WorkConnectorHealthStatus::Failed,
            message,
            checked_at,
            started.elapsed(),
            Vec::new(),
        )),
    }
}

/// Execute one MCP tool on the Host side.
///
/// Work Pi only supplies the logical connector/tool name. Configuration and
/// credentials are resolved here so a model-facing Pi process never needs to
/// perform the external request itself. The caller is responsible for putting
/// this operation through ToolPipeline before reaching this function.
pub async fn call_with_paths(
    paths: &WorkPaths,
    name: &str,
    tool_name: &str,
    arguments: &Value,
    proxy_url: Option<&str>,
) -> Result<Value, String> {
    let (name, config) = read_connector(paths, name)?;
    if config
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("MCP 连接器 '{name}' 已停用"));
    }

    let tool_name = tool_name.trim();
    if tool_name.is_empty() || tool_name.chars().count() > 200 {
        return Err("MCP 工具名称无效".into());
    }
    if tool_name.chars().any(char::is_control) {
        return Err("MCP 工具名称包含非法控制字符".into());
    }

    let result = match connector_transport(&config) {
        "stdio" => call_stdio(paths, &config, tool_name, arguments).await,
        "streamable-http" => call_streamable_http(&config, tool_name, arguments, proxy_url).await,
        "sse" => call_sse(&config, tool_name, arguments, proxy_url).await,
        _ => Err("不支持的 MCP 连接器传输类型".into()),
    }?;

    let encoded =
        serde_json::to_vec(&result).map_err(|error| format!("MCP 工具结果无法序列化: {error}"))?;
    if encoded.len() > MAX_RPC_RESPONSE_BYTES {
        return Err(format!(
            "MCP 工具结果过大（超过 {} bytes）",
            MAX_RPC_RESPONSE_BYTES
        ));
    }
    Ok(result)
}

/// Discover MCP tools on the Host side and return the protocol result object.
///
/// This is deliberately separate from `test_with_paths`: the internal MCP
/// bridge uses the complete tool metadata to keep the Pi adapter useful while
/// still ensuring that the actual connector is contacted by the Host.
pub async fn list_tools_with_paths(
    paths: &WorkPaths,
    name: &str,
    proxy_url: Option<&str>,
) -> Result<Value, String> {
    let (name, config) = read_connector(paths, name)?;
    if config
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("MCP 连接器 '{name}' 已停用"));
    }

    match connector_transport(&config) {
        "stdio" => list_stdio(paths, &config).await,
        "streamable-http" => list_streamable_http(&config, proxy_url).await,
        "sse" => list_sse(&config, proxy_url).await,
        _ => Err("不支持的 MCP 连接器传输类型".into()),
    }
}

fn health(
    name: String,
    status: WorkConnectorHealthStatus,
    message: String,
    checked_at: String,
    elapsed: Duration,
    tool_names: Vec<String>,
) -> WorkConnectorHealth {
    WorkConnectorHealth {
        name,
        status,
        message,
        checked_at,
        latency_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        tool_count: tool_names.len().min(u32::MAX as usize) as u32,
        tool_names,
    }
}

fn read_connector(paths: &WorkPaths, name: &str) -> Result<(String, Map<String, Value>), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("MCP 连接器名称不能为空".into());
    }
    let root = read_runtime_root(paths).map_err(|error| format!("Work MCP 配置无效: {error}"))?;
    let config = root
        .get("mcpServers")
        .or_else(|| root.get("mcp_servers"))
        .and_then(Value::as_object)
        .and_then(|servers| servers.get(name))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| format!("Work MCP 连接器 '{name}' 不存在"))?;
    Ok((name.to_string(), config))
}

/// Merge generated package projections without exposing their contents to the
/// Pi process. Legacy user-managed configuration remains untouched.
fn read_runtime_root(paths: &WorkPaths) -> Result<Value, String> {
    let mut root = connectors::read_runtime_root(paths)?;
    merge_generated_mcp_config(
        &mut root,
        &paths.work_connector_mcp_config_path(),
        "Connector Package",
        256 * 1024,
    )?;
    crate::work::connector_package_manager::resolve_runtime_secrets(paths, &mut root)?;
    merge_generated_mcp_config(
        &mut root,
        &crate::storage::agent_plugins::agent_plugin_mcp_config_path_with_root(
            paths.data_root(),
            "work",
        )?,
        "WorkBuddy expert",
        256 * 1024,
    )?;
    Ok(root)
}

fn merge_generated_mcp_config(
    root: &mut Value,
    path: &Path,
    label: &str,
    max_bytes: u64,
) -> Result<(), String> {
    let Ok(path_metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(format!("{label} MCP configuration is not a regular file"));
    }
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes {
        return Err(format!("{label} MCP configuration is too large"));
    }
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let generated_root = serde_json::from_str::<Value>(&content)
        .map_err(|error| format!("Invalid {label} MCP configuration: {error}"))?;
    let generated_servers = generated_root
        .get("mcpServers")
        .or_else(|| generated_root.get("mcp_servers"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{label} MCP configuration must contain mcpServers"))?;
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| "Work MCP configuration must be a JSON object".to_string())?;
    let servers = if root_object.contains_key("mcpServers") {
        root_object
            .get_mut("mcpServers")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "mcpServers must be a JSON object".to_string())?
    } else if root_object.contains_key("mcp_servers") {
        root_object
            .get_mut("mcp_servers")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "mcp_servers must be a JSON object".to_string())?
    } else {
        root_object.insert("mcpServers".into(), Value::Object(Map::new()));
        root_object
            .get_mut("mcpServers")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "mcpServers must be a JSON object".to_string())?
    };
    for (name, config) in generated_servers {
        servers.insert(name.clone(), config.clone());
    }
    Ok(())
}

fn connector_transport(config: &Map<String, Value>) -> &str {
    match config
        .get("type")
        .or_else(|| config.get("transport"))
        .and_then(Value::as_str)
    {
        Some("http") | Some("streamable-http") => "streamable-http",
        Some("sse") => "sse",
        _ if config.get("url").and_then(Value::as_str).is_some() => "streamable-http",
        _ => "stdio",
    }
}

fn mcp_execution_context(
    paths: &WorkPaths,
    config: &Map<String, Value>,
) -> Result<(PathBuf, Vec<PathBuf>, Vec<PathBuf>, bool), String> {
    let profile = paths.work_profile_dir();
    let Some(plugin_root) = config.get("_agentcabinPluginRoot").and_then(Value::as_str) else {
        return Ok((profile, Vec::new(), Vec::new(), false));
    };
    let plugin_data = config
        .get("_agentcabinPluginData")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "WorkBuddy expert MCP configuration is missing its data directory".to_string()
        })?;
    let plugin_root = std::fs::canonicalize(plugin_root)
        .map_err(|error| format!("WorkBuddy expert root is not accessible: {error}"))?;
    let plugin_data = std::fs::canonicalize(plugin_data)
        .map_err(|error| format!("WorkBuddy expert data directory is not accessible: {error}"))?;
    let packages_root = std::fs::canonicalize(
        crate::storage::agent_plugins::agent_plugin_packages_dir_with_root(paths.data_root()),
    )
    .map_err(|error| format!("WorkBuddy expert packages directory is not accessible: {error}"))?;
    let data_root = std::fs::canonicalize(
        crate::storage::agent_plugins::agent_plugin_data_dir_with_root(paths.data_root()),
    )
    .map_err(|error| format!("WorkBuddy expert data root is not accessible: {error}"))?;
    if !plugin_root.starts_with(&packages_root) || !plugin_data.starts_with(&data_root) {
        return Err("WorkBuddy expert MCP paths are outside AgentCabin-managed roots".to_string());
    }
    let current_dir = config
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| plugin_root.clone());
    let current_dir = std::fs::canonicalize(current_dir)
        .map_err(|error| format!("WorkBuddy expert MCP cwd is not accessible: {error}"))?;
    if !current_dir.starts_with(&plugin_root) && !current_dir.starts_with(&plugin_data) {
        return Err(
            "WorkBuddy expert MCP cwd is outside the package and data directory".to_string(),
        );
    }
    Ok((current_dir, vec![plugin_data], vec![plugin_root], true))
}

async fn call_stdio(
    paths: &WorkPaths,
    config: &Map<String, Value>,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let command_name = config
        .get("command")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "stdio 连接器缺少 command".to_string())?;
    let args = config
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "stdio 连接器参数必须是字符串".to_string())
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let profile = paths.work_profile_dir();
    std::fs::create_dir_all(&profile)
        .map_err(|error| format!("无法创建 Work MCP profile 目录: {error}"))?;
    let mut envs = vec![
        (
            "PI_CODING_AGENT_DIR".into(),
            profile.to_string_lossy().into_owned(),
        ),
        ("PATH".into(), augmented_path()),
    ];
    envs.extend(configured_environment(config)?);
    let (current_dir, writable_roots, read_only_roots, read_only_cwd) =
        mcp_execution_context(paths, config)?;
    let raw_command = ExecutionCommand {
        program: PathBuf::from(command_name),
        args: args.iter().cloned().map(OsString::from).collect(),
        current_dir,
        envs,
    };
    let confined = if read_only_cwd {
        WorkSandboxLauncher::confine_work_command_with_read_only_cwd(
            &raw_command,
            &profile,
            &writable_roots,
            &read_only_roots,
            true,
        )
    } else {
        WorkSandboxLauncher::confine_work_command(
            &raw_command,
            &profile,
            &writable_roots,
            &read_only_roots,
        )
    }
    .map_err(|error| format!("无法在 Work sandbox 中启动 stdio MCP Server: {error}"))?;
    let mut command = Command::new(&confined.program);
    command
        .env_clear()
        .args(&confined.args)
        .current_dir(&confined.current_dir)
        .env_remove("CLAUDECODE");
    for (key, value) in &confined.envs {
        command.env(key, value);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .hide_console();
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 stdio MCP Server: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法连接 stdio MCP Server 的输入".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法连接 stdio MCP Server 的输出".to_string())?;
    let mut reader = BufReader::new(stdout);

    let result = async {
        write_json_rpc(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "AgentCabin Work", "version": "1.0.0" }
                }
            }),
        )
        .await?;
        let _ = read_json_rpc_result(&mut reader, 1).await?;
        write_json_rpc(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
        )
        .await?;
        let arguments = if arguments.is_object() {
            arguments.clone()
        } else {
            json!({})
        };
        write_json_rpc(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": tool_name, "arguments": arguments }
            }),
        )
        .await?;
        read_json_rpc_result(&mut reader, 2).await
    }
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

fn http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = crate::work::web::with_proxy(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(PROBE_TIMEOUT)
            .user_agent("AgentCabin/1.0.0 (Work MCP Host)"),
        proxy_url,
    )?;
    if proxy_url.is_none() {
        // Do not inherit a machine-wide proxy for a connector that did not
        // opt into one. An explicit Work proxy must remain effective.
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|error| format!("无法创建 MCP HTTP 客户端: {error}"))
}

async fn call_streamable_http(
    config: &Map<String, Value>,
    tool_name: &str,
    arguments: &Value,
    proxy_url: Option<&str>,
) -> Result<Value, String> {
    let url = connector_url(config)?;
    let client = http_client(proxy_url)?;
    let headers = http_headers(config, "application/json, text/event-stream")?;
    call_http_session(&client, &url, &headers, tool_name, arguments).await
}

async fn call_sse(
    config: &Map<String, Value>,
    tool_name: &str,
    arguments: &Value,
    proxy_url: Option<&str>,
) -> Result<Value, String> {
    let base_url = connector_url(config)?;
    let client = http_client(proxy_url)?;
    let headers = http_headers(config, "text/event-stream")?;
    let response = client
        .get(base_url.clone())
        .headers(headers.clone())
        .send()
        .await
        .map_err(|error| format!("无法连接 MCP SSE Server: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "MCP SSE Server 返回 HTTP {}",
            response.status().as_u16()
        ));
    }
    let endpoint = sse_endpoint(response, &base_url).await?;
    call_http_session(&client, &endpoint, &headers, tool_name, arguments).await
}

async fn probe_stdio(
    paths: &WorkPaths,
    config: &Map<String, Value>,
) -> Result<Vec<String>, String> {
    let result = list_stdio(paths, config).await?;
    extract_tool_names(&result)
}

async fn list_stdio(paths: &WorkPaths, config: &Map<String, Value>) -> Result<Value, String> {
    let command_name = config
        .get("command")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "stdio 连接器缺少 command".to_string())?;
    let args = config
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "stdio 连接器参数必须是字符串".to_string())
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let profile = paths.work_profile_dir();
    std::fs::create_dir_all(&profile)
        .map_err(|error| format!("无法创建 Work MCP profile 目录: {error}"))?;
    let mut envs = vec![
        (
            "PI_CODING_AGENT_DIR".into(),
            profile.to_string_lossy().into_owned(),
        ),
        ("PATH".into(), augmented_path()),
    ];
    envs.extend(configured_environment(config)?);
    let (current_dir, writable_roots, read_only_roots, read_only_cwd) =
        mcp_execution_context(paths, config)?;
    let raw_command = ExecutionCommand {
        program: PathBuf::from(command_name),
        args: args.iter().cloned().map(OsString::from).collect(),
        current_dir,
        envs,
    };
    let confined = if read_only_cwd {
        WorkSandboxLauncher::confine_work_command_with_read_only_cwd(
            &raw_command,
            &profile,
            &writable_roots,
            &read_only_roots,
            true,
        )
    } else {
        WorkSandboxLauncher::confine_work_command(
            &raw_command,
            &profile,
            &writable_roots,
            &read_only_roots,
        )
    }
    .map_err(|error| format!("无法在 Work sandbox 中启动 stdio MCP Server: {error}"))?;
    let mut command = Command::new(&confined.program);
    command
        .env_clear()
        .args(&confined.args)
        .current_dir(&confined.current_dir)
        .env_remove("CLAUDECODE");
    for (key, value) in &confined.envs {
        command.env(key, value);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .hide_console();
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 stdio MCP Server，请检查 command 和参数".to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法连接 stdio MCP Server 的输入".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法连接 stdio MCP Server 的输出".to_string())?;
    let mut reader = BufReader::new(stdout);
    let result = list_json_rpc(&mut stdin, &mut reader).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

fn configured_environment(config: &Map<String, Value>) -> Result<Vec<(String, String)>, String> {
    let mut configured = Vec::new();
    if let Some(environment_value) = config.get("env") {
        let values = environment_value
            .as_object()
            .ok_or_else(|| "MCP 连接器 env 必须是对象".to_string())?;
        for (key, value) in values {
            if key.starts_with("AGENTCABIN_")
                || RESERVED_WORK_ENV_KEYS
                    .iter()
                    .any(|reserved| key.eq_ignore_ascii_case(reserved))
            {
                return Err(format!("MCP 连接器环境变量 '{key}' 不能覆盖 Work 控制变量"));
            }
            if key.is_empty() || key.contains('=') || key.chars().any(|ch| ch == '\0') {
                return Err(format!("MCP 连接器环境变量 '{key}' 无效"));
            }
            let value = value
                .as_str()
                .ok_or_else(|| format!("MCP 连接器环境变量 '{key}' 必须是字符串"))?;
            if value.chars().any(|ch| ch == '\0') {
                return Err(format!("MCP 连接器环境变量 '{key}' 的值无效"));
            }
            configured.push((key.to_string(), value.to_string()));
        }
    }
    Ok(configured)
}

async fn list_json_rpc(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
) -> Result<Value, String> {
    write_json_rpc(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "AgentCabin Work", "version": "1.0.0" }
            }
        }),
    )
    .await?;
    let _ = read_json_rpc_result(reader, 1).await?;
    write_json_rpc(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    )
    .await?;
    write_json_rpc(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    )
    .await?;
    read_json_rpc_result(reader, 2).await
}

async fn write_json_rpc(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    let encoded = serde_json::to_string(message).map_err(|error| error.to_string())?;
    stdin
        .write_all(format!("{encoded}\n").as_bytes())
        .await
        .map_err(|_| "无法向 MCP Server 发送请求".to_string())?;
    stdin
        .flush()
        .await
        .map_err(|_| "无法刷新 MCP Server 请求".to_string())
}

async fn read_json_rpc_result(
    reader: &mut BufReader<ChildStdout>,
    id: u64,
) -> Result<Value, String> {
    let result = timeout(PROBE_TIMEOUT, async {
        loop {
            let mut line = String::new();
            let read = reader
                .read_line(&mut line)
                .await
                .map_err(|_| "读取 MCP Server 响应失败".to_string())?;
            if read == 0 {
                return Err("MCP Server 在返回结果前退出".to_string());
            }
            if line.len() > MAX_RPC_RESPONSE_BYTES {
                return Err(format!(
                    "MCP stdio 响应过大（超过 {} bytes）",
                    MAX_RPC_RESPONSE_BYTES
                ));
            }
            let line = line.trim();
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("id") != Some(&Value::from(id)) {
                continue;
            }
            if value.get("error").is_some() {
                return Err("MCP Server 返回协议错误".to_string());
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "MCP Server 响应缺少 result".to_string());
        }
    })
    .await;
    match result {
        Ok(value) => value,
        Err(_) => Err("MCP Server 响应超时".into()),
    }
}

async fn probe_streamable_http(
    config: &Map<String, Value>,
    proxy_url: Option<&str>,
) -> Result<Vec<String>, String> {
    let result = list_streamable_http(config, proxy_url).await?;
    extract_tool_names(&result)
}

async fn list_streamable_http(
    config: &Map<String, Value>,
    proxy_url: Option<&str>,
) -> Result<Value, String> {
    let url = connector_url(config)?;
    let client = http_client(proxy_url)?;
    let headers = http_headers(config, "application/json, text/event-stream")?;
    list_http_session(&client, &url, &headers).await
}

async fn probe_sse(
    config: &Map<String, Value>,
    proxy_url: Option<&str>,
) -> Result<Vec<String>, String> {
    let result = list_sse(config, proxy_url).await?;
    extract_tool_names(&result)
}

async fn list_sse(config: &Map<String, Value>, proxy_url: Option<&str>) -> Result<Value, String> {
    let base_url = connector_url(config)?;
    let client = http_client(proxy_url)?;
    let headers = http_headers(config, "text/event-stream")?;
    let response = client
        .get(base_url.clone())
        .headers(headers.clone())
        .send()
        .await
        .map_err(|_| "无法连接 MCP SSE Server".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "MCP SSE Server 返回 HTTP {}",
            response.status().as_u16()
        ));
    }
    let endpoint = sse_endpoint(response, &base_url).await?;
    list_http_session(&client, &endpoint, &headers).await
}

async fn list_http_session(
    client: &reqwest::Client,
    url: &Url,
    headers: &HeaderMap,
) -> Result<Value, String> {
    let (session_headers, session_id) = initialize_http_session(client, url, headers).await?;
    let tools_list = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let (result, _) = post_rpc(
        client,
        url,
        &session_headers,
        &tools_list,
        session_id.as_deref(),
    )
    .await?;
    result.ok_or_else(|| "MCP HTTP Server 未返回 tools/list 结果".to_string())
}

async fn initialize_http_session(
    client: &reqwest::Client,
    url: &Url,
    headers: &HeaderMap,
) -> Result<(HeaderMap, Option<String>), String> {
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "AgentCabin Work", "version": "1.0.0" }
        }
    });
    let (initialize_result, session_id) = post_rpc(client, url, headers, &initialize, None).await?;
    let initialize_result =
        initialize_result.ok_or_else(|| "MCP HTTP Server 未返回 initialize 结果".to_string())?;
    let protocol_version = initialize_result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(MCP_PROTOCOL_VERSION);
    let mut session_headers = headers.clone();
    session_headers.insert(
        HeaderName::from_static("mcp-protocol-version"),
        HeaderValue::from_str(protocol_version)
            .map_err(|_| "MCP Server 返回了无效的协议版本".to_string())?,
    );
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    let _ = post_rpc(
        client,
        url,
        &session_headers,
        &initialized,
        session_id.as_deref(),
    )
    .await?;
    Ok((session_headers, session_id))
}

async fn call_http_session(
    client: &reqwest::Client,
    url: &Url,
    headers: &HeaderMap,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let (session_headers, session_id) = initialize_http_session(client, url, headers).await?;
    let arguments = if arguments.is_object() {
        arguments.clone()
    } else {
        json!({})
    };
    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": tool_name, "arguments": arguments }
    });
    let (result, _) = post_rpc(
        client,
        url,
        &session_headers,
        &request,
        session_id.as_deref(),
    )
    .await?;
    result.ok_or_else(|| "MCP HTTP Server 未返回 tools/call 结果".to_string())
}

async fn post_rpc(
    client: &reqwest::Client,
    url: &Url,
    headers: &HeaderMap,
    message: &Value,
    session_id: Option<&str>,
) -> Result<(Option<Value>, Option<String>), String> {
    let mut request = client
        .post(url.clone())
        .headers(headers.clone())
        .json(message);
    if let Some(session_id) = session_id {
        request = request.header("Mcp-Session-Id", session_id);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("MCP HTTP 请求失败: {e}"))?;
    let response_session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| session_id.map(str::to_string));
    if !response.status().is_success() {
        return Err(format!(
            "MCP HTTP Server 返回 HTTP {}",
            response.status().as_u16()
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| "读取 MCP HTTP 响应失败".to_string())?;
    if body.len() > MAX_RPC_RESPONSE_BYTES {
        return Err(format!(
            "MCP HTTP 响应过大（超过 {} bytes）",
            MAX_RPC_RESPONSE_BYTES
        ));
    }
    let body = String::from_utf8(body.to_vec())
        .map_err(|_| "MCP HTTP 响应不是有效的 UTF-8 JSON/SSE".to_string())?;
    if body.trim().is_empty() {
        return Ok((None, response_session_id));
    }
    let result = parse_rpc_body(&body)?;
    Ok((Some(result), response_session_id))
}

fn parse_rpc_body(body: &str) -> Result<Value, String> {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        return rpc_result(value);
    }
    for line in body.lines().rev() {
        let candidate = line.trim().strip_prefix("data:").unwrap_or(line).trim();
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return rpc_result(value);
        }
    }
    Err("MCP Server 返回了无法解析的响应".into())
}

fn rpc_result(value: Value) -> Result<Value, String> {
    if value.get("error").is_some() {
        return Err("MCP Server 返回协议错误".into());
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "MCP Server 响应缺少 result".into())
}

async fn sse_endpoint(response: reqwest::Response, base_url: &Url) -> Result<Url, String> {
    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    let mut event_name = String::new();
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("MCP SSE Server 未发送 endpoint".into());
        }
        let Some(chunk) = timeout(remaining, stream.next())
            .await
            .map_err(|_| "MCP SSE Server 未发送 endpoint".to_string())?
        else {
            return Err("MCP SSE Server 在返回 endpoint 前关闭连接".into());
        };
        let chunk = chunk.map_err(|_| "读取 MCP SSE 响应失败".to_string())?;
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = pending.find('\n') {
            let line = pending[..index].trim_end_matches('\r').to_string();
            pending = pending[index + 1..].to_string();
            if let Some(value) = line.strip_prefix("event:") {
                event_name = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                if event_name == "endpoint" {
                    return base_url
                        .join(value.trim())
                        .map_err(|_| "MCP SSE endpoint 无效".to_string());
                }
            }
        }
    }
}

fn connector_url(config: &Map<String, Value>) -> Result<Url, String> {
    let raw = config
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "HTTP MCP 连接器缺少 URL".to_string())?;
    let url = Url::parse(raw).map_err(|_| "MCP URL 无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("MCP URL 只支持 http 或 https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("MCP URL 不允许内嵌用户名或密码".into());
    }
    for (key, _) in url.query_pairs() {
        let key = key.to_ascii_lowercase();
        if ["token", "key", "secret", "password", "auth", "credential"]
            .iter()
            .any(|sensitive| key.contains(sensitive))
        {
            return Err("MCP URL 不允许通过查询参数传递凭据".into());
        }
    }
    Ok(url)
}

fn http_headers(config: &Map<String, Value>, accept: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_str(accept).map_err(|_| "MCP Accept 请求头无效".to_string())?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(values) = config.get("headers") {
        let values = values
            .as_object()
            .ok_or_else(|| "MCP 连接器 headers 必须是对象".to_string())?;
        for (key, value) in values {
            let value = value
                .as_str()
                .ok_or_else(|| format!("MCP 请求头 '{key}' 必须是字符串"))?;
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| format!("MCP 请求头名称 '{key}' 无效"))?;
            let header_value =
                HeaderValue::from_str(value).map_err(|_| format!("MCP 请求头 '{key}' 的值无效"))?;
            headers.insert(header_name, header_value);
        }
    }
    Ok(headers)
}

fn extract_tool_names(result: &Value) -> Result<Vec<String>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "MCP tools/list 响应缺少 tools".to_string())?;
    Ok(tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .take(MAX_TOOL_NAMES)
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn paths(temp: &TempDir) -> WorkPaths {
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        paths
    }

    #[test]
    fn rejects_environment_overrides_of_work_control_variables() {
        for key in [
            "AGENTCABIN_WORK_RUN_DIR",
            "AGENTCABIN_WORKSPACE_ROOT",
            "PI_CODING_AGENT_DIR",
            "home",
            "npm_config_cache",
        ] {
            let config = serde_json::json!({ "env": { key: "attacker-controlled" } });
            let config = config.as_object().unwrap();
            assert!(
                configured_environment(config).is_err(),
                "{key} must be reserved"
            );
        }
    }

    #[test]
    fn rejects_invalid_environment_names_and_values() {
        let config = serde_json::json!({ "env": { "A=B": "value" } });
        assert!(configured_environment(config.as_object().unwrap()).is_err());

        let config = serde_json::json!({ "env": { "TOKEN": "bad\u{0000}value" } });
        assert!(configured_environment(config.as_object().unwrap()).is_err());
    }

    #[tokio::test]
    async fn reports_disabled_connector_without_starting_a_server() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            r#"{"mcpServers":{"disabled":{"command":"not-a-real-command","disabled":true}}}"#,
        )
        .unwrap();

        let health = test_with_paths(&paths, "disabled").await.unwrap();
        assert_eq!(health.status, WorkConnectorHealthStatus::Disabled);
        assert_eq!(health.tool_count, 0);
    }

    #[tokio::test]
    async fn skips_adapter_install_when_no_connector_is_enabled() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(paths.work_mcp_config_path(), r#"{"mcpServers":{}}"#).unwrap();

        ensure_adapter_for_paths(&paths).await.unwrap();

        assert!(!connectors::is_adapter_installed(&paths));
    }

    #[tokio::test]
    async fn accepts_an_already_installed_adapter_without_reinstalling() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            r#"{"mcpServers":{"fixture":{"command":"fixture"}}}"#,
        )
        .unwrap();
        let package_dir = paths
            .pi_system_dir()
            .join("npm")
            .join("node_modules")
            .join("pi-mcp-adapter");
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(
            package_dir.join("package.json"),
            format!(
                r#"{{"name":"pi-mcp-adapter","version":"{}"}}"#,
                crate::work::system_packages::PI_MCP_ADAPTER_VERSION
            ),
        )
        .unwrap();

        ensure_adapter_for_paths(&paths).await.unwrap();

        assert!(connectors::is_adapter_installed(&paths));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probes_stdio_server_and_lists_tools() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let server = temp.path().join("server.sh");
        fs::write(
            &server,
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *"\"method\":\"initialize\""*) echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}' ;;
    *"\"method\":\"tools/list\""*) echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"fixture_echo"},{"name":"fixture_status"}]}}' ;;
  esac
done
"#,
        )
        .unwrap();
        fs::set_permissions(&server, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            paths.work_mcp_config_path(),
            serde_json::json!({
                "mcpServers": {
                    "fixture": { "command": server.to_string_lossy() }
                }
            })
            .to_string(),
        )
        .unwrap();

        let health = test_with_paths(&paths, "fixture").await.unwrap();
        assert_eq!(health.status, WorkConnectorHealthStatus::Healthy);
        assert_eq!(health.tool_names, vec!["fixture_echo", "fixture_status"]);
    }

    #[tokio::test]
    async fn parses_streamable_http_tools() {
        use axum::{routing::post, Json, Router};
        use tokio::net::TcpListener;

        async fn handler(Json(request): Json<Value>) -> Json<Value> {
            match request.get("method").and_then(Value::as_str) {
                Some("tools/list") => Json(json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": { "tools": [{ "name": "http_echo" }] }
                })),
                _ => Json(json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": { "capabilities": {} }
                })),
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(handler)))
                .await
                .unwrap();
        });
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            serde_json::json!({
                "mcpServers": {
                    "http": { "type": "streamable-http", "url": format!("http://{address}/mcp") }
                }
            })
            .to_string(),
        )
        .unwrap();

        let health = test_with_paths(&paths, "http").await.unwrap();
        server.abort();
        assert_eq!(
            health.status,
            WorkConnectorHealthStatus::Healthy,
            "Streamable HTTP failed: {}",
            health.message
        );
        assert_eq!(health.tool_names, vec!["http_echo"]);
    }

    #[tokio::test]
    async fn calls_streamable_http_tool_on_host() {
        use axum::{routing::post, Json, Router};
        use tokio::net::TcpListener;

        async fn handler(Json(request): Json<Value>) -> Json<Value> {
            let result = match request.get("method").and_then(Value::as_str) {
                Some("tools/call") => json!({
                    "content": [{ "type": "text", "text": "host-ok" }]
                }),
                _ => json!({ "capabilities": {} }),
            };
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": result
            }))
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(handler)))
                .await
                .unwrap();
        });
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            serde_json::json!({
                "mcpServers": {
                    "http": { "type": "streamable-http", "url": format!("http://{address}/mcp") }
                }
            })
            .to_string(),
        )
        .unwrap();

        let result = call_with_paths(&paths, "http", "http_echo", &json!({ "x": 1 }), None)
            .await
            .unwrap();
        server.abort();
        assert_eq!(result["content"][0]["text"], "host-ok");
    }

    #[tokio::test]
    async fn parses_sse_server_endpoint_and_tools() {
        use axum::{
            body::Body,
            http::Response,
            routing::{get, post},
            Json, Router,
        };
        use tokio::net::TcpListener;

        async fn endpoint() -> Response<Body> {
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from("event: endpoint\ndata: /messages\n\n"))
                .unwrap()
        }

        async fn handler(Json(request): Json<Value>) -> Json<Value> {
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": if request.get("method").and_then(Value::as_str) == Some("tools/list") {
                    json!({ "tools": [{ "name": "sse_echo" }] })
                } else {
                    json!({ "capabilities": {} })
                }
            }))
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/sse", get(endpoint))
                    .route("/messages", post(handler)),
            )
            .await
            .unwrap();
        });
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            serde_json::json!({
                "mcpServers": {
                    "sse": { "type": "sse", "url": format!("http://{address}/sse") }
                }
            })
            .to_string(),
        )
        .unwrap();

        let health = test_with_paths(&paths, "sse").await.unwrap();
        server.abort();
        assert_eq!(
            health.status,
            WorkConnectorHealthStatus::Healthy,
            "SSE failed: {}",
            health.message
        );
        assert_eq!(health.tool_names, vec!["sse_echo"]);
    }
}
