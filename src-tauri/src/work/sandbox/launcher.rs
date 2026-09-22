//! OS-level confinement for processes owned by Work.
//!
//! Work's Pi permission extension remains the user-facing approval layer. This
//! launcher is the hard boundary below it: every Pi child process is started
//! through the platform sandbox, with the user's HOME kept for CLI
//! compatibility and only the declared Work roots mounted/allowed for writes.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use url::Url;

use super::{ConfinedCommand, ExecutionCommand, SandboxError};

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;
#[cfg(target_os = "macos")]
#[path = "launcher_macos.rs"]
mod macos_backend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// Work processes are network-isolated unless the Host explicitly opts in.
    /// Web/MCP access should use the authenticated Host bridge instead of
    /// handing a child process a raw network capability.
    Inherit,
    Deny,
    ProxyOnly,
    /// Explicitly allow the configured model provider to be reached by the
    /// runtime. Work Pi otherwise cannot call its LLM provider because the
    /// provider request originates inside the Pi child process.
    Provider,
}

pub(crate) const WORK_NETWORK_POLICY_ENV: &str = "AGENTCABIN_WORK_NETWORK_POLICY";
pub(crate) const WORK_PROVIDER_NETWORK_POLICY: &str = "provider";

#[derive(Debug, Clone)]
pub struct WorkSandboxPolicy {
    pub cwd: PathBuf,
    pub read_write: Vec<PathBuf>,
    pub read_only: Vec<PathBuf>,
    pub hidden: Vec<PathBuf>,
    pub network: NetworkPolicy,
    pub env: BTreeMap<String, String>,
}

pub struct WorkSandboxLauncher;

impl WorkSandboxLauncher {
    /// Prepare a command for an explicitly approved host execution.
    ///
    /// Generic commands retain their exact argv. Office commands get the
    /// additional runtime setup they need: a resolved executable, headless
    /// mode, and an isolated LibreOffice user profile.
    pub(crate) fn prepare_host_command(
        command: &ExecutionCommand,
        temp_dir: &Path,
    ) -> Result<ExecutionCommand, SandboxError> {
        let program = resolve_office_program(resolve_program(&command.program, &command.envs)?)?;
        if !is_office_program(&program) {
            return Ok(command.clone());
        }

        let mut prepared = command.clone();
        prepared.program = program;
        prepared.args = prepare_command_args(&prepared.program, &prepared.args, temp_dir)?;
        prepared.envs.retain(|(key, _)| key != "SAL_USE_VCLPLUGIN");
        prepared
            .envs
            .push(("SAL_USE_VCLPLUGIN".into(), "svp".into()));
        Ok(prepared)
    }

    /// Confine a process that is part of Work.
    ///
    /// `work_profile` is always writable because it owns the Work Pi profile.
    /// `writable_roots` and `read_only_roots` are the additional roots needed
    /// by the current Work operation. User HOME is never granted wholesale as
    /// a writable root; connector-specific config directories are passed in
    /// through `writable_roots` by the Host.
    pub fn confine_work_command(
        command: &ExecutionCommand,
        work_profile: &Path,
        writable_roots: &[PathBuf],
        read_only_roots: &[PathBuf],
    ) -> Result<ConfinedCommand, SandboxError> {
        Self::confine_work_command_with_read_only_cwd(
            command,
            work_profile,
            writable_roots,
            read_only_roots,
            false,
        )
    }

    /// Confine a Work process whose configured cwd is package material. The
    /// cwd is still used for process semantics, but it is granted read-only
    /// access instead of being added to Work's writable roots.
    pub fn confine_work_command_with_read_only_cwd(
        command: &ExecutionCommand,
        work_profile: &Path,
        writable_roots: &[PathBuf],
        read_only_roots: &[PathBuf],
        read_only_cwd: bool,
    ) -> Result<ConfinedCommand, SandboxError> {
        let profile = absolute_existing_directory(work_profile, "Work profile")?;
        let cwd = absolute_existing_directory(&command.current_dir, "Work cwd")?;
        let program = resolve_office_program(resolve_program(&command.program, &command.envs)?)?;

        let real_home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| SandboxError::PolicyUnsupported("Work requires HOME".into()))?;
        let real_home = absolute_existing_directory(&real_home, "User HOME")?;

        let run_dir = env_value(command, "AGENTCABIN_WORK_RUN_DIR");
        let managed_dir = env_value(command, "AGENTCABIN_MANAGED_STATE_DIR");
        let temp_dir = if !run_dir.trim().is_empty() {
            PathBuf::from(run_dir).join("tmp")
        } else if !managed_dir.trim().is_empty() {
            PathBuf::from(managed_dir).join("scratch")
        } else {
            profile.join("tmp")
        };
        fs::create_dir_all(&temp_dir).map_err(|error| {
            SandboxError::Unavailable(format!(
                "failed to create Work temp directory {}: {error}",
                temp_dir.display()
            ))
        })?;

        let mut read_write = vec![profile.clone(), temp_dir.clone()];
        if !read_only_cwd {
            read_write.push(cwd.clone());
        }
        read_write.extend(writable_roots.iter().cloned());

        let mut read_only = vec![program.parent().unwrap_or(&program).to_path_buf()];
        // A Connector CLI may be a small script wrapper (for example an npm
        // `.bin` entry whose script lives in `scripts/`) and then spawn the
        // package's real native binary from a sibling `bin/` directory.  The
        // wrapper itself is already an executable root, but allowing only its
        // parent would make that perfectly valid child process fail closed.
        // Grant the nearest package root as read-only/process-executable
        // material; this never grants write access and stays bounded by the
        // package.json marker rather than walking up to an arbitrary host
        // directory.
        if let Some(package_root) = package_root_for_program(&program) {
            read_only.push(package_root);
        }
        if let Some(toolchain_root) = toolchain_base_root(&program) {
            read_only.push(toolchain_root);
        }
        read_only.extend(path_entries(&env_value(command, "PATH")));
        if let Some(interpreter) = shebang_interpreter(&program, &command.envs) {
            read_only.push(interpreter.parent().unwrap_or(&interpreter).to_path_buf());
            if let Some(toolchain_root) = toolchain_base_root(&interpreter) {
                read_only.push(toolchain_root);
            }
        }
        if read_only_cwd {
            read_only.push(cwd.clone());
        }
        read_only.extend(read_only_roots.iter().cloned());

        let mut hidden = Vec::new();
        for sensitive in [
            ".ssh",
            ".gnupg",
            ".aws",
            ".config/gcloud",
            ".config/gh",
            "Library/Keychains",
            ".netrc",
            ".bash_history",
            ".zsh_history",
            ".python_history",
            ".node_repl_history",
        ] {
            let target = real_home.join(sensitive);
            if target.exists() {
                hidden.push(target);
            }
        }

        let mut env = BTreeMap::new();
        for (key, value) in &command.envs {
            env.insert(key.clone(), value.clone());
        }
        env.insert("HOME".into(), real_home.to_string_lossy().into_owned());
        env.insert("TMPDIR".into(), temp_dir.to_string_lossy().into_owned());
        env.insert("TMP".into(), temp_dir.to_string_lossy().into_owned());
        env.insert("TEMP".into(), temp_dir.to_string_lossy().into_owned());
        if is_office_program(&program) {
            // LibreOffice writes a user installation (lock files, registry, and
            // first-run state) before it touches the workbook. Keep that state
            // in the already-authorized per-execution temp root instead of
            // allowing it to probe the real user's profile.
            env.insert("SAL_USE_VCLPLUGIN".into(), "svp".into());
        }
        let npm_cache_dir = profile.join("npm-cache");
        fs::create_dir_all(&npm_cache_dir).map_err(|error| {
            SandboxError::Unavailable(format!(
                "failed to create Work npm cache directory {}: {error}",
                npm_cache_dir.display()
            ))
        })?;
        let npm_cache_str = npm_cache_dir.to_string_lossy().into_owned();
        env.entry("npm_config_cache".into())
            .or_insert_with(|| npm_cache_str.clone());
        env.entry("NPM_CONFIG_CACHE".into())
            .or_insert_with(|| npm_cache_str);

        let network = network_policy_for_command(command);
        let policy = WorkSandboxPolicy {
            cwd: cwd.clone(),
            read_write: normalize_roots(read_write, true)?,
            read_only: normalize_roots(read_only, false)?,
            hidden: normalize_roots(hidden, false)?,
            network,
            env,
        };

        let mut resolved = command.clone();
        resolved.program = program;
        resolved.current_dir = cwd;
        resolved.args = prepare_command_args(&resolved.program, &resolved.args, &temp_dir)?;
        resolved.envs = policy
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        platform_confine(&resolved, &policy)
    }
}

fn platform_confine(
    command: &ExecutionCommand,
    policy: &WorkSandboxPolicy,
) -> Result<ConfinedCommand, SandboxError> {
    #[cfg(target_os = "macos")]
    {
        return macos_backend::confine(command, policy);
    }

    #[cfg(target_os = "linux")]
    {
        return linux::confine(command, policy);
    }

    #[allow(unreachable_code)]
    Err(SandboxError::Unavailable(
        "Work OS sandbox is not implemented on this operating system (fail-closed)".into(),
    ))
}

fn absolute_existing_directory(path: &Path, label: &str) -> Result<PathBuf, SandboxError> {
    if !path.is_absolute() {
        return Err(SandboxError::PolicyUnsupported(format!(
            "{label} must be an absolute path: {}",
            path.display()
        )));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        SandboxError::PolicyUnsupported(format!(
            "{label} is not accessible: {} ({error})",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(SandboxError::PolicyUnsupported(format!(
            "{label} is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn resolve_program(program: &Path, envs: &[(String, String)]) -> Result<PathBuf, SandboxError> {
    if program.is_absolute() {
        return canonical_program(program);
    }

    let path = env_value_from_pairs(envs, "PATH")
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    for directory in std::env::split_paths(&path) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        let candidate = directory.join(program);
        if candidate.is_file() {
            return canonical_program(&candidate);
        }
    }
    Err(SandboxError::PolicyUnsupported(format!(
        "Work executable was not found: {}",
        program.display()
    )))
}

fn is_office_program(program: &Path) -> bool {
    matches!(
        program
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase())
            .as_deref(),
        Some("soffice")
            | Some("soffice.bin")
            | Some("libreoffice")
            | Some("ooffice")
            | Some("soffice.wrapper.sh")
            | Some("libreoffice.wrapper.sh")
    )
}

/// Homebrew and some Linux packages expose LibreOffice through a shell
/// wrapper. Resolve the real binary before generating the Seatbelt profile so
/// the wrapper cannot try to exec an application bundle outside the allowed
/// executable roots.
fn resolve_office_program(program: PathBuf) -> Result<PathBuf, SandboxError> {
    if !is_office_program(&program) {
        return Ok(program);
    }

    let source = fs::read_to_string(&program).unwrap_or_default();
    // Wrapper names are not consistent across platforms: Homebrew uses an
    // extension, while several Linux packages use a plain `libreoffice`
    // shell script. Use the shebang as the authoritative wrapper signal and
    // leave real ELF/Mach-O binaries untouched.
    if !source.starts_with("#!") {
        return Ok(program);
    }

    for token in source
        .split(|character: char| character.is_whitespace() || character == '\'' || character == '"')
    {
        if !token.starts_with('/') {
            continue;
        }
        let candidate = Path::new(token);
        if candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
                )
            })
            && candidate.is_file()
        {
            return canonical_program(candidate);
        }
    }
    Ok(program)
}

/// Prepare a known office launcher without widening the Work write boundary.
/// LibreOffice always runs headless in Work and receives a private profile.
fn prepare_command_args(
    program: &Path,
    args: &[OsString],
    temp_dir: &Path,
) -> Result<Vec<OsString>, SandboxError> {
    if !is_office_program(program) {
        return Ok(args.to_vec());
    }

    let office_profile = temp_dir.join("libreoffice-profile");
    fs::create_dir_all(&office_profile).map_err(|error| {
        SandboxError::Unavailable(format!(
            "failed to create LibreOffice private profile {}: {error}",
            office_profile.display()
        ))
    })?;
    let profile_url = Url::from_file_path(&office_profile).map_err(|_| {
        SandboxError::PolicyUnsupported(format!(
            "cannot convert LibreOffice private profile to a file URL: {}",
            office_profile.display()
        ))
    })?;

    let mut normalized = Vec::with_capacity(args.len() + 2);
    let mut has_headless = false;
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        let text = arg.to_string_lossy();
        if text == "-env:UserInstallation" || text == "--env:UserInstallation" {
            skip_next = true;
            continue;
        }
        if text.starts_with("-env:UserInstallation=") || text.starts_with("--env:UserInstallation=")
        {
            continue;
        }
        if text == "--headless" {
            has_headless = true;
        }
        normalized.push(arg.clone());
    }

    let mut prepared = vec![OsString::from(format!(
        "-env:UserInstallation={profile_url}"
    ))];
    if !has_headless {
        prepared.push(OsString::from("--headless"));
    }
    prepared.extend(normalized);
    Ok(prepared)
}

/// Resolve the executable named by a script shebang. This matters for Pi's
/// `#!/usr/bin/env node`: Homebrew's `/opt/homebrew/bin/node` is commonly a
/// symlink, while Seatbelt evaluates the real `/opt/homebrew/Cellar/...` target
/// for `process-exec`.
fn shebang_interpreter(program: &Path, envs: &[(String, String)]) -> Option<PathBuf> {
    let source = fs::read_to_string(program).ok()?;
    let first_line = source.lines().next()?.trim();
    let shebang = first_line.strip_prefix("#!")?.trim();
    let mut tokens = shebang.split_whitespace();
    let first = tokens.next()?;
    let interpreter = if Path::new(first).file_name().and_then(|name| name.to_str()) == Some("env")
    {
        let mut token = tokens.next()?;
        if token == "-S" {
            token = tokens.next()?;
        }
        token
    } else {
        first
    };
    let interpreter_path = Path::new(interpreter);
    if interpreter_path.is_absolute() {
        canonical_program(interpreter_path).ok()
    } else {
        resolve_program(interpreter_path, envs).ok()
    }
}

fn toolchain_base_root(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    if parent.file_name()?.to_str()? == "bin" {
        let base = parent.parent()?;
        if base != Path::new("/")
            && base != Path::new("/usr")
            && base != Path::new("/usr/local")
            && base != Path::new("/opt")
            && base.is_dir()
        {
            if let Some(home) = std::env::var_os("HOME") {
                if base == Path::new(&home) {
                    return None;
                }
            }
            return Some(base.to_path_buf());
        }
    }
    None
}

fn package_root_for_program(program: &Path) -> Option<PathBuf> {
    let mut directory = program.parent()?.to_path_buf();
    for _ in 0..12 {
        if directory.join("package.json").is_file() {
            return Some(directory);
        }
        let parent = directory.parent()?.to_path_buf();
        if parent == directory {
            break;
        }
        directory = parent;
    }
    None
}

fn canonical_program(path: &Path) -> Result<PathBuf, SandboxError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        SandboxError::PolicyUnsupported(format!(
            "Work executable is not accessible: {} ({error})",
            path.display()
        ))
    })?;
    if !canonical.is_file() {
        return Err(SandboxError::PolicyUnsupported(format!(
            "Work executable is not a file: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn normalize_roots(paths: Vec<PathBuf>, writable: bool) -> Result<Vec<PathBuf>, SandboxError> {
    let mut normalized = Vec::new();
    for path in paths {
        if !path.is_absolute() {
            return Err(SandboxError::PolicyUnsupported(format!(
                "Work sandbox root must be absolute: {}",
                path.display()
            )));
        }
        let path = if path.exists() {
            fs::canonicalize(&path).map_err(|error| {
                SandboxError::PolicyUnsupported(format!(
                    "cannot resolve Work sandbox root {}: {error}",
                    path.display()
                ))
            })?
        } else if writable {
            return Err(SandboxError::PolicyUnsupported(format!(
                "writable Work sandbox root does not exist: {}",
                path.display()
            )));
        } else {
            continue;
        };
        if path == Path::new("/") {
            return Err(SandboxError::PolicyUnsupported(
                "refusing to grant the Work sandbox access to filesystem root".into(),
            ));
        }
        if !normalized.contains(&path) {
            normalized.push(path);
        }
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn path_entries(path: &str) -> Vec<PathBuf> {
    std::env::split_paths(path)
        .filter(|path| path.is_absolute() && path.is_dir())
        .collect()
}

fn env_value(command: &ExecutionCommand, key: &str) -> String {
    env_value_from_pairs(&command.envs, key)
        .or_else(|| std::env::var(key).ok())
        .unwrap_or_default()
}

fn env_value_from_pairs(envs: &[(String, String)], key: &str) -> Option<String> {
    envs.iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
}

fn network_policy_for_command(command: &ExecutionCommand) -> NetworkPolicy {
    match env_value_from_pairs(&command.envs, WORK_NETWORK_POLICY_ENV).as_deref() {
        Some(WORK_PROVIDER_NETWORK_POLICY) => NetworkPolicy::Provider,
        Some("proxy") => NetworkPolicy::ProxyOnly,
        Some("inherit") => NetworkPolicy::Inherit,
        _ => NetworkPolicy::Deny,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::ffi::OsString;

    #[test]
    fn path_entries_only_keeps_existing_absolute_directories() {
        let entries = path_entries("/usr/bin:/definitely/missing:/bin");
        assert!(entries.contains(&PathBuf::from("/usr/bin")));
        assert!(!entries.contains(&PathBuf::from("/definitely/missing")));
    }

    #[test]
    fn relative_program_is_resolved_from_command_path() {
        let path =
            resolve_program(Path::new("sh"), &[("PATH".into(), "/usr/bin:/bin".into())]).unwrap();
        assert!(path.is_absolute());
        assert!(path.is_file());
    }

    #[test]
    fn provider_network_requires_an_explicit_launch_marker() {
        let command = ExecutionCommand {
            program: PathBuf::from("/usr/bin/true"),
            args: Vec::new(),
            current_dir: PathBuf::from("/tmp"),
            envs: vec![(
                WORK_NETWORK_POLICY_ENV.into(),
                WORK_PROVIDER_NETWORK_POLICY.into(),
            )],
        };
        assert_eq!(
            network_policy_for_command(&command),
            NetworkPolicy::Provider
        );
        let command_without_marker = ExecutionCommand {
            envs: Vec::new(),
            ..command
        };
        assert_eq!(
            network_policy_for_command(&command_without_marker),
            NetworkPolicy::Deny
        );
    }

    #[test]
    fn finds_nearest_package_root_for_script_wrappers() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("node_modules/@example/cli");
        let script = package.join("scripts/run.js");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(package.join("package.json"), "{}\n").unwrap();
        std::fs::write(&script, "#!/usr/bin/env node\n").unwrap();

        assert_eq!(
            package_root_for_program(&script.canonicalize().unwrap()),
            Some(package.canonicalize().unwrap())
        );
    }

    #[test]
    fn resolves_libreoffice_shell_wrapper_to_real_binary() {
        let root = tempfile::tempdir().unwrap();
        let wrapper = root.path().join("libreoffice");
        let binary = root.path().join("LibreOffice.app/Contents/MacOS/soffice");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "binary").unwrap();
        std::fs::write(
            &wrapper,
            format!("#!/bin/sh\n'{}' \"$@\"\n", binary.display()),
        )
        .unwrap();

        assert_eq!(
            resolve_office_program(wrapper).unwrap(),
            binary.canonicalize().unwrap()
        );
    }

    #[test]
    fn office_commands_are_headless_and_use_private_profile() {
        let root = tempfile::tempdir().unwrap();
        let temp_dir = root.path().join("tmp");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let args = vec![
            OsString::from("-env:UserInstallation=file:///Users/other/Library/LibreOffice"),
            OsString::from("--convert-to"),
            OsString::from("xlsx"),
        ];

        let prepared =
            prepare_command_args(Path::new("/usr/bin/soffice"), &args, &temp_dir).unwrap();
        let rendered: Vec<String> = prepared
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(rendered[0].starts_with("-env:UserInstallation=file://"));
        assert!(rendered[0].contains("libreoffice-profile"));
        assert_eq!(rendered[1], "--headless");
        assert!(!rendered.iter().any(|arg| arg.contains("/Users/other")));
        assert!(temp_dir.join("libreoffice-profile").is_dir());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolves_real_target_of_pi_shebang_interpreter() {
        let pi =
            Path::new("/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/dist/cli.js");
        if !pi.is_file() {
            return;
        }
        let interpreter = shebang_interpreter(
            pi,
            &[("PATH".into(), "/opt/homebrew/bin:/usr/bin:/bin".into())],
        )
        .expect("Pi shebang interpreter should resolve");
        let interpreter = interpreter.to_string_lossy();
        assert!(interpreter.ends_with("/bin/node"));
        assert!(
            interpreter.starts_with("/opt/homebrew/Cellar/node/")
                || interpreter.starts_with("/opt/homebrew/opt/node/")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_work_launcher_constrains_malicious_extension_and_child_process() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let workspace = root.path().join("workspace");
        let outside_dir = root.path().join("outside");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();
        let profile = std::fs::canonicalize(profile).unwrap();
        let workspace = std::fs::canonicalize(workspace).unwrap();
        let outside_dir = std::fs::canonicalize(outside_dir).unwrap();
        let escape_link = workspace.join("escape-link");
        symlink(&outside_dir, &escape_link).unwrap();
        let real_home = std::env::var("HOME").unwrap();

        let raw = ExecutionCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec![
                OsString::from("-c"),
                OsString::from(format!(
                    "test \"$HOME\" = \"$REAL_HOME\" && printf ok > allowed.txt && \
                     if /bin/ls \"$REAL_HOME/.ssh\" >/dev/null 2>&1; then exit 20; fi && \
                     if printf direct > \"$OUTSIDE_DIR/direct.txt\"; then exit 21; fi && \
                     if /bin/sh -c 'printf child > \"$OUTSIDE_DIR/child.txt\"'; then exit 22; fi && \
                     if /bin/sh -c 'printf symlink > \"$ESCAPE_LINK/escape.txt\"'; then exit 23; fi && \
                     test ! -e \"$OUTSIDE_DIR/direct.txt\" && \
                    test ! -e \"$OUTSIDE_DIR/child.txt\" && \
                     test ! -e \"$OUTSIDE_DIR/escape.txt\""
                )),
            ],
            current_dir: workspace.clone(),
            envs: vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("REAL_HOME".into(), real_home),
                ("OUTSIDE_DIR".into(), outside_dir.to_string_lossy().into_owned()),
                ("ESCAPE_LINK".into(), escape_link.to_string_lossy().into_owned()),
            ],
        };
        let confined = match WorkSandboxLauncher::confine_work_command(&raw, &profile, &[], &[]) {
            Ok(confined) => confined,
            Err(SandboxError::Unavailable(_)) => return,
            Err(error) => panic!("unexpected Work sandbox policy error: {error}"),
        };
        let mut command = Command::new(&confined.program);
        command
            .args(&confined.args)
            .current_dir(&confined.current_dir);
        for (key, value) in &confined.envs {
            command.env(key, value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "sandbox command failed: status={}, stdout={}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(workspace.join("allowed.txt").is_file());
        assert!(
            !outside_dir.join("direct.txt").exists(),
            "sandbox allowed a direct write outside Work roots"
        );
        assert!(
            !outside_dir.join("child.txt").exists(),
            "sandbox allowed a child process write outside Work roots"
        );
        assert!(
            !outside_dir.join("escape.txt").exists(),
            "sandbox allowed a symlink escape outside Work roots"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_application_cli_store_allows_script_wrapper_package_children() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let workspace = root.path().join("workspace");
        let package = root
            .path()
            .join("binaries/node/cli-connector-packages/node_modules/@example/cli");
        let script = package.join("scripts/run.js");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(package.join("package.json"), "{}\n").unwrap();
        std::fs::write(&script, "#!/usr/bin/env node\n").unwrap();

        let raw = ExecutionCommand {
            program: script,
            args: Vec::new(),
            current_dir: workspace,
            envs: vec![("PATH".into(), "/usr/bin:/bin".into())],
        };
        let confined = match WorkSandboxLauncher::confine_work_command(&raw, &profile, &[], &[]) {
            Ok(confined) => confined,
            Err(SandboxError::Unavailable(_)) => return,
            Err(error) => panic!("unexpected Work sandbox policy error: {error}"),
        };
        let profile_text = confined.args[1].to_string_lossy();
        let package = package.canonicalize().unwrap();
        assert!(profile_text.contains(&format!(
            "(allow process-exec (subpath \"{}\"))",
            package.to_string_lossy()
        )));
    }
}
