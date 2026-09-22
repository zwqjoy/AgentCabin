use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::work::models::{WorkAccessRoot, WorkExecutionManifest};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxEnforcement {
    None,
    Partial,
    Full,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRequirement {
    FullRequired,
    PartialAllowed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SandboxExecutionPolicy {
    pub mode: SandboxMode,
    pub requirement: SandboxRequirement,
    pub workspace_root: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub write_roots: Vec<PathBuf>,
    pub temp_root: Option<PathBuf>,
    pub work_run_id: String,
    pub execution_id: String,
}

pub struct SandboxPolicyResolver;

impl SandboxPolicyResolver {
    /// Resolve a fine-grained SandboxExecutionPolicy strictly driven by manifest areas, access roots, and runtime descriptors.
    /// `sandbox_root` is a Host-owned directory (e.g. data_root) that is NOT inside any capability-writable workspace area.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        workspace_dir: &Path,
        resource_dir: Option<&Path>,
        manifest: &WorkExecutionManifest,
        access_roots: &[WorkAccessRoot],
        runtime_read_roots: &[PathBuf],
        work_run_id: &str,
        execution_id: &str,
        sandbox_root: &Path,
    ) -> Result<SandboxExecutionPolicy, String> {
        crate::work::paths::validate_workspace_id(work_run_id)
            .map_err(|error| format!("Invalid WorkRun ID for sandbox policy: {error}"))?;
        crate::work::paths::validate_workspace_id(execution_id)
            .map_err(|error| format!("Invalid execution ID for sandbox policy: {error}"))?;

        let mut read_roots = Vec::new();
        let mut write_roots = Vec::new();

        // 1. Resource directory is always read-only
        if let Some(r_dir) = resource_dir {
            if r_dir.exists() {
                read_roots.push(canonicalize_or_original(r_dir));
            } else {
                read_roots.push(r_dir.to_path_buf());
            }
        }

        // 2. Runtime libraries and prefix read roots
        for rt_root in runtime_read_roots {
            if rt_root.exists() {
                read_roots.push(canonicalize_or_original(rt_root));
            } else {
                read_roots.push(rt_root.clone());
            }
        }

        // 3. Validate and map declared readable areas
        for area in &manifest.readable_areas {
            let dir = resolve_workspace_area(workspace_dir, area)?;
            read_roots.push(dir);
        }

        // 4. Validate and map declared writable areas (writable areas are also readable)
        for area in &manifest.writable_areas {
            let dir = resolve_workspace_area(workspace_dir, area)?;
            read_roots.push(dir.clone());
            write_roots.push(dir);
        }

        // 5. Map explicit WorkAccessRoots
        for root in access_roots {
            let path = PathBuf::from(&root.path);
            let canonical_root = if path.exists() {
                canonicalize_or_original(&path)
            } else {
                path
            };

            read_roots.push(canonical_root.clone());
            if root.writable && !manifest.writable_areas.is_empty() {
                write_roots.push(canonical_root);
            }
        }

        // 6. Determine Mode and the per-execution private temp root.
        // Work processes keep the real user HOME for CLI compatibility; the
        // OS sandbox still limits writes to the declared roots below.
        let (mode, temp_root) = if write_roots.is_empty() {
            // ReadOnly mode: strictly no persistent workspace mutation and no private temp write root.
            (SandboxMode::ReadOnly, None)
        } else {
            // WorkspaceWrite mode: isolated per-execution temp under Host-owned sandbox_root
            // (NOT inside the workspace, preventing capability symlink-poisoning attacks).
            let sandbox_tmp_anchor = canonicalize_or_original(sandbox_root).join("sandbox-tmp");

            // Symlink guard: refuse to create dirs through a pre-planted symlink.
            if sandbox_tmp_anchor.is_symlink() {
                return Err(format!(
                    "Sandbox temp anchor {:?} is a symlink; Host refuses to create temp dirs through symlinks",
                    sandbox_tmp_anchor
                ));
            }

            let exec_sandbox_base = sandbox_tmp_anchor.join(work_run_id).join(execution_id);
            let temp_dir = exec_sandbox_base.join("tmp");
            std::fs::create_dir_all(&temp_dir).map_err(|error| {
                format!(
                    "Failed to create per-execution sandbox temp directory {}: {error}",
                    temp_dir.display()
                )
            })?;
            let temp_dir = canonicalize_or_original(&temp_dir);

            read_roots.push(temp_dir.clone());
            write_roots.push(temp_dir.clone());

            (SandboxMode::WorkspaceWrite, Some(temp_dir))
        };

        // Deduplicate paths
        read_roots.sort();
        read_roots.dedup();
        write_roots.sort();
        write_roots.dedup();

        let requirement = SandboxRequirement::FullRequired;

        Ok(SandboxExecutionPolicy {
            mode,
            requirement,
            workspace_root: canonicalize_or_original(workspace_dir),
            read_roots,
            write_roots,
            temp_root,
            work_run_id: work_run_id.to_string(),
            execution_id: execution_id.to_string(),
        })
    }
}

fn resolve_workspace_area(workspace_dir: &Path, area: &str) -> Result<PathBuf, String> {
    let normalized = area.trim().to_lowercase();
    let area_dir = match normalized.as_str() {
        "input" => workspace_dir.join("input"),
        "context" => workspace_dir.join("context"),
        "scratch" => workspace_dir.join("scratch"),
        "output" => workspace_dir.join("output"),
        _ => {
            return Err(format!(
                "Unknown workspace area in execution manifest: '{area}'"
            ))
        }
    };

    if area_dir.exists() {
        Ok(canonicalize_or_original(&area_dir))
    } else {
        Ok(area_dir)
    }
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
