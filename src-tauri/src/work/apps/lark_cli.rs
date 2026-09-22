use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::work::apps::models::{AppAccount, AppConnection, ConnectionStatus};
use crate::work::connector_package_manager;
use crate::work::paths::WorkPaths;

pub const LARK_CLI_VERSION: &str = "1.0.89";
const LARK_CLI_PACKAGE: &str = "@larksuite/cli@1.0.89";
const AUTH_TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) const FEISHU_CLI_JSON: &str = r#"{
  "command": "lark-cli",
  "operations": {
    "versionCheck": {
      "args": ["--version"],
      "output": "text",
      "requiresConfirmation": false
    },
    "configShow": {
      "args": ["config", "show"],
      "output": "text",
      "requiresConfirmation": false
    },
    "init": {
      "args": ["config", "init", "--new", "--lang", "zh"],
      "timeoutSeconds": 600,
      "output": "text",
      "requiresConfirmation": true
    },
    "auth": {
      "args": ["auth", "login", "--recommend"],
      "timeoutSeconds": 600,
      "output": "text",
      "requiresConfirmation": true
    },
    "status": {
      "args": ["auth", "status", "--json", "--verify"],
      "timeoutSeconds": 120,
      "output": "json",
      "requiresConfirmation": false
    },
    "searchUser": {
      "args": ["contact", "+search-user"],
      "timeoutSeconds": 120,
      "output": "json",
      "requiresConfirmation": false
    },
    "unAuth": {
      "args": ["auth", "logout"],
      "timeoutSeconds": 120,
      "output": "text",
      "requiresConfirmation": true
    },
    "sendMessage": {
      "args": ["im", "+messages-send"],
      "timeoutSeconds": 120,
      "output": "json",
      "requiresConfirmation": true
    }
  },
  "redactionFields": ["token", "secret", "password", "authorization", "api_key", "access_token", "refresh_token"]
}"#;

const LARK_SKILLS: &[(&str, &str, &str)] = &[
    (
        "lark-contact",
        "按姓名或关键词解析飞书用户 open_id",
        r#"---
name: lark-contact
description: 按姓名或关键词解析飞书用户 open_id
---

# 飞书通讯录

解析姓名、邮箱或关键词时，必须使用 `work_run_connector_cli` 的 `feishu/searchUser` 操作，不要调用全局 `lark-cli`，也不要退回 `work_run_command`。args 形如：
["--as", "user", "--query", "<关键词>"]

只把 CLI 返回的明确用户 open_id 作为后续飞书操作的收件人；存在多个匹配时先向用户确认，不要猜测。
"#,
    ),
    (
        "lark-im",
        "发送与查询飞书/Lark 即时通讯消息、单聊、群聊通知与富文本卡片",
        r#"---
name: lark-im
description: 发送与查询飞书/Lark 即时通讯消息、单聊、群聊通知与富文本卡片
---

# 飞书即时通讯

发送消息必须使用 `work_run_connector_cli` 的 `feishu/sendMessage` 操作，不要调用全局 `lark-cli`，也不要退回 `work_run_command`。默认按用户要求使用机器人身份发送；文本消息的 args 形如：
["--as", "bot", "--user-id", "<接收方 user_id 或 chat_id>", "--text", "<消息正文>"]

只有 CLI 返回 ok: true 且包含 message_id 时，才可以向用户报告发送成功。
"#,
    ),
    (
        "lark-doc",
        "创建、读取、更新飞书/Lark 云文档与知识库页面",
        r#"---
name: lark-doc
description: 创建、读取、更新飞书/Lark 云文档与知识库页面
---

# 飞书云文档

处理云文档、知识库和会议纪要时使用 AgentCabin 已投影的飞书 CLI Package 操作。
"#,
    ),
    (
        "lark-base",
        "读写飞书多维表格（Base / Bitable）",
        r#"---
name: lark-base
description: 读写飞书多维表格（Base / Bitable）
---

# 飞书多维表格

检索、管理或更新多维表格时使用 AgentCabin 已投影的飞书 CLI Package 操作。
"#,
    ),
    (
        "lark-calendar",
        "查询飞书日程、预定会议与创建日程",
        r#"---
name: lark-calendar
description: 查询飞书日程、预定会议与创建日程
---

# 飞书日历

处理日程和会议时使用 AgentCabin 已投影的飞书 CLI Package 操作。
"#,
    ),
    (
        "lark-mail",
        "读取、搜索与发送飞书企业邮箱邮件",
        r#"---
name: lark-mail
description: 读取、搜索与发送飞书企业邮箱邮件
---

# 飞书邮箱

处理飞书邮箱时使用 AgentCabin 已投影的飞书 CLI Package 操作。
"#,
    ),
    (
        "lark-sheet",
        "读写飞书电子表格单元格数据",
        r#"---
name: lark-sheet
description: 读写飞书电子表格单元格数据
---

# 飞书电子表格

电子表格写入必须经过 Work policy 的确认。
"#,
    ),
    (
        "lark-task",
        "创建与管理飞书待办任务",
        r#"---
name: lark-task
description: 创建与管理飞书待办任务
---

# 飞书任务

待办任务的外部写入必须经过 Work policy 的确认。
"#,
    ),
];

pub(crate) fn builtin_skill_files() -> &'static [(&'static str, &'static str, &'static str)] {
    LARK_SKILLS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkCliInfo {
    pub installed: bool,
    pub command: String,
    pub version: Option<String>,
    pub auth_url: String,
    #[serde(rename = "privateInstall")]
    pub private_install: bool,
    #[serde(rename = "configScope")]
    pub config_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LarkIdentitySummary {
    pub identity: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub open_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LarkAuthStatus {
    pub task_id: Option<String>,
    pub status: String,
    pub stage: String,
    pub message: String,
    pub url: Option<String>,
    pub identity: Option<LarkIdentitySummary>,
    pub connection: Option<AppConnection>,
}

static AUTH_TASKS: OnceLock<Mutex<HashMap<String, LarkAuthStatus>>> = OnceLock::new();

fn auth_tasks() -> &'static Mutex<HashMap<String, LarkAuthStatus>> {
    AUTH_TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn private_lark_cli_path(paths: &WorkPaths) -> PathBuf {
    crate::work::cli_runtime::managed_command_path(paths, "lark-cli")
}

pub(crate) fn private_lark_cli_exists(paths: &WorkPaths) -> bool {
    private_lark_cli_path(paths).is_file()
}

fn lark_path(paths: &WorkPaths) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    crate::work::cli_runtime::augment_path(paths, &current)
}

fn configure_lark_command(command: &mut Command, paths: &WorkPaths) {
    // Keep the CLI's native user configuration location. Work supplies the
    // AgentCabin-managed application-global binary, while the sandbox grants
    // only the HOME paths declared by the Connector Package. This matches
    // WorkBuddy and keeps user-installed CLI credentials usable without
    // exposing the whole HOME as writable.
    command.env("PATH", lark_path(paths));
}

fn cli_output_text(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    }
}

pub fn check_lark_cli_installed(paths: &WorkPaths) -> LarkCliInfo {
    let version = if private_lark_cli_exists(paths) {
        let mut command = Command::new(private_lark_cli_path(paths));
        configure_lark_command(&mut command, paths);
        command
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| cli_output_text(&output))
            .and_then(|text| {
                text.lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
            })
    } else {
        None
    };

    LarkCliInfo {
        installed: version.is_some(),
        command: "lark-cli (AgentCabin 应用级托管)".into(),
        version,
        auth_url: "https://open.feishu.cn/page/cli?from=cli".into(),
        private_install: true,
        config_scope: "User HOME / AgentCabin 应用级 CLI 包".into(),
    }
}

pub fn install_lark_cli(paths: &WorkPaths) -> Result<String, String> {
    paths.ensure_layout()?;
    crate::work::cli_runtime::install_npm_package(paths, LARK_CLI_PACKAGE)?;
    if !private_lark_cli_exists(paths) {
        return Err(format!(
            "npm 安装完成，但未找到 AgentCabin 应用级 lark-cli: {}",
            private_lark_cli_path(paths).display()
        ));
    }
    Ok(format!(
        "AgentCabin 应用级 lark-cli {LARK_CLI_VERSION} 安装成功"
    ))
}

pub fn mount_lark_skills_to_profile(paths: &WorkPaths) -> Result<Vec<String>, String> {
    connector_package_manager::ensure_builtin_feishu_package(paths)?;
    Ok(LARK_SKILLS
        .iter()
        .map(|(slug, _, _)| (*slug).to_string())
        .collect())
}

pub fn configure_lark_cli(app_id: &str, app_secret: &str) -> Result<(), String> {
    let paths = WorkPaths::app();
    configure_lark_cli_with_paths(&paths, app_id, app_secret)
}

pub fn configure_lark_cli_with_paths(
    paths: &WorkPaths,
    app_id: &str,
    app_secret: &str,
) -> Result<(), String> {
    paths.ensure_layout()?;
    if !private_lark_cli_exists(paths) {
        return Err("AgentCabin 应用级 lark-cli 尚未安装".into());
    }

    let mut downgrade = Command::new(private_lark_cli_path(paths));
    configure_lark_command(&mut downgrade, paths);
    let _ = downgrade.args(["config", "keychain-downgrade"]).output();

    let mut command = Command::new(private_lark_cli_path(paths));
    configure_lark_command(&mut command, paths);
    let mut child = command
        .args([
            "config",
            "init",
            "--brand",
            "feishu",
            "--app-id",
            app_id.trim(),
            "--app-secret-stdin",
            "--force-init",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 AgentCabin 应用级 lark-cli 配置失败: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(app_secret.trim().as_bytes())
            .map_err(|error| format!("写入 lark-cli 配置输入失败: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("等待 lark-cli 配置完成失败: {error}"))?;
    if !output.status.success() {
        return Err(format!("lark-cli 配置失败: {}", cli_output_text(&output)));
    }
    Ok(())
}

fn run_lark_cli(paths: &WorkPaths, args: &[&str], timeout: Duration) -> Result<String, String> {
    if !private_lark_cli_exists(paths) {
        return Err("AgentCabin 应用级 lark-cli 尚未安装，请先安装连接器运行时".into());
    }
    let mut command = Command::new(private_lark_cli_path(paths));
    configure_lark_command(&mut command, paths);
    let child = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 lark-cli 失败: {error}"))?;
    wait_for_output(child, timeout)
}

fn wait_for_output(mut child: Child, timeout: Duration) -> Result<String, String> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("等待 lark-cli 失败: {error}"))?
        {
            let output = child
                .wait_with_output()
                .map_err(|error| format!("读取 lark-cli 输出失败: {error}"))?;
            let text = cli_output_text(&output);
            if status.success() {
                return Ok(text);
            }
            return Err(if text.is_empty() {
                format!("lark-cli 退出失败: {status}")
            } else {
                text
            });
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("lark-cli 操作超过 {} 秒", timeout.as_secs()));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn new_pending_auth(task_id: String) -> LarkAuthStatus {
    LarkAuthStatus {
        task_id: Some(task_id),
        status: "pending".into(),
        stage: "starting".into(),
        message: "正在准备 AgentCabin 应用级飞书连接器…".into(),
        url: None,
        identity: None,
        connection: None,
    }
}

fn update_auth_task(task_id: &str, update: impl FnOnce(&mut LarkAuthStatus)) {
    if let Ok(mut tasks) = auth_tasks().lock() {
        if let Some(status) = tasks.get_mut(task_id) {
            update(status);
        }
    }
}

pub fn start_browser_auth(
    paths: &WorkPaths,
    alias: Option<&str>,
    email: Option<&str>,
) -> Result<LarkAuthStatus, String> {
    paths.ensure_layout()?;
    connector_package_manager::ensure_builtin_feishu_package(paths)?;
    if !private_lark_cli_exists(paths) {
        return Err("AgentCabin 应用级 lark-cli 尚未安装，请先点击“安装 CLI”".into());
    }

    if let Ok(identity) = verify_lark_auth(paths) {
        let connection = persist_feishu_connection(paths, &identity, alias, email)?;
        let task_id = format!("lark-auth-{}", uuid::Uuid::new_v4());
        let status = LarkAuthStatus {
            task_id: Some(task_id.clone()),
            status: "authenticated".into(),
            stage: "verified".into(),
            message: "飞书连接已存在，已完成本地验证。".into(),
            url: None,
            identity: Some(identity),
            connection: Some(connection),
        };
        auth_tasks()
            .lock()
            .map_err(|_| "飞书授权状态锁不可用".to_string())?
            .insert(task_id, status.clone());
        return Ok(status);
    }

    let task_id = format!("lark-auth-{}", uuid::Uuid::new_v4());
    let status = new_pending_auth(task_id.clone());
    auth_tasks()
        .lock()
        .map_err(|_| "飞书授权状态锁不可用".to_string())?
        .insert(task_id.clone(), status.clone());

    let task_paths = paths.clone();
    let alias = alias.map(str::to_string);
    let email = email.map(str::to_string);
    thread::spawn(move || {
        let result =
            run_browser_auth_task(&task_paths, &task_id, alias.as_deref(), email.as_deref());
        if let Err(error) = result {
            update_auth_task(&task_id, |status| {
                status.status = "error".into();
                status.stage = "failed".into();
                status.message = error;
            });
        }
    });

    Ok(status)
}

pub fn auth_status(paths: &WorkPaths, task_id: Option<&str>) -> Result<LarkAuthStatus, String> {
    if let Some(task_id) = task_id {
        if let Ok(tasks) = auth_tasks().lock() {
            if let Some(status) = tasks.get(task_id) {
                return Ok(status.clone());
            }
        }
    }
    match verify_lark_auth(paths) {
        Ok(identity) => {
            let connection = crate::work::apps::storage::get_connection(paths, "feishu")
                .map_err(|error| error.to_string())?;
            Ok(LarkAuthStatus {
                task_id: task_id.map(str::to_string),
                status: "authenticated".into(),
                stage: "verified".into(),
                message: "飞书 CLI 用户身份已验证。".into(),
                url: None,
                identity: Some(identity),
                connection,
            })
        }
        Err(error) => Ok(LarkAuthStatus {
            task_id: task_id.map(str::to_string),
            status: "idle".into(),
            stage: "not_authenticated".into(),
            message: error,
            url: None,
            identity: None,
            connection: None,
        }),
    }
}

pub fn logout_with_paths(paths: &WorkPaths) -> Result<(), String> {
    if !private_lark_cli_exists(paths) {
        return Ok(());
    }
    let _ = run_lark_cli(paths, &["auth", "logout"], Duration::from_secs(120))?;
    Ok(())
}

fn run_browser_auth_task(
    paths: &WorkPaths,
    task_id: &str,
    alias: Option<&str>,
    email: Option<&str>,
) -> Result<(), String> {
    let _ = run_lark_cli(
        paths,
        &["config", "keychain-downgrade"],
        Duration::from_secs(30),
    );

    let config_ready = run_lark_cli(paths, &["config", "show"], Duration::from_secs(30)).is_ok();
    if !config_ready {
        update_auth_task(task_id, |status| {
            status.stage = "app_registration".into();
            status.message = "正在打开飞书网页，创建并保存 AgentCabin 专属应用配置…".into();
        });
        run_interactive_stage(
            paths,
            task_id,
            &["config", "init", "--new", "--lang", "zh"],
            "app_registration",
            "请在浏览器中完成飞书应用创建；App ID/Secret 会由 CLI 自动保存，不需要回填。",
        )?;
    }

    update_auth_task(task_id, |status| {
        status.stage = "user_oauth".into();
        status.message = "正在打开飞书用户授权页面，请完成授权…".into();
        status.url = None;
    });
    run_interactive_stage(
        paths,
        task_id,
        &["auth", "login", "--recommend"],
        "user_oauth",
        "请在浏览器中确认飞书用户授权；AgentCabin 会自动等待结果。",
    )?;

    update_auth_task(task_id, |status| {
        status.stage = "verifying".into();
        status.message = "授权页面已返回，正在用 lark-cli --verify 检查身份与权限…".into();
        status.url = None;
    });
    let identity = verify_lark_auth(paths)?;
    let connection = persist_feishu_connection(paths, &identity, alias, email)?;
    update_auth_task(task_id, |status| {
        status.status = "authenticated".into();
        status.stage = "verified".into();
        status.message = "飞书已连接，AgentCabin 应用级 CLI 已完成用户身份验证。".into();
        status.identity = Some(identity);
        status.connection = Some(connection);
        status.url = None;
    });
    Ok(())
}

fn spawn_output_reader<R>(pipe: R, sender: mpsc::Sender<String>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
}

fn run_interactive_stage(
    paths: &WorkPaths,
    task_id: &str,
    args: &[&str],
    stage: &str,
    message: &str,
) -> Result<(), String> {
    let mut command = Command::new(private_lark_cli_path(paths));
    configure_lark_command(&mut command, paths);
    let mut child = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 lark-cli 授权流程失败: {error}"))?;

    let (sender, receiver) = mpsc::channel::<String>();
    if let Some(pipe) = child.stdout.take() {
        spawn_output_reader(pipe, sender.clone());
    }
    if let Some(pipe) = child.stderr.take() {
        spawn_output_reader(pipe, sender.clone());
    }
    drop(sender);

    let started = Instant::now();
    let mut output = String::new();
    let mut opened_url = false;
    loop {
        while let Ok(line) = receiver.try_recv() {
            if !line.trim().is_empty() {
                output.push_str(&line);
                output.push('\n');
            }
            if !opened_url {
                if let Some(url) = extract_allowed_auth_url(&line) {
                    opened_url = true;
                    open_auth_url(&url)?;
                    update_auth_task(task_id, |status| {
                        status.stage = stage.into();
                        status.message = message.into();
                        status.url = Some(url);
                    });
                }
            }
        }

        if let Some(exit) = child
            .try_wait()
            .map_err(|error| format!("等待 lark-cli 授权流程失败: {error}"))?
        {
            thread::sleep(Duration::from_millis(50));
            while let Ok(line) = receiver.try_recv() {
                output.push_str(&line);
                output.push('\n');
            }
            if exit.success() {
                return Ok(());
            }
            return Err(format!(
                "lark-cli {stage} 失败: {}",
                sanitize_cli_error(&output)
            ));
        }
        if started.elapsed() >= AUTH_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("飞书 {stage} 超过 10 分钟未完成，请重新发起授权"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn sanitize_cli_error(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        "未返回错误详情".into()
    } else {
        text.chars().take(1200).collect()
    }
}

fn open_auth_url(url: &str) -> Result<(), String> {
    if extract_allowed_auth_url(url).as_deref() != Some(url) {
        return Err("lark-cli 返回了不在飞书授权域名白名单内的 URL，已拒绝打开".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    let status = command
        .arg(url)
        .status()
        .map_err(|error| format!("打开飞书授权页面失败: {error}"))?;
    if !status.success() {
        return Err(format!("打开飞书授权页面失败: {status}"));
    }
    Ok(())
}

fn extract_allowed_auth_url(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let candidate = token.trim_matches(|character: char| {
            matches!(
                character,
                '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        });
        let candidate = candidate.trim_end_matches(['.', ':']);
        let Ok(url) = Url::parse(candidate) else {
            continue;
        };
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            continue;
        }
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if matches!(
            host.as_str(),
            "open.feishu.cn"
                | "accounts.feishu.cn"
                | "open.larksuite.com"
                | "accounts.larksuite.com"
        ) {
            return Some(url.to_string());
        }
    }
    None
}

pub fn verify_lark_auth(paths: &WorkPaths) -> Result<LarkIdentitySummary, String> {
    let output = run_lark_cli(
        paths,
        &["auth", "status", "--json", "--verify"],
        Duration::from_secs(120),
    )?;
    parse_lark_auth_status(&output)
}

fn parse_lark_auth_status(output: &str) -> Result<LarkIdentitySummary, String> {
    let value = json_from_cli_output(output)
        .ok_or_else(|| "lark-cli auth status 未返回可解析的 JSON".to_string())?;
    let verified = value
        .get("verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(verified);
    let identity = value
        .get("identity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !ok || identity != "user" {
        return Err("lark-cli 当前不是已验证的 user 身份，请完成浏览器授权".into());
    }
    Ok(LarkIdentitySummary {
        identity,
        display_name: find_string_field(
            &value,
            &["display_name", "displayName", "name", "user_name"],
        ),
        email: find_string_field(&value, &["email", "user_email"]),
        open_id: find_string_field(&value, &["open_id", "openId"]),
    })
}

fn json_from_cli_output(output: &str) -> Option<Value> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    for line in output.lines().rev() {
        let line = line.trim();
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            return Some(value);
        }
    }
    let start = output.find('{')?;
    let end = output.rfind('}')?;
    serde_json::from_str(&output[start..=end]).ok()
}

fn find_string_field(value: &Value, fields: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if fields.iter().any(|field| key.eq_ignore_ascii_case(field)) {
                    if let Some(value) = child.as_str().filter(|value| !value.trim().is_empty()) {
                        return Some(value.to_string());
                    }
                }
                if let Some(value) = find_string_field(child, fields) {
                    return Some(value);
                }
            }
            None
        }
        Value::Array(values) => values
            .iter()
            .find_map(|child| find_string_field(child, fields)),
        _ => None,
    }
}

pub(crate) fn persist_feishu_connection(
    paths: &WorkPaths,
    identity: &LarkIdentitySummary,
    alias: Option<&str>,
    email: Option<&str>,
) -> Result<AppConnection, String> {
    let previous = crate::work::apps::storage::get_connection(paths, "feishu")
        .map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    let account_email = email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| identity.email.clone());
    let account_id = previous
        .as_ref()
        .and_then(|connection| connection.accounts.first())
        .map(|account| account.account_id.clone())
        .unwrap_or_else(|| format!("acc_feishu_{}", uuid::Uuid::new_v4()));
    let display_name = alias
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| account_email.clone())
        .or_else(|| identity.display_name.clone())
        .unwrap_or_else(|| "飞书账号".into());
    let account = AppAccount {
        account_id,
        alias: alias
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        display_name: Some(display_name),
        email: account_email,
        status: ConnectionStatus::Connected,
        created_at: previous
            .as_ref()
            .and_then(|connection| connection.accounts.first())
            .map(|account| account.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        last_used_at: Some(now.clone()),
    };
    let connection = AppConnection {
        connection_id: "conn_feishu_cli".into(),
        app_id: "feishu".into(),
        provider: "native".into(),
        status: ConnectionStatus::Connected,
        accounts: vec![account],
        last_checked_at: Some(now.clone()),
        created_at: previous
            .as_ref()
            .map(|connection| connection.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    crate::work::apps::storage::save_connection(paths, &connection)
        .map_err(|error| error.to_string())?;
    connector_package_manager::ensure_builtin_feishu_package(paths)?;
    connector_package_manager::set_auth_status_with_paths(
        paths,
        "feishu",
        crate::work::connector_package::ConnectorAuthStatus::Authenticated,
    )?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cli_paths_are_application_scoped_and_do_not_use_system_global_path() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        assert!(private_lark_cli_path(&paths)
            .to_string_lossy()
            .contains("cli-connector-packages"));
        assert!(private_lark_cli_path(&paths).starts_with(paths.cli_connector_packages_dir()));
        assert!(!private_lark_cli_path(&paths).starts_with(paths.work_profile_dir()));
        assert!(paths
            .work_lark_cli_config_dir()
            .to_string_lossy()
            .contains("host-secrets"));
    }

    #[test]
    fn only_feishu_auth_domains_are_openable() {
        assert!(
            extract_allowed_auth_url("Open https://accounts.feishu.cn/authorize?code=abc")
                .is_some()
        );
        assert!(extract_allowed_auth_url("https://example.com/steal").is_none());
        assert!(extract_allowed_auth_url("http://accounts.feishu.cn/insecure").is_none());
    }

    #[test]
    fn parses_official_ok_user_status_without_returning_secrets() {
        let identity = parse_lark_auth_status(
            r#"{"ok":true,"identity":"user","user":{"name":"曾文琦","email":"user@example.com","open_id":"ou_x"}}"#,
        )
        .unwrap();
        assert_eq!(identity.identity, "user");
        assert_eq!(identity.email.as_deref(), Some("user@example.com"));
        assert_eq!(identity.open_id.as_deref(), Some("ou_x"));
    }

    #[test]
    fn parses_current_status_verified_user_shape() {
        let identity = parse_lark_auth_status(
            r#"{"appId":"cli_app","identity":"user","verified":true,"identities":{"user":{"available":true,"verified":true,"email":"user@example.com","open_id":"ou_x"}}}"#,
        )
        .unwrap();
        assert_eq!(identity.identity, "user");
        assert_eq!(identity.email.as_deref(), Some("user@example.com"));
        assert_eq!(identity.open_id.as_deref(), Some("ou_x"));
    }
}
