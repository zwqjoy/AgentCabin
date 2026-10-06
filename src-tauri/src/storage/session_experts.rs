//! Durable, per-conversation expert selection. `null` is an explicit exit, never a legacy fallback.
use super::agent_plugins;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionExpert {
    pub id: String,
    pub name: String,
    pub title: String,
    pub icon: String,
    pub is_team: bool,
}

fn selection_path(root: &Path, run_id: &str) -> Result<PathBuf, String> {
    let mut components = Path::new(run_id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("Invalid conversation id".into());
    }
    Ok(root.join("runs").join(run_id).join("expert.json"))
}

pub fn resolve_with_root(root: &Path, plugin_id: &str) -> Result<SessionExpert, String> {
    let (plugin, _, _) = agent_plugins::selected_expert_resources_with_root(root, plugin_id)?;
    // Resolve all mandatory role/preload files before accepting the selection.
    agent_plugins::selected_expert_prompt_with_root(root, plugin_id)?;
    agent_plugins::selected_expert_member_prompts_with_root(root, plugin_id)?;
    let title = plugin
        .display_name
        .clone()
        .unwrap_or_else(|| plugin.name.clone());
    Ok(SessionExpert {
        id: plugin.id,
        name: plugin.name,
        icon: title.chars().next().unwrap_or('专').to_string(),
        title,
        is_team: plugin.expert_kind.as_deref() == Some("expert-team"),
    })
}

pub fn get_with_root(root: &Path, run_id: &str) -> Result<Option<SessionExpert>, String> {
    let path = selection_path(root, run_id)?;
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).map_err(|e| format!("专家会话配置损坏: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Read compatibility only. New conversations always write expert.json before launch.
            let meta_path = root.join("runs").join(run_id).join("meta.json");
            let meta: serde_json::Value = std::fs::read_to_string(meta_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let prompt = meta
                .get("prompt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let Some((_, rest)) = prompt.split_once("[当前协作专家:") else {
                return Ok(None);
            };
            let Some((title, _)) = rest.split_once(']') else {
                return Ok(None);
            };
            let title = title.replace("(专家团队)", "").replace("(专家团)", "");
            let plugin = agent_plugins::list_agent_plugins_with_root(root)
                .into_iter()
                .find(|p| {
                    p.expert_kind.is_some()
                        && (p.id == title.trim()
                            || p.name == title.trim()
                            || p.display_name.as_deref() == Some(title.trim()))
                });
            plugin.map(|p| resolve_with_root(root, &p.id)).transpose()
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn save_with_root(
    root: &Path,
    run_id: &str,
    expert: Option<&SessionExpert>,
) -> Result<(), String> {
    let path = selection_path(root, run_id)?;
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("对话不存在".into());
    }
    let contents = serde_json::to_vec_pretty(&expert).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    crate::agent::runtime_providers::write_managed_file(&temporary, contents, "expert selection")?;
    if std::fs::symlink_metadata(&path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
    {
        let _ = std::fs::remove_file(&temporary);
        return Err("专家配置必须是普通文件".into());
    }
    std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
}

pub fn prompt_for_run(run_id: &str) -> Result<Option<String>, String> {
    let root = super::data_dir();
    let Some(expert) = get_with_root(&root, run_id)? else {
        return Ok(None);
    };
    agent_plugins::selected_expert_prompt_with_root(&root, &expert.id).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn install(root: &Path, id: &str) {
        let source = root.join(format!("source-{id}"));
        fs::create_dir_all(source.join(".codebuddy-plugin")).unwrap();
        fs::create_dir_all(source.join("agents")).unwrap();
        fs::create_dir_all(source.join("skills/write")).unwrap();
        fs::write(
            source.join(".codebuddy-plugin/plugin.json"),
            serde_json::json!({
                "name": id, "version": "1.0.0", "description": "writer",
                "expertType": "agent", "agentName": "lead", "agents": ["./agents/lead.md"],
                "skills": ["./skills/write"], "displayName": {"zh": id}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            source.join("agents/lead.md"),
            "---\nname: lead\nskills:\n  - writing\n---\nLEAD_ROLE_BODY",
        )
        .unwrap();
        fs::write(
            source.join("skills/write/SKILL.md"),
            "---\nname: writing\ndescription: writes documents\n---\nPRELOADED_SKILL_BODY",
        )
        .unwrap();
        agent_plugins::install_agent_plugin_with_root(root, source.to_str().unwrap()).unwrap();
        agent_plugins::set_agent_plugin_trust_with_root(root, id, true).unwrap();
    }

    #[test]
    fn selection_is_durable_and_explicit_exit_overrides_legacy_prompt() {
        let root = TempDir::new().unwrap();
        install(root.path(), "writer");
        fs::create_dir_all(root.path().join("runs/run-a")).unwrap();
        fs::write(
            root.path().join("runs/run-a/meta.json"),
            r#"{"prompt":"[当前协作专家: writer]\nhello"}"#,
        )
        .unwrap();
        assert_eq!(
            get_with_root(root.path(), "run-a").unwrap().unwrap().id,
            "writer"
        );
        let selected = resolve_with_root(root.path(), "writer").unwrap();
        save_with_root(root.path(), "run-a", Some(&selected)).unwrap();
        assert_eq!(get_with_root(root.path(), "run-a").unwrap(), Some(selected));
        save_with_root(root.path(), "run-a", None).unwrap();
        assert!(get_with_root(root.path(), "run-a").unwrap().is_none());
        assert!(get_with_root(root.path(), "../run-a").is_err());
    }

    #[tokio::test]
    async fn switching_and_exit_update_expert_skills_without_global_enablement() {
        let root = TempDir::new().unwrap();
        install(root.path(), "writer");
        install(root.path(), "reviewer");
        fs::create_dir_all(root.path().join("runs/run-a")).unwrap();
        for id in [Some("writer"), Some("reviewer"), None] {
            let selected = id.map(|id| resolve_with_root(root.path(), id).unwrap());
            save_with_root(root.path(), "run-a", selected.as_ref()).unwrap();
            let caps = crate::agent::capability_resolver::CapabilityResolver::resolve(
                root.path(),
                crate::work::models::AppMode::Code,
                crate::agent::capability_resolver::RuntimeProviderKind::Pi,
                root.path().to_str().unwrap(),
                "run-a",
            )
            .await
            .unwrap();
            let expert_owners = caps
                .enabled_skills
                .iter()
                .filter_map(|s| s.owner.as_ref())
                .filter(|o| o.kind == "expert")
                .map(|o| o.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            let expected = id.into_iter().collect::<std::collections::HashSet<_>>();
            assert_eq!(expert_owners, expected);
        }
    }

    #[test]
    fn selected_expert_mcp_is_host_resolved_without_global_enablement_and_revoked_on_exit() {
        let root = TempDir::new().unwrap();
        install(root.path(), "writer");
        let package =
            agent_plugins::agent_plugin_packages_dir_with_root(root.path()).join("writer");
        fs::write(package.join("mcp.json"), serde_json::json!({
            "$schema": agent_plugins::MCP_SCHEMA,
            "mcpServers": { "echo": { "type": "stdio", "command": "node", "env": { "TOKEN": "HOST_ONLY_SECRET" } } },
        }).to_string()).unwrap();
        fs::create_dir_all(root.path().join("runs/run-a")).unwrap();
        let selected = resolve_with_root(root.path(), "writer").unwrap();
        save_with_root(root.path(), "run-a", Some(&selected)).unwrap();
        assert!(agent_plugins::list_enabled_mcp_servers_with_root(root.path()).is_empty());
        let server_id = "agent-plugin--writer--mcp--echo";
        let config =
            agent_plugins::selected_expert_mcp_config_with_root(root.path(), "run-a", server_id)
                .unwrap()
                .unwrap();
        assert_eq!(config["command"], "node");
        assert_eq!(config["env"]["TOKEN"], "HOST_ONLY_SECRET");
        assert!(agent_plugins::selected_expert_mcp_config_with_root(
            root.path(),
            "run-b",
            server_id
        )
        .unwrap()
        .is_none());
        save_with_root(root.path(), "run-a", None).unwrap();
        assert!(agent_plugins::selected_expert_mcp_config_with_root(
            root.path(),
            "run-a",
            server_id
        )
        .unwrap()
        .is_none());
        save_with_root(root.path(), "run-a", Some(&selected)).unwrap();
        agent_plugins::set_agent_plugin_trust_with_root(root.path(), "writer", false).unwrap();
        assert!(agent_plugins::selected_expert_mcp_config_with_root(
            root.path(),
            "run-a",
            server_id
        )
        .is_err());
    }

    #[test]
    fn full_lead_instructions_and_declared_preload_are_loaded_and_validated() {
        let root = TempDir::new().unwrap();
        install(root.path(), "writer");
        let prompt =
            agent_plugins::selected_expert_prompt_with_root(root.path(), "writer").unwrap();
        assert!(prompt.contains("LEAD_ROLE_BODY"));
        assert!(prompt.contains("PRELOADED_SKILL_BODY"));
        assert!(!prompt.contains("name: lead"));
        let lead = agent_plugins::agent_plugin_packages_dir_with_root(root.path())
            .join("writer/agents/lead.md");
        fs::write(lead, "---\nname: lead\nskills: [missing]\n---\nrole").unwrap();
        assert!(resolve_with_root(root.path(), "writer")
            .unwrap_err()
            .contains("预加载技能未找到"));
    }

    #[test]
    fn corrupt_selection_fails_closed_and_other_conversations_do_not_inherit_it() {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("runs/run-a")).unwrap();
        fs::write(root.path().join("runs/run-a/expert.json"), "{broken").unwrap();
        assert!(get_with_root(root.path(), "run-a").is_err());
        assert!(get_with_root(root.path(), "run-b").unwrap().is_none());
    }
}
