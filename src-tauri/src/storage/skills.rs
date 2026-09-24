use crate::models::{PluginOperationResult, StandaloneSkill};
use include_dir::{include_dir, Dir, DirEntry};
use std::path::Path;

static BUILTIN_VISUALIZATION_SKILL: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/resources/skills/agentcabin-visualization");

/// Materialize the packaged visualization skill into the shared skills catalog.
/// Existing user-owned skills with the same ID are preserved.
pub fn ensure_builtin_visualization_skill_with_root(root: &Path) -> Result<(), String> {
    let skills_root = crate::storage::profile_bindings::shared_skills_dir_with_root(root);
    crate::storage::profile_bindings::ensure_managed_directory(
        &skills_root,
        "shared skills directory",
    )?;

    let target = skills_root.join("agentcabin-visualization");
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Built-in visualization skill target is not a real directory: {}",
                target.display()
            ));
        }
        if !is_builtin_skill_dir(&target) {
            return Err(format!(
                "Preserving user-managed skill that uses the built-in ID: {}",
                target.display()
            ));
        }
    } else {
        crate::storage::profile_bindings::ensure_managed_directory(
            &target,
            "built-in visualization skill directory",
        )?;
    }

    copy_embedded_skill_files(&BUILTIN_VISUALIZATION_SKILL, &target)?;
    crate::storage::profile_bindings::write_managed_file(
        &target.join(".origin"),
        "builtin\n",
        "built-in skill origin",
    )?;
    Ok(())
}

pub fn ensure_builtin_visualization_skill() -> Result<(), String> {
    ensure_builtin_visualization_skill_with_root(&crate::storage::data_dir())
}

fn copy_embedded_skill_files(source: &Dir<'_>, target: &Path) -> Result<(), String> {
    for entry in source.entries() {
        match entry {
            DirEntry::Dir(directory) => {
                let name = directory
                    .path()
                    .file_name()
                    .ok_or_else(|| "Built-in skill contains an invalid directory".to_string())?;
                let child = target.join(name);
                crate::storage::profile_bindings::ensure_managed_directory(
                    &child,
                    "built-in skill resource directory",
                )?;
                copy_embedded_skill_files(directory, &child)?;
            }
            DirEntry::File(file) => {
                if file.path().file_name().and_then(|name| name.to_str()) == Some(".DS_Store") {
                    continue;
                }
                let name = file
                    .path()
                    .file_name()
                    .ok_or_else(|| "Built-in skill contains an invalid file".to_string())?;
                crate::storage::profile_bindings::write_managed_file(
                    &target.join(name),
                    file.contents(),
                    "built-in skill resource",
                )?;
            }
        }
    }
    Ok(())
}

pub fn list_skills(_cwd: Option<&str>) -> Vec<StandaloneSkill> {
    list_skills_with_root(&crate::storage::data_dir())
}

pub fn list_skills_with_root(root: &Path) -> Vec<StandaloneSkill> {
    let _ = crate::storage::profile_bindings::migrate_legacy_skills_with_root(root);
    let mut skills = Vec::new();
    let skills_root = root.join("skills");
    if crate::storage::profile_bindings::is_real_directory_without_symlink(&skills_root) {
        scan_skills_dir(root, &skills_root, "user", &mut skills);
    }

    skills
}

fn scan_skills_dir(root: &Path, dir: &Path, scope: &str, skills: &mut Vec<StandaloneSkill>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let skill_md = path.join("SKILL.md");
            let skill_md_is_regular = std::fs::symlink_metadata(&skill_md)
                .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
                .unwrap_or(false);
            if skill_md_is_regular {
                if let Some(skill) = parse_skill_file(root, &skill_md, scope) {
                    if !skills.iter().any(|s| s.path == skill.path) {
                        skills.push(skill);
                    }
                }
            }
        } else if meta.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            if let Some(skill) = parse_skill_file(root, &path, scope) {
                if !skills.iter().any(|s| s.path == skill.path) {
                    skills.push(skill);
                }
            }
        }
    }
}

fn parse_skill_file(root: &Path, path: &Path, scope: &str) -> Option<StandaloneSkill> {
    let content = std::fs::read_to_string(path).ok()?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skill");
    let directory_slug = if stem.to_lowercase() == "skill" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("skill")
            .to_string()
    } else {
        stem.to_string()
    };

    let (_front_name, description) = parse_frontmatter(&content);
    let id = directory_slug;

    let enabled = crate::storage::profile_bindings::is_skill_enabled_with_root(root, &id);
    let is_builtin = path
        .parent()
        .map(|parent| parent.join(".origin"))
        .and_then(|origin| std::fs::read_to_string(origin).ok())
        .is_some_and(|origin| origin.trim() == "builtin");

    Some(StandaloneSkill {
        name: id,
        description,
        path: path.to_string_lossy().to_string(),
        scope: if is_builtin {
            "system".to_string()
        } else {
            scope.to_string()
        },
        agent: "universal".into(),
        source_kind: None,
        enabled,
        disabled_by: if !enabled {
            Some(crate::models::SkillDisabledBy::Name)
        } else {
            None
        },
        can_edit: !is_builtin,
        can_delete: !is_builtin,
        can_toggle: !is_builtin,
    })
}

fn parse_frontmatter(content: &str) -> (String, String) {
    if !content.starts_with("---") {
        return (String::new(), String::new());
    }
    let rest = &content[3..];
    let end_idx = match rest.find("\n---") {
        Some(i) => i,
        None => return (String::new(), String::new()),
    };
    let fm_text = &rest[..end_idx];
    let mut name = String::new();
    let mut description = String::new();
    for line in fm_text.lines() {
        let line = line.trim();
        if let Some(stripped) = line.strip_prefix("name:") {
            name = stripped
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        } else if let Some(stripped) = line.strip_prefix("description:") {
            description = stripped
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        }
    }
    (name, description)
}

pub fn create_skill(
    name: &str,
    description: &str,
    content: &str,
    scope: &str,
    _cwd: Option<&str>,
) -> Result<StandaloneSkill, String> {
    create_skill_with_root(
        &crate::storage::data_dir(),
        name,
        description,
        content,
        scope,
    )
}

pub fn create_skill_with_root(
    root: &Path,
    name: &str,
    description: &str,
    content: &str,
    scope: &str,
) -> Result<StandaloneSkill, String> {
    let name = crate::storage::profile_bindings::validate_id(name, "Skill")?;

    let target_dir = root.join("skills");
    crate::storage::profile_bindings::ensure_managed_directory(
        &target_dir,
        "shared skills directory",
    )?;

    let skill_folder = target_dir.join(&name);
    if is_builtin_skill_dir(&skill_folder) {
        return Err(format!("Cannot replace built-in skill '{}'", name));
    }
    crate::storage::profile_bindings::ensure_managed_directory(
        &skill_folder,
        "shared skill directory",
    )?;

    let skill_file = skill_folder.join("SKILL.md");
    let full_content = if content.contains("---") {
        content.to_string()
    } else {
        format!(
            "---\nname: \"{}\"\ndescription: \"{}\"\n---\n\n{}",
            name, description, content
        )
    };

    crate::storage::profile_bindings::write_managed_file(
        &skill_file,
        full_content,
        "shared SKILL.md",
    )?;

    crate::storage::profile_bindings::set_skill_binding_with_root(root, &name, true, None)?;

    Ok(StandaloneSkill {
        name: name.clone(),
        description: description.to_string(),
        path: skill_file.to_string_lossy().to_string(),
        scope: scope.to_string(),
        agent: "universal".into(),
        source_kind: None,
        enabled: true,
        disabled_by: None,
        can_edit: true,
        can_delete: true,
        can_toggle: true,
    })
}

pub fn toggle_skill(skill_id: &str, enabled: bool) -> Result<PluginOperationResult, String> {
    let normalized_id = crate::storage::profile_bindings::validate_id(skill_id, "Skill")?;
    if is_builtin_skill_dir(
        &crate::storage::profile_bindings::shared_skills_dir().join(normalized_id),
    ) {
        return Err("Built-in skills cannot be toggled".to_string());
    }
    crate::storage::profile_bindings::set_skill_binding(
        skill_id,
        enabled,
        if !enabled {
            Some("user".to_string())
        } else {
            None
        },
    )?;
    Ok(PluginOperationResult {
        success: true,
        message: format!(
            "Skill '{}' {} globally",
            skill_id,
            if enabled { "enabled" } else { "disabled" }
        ),
    })
}

pub fn delete_skill(path: &str) -> Result<PluginOperationResult, String> {
    delete_skill_with_root(&crate::storage::data_dir(), path)
}

pub fn delete_skill_with_root(root: &Path, path: &str) -> Result<PluginOperationResult, String> {
    let skills_root = root.join("skills");
    let root_meta = std::fs::symlink_metadata(&skills_root)
        .map_err(|_| "Managed skills directory does not exist".to_string())?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err("Managed skills directory is not a regular directory".into());
    }

    let requested = Path::new(path);
    let target = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let relative = target
        .strip_prefix(&skills_root)
        .map_err(|_| "Skill path must stay inside the managed skills directory".to_string())?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Skill path must stay inside the managed skills directory".into());
    }

    let mut current = skills_root.clone();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err("Skill path contains an invalid component".into());
        };
        current.push(name);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "Skill file does not exist".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Skill path cannot traverse a symbolic link".into());
        }
    }

    let target_meta =
        std::fs::symlink_metadata(&target).map_err(|_| "Skill file does not exist".to_string())?;
    if !target_meta.is_file() {
        return Err("Skill path must point to a regular file".into());
    }

    let components = relative.components().collect::<Vec<_>>();
    let (skill_name, remove_target) = match components.as_slice() {
        [std::path::Component::Normal(file_name)] => {
            let name = Path::new(file_name)
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| "Invalid skill file name".to_string())?;
            (name.to_string(), target.clone())
        }
        [std::path::Component::Normal(skill_dir), std::path::Component::Normal(file_name)]
            if file_name.to_str() == Some("SKILL.md") =>
        {
            let name = skill_dir
                .to_str()
                .ok_or_else(|| "Invalid skill directory name".to_string())?;
            (name.to_string(), skills_root.join(name))
        }
        _ => {
            return Err(
                "Skill path must be <managed>/skills/<id>.md or <managed>/skills/<id>/SKILL.md"
                    .into(),
            )
        }
    };
    let skill_name = crate::storage::profile_bindings::validate_id(&skill_name, "Skill")?;

    let builtin_dir = if remove_target == target {
        target.parent().unwrap_or(&target)
    } else {
        &remove_target
    };
    if is_builtin_skill_dir(builtin_dir) {
        return Err("Built-in skills cannot be deleted".to_string());
    }

    if remove_target == target {
        std::fs::remove_file(&remove_target)
            .map_err(|error| format!("Failed to delete skill: {error}"))?;
    } else {
        std::fs::remove_dir_all(&remove_target)
            .map_err(|error| format!("Failed to delete skill: {error}"))?;
    }

    if !skill_name.is_empty() {
        crate::storage::profile_bindings::remove_skill_binding_with_root(root, &skill_name)?;
    }

    Ok(PluginOperationResult {
        success: true,
        message: "Deleted skill successfully".into(),
    })
}

fn is_builtin_skill_dir(path: &Path) -> bool {
    path.join(".origin").is_file()
        && std::fs::read_to_string(path.join(".origin"))
            .map(|origin| origin.trim() == "builtin")
            .unwrap_or(false)
}

// ══════════════════════════════════════════════════════════════════════════════
// Backward-compatible aliases for smooth transition from legacy pi_skills
// ══════════════════════════════════════════════════════════════════════════════
pub use create_skill as create_pi_skill;
pub use create_skill_with_root as create_pi_skill_with_root;
pub use delete_skill as delete_pi_skill;
pub use delete_skill_with_root as delete_pi_skill_with_root;
pub use list_skills as list_pi_skills;
pub use list_skills_with_root as list_pi_skills_with_root;
pub use toggle_skill as toggle_pi_skill;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skills_crud() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        let created = create_skill_with_root(
            root,
            "test-crud-skill",
            "A test skill",
            "# Skill Instructions",
            "user",
        );
        assert!(created.is_ok());
        let skill = created.unwrap();
        assert_eq!(skill.name, "test-crud-skill");

        let list = list_skills_with_root(root);
        assert!(list.iter().any(|s| s.name == "test-crud-skill"));

        let del_res = delete_skill_with_root(root, &skill.path);
        assert!(del_res.is_ok());

        let list_after = list_skills_with_root(root);
        assert!(!list_after.iter().any(|s| s.name == "test-crud-skill"));
    }

    #[test]
    fn connector_package_skills_stay_out_of_standalone_listing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("source-package");
        std::fs::create_dir_all(source.join("skills/example")).unwrap();
        std::fs::write(
            source.join("connector-meta.json"),
            r#"{
              "source": "example.connector",
              "name": "Example Connector",
              "version": "1.0.0",
              "type": "mcp",
              "skills": ["skills/example"]
            }"#,
        )
        .unwrap();
        std::fs::write(
            source.join("mcp.json"),
            r#"{"mcpServers":{"default":{"url":"https://example.test/mcp"}}}"#,
        )
        .unwrap();
        std::fs::write(
            source.join("skills/example/SKILL.md"),
            "---\nname: Example\ndescription: Package skill\n---\n\n# Example\n",
        )
        .unwrap();

        let paths = crate::work::paths::WorkPaths::new(root.to_path_buf());
        let installed =
            crate::work::connector_package_manager::install_with_paths(&paths, &source).unwrap();
        crate::work::connector_package_manager::set_trusted_with_paths(
            &paths,
            &installed.manifest.id,
            true,
        )
        .unwrap();
        crate::work::connector_package_manager::set_enabled_with_paths(
            &paths,
            &installed.manifest.id,
            true,
        )
        .unwrap();

        // Package-owned helper skills remain available to the Connector runtime,
        // but they are not standalone user-selectable Skills.
        assert_eq!(
            crate::work::connector_package_manager::package_skill_sources(&paths)
                .unwrap()
                .len(),
            1
        );
        assert!(!list_skills_with_root(root)
            .iter()
            .any(|skill| skill.name == "example"));
    }

    #[test]
    fn delete_rejects_paths_outside_managed_skills_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("keep.md");
        std::fs::write(&outside_file, "keep").unwrap();

        let result = delete_skill_with_root(root, &outside_file.to_string_lossy());

        assert!(result.is_err());
        assert!(outside_file.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn skill_listing_and_creation_reject_symlinked_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("managed");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("native.md"), "native").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        symlink(&outside, root.join("skills")).unwrap();

        assert!(list_skills_with_root(&root).is_empty());
        let result = create_skill_with_root(&root, "should-fail", "", "content", "user");
        assert!(result.is_err());
        assert!(outside.join("native.md").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn delete_rejects_symlinked_skill_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let skills_root = root.join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("SKILL.md");
        std::fs::write(&outside_file, "keep").unwrap();
        std::os::unix::fs::symlink(&outside, skills_root.join("escape")).unwrap();

        let result = delete_skill_with_root(
            root,
            &skills_root
                .join("escape")
                .join("SKILL.md")
                .to_string_lossy(),
        );

        assert!(result.is_err());
        assert!(outside_file.is_file());
    }
}
