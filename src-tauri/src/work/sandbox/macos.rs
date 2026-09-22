use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::work::sandbox::policy::{
    SandboxEnforcement, SandboxExecutionPolicy, SandboxMode, SandboxRequirement,
};
use crate::work::sandbox::{
    ConfinedCommand, ExecutionCommand, NativeSandboxProvider, SandboxError,
};

static MACOS_PROBE_CACHE: OnceLock<Result<SandboxEnforcement, SandboxError>> = OnceLock::new();

pub struct MacosSeatbeltProvider;

impl MacosSeatbeltProvider {
    pub fn new() -> Self {
        Self
    }

    /// Generate a Tiny Scheme Profile for macOS Seatbelt
    pub fn generate_profile(
        command: &ExecutionCommand,
        policy: &SandboxExecutionPolicy,
        resource_dir: Option<&Path>,
    ) -> String {
        let mut profile = String::from("(version 1)\n(deny default)\n\n");

        // Basic execution rights
        profile.push_str(";; Process execution and forking\n");
        profile.push_str("(allow process-fork)\n");
        profile.push_str(&format!(
            "(allow process-exec (literal \"{}\"))\n",
            escape_scheme_path(&command.program)
        ));
        if let Ok(canonical_bin) = command.program.canonicalize() {
            if canonical_bin != command.program {
                profile.push_str(&format!(
                    "(allow process-exec (literal \"{}\"))\n",
                    escape_scheme_path(&canonical_bin)
                ));
            }
        }
        profile.push_str("(allow process-exec (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/usr/bin\") (subpath \"/usr/sbin\") (subpath \"/usr/local\") (subpath \"/opt/homebrew\") (subpath \"/opt/local\") (subpath \"/Applications/Xcode.app\") (subpath \"/Library/Developer\") (subpath \"/Library/Frameworks\") (subpath \"/System/Library\") (subpath \"/System/Volumes/Data/Library/Developer\"))\n");
        for root in policy.read_roots.iter().chain(policy.write_roots.iter()) {
            if root.is_dir() && root != Path::new("/") {
                profile.push_str(&format!(
                    "(allow process-exec (subpath \"{}\"))\n",
                    escape_scheme_path(root)
                ));
            }
        }
        profile.push('\n');

        // Platform runtime baseline (read-only)
        //
        // NOTE (macOS 15+): in the cryptex/SSV era, dyld's cold start requires read access
        // that cannot be covered by ANY enumerated subpath allowlist — only a literal
        // (subpath "/") keeps dynamically-linked executables alive under (deny default).
        // An enumerated baseline (e.g. /usr/lib, /System, /Library) makes the sandboxed
        // process abort inside dyld init (SIGABRT with no stderr), which previously caused
        // probe() to conclude Seatbelt was unusable and fail-closed every capability.
        // Reads are therefore allowed system-wide; confinement is enforced via:
        //   (a) the write allowlist (writes stay limited to policy.write_roots),
        //   (b) the implicit network denial from (deny default),
        //   (c) the explicit credential deny-list below (deny always wins over allow).
        profile
            .push_str(";; Platform runtime read-only baseline (macOS 15+: unqualified read required for dyld)\n");
        profile.push_str("(allow sysctl-read)\n");
        profile.push_str("(allow file-read-metadata)\n");
        profile.push_str("(allow file-read* (subpath \"/\"))\n");
        profile.push_str(";; Sensitive credential deny-list (firmlink-real paths are also matched by Seatbelt)\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.ssh(/|$)\"))\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.gnupg(/|$)\"))\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.aws(/|$)\"))\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.config/gcloud(/|$)\"))\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.config/gh(/|$)\"))\n");
        profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/Library/Keychains(/|$)\"))\n\n");

        // Capability resource directory read-only
        if let Some(r_dir) = resource_dir {
            let canonical_rdir = r_dir.canonicalize().unwrap_or_else(|_| r_dir.to_path_buf());
            profile.push_str(";; Capability resource directory (Read-Only)\n");
            profile.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n\n",
                escape_scheme_path(&canonical_rdir)
            ));
        }

        // Declared Policy Read Roots
        profile.push_str(";; Declared Policy Read Roots\n");
        for root in &policy.read_roots {
            profile.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                escape_scheme_path(root)
            ));
        }
        profile.push('\n');

        // Declared Policy Write Roots
        profile.push_str(";; Declared Policy Write Roots & Essential system devices\n");
        profile.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/dtracehelper\") (literal \"/dev/tty\") (literal \"/dev/ptmx\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/stdin\") (subpath \"/dev/fd\"))\n");
        profile.push_str("(allow file-ioctl (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/dtracehelper\") (literal \"/dev/tty\") (literal \"/dev/ptmx\") (subpath \"/dev/fd\"))\n");
        profile.push_str("(allow ipc-posix-shm*)\n");
        for root in &policy.write_roots {
            profile.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                escape_scheme_path(root)
            ));
        }
        profile.push('\n');

        // Private Temp Root (only present in WorkspaceWrite mode)
        if let Some(temp_root) = &policy.temp_root {
            profile.push_str(";; Private Temp Root (WorkspaceWrite mode only)\n");
            profile.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                escape_scheme_path(temp_root)
            ));
            profile.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                escape_scheme_path(temp_root)
            ));
            profile.push('\n');
        }

        profile
    }

    /// Uncached functional probe: compile and apply a real minimal Seatbelt profile
    /// with /usr/bin/true to verify the sandbox actually confines in this environment.
    fn probe_uncached() -> Result<SandboxEnforcement, SandboxError> {
        #[cfg(target_os = "macos")]
        {
            if !Path::new("/usr/bin/sandbox-exec").exists() {
                return Err(SandboxError::Unavailable(
                    "/usr/bin/sandbox-exec not found on this macOS system".to_string(),
                ));
            }

            // Functional probe: compile and apply a real minimal (deny default) Seatbelt profile with /usr/bin/true
            let dummy_policy = SandboxExecutionPolicy {
                mode: SandboxMode::ReadOnly,
                requirement: SandboxRequirement::FullRequired,
                workspace_root: PathBuf::from("/usr"),
                read_roots: vec![PathBuf::from("/usr"), PathBuf::from("/bin")],
                write_roots: vec![],
                temp_root: None,
                work_run_id: "probe".to_string(),
                execution_id: "probe".to_string(),
            };
            let dummy_cmd = ExecutionCommand {
                program: PathBuf::from("/usr/bin/true"),
                args: vec![],
                current_dir: PathBuf::from("/usr/bin"),
                envs: vec![],
            };

            let profile_str = Self::generate_profile(&dummy_cmd, &dummy_policy, None);

            let probe_res = std::process::Command::new("/usr/bin/sandbox-exec")
                .arg("-p")
                .arg(&profile_str)
                .arg("/usr/bin/true")
                .output();

            match probe_res {
                Ok(output) if output.status.success() => Ok(SandboxEnforcement::Full),
                Ok(output) => {
                    let err = String::from_utf8_lossy(&output.stderr).to_string();
                    Err(SandboxError::Unavailable(format!(
                        "sandbox-exec cannot apply functional Seatbelt profile in current environment: {err}"
                    )))
                }
                Err(e) => Err(SandboxError::Unavailable(format!(
                    "Failed to execute sandbox-exec probe: {e}"
                ))),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(SandboxError::Unavailable(
                "macOS Seatbelt provider is only supported on macOS".to_string(),
            ))
        }
    }
}

impl Default for MacosSeatbeltProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSandboxProvider for MacosSeatbeltProvider {
    fn name(&self) -> &'static str {
        "macos-seatbelt"
    }

    fn probe(&self) -> Result<SandboxEnforcement, SandboxError> {
        // Cache success only. A failed probe must be re-executed on the next call so that
        // transient failures (or a fixed environment) can recover without an app restart;
        // previously the OnceLock cached failures forever, making UI retries meaningless.
        if let Some(cached) = MACOS_PROBE_CACHE.get() {
            return cached.clone();
        }

        let result = Self::probe_uncached();
        if result.is_ok() {
            let _ = MACOS_PROBE_CACHE.set(result.clone());
        }
        result
    }

    fn confine(
        &self,
        command: &ExecutionCommand,
        policy: &SandboxExecutionPolicy,
    ) -> Result<ConfinedCommand, SandboxError> {
        let enforcement = self.probe()?;
        if policy.requirement == SandboxRequirement::FullRequired
            && enforcement != SandboxEnforcement::Full
        {
            return Err(SandboxError::EnforcementInsufficient {
                required: policy.requirement,
                actual: enforcement,
            });
        }

        // Ensure the private temp directory exists if specified in policy.
        if let Some(temp_root) = &policy.temp_root {
            std::fs::create_dir_all(temp_root).map_err(|error| {
                SandboxError::Unavailable(format!(
                    "failed to create execution sandbox temp directory {}: {error}",
                    temp_root.display()
                ))
            })?;
        }

        let profile_str = Self::generate_profile(command, policy, None);

        let sandbox_exec = PathBuf::from("/usr/bin/sandbox-exec");
        let mut wrapped_args = Vec::new();
        wrapped_args.push(OsString::from("-p"));
        wrapped_args.push(OsString::from(profile_str));
        wrapped_args.push(command.program.as_os_str().to_os_string());
        for arg in &command.args {
            wrapped_args.push(arg.clone());
        }

        Ok(ConfinedCommand {
            program: sandbox_exec,
            args: wrapped_args,
            current_dir: command.current_dir.clone(),
            envs: command.envs.clone(),
            enforcement,
            denial_signatures: vec![
                "operation not permitted".to_string(),
                "sandbox: denied".to_string(),
                "deny file-write".to_string(),
                "deny file-read".to_string(),
                "permission denied".to_string(),
            ],
            runner_failure_signatures: vec![
                "sandbox-exec:".to_string(),
                "sandbox_apply:".to_string(),
                "failed to compile profile".to_string(),
            ],
        })
    }
}

fn escape_scheme_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
