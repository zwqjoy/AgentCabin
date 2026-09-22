use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::storage;

const WORK_PROFILE_DIRS: &[&str] = &[
    "skills",
    "capabilities",
    "connectors",
    "pi-extensions",
    "artifact-tools",
    "policies",
];

#[derive(Debug, Clone)]
pub struct WorkPaths {
    data_root: PathBuf,
}

impl WorkPaths {
    pub fn new(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    pub fn app() -> Self {
        Self::new(storage::data_dir())
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn work_profile_dir(&self) -> PathBuf {
        self.data_root.join("profiles").join("work")
    }

    /// AgentCabin-managed Pi system packages shared by Code and Work.
    /// These are implementation dependencies, not user-installed extensions.
    pub fn pi_system_dir(&self) -> PathBuf {
        self.data_root.join("pi").join("system")
    }

    /// AgentCabin-owned Browser Worker files. The packaged app copies the small
    /// native-CDP worker bundle here without installing browser dependencies.
    pub fn browser_worker_dir(&self) -> PathBuf {
        self.pi_system_dir().join("browser-worker")
    }

    /// Legacy location retained for cleanup/backward-compatible path handling.
    pub fn browser_browsers_dir(&self) -> PathBuf {
        self.data_root
            .join("binaries")
            .join("playwright")
            .join("browsers")
    }

    /// Legacy location retained for cleanup/backward-compatible path handling.
    pub fn browser_npm_cache_dir(&self) -> PathBuf {
        self.data_root
            .join("binaries")
            .join("playwright")
            .join("npm-cache")
    }

    pub fn shared_skills_dir(&self) -> PathBuf {
        self.data_root.join("skills")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.data_root.join("workspaces")
    }

    /// Root directory for standalone (workspace-less) Work task runs.
    /// Each standalone conversation gets its own subdirectory keyed by run id.
    pub fn standalone_tasks_dir(&self) -> PathBuf {
        self.data_root.join("standalone_tasks")
    }

    pub fn global_library_dir(&self) -> PathBuf {
        self.data_root.join("library")
    }

    pub fn ensure_global_library_dir(&self) -> Result<PathBuf, String> {
        let dir = self.global_library_dir();
        storage::ensure_dir(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    pub fn workspace_library_dir(&self, workspace_id: &str) -> Result<PathBuf, String> {
        Ok(self.workspace_dir(workspace_id)?.join("library"))
    }

    pub fn ensure_workspace_library_dir(&self, workspace_id: &str) -> Result<PathBuf, String> {
        let dir = self.workspace_library_dir(workspace_id)?;
        storage::ensure_dir(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    pub fn work_extensions_dir(&self) -> PathBuf {
        self.work_profile_dir().join("extensions")
    }

    pub fn work_mcp_config_path(&self) -> PathBuf {
        self.work_profile_dir().join("mcp.json")
    }

    pub fn work_mcp_secrets_path(&self) -> PathBuf {
        self.work_profile_dir().join("mcp-secrets.json")
    }

    /// Root for Connector Packages. This is intentionally separate from the
    /// legacy `mcp.json` server map; packages carry their own manifest and
    /// runtime entrypoints.
    pub fn work_connector_packages_dir(&self) -> PathBuf {
        self.work_profile_dir().join("connectors")
    }

    /// Non-secret lifecycle state for installed Connector Packages.
    pub fn work_connector_states_path(&self) -> PathBuf {
        self.work_profile_dir().join("connector-states.json")
    }

    /// Host-only credentials collected from WorkBuddy token-schema.json.
    pub fn work_connector_secrets_path(&self) -> PathBuf {
        self.work_profile_dir()
            .join("connector-package-secrets.json")
    }

    /// Generated MCP projection for trusted, enabled Connector Packages.
    /// This is kept separate from the user-managed legacy `mcp.json` so
    /// Package lifecycle changes never rewrite custom MCP configuration.
    pub fn work_connector_mcp_config_path(&self) -> PathBuf {
        self.work_profile_dir().join("connector-package-mcp.json")
    }

    /// Generated non-secret CLI projection for trusted, enabled Connector
    /// Packages. CLI credentials are never written here.
    pub fn work_connector_cli_config_path(&self) -> PathBuf {
        self.work_profile_dir().join("connector-package-cli.json")
    }

    /// AgentCabin's application-level Node runtime root.
    ///
    /// This mirrors WorkBuddy's `binaries/node` layout. The runtime and
    /// Connector Package enablement are shared by all AgentCabin runtimes;
    /// each runtime still receives its own isolated projection.
    pub fn cli_runtime_root(&self) -> PathBuf {
        self.data_root.join("binaries").join("node")
    }

    /// AgentCabin-owned global installation root for CLI Connector Packages.
    ///
    /// "Global" here means global to AgentCabin, never the user's system npm
    /// prefix. Keeping this outside `profiles/work` prevents every future Work
    /// profile or runtime from installing a duplicate copy of the same CLI.
    pub fn cli_connector_packages_dir(&self) -> PathBuf {
        self.cli_runtime_root().join("cli-connector-packages")
    }

    /// Bin directory used by npm's application-scoped global prefix.
    pub fn cli_connector_bin_dir(&self) -> PathBuf {
        self.cli_connector_packages_dir().join("bin")
    }

    /// npm cache isolated to AgentCabin's managed CLI runtime.
    pub fn cli_connector_cache_dir(&self) -> PathBuf {
        self.cli_runtime_root().join("cli-connector-cache")
    }

    /// Legacy per-profile location used by earlier AgentCabin builds.
    pub fn legacy_work_cli_connector_packages_dir(&self) -> PathBuf {
        self.work_profile_dir().join("cli-connector-packages")
    }

    /// Compatibility accessor for callers that still use the old name. The
    /// returned path is now application-global, not profile-scoped.
    pub fn work_cli_connector_packages_dir(&self) -> PathBuf {
        self.cli_connector_packages_dir()
    }

    /// Legacy AgentCabin-owned lark-cli configuration root retained for
    /// migration/cleanup. New Work CLI invocations use the CLI's native user
    /// configuration location and Connector Package homePaths declarations.
    pub fn work_lark_cli_config_dir(&self) -> PathBuf {
        self.data_root.join("host-secrets").join("lark-cli")
    }

    /// AgentCabin-wide Browser configuration. Shared by Code and Work sessions.
    pub fn work_browser_config_path(&self) -> PathBuf {
        self.data_root.join("browser").join("config.json")
    }

    /// AgentCabin-wide Browser secret store. The file is written with mode 0600 on
    /// Unix and is never returned to the frontend.
    pub fn work_browser_secrets_path(&self) -> PathBuf {
        self.data_root.join("browser").join("secrets.json")
    }

    /// Work-owned Connected Apps configuration (non-sensitive connection metadata).
    pub fn work_apps_config_path(&self) -> PathBuf {
        self.work_profile_dir().join("apps.json")
    }

    /// Host-owned Connected Apps secret store (OAuth tokens / API keys).
    ///
    /// This deliberately lives outside the Work Pi profile.  The Work profile
    /// is mounted into the Pi sandbox so Pi can use its skills and MCP
    /// configuration; putting app credentials there would make a 0600 file
    /// readable by the same Pi process.  The Host bridge is the only code path
    /// that should read this location.
    pub fn work_apps_secrets_path(&self) -> PathBuf {
        self.data_root
            .join("host-secrets")
            .join("work-app-secrets.json")
    }

    /// Location used by the first Apps implementation.  Read once for
    /// migration, then remove it so a stale copy is not left in the Pi
    /// readable profile.
    pub fn legacy_work_apps_secrets_path(&self) -> PathBuf {
        self.work_profile_dir().join("app-secrets.json")
    }

    pub fn indexes_dir(&self) -> PathBuf {
        self.data_root.join("indexes")
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.data_root.join("tasks")
    }

    pub fn inbox_dir(&self) -> PathBuf {
        self.data_root.join("inbox")
    }

    pub fn ensure_layout(&self) -> Result<(), String> {
        storage::ensure_dir(&self.data_root).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.pi_system_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.workspaces_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.standalone_tasks_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.tasks_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.inbox_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.indexes_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.cli_runtime_root()).map_err(|e| e.to_string())?;
        self.migrate_legacy_cli_connector_packages()?;
        crate::storage::profile_bindings::migrate_legacy_skills_with_root(&self.data_root).ok();
        storage::ensure_dir(&self.cli_connector_packages_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.cli_connector_bin_dir()).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.cli_connector_cache_dir()).map_err(|e| e.to_string())?;

        let profile_dir = self.work_profile_dir();
        storage::ensure_dir(&profile_dir).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.work_extensions_dir()).map_err(|e| e.to_string())?;
        for child in WORK_PROFILE_DIRS {
            storage::ensure_dir(&profile_dir.join(child)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Move the old Work-profile CLI store into the application-level store
    /// once, preserving installed packages and avoiding a surprise reinstall.
    /// If both locations exist we leave the legacy copy untouched and always
    /// use the new location; deleting user data is deliberately out of scope.
    fn migrate_legacy_cli_connector_packages(&self) -> Result<(), String> {
        let legacy = self.legacy_work_cli_connector_packages_dir();
        let target = self.cli_connector_packages_dir();
        let legacy_metadata = match fs::symlink_metadata(&legacy) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        if target.exists() {
            return Ok(());
        }
        if legacy_metadata.file_type().is_symlink() || !legacy_metadata.is_dir() {
            return Err(format!(
                "legacy CLI Connector Package path is not a directory: {}",
                legacy.display()
            ));
        }
        fs::rename(&legacy, &target).map_err(|error| {
            format!(
                "failed to migrate legacy CLI Connector Package directory {} to {}: {error}",
                legacy.display(),
                target.display()
            )
        })
    }

    pub fn task_dir(&self, id: &str) -> Result<PathBuf, String> {
        validate_workspace_id(id)?; // Reuses alphanumeric + dash check
        Ok(self.tasks_dir().join(id))
    }

    pub fn task_manifest_path(&self, id: &str) -> Result<PathBuf, String> {
        Ok(self.task_dir(id)?.join("task.json"))
    }

    pub fn task_runs_dir(&self, id: &str) -> Result<PathBuf, String> {
        Ok(self.task_dir(id)?.join("runs"))
    }

    pub fn task_run_ledger_path(&self, task_id: &str, run_id: &str) -> Result<PathBuf, String> {
        validate_workspace_id(run_id)?;
        Ok(self
            .task_runs_dir(task_id)?
            .join(format!("{run_id}.ledger.jsonl")))
    }

    pub fn inbox_item_path(&self, id: &str) -> Result<PathBuf, String> {
        validate_workspace_id(id)?;
        Ok(self.inbox_dir().join(format!("{id}.json")))
    }

    pub fn workspace_dir(&self, id: &str) -> Result<PathBuf, String> {
        validate_workspace_id(id)?;
        Ok(self.workspaces_dir().join(id))
    }

    /// Per-run directory for a standalone (workspace-less) Work task. The
    /// agent's cwd and all of its deliverables live under this directory.
    pub fn standalone_task_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        validate_workspace_id(run_id)?; // Reuses alphanumeric + dash check
        Ok(self.standalone_tasks_dir().join(run_id))
    }

    /// Output directory that holds standalone task deliverables (artifacts).
    pub fn standalone_output_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        Ok(self.standalone_task_dir(run_id)?.join("output"))
    }

    /// Create the on-disk layout for a standalone task and return its cwd.
    pub fn ensure_standalone_task_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        let dir = self.standalone_task_dir(run_id)?;
        storage::ensure_dir(&dir).map_err(|e| e.to_string())?;
        storage::ensure_dir(&self.standalone_output_dir(run_id)?).map_err(|e| e.to_string())?;
        storage::ensure_dir(&dir.join("scratch")).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    pub fn manifest_path(&self, id: &str) -> Result<PathBuf, String> {
        Ok(self.workspace_dir(id)?.join("manifest.json"))
    }

    /// Resolve a relative path inside a standalone task directory.
    pub fn resolve_standalone_path(
        &self,
        run_id: &str,
        relative: &Path,
        _writable: bool,
    ) -> Result<PathBuf, String> {
        let task_dir = self.standalone_task_dir(run_id)?;
        let rel_str = relative.to_string_lossy();
        let trimmed = rel_str.trim().trim_start_matches("./");
        if trimmed == "." || trimmed.is_empty() {
            return Ok(task_dir);
        }
        let clean_path = Path::new(trimmed);
        validate_relative_path(clean_path)?;
        let candidate = task_dir.join(clean_path);
        ensure_within(&task_dir, &candidate)?;
        Ok(candidate)
    }

    /// Return the canonical user-selected root when a Workspace explicitly
    /// opts into writing `output/` directly into that folder.
    fn primary_output_root(&self, workspace_id: &str) -> Result<Option<PathBuf>, String> {
        let manifest_path = self.manifest_path(workspace_id)?;
        if !manifest_path.is_file() {
            return Ok(None);
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|error| format!("Cannot read Workspace manifest: {error}"))?;
        let manifest: serde_json::Value = serde_json::from_str(&content)
            .map_err(|error| format!("Invalid Workspace manifest: {error}"))?;
        let storage_mode = manifest
            .get("artifactStorageMode")
            .or_else(|| manifest.get("artifact_storage_mode"))
            .and_then(|value| value.as_str())
            .unwrap_or("managed");
        if storage_mode != "primary_work_root" {
            return Ok(None);
        }
        if manifest
            .get("rootKind")
            .or_else(|| manifest.get("root_kind"))
            .and_then(|value| value.as_str())
            != Some("local_folder")
        {
            return Err("直接保存成果要求 Workspace 关联本地文件夹".into());
        }
        let raw_root = manifest
            .get("primaryWorkRoot")
            .or_else(|| manifest.get("primary_work_root"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "直接保存成果缺少本地工作目录".to_string())?;
        let root = PathBuf::from(raw_root);
        let metadata = fs::metadata(&root)
            .map_err(|error| format!("本地工作目录不可用 ({}): {error}", root.display()))?;
        if !metadata.is_dir() {
            return Err(format!("本地工作目录不是目录: {}", root.display()));
        }
        fs::canonicalize(&root)
            .map(Some)
            .map_err(|error| format!("无法解析本地工作目录: {error}"))
    }

    /// Resolve a path inside a workspace and enforce its read/write boundary.
    /// This is used by future Work tools; keeping the check here makes it hard
    /// for a tool implementation to accidentally bypass the policy.
    pub fn resolve_workspace_path(
        &self,
        workspace_id: &str,
        relative: &Path,
        writable: bool,
    ) -> Result<PathBuf, String> {
        let workspace_dir = self.workspace_dir(workspace_id)?;
        validate_relative_path(relative)?;
        let first = relative.components().next();
        let area = match first {
            Some(Component::Normal(value)) => value.to_string_lossy(),
            _ => {
                return Err(
                    "Workspace path must start with input, scratch, output, or context".into(),
                )
            }
        };
        if !matches!(area.as_ref(), "input" | "scratch" | "output" | "context") {
            return Err("Workspace path is outside the allowed areas".into());
        }
        if writable && area == "input" {
            return Err("Workspace input is read-only".into());
        }

        if area == "output" {
            if let Some(primary_root) = self.primary_output_root(workspace_id)? {
                let candidate = primary_root.join(relative);
                ensure_within(&primary_root, &candidate)?;
                return Ok(candidate);
            }
        }

        let base = workspace_dir.join(area.as_ref());
        let candidate = workspace_dir.join(relative);
        ensure_within(&workspace_dir, &candidate)?;
        if base.exists() {
            ensure_within(&base, &candidate)?;
        }
        Ok(candidate)
    }

    /// Authoritative resolution of any requested path (relative or absolute) within
    /// the workspace boundary, standalone task boundary, or an authorized external root.
    pub fn resolve_and_confine_target(
        &self,
        workspace_id: &str,
        run_id: Option<&str>,
        raw_path: &str,
        writable: bool,
    ) -> Result<PathBuf, String> {
        let raw = raw_path.trim();
        if raw.is_empty() {
            return Err("Requested path cannot be empty".into());
        }

        let p = Path::new(raw);
        if p.is_absolute() {
            if !workspace_id.trim().is_empty() {
                // 1. Check if p is within the managed workspace directory itself (output/, scratch/, context/, input/)
                if let Ok(ws_dir) = self.workspace_dir(workspace_id) {
                    let p_canon = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                    let ws_canon = ws_dir.canonicalize().unwrap_or_else(|_| ws_dir.clone());
                    if p.starts_with(&ws_dir) || p_canon.starts_with(&ws_canon) {
                        ensure_within(&ws_dir, p)?;
                        return Ok(p.to_path_buf());
                    }
                }

                // 2. Check against primary work root (if LocalFolder) and registered access roots in workspace manifest
                if let Ok(manifest_path) = self.manifest_path(workspace_id) {
                    if manifest_path.exists() {
                        if let Ok(manifest_content) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(manifest_json) =
                                serde_json::from_str::<serde_json::Value>(&manifest_content)
                            {
                                if let Some(primary_root) = manifest_json
                                    .get("primaryWorkRoot")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.trim().is_empty())
                                {
                                    let primary_buf = PathBuf::from(primary_root);
                                    if p.starts_with(&primary_buf) {
                                        ensure_within(&primary_buf, p)?;
                                        return Ok(p.to_path_buf());
                                    }
                                }

                                if let Some(access_roots) =
                                    manifest_json.get("accessRoots").and_then(|v| v.as_array())
                                {
                                    for root in access_roots {
                                        if let Some(root_path) =
                                            root.get("path").and_then(|v| v.as_str())
                                        {
                                            let root_writable = root
                                                .get("writable")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(false);
                                            let root_buf = PathBuf::from(root_path);
                                            if p.starts_with(&root_buf) {
                                                if writable && !root_writable {
                                                    return Err(format!(
                                                        "Authorized external directory '{}' is read-only",
                                                        root_path
                                                    ));
                                                }
                                                ensure_within(&root_buf, p)?;
                                                return Ok(p.to_path_buf());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(rid) = run_id {
                if let Ok(task_dir) = self.standalone_task_dir(rid) {
                    let p_canon = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                    let task_canon = task_dir.canonicalize().unwrap_or_else(|_| task_dir.clone());
                    if p.starts_with(&task_dir) || p_canon.starts_with(&task_canon) {
                        ensure_within(&task_dir, p)?;
                        return Ok(p.to_path_buf());
                    }
                }
            }
            return Err(format!(
                "Absolute path '{}' is not within an authorized external directory",
                raw
            ));
        }

        if workspace_id.trim().is_empty() {
            if let Some(rid) = run_id {
                return self.resolve_standalone_path(rid, p, writable);
            }
            return Err("Workspace ID is required for workspace-relative path resolution".into());
        }

        // Check if this workspace is a LocalFolder workspace
        if let Ok(manifest_path) = self.manifest_path(workspace_id) {
            if manifest_path.exists() {
                if let Ok(manifest_content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(manifest_json) =
                        serde_json::from_str::<serde_json::Value>(&manifest_content)
                    {
                        let root_kind = manifest_json
                            .get("rootKind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("managed");
                        if root_kind == "local_folder" {
                            if let Some(primary_root) = manifest_json
                                .get("primaryWorkRoot")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.trim().is_empty())
                            {
                                let first = p.components().next();
                                let area = match first {
                                    Some(Component::Normal(value)) => value.to_string_lossy(),
                                    _ => "".into(),
                                };
                                // If targeting managed internal areas explicitly, route to managed dir
                                if matches!(
                                    area.as_ref(),
                                    "input" | "scratch" | "output" | "context"
                                ) {
                                    return self.resolve_workspace_path(workspace_id, p, writable);
                                }

                                validate_relative_path(p)?;
                                let primary_buf = PathBuf::from(primary_root);
                                let candidate = primary_buf.join(p);
                                ensure_within(&primary_buf, &candidate)?;
                                return Ok(candidate);
                            }
                        }
                    }
                }
            }
        }

        // Relative path inside managed workspace
        self.resolve_workspace_path(workspace_id, p, writable)
    }

    /// Resolve a command working directory. Unlike file paths, a command may
    /// intentionally run from the Workspace root (`.` / `./`). Keep this
    /// special case here instead of weakening `validate_relative_path`, which
    /// protects Work file and artifact paths from root-level access.
    pub fn resolve_command_cwd(
        &self,
        workspace_id: &str,
        run_id: Option<&str>,
        raw_path: &str,
        full_access: bool,
    ) -> Result<PathBuf, String> {
        let raw = raw_path.trim();
        if raw == "." || raw == "./" {
            let root = if workspace_id.trim().is_empty() {
                let run_id = run_id.ok_or("Run ID is required for standalone Work cwd")?;
                self.resolve_standalone_path(run_id, Path::new("."), false)?
            } else {
                self.resolve_workspace_command_root(workspace_id)?
            };
            return std::fs::canonicalize(&root)
                .map_err(|error| format!("Cannot resolve Work cwd '{}': {error}", raw));
        }

        // Agents may receive the canonical Workspace root from a previous
        // tool result and reuse it as cwd. Treat that absolute form exactly
        // like `.` while keeping it inside the same command root; generic
        // file-path resolution intentionally remains stricter.
        if !full_access && Path::new(raw).is_absolute() {
            let root = if workspace_id.trim().is_empty() {
                let run_id = run_id.ok_or("Run ID is required for standalone Work cwd")?;
                self.resolve_standalone_path(run_id, Path::new("."), false)?
            } else {
                self.resolve_workspace_command_root(workspace_id)?
            };
            let canonical_root = std::fs::canonicalize(&root).map_err(|error| {
                format!("Cannot resolve Work cwd root '{}': {error}", root.display())
            })?;
            let canonical_cwd = std::fs::canonicalize(raw)
                .map_err(|error| format!("Cannot resolve Work cwd '{}': {error}", raw))?;
            ensure_within(&canonical_root, &canonical_cwd)?;
            if !canonical_cwd.is_dir() {
                return Err(format!(
                    "Work cwd is not a directory: {}",
                    canonical_cwd.display()
                ));
            }
            return Ok(canonical_cwd);
        }

        if full_access {
            self.resolve_full_access_target(workspace_id, run_id, raw, false)
        } else {
            self.resolve_and_confine_target(workspace_id, run_id, raw, false)
        }
    }

    fn resolve_workspace_command_root(&self, workspace_id: &str) -> Result<PathBuf, String> {
        let workspace_dir = self.workspace_dir(workspace_id)?;
        let manifest_path = self.manifest_path(workspace_id)?;
        let root = if manifest_path.is_file() {
            let content = fs::read_to_string(&manifest_path)
                .map_err(|error| format!("Cannot read Workspace manifest: {error}"))?;
            let manifest: serde_json::Value = serde_json::from_str(&content)
                .map_err(|error| format!("Invalid Workspace manifest: {error}"))?;
            let root_kind = manifest
                .get("rootKind")
                .or_else(|| manifest.get("root_kind"))
                .and_then(|value| value.as_str())
                .unwrap_or("managed");
            if root_kind == "local_folder" {
                let primary = manifest
                    .get("primaryWorkRoot")
                    .or_else(|| manifest.get("primary_work_root"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        "Workspace primary working folder is not configured".to_string()
                    })?;
                let primary_path = PathBuf::from(primary);
                if !primary_path.is_absolute() {
                    return Err("Workspace primary working folder must be an absolute path".into());
                }
                primary_path
            } else {
                workspace_dir
            }
        } else {
            workspace_dir
        };

        if !root.is_dir() {
            return Err(format!("Work cwd is not a directory: {}", root.display()));
        }
        Ok(root)
    }

    /// Resolve a path for the explicit FullAccess Work mode. Relative paths
    /// remain relative to the current Work root, while absolute paths are
    /// allowed to address the host filesystem. This is intentionally separate
    /// from `resolve_and_confine_target` so ordinary Work modes keep their
    /// fail-closed path boundary.
    pub fn resolve_full_access_target(
        &self,
        workspace_id: &str,
        run_id: Option<&str>,
        raw_path: &str,
        _writable: bool,
    ) -> Result<PathBuf, String> {
        let raw = raw_path.trim();
        if raw.is_empty() {
            return Err("Requested path cannot be empty".into());
        }
        if raw.contains('\0') || raw.contains("://") {
            return Err("Requested path is invalid".into());
        }

        let base = if workspace_id.trim().is_empty() {
            let run_id = run_id.ok_or("Run ID is required for standalone FullAccess paths")?;
            self.standalone_task_dir(run_id)?
        } else {
            let mut resolved_base = self.workspace_dir(workspace_id)?;
            if let Ok(manifest_path) = self.manifest_path(workspace_id) {
                if manifest_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if json.get("rootKind").and_then(|v| v.as_str()) == Some("local_folder")
                            {
                                if let Some(primary) =
                                    json.get("primaryWorkRoot").and_then(|v| v.as_str())
                                {
                                    if !primary.trim().is_empty() {
                                        resolved_base = PathBuf::from(primary);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            resolved_base
        };
        let candidate = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            base.join(raw)
        };

        // Canonicalize the longest existing prefix, then append any missing
        // suffix. This keeps writes to new files possible without restoring a
        // traversal restriction that FullAccess explicitly opts out of.
        let mut existing = candidate.clone();
        let mut suffix = Vec::new();
        while !existing.exists() {
            let name = existing
                .file_name()
                .ok_or_else(|| format!("Cannot resolve FullAccess path '{}'", raw))?
                .to_os_string();
            suffix.push(name);
            if !existing.pop() {
                return Err(format!("Cannot resolve FullAccess path '{raw}'"));
            }
        }

        let mut resolved = std::fs::canonicalize(&existing)
            .map_err(|error| format!("Cannot resolve FullAccess path '{}': {error}", raw))?;
        for part in suffix.iter().rev() {
            resolved.push(part);
        }
        Ok(resolved)
    }

    /// Authoritative resolution of any requested path (relative or absolute) within
    /// the workspace boundary or an authorized external root.
    pub fn resolve_and_confine_path(
        &self,
        workspace_id: &str,
        raw_path: &str,
        writable: bool,
    ) -> Result<PathBuf, String> {
        self.resolve_and_confine_target(workspace_id, None, raw_path, writable)
    }

    /// Authoritative validator for external access roots.
    /// An external access root MUST:
    /// - be an absolute path (rejects relative paths, including input/, scratch/, output/, context/)
    /// - exist and resolve to a directory (rejects non-existent paths or individual files)
    /// - resolve strictly outside the current Workspace boundary (rejects workspace paths and symlinks resolving inside)
    ///
    /// Canonicalizes the path before performing the boundary comparison.
    /// Returns the canonical `PathBuf` on success.
    pub fn validate_external_access_root(
        &self,
        workspace_id: &str,
        raw_path: &str,
    ) -> Result<PathBuf, String> {
        let raw = raw_path.trim();
        if raw.is_empty() {
            return Err("External access root path cannot be empty".into());
        }

        let p = Path::new(raw);
        if !p.is_absolute() {
            return Err(format!(
                "Path '{}' is a relative path. External access roots must be absolute paths outside the Workspace. Workspace-internal paths (e.g. input/, scratch/) are already accessible without external approval.",
                raw
            ));
        }

        if workspace_id.trim().is_empty() {
            return Err(
                "External directory authorization requires a Workspace; standalone Work tasks cannot authorize external directories"
                    .into(),
            );
        }

        if !p.exists() {
            return Err(format!(
                "External access root path '{}' does not exist",
                raw
            ));
        }

        if !p.is_dir() {
            return Err(format!(
                "Path '{}' is a file. Only directories can be authorized as external access roots.",
                raw
            ));
        }

        let canonical_target = match std::fs::canonicalize(p) {
            Ok(canon) => canon,
            Err(err) => return Err(format!("Failed to resolve path '{}': {}", raw, err)),
        };

        if !canonical_target.is_dir() {
            return Err(format!(
                "Canonical path '{}' is not a directory",
                canonical_target.display()
            ));
        }

        let workspace_dir = self.workspace_dir(workspace_id)?;
        let canonical_ws = if workspace_dir.exists() {
            std::fs::canonicalize(&workspace_dir)
                .map_err(|e| format!("Failed to canonicalize workspace directory: {}", e))?
        } else {
            workspace_dir
        };

        if canonical_target == canonical_ws || canonical_target.starts_with(&canonical_ws) {
            return Err(format!(
                "Path '{}' resolves inside the current Workspace. Workspace-internal paths are already accessible and must not be requested as external access roots.",
                raw
            ));
        }

        // Also ensure it does not resolve inside primary work root if LocalFolder
        if let Ok(manifest_path) = self.manifest_path(workspace_id) {
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(primary) = json.get("primaryWorkRoot").and_then(|v| v.as_str())
                        {
                            if let Ok(primary_canon) = std::fs::canonicalize(primary) {
                                if canonical_target == primary_canon
                                    || canonical_target.starts_with(&primary_canon)
                                {
                                    return Err(format!(
                                        "Path '{}' resolves inside the Primary Working Folder. The working folder is already fully accessible and must not be added as an external access root.",
                                        raw
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        // The application data directory (~/.agentcabin) contains internal
        // state (workspaces, capabilities, tasks, ledgers). It is not a
        // user-owned data directory and must never be exposed as an external
        // access root — capability schemas are surfaced through tool
        // descriptions, not by letting the agent read manifest files.
        let canonical_data_root = if self.data_root.exists() {
            std::fs::canonicalize(&self.data_root).unwrap_or_else(|_| self.data_root.clone())
        } else {
            self.data_root.clone()
        };
        if canonical_target == canonical_data_root
            || canonical_target.starts_with(&canonical_data_root)
        {
            return Err(format!(
                "Path '{}' is inside the AgentCabin application data directory. This is an internal directory and cannot be authorized as an external access root.",
                raw
            ));
        }

        Ok(canonical_target)
    }
}

pub fn validate_workspace_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.len() > 100
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err("Invalid workspace id".into());
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("Workspace path must be relative".into());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir => {
                return Err("Workspace path cannot contain . or ..".into())
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("Workspace path must be relative".into())
            }
        }
    }
    Ok(())
}

pub fn ensure_within(base: &Path, candidate: &Path) -> Result<(), String> {
    let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let mut existing = candidate.to_path_buf();
    let mut suffix = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            suffix.push(name.to_os_string());
        }
        if let Some(parent) = existing.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            existing = parent.to_path_buf();
        } else {
            break;
        }
    }
    let mut candidate = std::fs::canonicalize(&existing).unwrap_or_else(|_| existing.clone());
    for component in suffix.iter().rev() {
        candidate.push(component);
    }
    if candidate.starts_with(&base) {
        Ok(())
    } else {
        Err("Workspace path escapes its allowed directory".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rejects_workspace_id_traversal() {
        for id in ["", ".", "..", "../escape", "a/b", "a\\b"] {
            assert!(validate_workspace_id(id).is_err(), "accepted {id:?}");
        }
    }

    #[test]
    fn allows_workspace_paths_inside_known_areas() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let workspace = paths.workspace_dir("demo").unwrap();
        std::fs::create_dir_all(workspace.join("scratch")).unwrap();

        let resolved = paths
            .resolve_workspace_path("demo", Path::new("scratch/notes.md"), true)
            .unwrap();
        assert!(resolved.ends_with("scratch/notes.md"));
    }

    #[test]
    fn cli_runtime_is_application_scoped_and_migrates_legacy_profile_store() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let legacy = paths.legacy_work_cli_connector_packages_dir();
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("marker.txt"), b"keep").unwrap();

        paths.ensure_layout().unwrap();

        assert!(paths
            .cli_connector_packages_dir()
            .starts_with(paths.data_root()));
        assert!(!paths
            .cli_connector_packages_dir()
            .starts_with(paths.work_profile_dir()));
        assert_eq!(
            std::fs::read(paths.cli_connector_packages_dir().join("marker.txt")).unwrap(),
            b"keep"
        );
        assert!(!legacy.exists());
        assert!(paths.cli_connector_bin_dir().is_dir());
        assert!(paths.cli_connector_cache_dir().is_dir());
    }

    #[test]
    fn work_layout_does_not_trigger_code_pi_migration() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let legacy_code_settings = paths.data_root().join("profiles/code/settings.json");
        std::fs::create_dir_all(legacy_code_settings.parent().unwrap()).unwrap();
        std::fs::write(&legacy_code_settings, "legacy").unwrap();

        paths.ensure_layout().unwrap();

        assert!(legacy_code_settings.is_file());
        assert!(!paths
            .data_root()
            .join("profiles/code/pi/settings.json")
            .exists());
    }

    #[test]
    fn rejects_workspace_path_traversal_and_read_only_input() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let workspace = paths.workspace_dir("demo").unwrap();
        std::fs::create_dir_all(workspace.join("input")).unwrap();

        assert!(paths
            .resolve_workspace_path("demo", Path::new("../escape.txt"), true)
            .is_err());
        assert!(paths
            .resolve_workspace_path("demo", Path::new("input/source.pdf"), true)
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_new_file_through_symlink_outside_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let workspace = paths.workspace_dir("demo").unwrap();
        std::fs::create_dir_all(workspace.join("scratch")).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, workspace.join("scratch/link")).unwrap();

        assert!(paths
            .resolve_workspace_path("demo", Path::new("scratch/link/new.txt"), true)
            .is_err());
    }

    #[test]
    fn validate_external_access_root_rejects_relative_and_workspace_paths() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let ws_dir = paths.workspace_dir("demo").unwrap();
        let input_dir = ws_dir.join("input");
        let scratch_dir = ws_dir.join("scratch");
        std::fs::create_dir_all(&input_dir).unwrap();
        std::fs::create_dir_all(&scratch_dir).unwrap();

        let input_file = input_dir.join("Q2-sales.xlsx");
        std::fs::write(&input_file, b"data").unwrap();

        // 1. Relative path rejected
        assert!(paths
            .validate_external_access_root("demo", "input/Q2-sales.xlsx")
            .is_err());
        assert!(paths
            .validate_external_access_root("demo", "scratch")
            .is_err());

        // 2. Absolute path to workspace input file rejected
        assert!(paths
            .validate_external_access_root("demo", input_file.to_str().unwrap())
            .is_err());

        // 3. Absolute path to workspace directory (input/ or scratch/) rejected
        assert!(paths
            .validate_external_access_root("demo", input_dir.to_str().unwrap())
            .is_err());
        assert!(paths
            .validate_external_access_root("demo", scratch_dir.to_str().unwrap())
            .is_err());
        assert!(paths
            .validate_external_access_root("demo", ws_dir.to_str().unwrap())
            .is_err());

        // 4. External file rejected
        let external_file = temp.path().join("external_file.txt");
        std::fs::write(&external_file, b"test").unwrap();
        assert!(paths
            .validate_external_access_root("demo", external_file.to_str().unwrap())
            .is_err());

        // 5. Valid external directory accepted
        let external_dir = temp.path().join("external_data");
        std::fs::create_dir_all(&external_dir).unwrap();
        let canon = paths
            .validate_external_access_root("demo", external_dir.to_str().unwrap())
            .unwrap();
        assert_eq!(canon, std::fs::canonicalize(&external_dir).unwrap());
    }

    #[test]
    fn validate_external_access_root_rejects_app_data_root() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let ws_dir = paths.workspace_dir("demo").unwrap();
        std::fs::create_dir_all(ws_dir.join("input")).unwrap();

        // data_root itself (~/.agentcabin) must be rejected
        assert!(paths
            .validate_external_access_root("demo", paths.data_root().to_str().unwrap())
            .is_err());

        // data_root/workspaces must be rejected
        assert!(paths
            .validate_external_access_root("demo", paths.workspaces_dir().to_str().unwrap())
            .is_err());

        // data_root/profiles/work (capability manifests) must be rejected
        assert!(paths
            .validate_external_access_root("demo", paths.work_profile_dir().to_str().unwrap())
            .is_err());

        // data_root/profiles/work/capabilities must be rejected
        let cap_dir = paths.work_profile_dir().join("capabilities");
        std::fs::create_dir_all(&cap_dir).unwrap();
        assert!(paths
            .validate_external_access_root("demo", cap_dir.to_str().unwrap())
            .is_err());
    }

    #[test]
    fn full_access_resolves_absolute_host_paths_without_access_root_registration() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let workspace = paths.workspace_dir("demo").unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let external = temp.path().join("outside");
        std::fs::create_dir_all(&external).unwrap();
        let target = external.join("new.txt");
        let canonical_target = std::fs::canonicalize(&external).unwrap().join("new.txt");

        let resolved = paths
            .resolve_full_access_target("demo", None, target.to_str().unwrap(), true)
            .unwrap();
        assert_eq!(resolved, canonical_target);

        let relative = paths
            .resolve_full_access_target("demo", None, "../../../outside/new.txt", true)
            .unwrap();
        assert_eq!(relative, canonical_target);
    }

    #[test]
    fn standalone_tasks_explain_that_external_access_requires_a_workspace() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let external_dir = temp.path().join("downloads");
        std::fs::create_dir_all(&external_dir).unwrap();

        let error = paths
            .validate_external_access_root("", external_dir.to_str().unwrap())
            .unwrap_err();
        assert_eq!(
            error,
            "External directory authorization requires a Workspace; standalone Work tasks cannot authorize external directories"
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_external_access_root_rejects_symlinks_into_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let ws_dir = paths.workspace_dir("demo").unwrap();
        let input_dir = ws_dir.join("input");
        std::fs::create_dir_all(&input_dir).unwrap();

        // External symlink pointing inside workspace input
        let external_link = temp.path().join("external_link_to_ws");
        symlink(&input_dir, &external_link).unwrap();

        assert!(paths
            .validate_external_access_root("demo", external_link.to_str().unwrap())
            .is_err());
    }

    #[test]
    fn allows_standalone_task_paths_and_output() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let run_id = "test-run-123";
        let task_dir = paths.ensure_standalone_task_dir(run_id).unwrap();

        let resolved_output = paths
            .resolve_and_confine_target("", Some(run_id), "output/story2.txt", true)
            .unwrap();
        assert_eq!(resolved_output, task_dir.join("output/story2.txt"));

        let resolved_root = paths
            .resolve_and_confine_target("", Some(run_id), ".", false)
            .unwrap();
        assert_eq!(resolved_root, task_dir);

        let resolved_scratch = paths
            .resolve_and_confine_target("", Some(run_id), "scratch/note.txt", true)
            .unwrap();
        assert_eq!(resolved_scratch, task_dir.join("scratch/note.txt"));

        assert!(paths
            .resolve_and_confine_target("", Some(run_id), "../escape.txt", true)
            .is_err());
    }

    #[test]
    fn command_cwd_allows_workspace_root_but_rejects_traversal() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Managed")
            .unwrap();
        let workspace_root = paths
            .resolve_command_cwd(&workspace.id, None, ".", false)
            .unwrap();
        assert_eq!(workspace_root, fs::canonicalize(&workspace.root).unwrap());
        assert_eq!(
            paths
                .resolve_command_cwd(&workspace.id, None, "./", false)
                .unwrap(),
            workspace_root
        );
        assert_eq!(
            paths
                .resolve_command_cwd(&workspace.id, None, workspace_root.to_str().unwrap(), false)
                .unwrap(),
            workspace_root
        );
        assert!(paths
            .resolve_command_cwd(&workspace.id, None, "../", false)
            .is_err());
        let external = temp.path().join("external");
        fs::create_dir_all(&external).unwrap();
        assert!(paths
            .resolve_command_cwd(&workspace.id, None, external.to_str().unwrap(), false)
            .is_err());
    }

    #[test]
    fn command_cwd_uses_local_folder_root_and_standalone_root() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let local_root = temp.path().join("LocalProject");
        fs::create_dir_all(&local_root).unwrap();
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create_from_folder(local_root.to_str().unwrap(), None)
            .unwrap();
        assert_eq!(
            paths
                .resolve_command_cwd(&workspace.id, None, ".", false)
                .unwrap(),
            fs::canonicalize(&local_root).unwrap()
        );
        assert_eq!(
            paths
                .resolve_command_cwd(&workspace.id, None, local_root.to_str().unwrap(), false)
                .unwrap(),
            fs::canonicalize(&local_root).unwrap()
        );

        let run_id = "standalone-cwd-test";
        let standalone_root = paths.ensure_standalone_task_dir(run_id).unwrap();
        assert_eq!(
            paths
                .resolve_command_cwd("", Some(run_id), ".", false)
                .unwrap(),
            fs::canonicalize(standalone_root).unwrap()
        );
    }

    #[test]
    fn primary_artifact_storage_routes_output_to_local_folder() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = crate::work::workspace::WorkspaceManager::new(paths.clone());
        let user_folder = temp.path().join("LocalProject");
        fs::create_dir_all(&user_folder).unwrap();
        let workspace = manager
            .create_from_folder(user_folder.to_str().unwrap(), None)
            .unwrap();

        manager
            .set_artifact_storage_mode(
                &workspace.id,
                crate::work::models::WorkArtifactStorageMode::PrimaryWorkRoot,
            )
            .unwrap();

        let output = paths
            .resolve_workspace_path(&workspace.id, Path::new("output/report.md"), true)
            .unwrap();
        assert_eq!(
            output,
            user_folder.canonicalize().unwrap().join("output/report.md")
        );

        let scratch = paths
            .resolve_workspace_path(&workspace.id, Path::new("scratch/note.md"), true)
            .unwrap();
        assert_eq!(
            scratch,
            paths
                .workspace_dir(&workspace.id)
                .unwrap()
                .join("scratch/note.md")
        );
    }

    #[test]
    fn local_folder_workspace_resolves_project_files_and_rejects_unauthorized() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let user_folder = temp.path().join("LocalProject");
        fs::create_dir_all(&user_folder).unwrap();
        let user_file = user_folder.join("src/lib.rs");
        fs::create_dir_all(user_folder.join("src")).unwrap();
        fs::write(&user_file, "pub fn hello() {}").unwrap();

        let ws_id = "local-ws-123";
        let ws_dir = paths.workspace_dir(ws_id).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();
        for area in ["input", "scratch", "output", "context"] {
            fs::create_dir_all(ws_dir.join(area)).unwrap();
        }

        let manifest = serde_json::json!({
            "id": ws_id,
            "name": "LocalProject",
            "root": ws_dir.to_str().unwrap(),
            "inputDir": ws_dir.join("input").to_str().unwrap(),
            "scratchDir": ws_dir.join("scratch").to_str().unwrap(),
            "outputDir": ws_dir.join("output").to_str().unwrap(),
            "contextDir": ws_dir.join("context").to_str().unwrap(),
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "artifactCount": 0,
            "archived": false,
            "accessRoots": [],
            "defaultPolicy": {
                "executionMode": "direct",
                "maxAutomatedSteps": 50,
                "allowExternalConnectors": false,
                "standingRules": []
            },
            "rootKind": "local_folder",
            "primaryWorkRoot": user_folder.to_str().unwrap()
        });
        fs::write(paths.manifest_path(ws_id).unwrap(), manifest.to_string()).unwrap();

        // 1. Relative path in project resolves to user directory
        let resolved_rel = paths
            .resolve_and_confine_target(ws_id, None, "src/lib.rs", true)
            .unwrap();
        assert_eq!(resolved_rel, user_folder.join("src/lib.rs"));

        // 2. Relative path in managed area resolves to managed state directory
        let resolved_scratch = paths
            .resolve_and_confine_target(ws_id, None, "scratch/notes.txt", true)
            .unwrap();
        assert_eq!(resolved_scratch, ws_dir.join("scratch/notes.txt"));

        // 3. Absolute path within user folder is accepted
        let resolved_abs = paths
            .resolve_and_confine_target(ws_id, None, user_file.to_str().unwrap(), true)
            .unwrap();
        assert_eq!(resolved_abs, user_file);

        // 4. Absolute path outside user folder (and not an access root) is rejected
        let outside_file = temp.path().join("unauthorized.txt");
        fs::write(&outside_file, "secret").unwrap();
        assert!(paths
            .resolve_and_confine_target(ws_id, None, outside_file.to_str().unwrap(), false)
            .is_err());
    }
}
