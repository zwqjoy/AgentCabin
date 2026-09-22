use crate::models::PluginOperationResult;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const STORE_FILE_NAME: &str = "prompt-templates.json";

/// A prompt template owned by AgentCabin and shared by all three agents.
///
/// Built-ins are compiled into the application. User templates are persisted in
/// `~/.agentcabin/prompt-templates.json`; no Claude, Codex, or Pi directory is
/// used as the source of truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: String,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredPromptTemplate {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: String,
}

fn store_path() -> PathBuf {
    crate::storage::data_dir().join(STORE_FILE_NAME)
}

/// List built-ins first, followed by the user's global AgentCabin templates.
pub fn list_prompt_templates() -> Result<Vec<PromptTemplate>, String> {
    let user_templates = load_user_templates()?;
    let user_names: HashSet<&str> = user_templates
        .iter()
        .map(|template| template.name.as_str())
        .collect();
    // A user template with the same command name becomes the managed override
    // for that built-in name, so the picker never shows two indistinguishable
    // `/commands`.
    let mut templates: Vec<PromptTemplate> = builtin_prompt_templates()
        .into_iter()
        .filter(|template| !user_names.contains(template.name.as_str()))
        .collect();
    templates.extend(user_templates.into_iter().map(|template| PromptTemplate {
        id: template.id,
        name: template.name,
        description: template.description,
        content: template.content,
        builtin: false,
    }));
    Ok(templates)
}

pub fn create_prompt_template(
    name: &str,
    description: Option<&str>,
    content: &str,
) -> Result<PromptTemplate, String> {
    let name = normalize_name(name)?;
    if content.trim().is_empty() {
        return Err("Prompt template content cannot be empty".into());
    }

    let mut user_templates = load_user_templates()?;
    if user_templates.iter().any(|template| template.name == name) {
        return Err(format!("Prompt template /{} already exists", name));
    }

    let template = StoredPromptTemplate {
        id: user_id(&name),
        name: name.clone(),
        description: normalize_description(description),
        content: content.to_string(),
    };
    user_templates.push(template.clone());
    save_user_templates(&user_templates)?;

    Ok(PromptTemplate {
        id: template.id,
        name: template.name,
        description: template.description,
        content: template.content,
        builtin: false,
    })
}

pub fn update_prompt_template(
    id: &str,
    name: &str,
    description: Option<&str>,
    content: &str,
) -> Result<PluginOperationResult, String> {
    if id.starts_with("builtin:") {
        return Err("Built-in prompt templates are read-only. Create a user copy to edit.".into());
    }
    if content.trim().is_empty() {
        return Err("Prompt template content cannot be empty".into());
    }

    let name = normalize_name(name)?;
    let mut user_templates = load_user_templates()?;
    let Some(index) = user_templates.iter().position(|template| template.id == id) else {
        return Err("Prompt template does not exist".into());
    };

    if user_templates
        .iter()
        .enumerate()
        .any(|(other_index, template)| other_index != index && template.name == name)
    {
        return Err(format!("Prompt template /{} already exists", name));
    }

    user_templates[index] = StoredPromptTemplate {
        id: user_id(&name),
        name,
        description: normalize_description(description),
        content: content.to_string(),
    };
    save_user_templates(&user_templates)?;

    Ok(PluginOperationResult {
        success: true,
        message: "Updated prompt template".into(),
    })
}

pub fn delete_prompt_template(id: &str) -> Result<PluginOperationResult, String> {
    if id.starts_with("builtin:") {
        return Err(
            "Built-in prompt templates cannot be deleted. Create a user copy instead.".into(),
        );
    }

    let mut user_templates = load_user_templates()?;
    let before = user_templates.len();
    user_templates.retain(|template| template.id != id);
    if user_templates.len() == before {
        return Err("Prompt template does not exist".into());
    }
    save_user_templates(&user_templates)?;

    Ok(PluginOperationResult {
        success: true,
        message: "Deleted prompt template".into(),
    })
}

fn load_user_templates() -> Result<Vec<StoredPromptTemplate>, String> {
    let path = store_path();
    if !path.exists() {
        // One-time compatibility import for templates previously created through
        // the old Pi-only screen. The imported copy is immediately owned by
        // AgentCabin; project-scoped Pi files are intentionally not imported.
        let migrated = import_legacy_pi_user_templates();
        if !migrated.is_empty() {
            save_user_templates(&migrated)?;
        }
        return Ok(migrated);
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read prompt templates: {}", error))?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&raw)
        .map_err(|error| format!("Failed to parse prompt templates: {}", error))
}

fn save_user_templates(templates: &[StoredPromptTemplate]) -> Result<(), String> {
    let data_dir = crate::storage::data_dir();
    crate::storage::ensure_dir(&data_dir)
        .map_err(|error| format!("Failed to create AgentCabin data directory: {}", error))?;
    let json = serde_json::to_string_pretty(templates)
        .map_err(|error| format!("Failed to serialize prompt templates: {}", error))?;
    fs::write(store_path(), format!("{}\n", json))
        .map_err(|error| format!("Failed to save prompt templates: {}", error))
}

fn user_id(name: &str) -> String {
    format!("user:{}", name)
}

fn normalize_name(name: &str) -> Result<String, String> {
    let name = name.trim().trim_start_matches('/');
    if name.is_empty() {
        return Err("Prompt template name cannot be empty".into());
    }
    if name.len() > 64 {
        return Err("Prompt template name cannot exceed 64 characters".into());
    }
    if !name
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(
            "Prompt template names may only contain letters, numbers, '-', '_' or '.'".into(),
        );
    }
    Ok(name.to_string())
}

fn normalize_description(description: Option<&str>) -> Option<String> {
    description
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(ToOwned::to_owned)
}

fn builtin_prompt_templates() -> Vec<PromptTemplate> {
    let templates: &[(&str, &str, &str)] = &[
        (
            "code-review",
            "Review code changes for quality, bugs, and best practices",
            include_str!("builtin_prompts/code-review.md"),
        ),
        (
            "commit-message",
            "Generate a conventional commit message from staged changes",
            include_str!("builtin_prompts/commit-message.md"),
        ),
        (
            "pr-description",
            "Generate a structured pull request description",
            include_str!("builtin_prompts/pr-description.md"),
        ),
        (
            "bug-analysis",
            "Analyze a bug and identify root cause with fix suggestions",
            include_str!("builtin_prompts/bug-analysis.md"),
        ),
        (
            "refactor-plan",
            "Plan a refactoring with step-by-step migration strategy",
            include_str!("builtin_prompts/refactor-plan.md"),
        ),
        (
            "test-cases",
            "Generate comprehensive test cases for a function or module",
            include_str!("builtin_prompts/test-cases.md"),
        ),
        (
            "architecture-review",
            "Review system architecture for scalability and maintainability",
            include_str!("builtin_prompts/architecture-review.md"),
        ),
        (
            "performance-audit",
            "Audit performance bottlenecks and suggest optimizations",
            include_str!("builtin_prompts/performance-audit.md"),
        ),
    ];

    templates
        .iter()
        .map(|(name, description, content)| PromptTemplate {
            id: format!("builtin:{}", name),
            name: (*name).to_string(),
            description: Some((*description).to_string()),
            content: strip_frontmatter(content),
            builtin: true,
        })
        .collect()
}

fn import_legacy_pi_user_templates() -> Vec<StoredPromptTemplate> {
    let Some(home) = crate::storage::home_dir().map(PathBuf::from) else {
        return Vec::new();
    };
    let dir = home.join(".pi").join("agent").join("prompts");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let builtin_names: HashSet<String> = builtin_prompt_templates()
        .iter()
        .map(|template| template.name.clone())
        .collect();
    let mut imported = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("md")
        {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(name) = normalize_name(stem) else {
            continue;
        };
        if builtin_names.contains(name.as_str())
            || imported
                .iter()
                .any(|item: &StoredPromptTemplate| item.name == name)
        {
            continue;
        }
        imported.push(StoredPromptTemplate {
            id: user_id(&name),
            name,
            description: extract_description(&raw),
            content: strip_frontmatter(&raw),
        });
    }
    imported.sort_by(|left, right| left.name.cmp(&right.name));
    imported
}

fn extract_description(content: &str) -> Option<String> {
    let Some(rest) = content.strip_prefix("---") else {
        return first_content_line(content);
    };
    let Some(end) = rest.find("\n---") else {
        return first_content_line(content);
    };
    for line in rest[..end].lines() {
        if let Some(value) = line.trim().strip_prefix("description:") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    first_content_line(&rest[end + 4..])
}

fn first_content_line(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with('#') && !line.starts_with("---"))
            .then(|| line.to_string())
    })
}

fn strip_frontmatter(content: &str) -> String {
    let Some(rest) = content.strip_prefix("---") else {
        return content.to_string();
    };
    let Some(end) = rest.find("\n---") else {
        return content.to_string();
    };
    rest[end + 4..].trim_start_matches(['\n', '\r']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_agent_neutral_and_strip_frontmatter() {
        let templates = builtin_prompt_templates();
        assert!(!templates.is_empty());
        assert!(templates.iter().all(|template| template.builtin));
        assert!(templates
            .iter()
            .all(|template| !template.content.starts_with("---")));
    }

    #[test]
    fn names_are_safe_and_slashes_are_normalized() {
        assert_eq!(normalize_name(" /review ").unwrap(), "review");
        assert!(normalize_name("../review").is_err());
        assert!(normalize_name("review/name").is_err());
    }

    #[test]
    fn description_falls_back_to_first_content_line() {
        assert_eq!(
            extract_description("---\nother: value\n---\n\nReview this change."),
            Some("Review this change.".into())
        );
    }
}
