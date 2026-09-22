pub mod assembler;
pub mod models;

#[cfg(test)]
pub mod tests;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub use assembler::WorkContextAssembler;
pub use models::*;

use crate::storage;

const CONTEXT_PLAN_FILE_NAME: &str = "work-context-plan.json";

/// Returns the durable per-run path for the WorkContextPlan file.
pub fn context_path(run_id: &str) -> PathBuf {
    storage::run_dir(run_id).join(CONTEXT_PLAN_FILE_NAME)
}

/// Load a WorkContextPlan for a run from disk if it exists.
pub fn load(run_id: &str) -> Result<Option<WorkContextPlan>, String> {
    load_from_path(&context_path(run_id))
}

/// Load a WorkContextPlan from a specific path.
pub fn load_from_path(path: &Path) -> Result<Option<WorkContextPlan>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("读取 Work Context Plan 失败: {error}"))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| format!("解析 Work Context Plan 失败: {error}"))
}

/// Atomically save a WorkContextPlan for a run.
pub fn save(run_id: &str, plan: &WorkContextPlan) -> Result<(), String> {
    let run_dir = storage::run_dir(run_id);
    storage::ensure_dir(&run_dir)
        .map_err(|error| format!("创建 Work Run 上下文目录失败: {error}"))?;
    save_to_path(&context_path(run_id), plan)
}

/// Atomically write the WorkContextPlan to the target path with crash-safety and strict permissions.
pub fn save_to_path(path: &Path, plan: &WorkContextPlan) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Work Context Plan 路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建 Work Context 目录失败: {error}"))?;

    let temporary = parent.join(format!(
        ".{CONTEXT_PLAN_FILE_NAME}.{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(plan)
        .map_err(|error| format!("序列化 Work Context Plan 失败: {error}"))?;

    // Safety check: ensure serialized JSON contains no credentials or secret leaks
    assert_no_secrets_in_json(&bytes)?;

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("创建 Work Context Plan 临时文件失败: {error}"))?;

    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("写入 Work Context Plan 失败: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置 Work Context Plan 权限失败: {error}"))?;
    }

    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("提交 Work Context Plan 失败: {error}"));
    }

    Ok(())
}

fn assert_no_secrets_in_json(bytes: &[u8]) -> Result<(), String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let lower = text.to_ascii_lowercase();
        // 1. Check for sensitive substrings, credential tokens, and PEM private keys
        if lower.contains("bearer ")
            || lower.contains("sk-ant-")
            || lower.contains("sk-proj-")
            || lower.contains("authorization:")
            || lower.contains("ghp_")
            || lower.contains("gho_")
            || lower.contains("xoxb-")
            || lower.contains("xoxp-")
            || lower.contains("glpat-")
            || lower.contains("eyjhbgcioi")
            || lower.contains("client_secret")
            || lower.contains("mcp_oauth_secret")
            || lower.contains("bridge_token")
            || lower.contains("-----begin private key-----")
            || lower.contains("-----begin rsa private key-----")
            || lower.contains("-----begin openssh private key-----")
            || lower.contains("-----begin ec private key-----")
            || lower.contains("-----begin dsa private key-----")
        {
            return Err("Security validation failed: WorkContextPlan must not contain credentials, tokens, or private keys".to_string());
        }

        // 2. Parse JSON tree and recursively check keys and values
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
            check_json_value_for_secrets(&val)?;
        }
    }
    Ok(())
}

fn check_json_value_for_secrets(value: &serde_json::Value) -> Result<(), String> {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let k_lower = k.to_ascii_lowercase();
                if is_sensitive_key_name(&k_lower) {
                    if let serde_json::Value::String(s) = v {
                        if !s.is_empty()
                            && s != crate::work::connectors::WORK_MCP_SECRET_PLACEHOLDER
                        {
                            return Err(format!(
                                "Security validation failed: sensitive key '{}' has non-empty value in WorkContextPlan",
                                k
                            ));
                        }
                    }
                }
                check_json_value_for_secrets(v)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                check_json_value_for_secrets(item)?;
            }
        }
        serde_json::Value::String(s) => {
            let s_lower = s.to_ascii_lowercase();
            if s_lower.starts_with("sk-")
                || s_lower.starts_with("ghp_")
                || s_lower.starts_with("gho_")
                || s_lower.starts_with("xoxb-")
                || s_lower.starts_with("glpat-")
                || s_lower.starts_with("bearer ")
            {
                return Err(
                    "Security validation failed: sensitive token detected in string value"
                        .to_string(),
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_sensitive_key_name(k: &str) -> bool {
    k.contains("password")
        || k.contains("api_key")
        || k.contains("apikey")
        || k.contains("secret")
        || k.contains("access_token")
        || k.contains("refresh_token")
        || k.contains("private_key")
        || k.contains("auth_token")
        || k == "token"
}
