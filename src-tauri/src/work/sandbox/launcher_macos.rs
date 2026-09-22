use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use super::{NetworkPolicy, WorkSandboxPolicy};
use crate::work::sandbox::{ConfinedCommand, ExecutionCommand, SandboxEnforcement, SandboxError};

static PROBE: OnceLock<Result<(), String>> = OnceLock::new();

pub(super) fn confine(
    command: &ExecutionCommand,
    policy: &WorkSandboxPolicy,
) -> Result<ConfinedCommand, SandboxError> {
    probe()?;
    let profile = generate_profile(command, policy);
    let mut args = vec![OsString::from("-p"), OsString::from(profile)];
    args.push(command.program.as_os_str().to_os_string());
    args.extend(command.args.iter().cloned());
    Ok(ConfinedCommand {
        program: PathBuf::from("/usr/bin/sandbox-exec"),
        args,
        current_dir: command.current_dir.clone(),
        envs: command.envs.clone(),
        enforcement: SandboxEnforcement::Full,
        denial_signatures: vec![
            "operation not permitted".into(),
            "sandbox: denied".into(),
            "deny file-read".into(),
            "deny file-write".into(),
            "permission denied".into(),
        ],
        runner_failure_signatures: vec![
            "sandbox-exec:".into(),
            "sandbox_apply:".into(),
            "failed to compile profile".into(),
        ],
    })
}

fn probe() -> Result<(), SandboxError> {
    if let Some(result) = PROBE.get() {
        return result.clone().map_err(SandboxError::Unavailable);
    }
    let result = if !Path::new("/usr/bin/sandbox-exec").is_file() {
        Err("/usr/bin/sandbox-exec not found".into())
    } else {
        let profile = concat!(
            "(version 1) ",
            "(deny default) ",
            "(allow process-fork) ",
            "(allow process-exec (literal \"/usr/bin/true\")) ",
            "(allow file-read* (subpath \"/\")) ",
            "(allow sysctl-read)"
        );
        match Command::new("/usr/bin/sandbox-exec")
            .args(["-p", profile, "/usr/bin/true"])
            .output()
        {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(format!(
                "functional Seatbelt probe failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => Err(format!("failed to run Seatbelt probe: {error}")),
        }
    };
    if result.is_ok() {
        let _ = PROBE.set(result.clone());
    }
    result.map_err(SandboxError::Unavailable)
}

pub(super) fn generate_profile(command: &ExecutionCommand, policy: &WorkSandboxPolicy) -> String {
    let mut profile = String::from("(version 1)\n(deny default)\n\n");
    profile.push_str(";; Pi and its descendants stay inside this Seatbelt\n");
    profile.push_str("(allow process-fork)\n");
    profile.push_str("(allow signal (target same-sandbox))\n");
    profile.push_str("(allow sysctl-read)\n");
    profile.push_str("(allow mach-lookup)\n");

    // macOS 15's dyld needs broad read visibility during cold start.
    // Platform baseline allows system and ordinary user-config reads, while
    // sensitive credentials and history files are explicitly denied by regex
    // and policy.hidden. Connector-specific HOME directories are writable only
    // when the Host adds them to policy.read_write.
    // Confinement is strictly enforced by denying all writes except policy.read_write.
    profile.push_str(";; Platform runtime baseline (read-only)\n");
    profile.push_str("(allow file-read-metadata)\n");
    profile.push_str("(allow file-read* (subpath \"/\"))\n");
    profile.push_str(";; Sensitive credential deny-list\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.ssh(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.gnupg(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.aws(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.config/gcloud(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.config/gh(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/Library/Keychains(/|$)\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.netrc$\"))\n");
    profile.push_str("(deny file-read* (regex #\"^/Users/[^/]+/\\.[-a-zA-Z0-9_]+_history$\"))\n\n");
    for hidden in &policy.hidden {
        profile.push_str(&format!(
            "(deny file-read* (subpath \"{}\"))\n",
            escape_scheme_path(hidden)
        ));
        profile.push_str(&format!(
            "(deny file-write* (subpath \"{}\"))\n",
            escape_scheme_path(hidden)
        ));
    }
    profile.push_str("(deny file-write* (subpath \"/\"))\n\n");

    profile.push_str(";; Executable/runtime paths\n");
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
    for root in process_exec_roots(policy) {
        profile.push_str(&format!(
            "(allow process-exec (subpath \"{}\"))\n",
            escape_scheme_path(&root)
        ));
    }

    profile.push_str(";; Work read-only roots\n");
    for root in policy.read_only.iter().chain(policy.read_write.iter()) {
        profile.push_str(&format!(
            "(allow file-read* (subpath \"{}\"))\n",
            escape_scheme_path(root)
        ));
    }
    profile.push_str(";; Work writable roots\n");
    profile.push_str(";; Essential system devices (Read/Write/Ioctl) & POSIX SHM\n");
    profile.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/dtracehelper\") (literal \"/dev/tty\") (literal \"/dev/ptmx\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/stdin\") (subpath \"/dev/fd\"))\n");
    profile.push_str("(allow file-ioctl (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/dtracehelper\") (literal \"/dev/tty\") (literal \"/dev/ptmx\") (subpath \"/dev/fd\"))\n");
    profile.push_str("(allow ipc-posix-shm*)\n");
    for root in &policy.read_write {
        profile.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            escape_scheme_path(root)
        ));
    }
    match policy.network {
        NetworkPolicy::Inherit => profile.push_str("(allow network*)\n"),
        NetworkPolicy::ProxyOnly | NetworkPolicy::Provider => {
            // Work's explicit network capabilities only need outbound sockets.
            // Keep inbound/listener access denied by Seatbelt.
            profile.push_str("(allow network-outbound)\n")
        }
        NetworkPolicy::Deny => {}
    }
    profile
}

fn process_exec_roots(policy: &WorkSandboxPolicy) -> Vec<PathBuf> {
    policy
        .read_only
        .iter()
        .chain(policy.read_write.iter())
        .filter(|path| path.is_dir() && *path != Path::new("/"))
        .cloned()
        .collect()
}

fn escape_scheme_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn profile_denies_sensitive_credentials_and_allows_work_roots() {
        let policy = WorkSandboxPolicy {
            cwd: PathBuf::from("/tmp/workspace"),
            read_write: vec![PathBuf::from("/Users/test/workspace")],
            read_only: vec![PathBuf::from("/usr/bin")],
            hidden: vec![PathBuf::from("/Users/test/.ssh")],
            network: NetworkPolicy::Inherit,
            env: BTreeMap::new(),
        };
        let command = ExecutionCommand {
            program: PathBuf::from("/usr/bin/true"),
            args: Vec::new(),
            current_dir: PathBuf::from("/tmp/workspace"),
            envs: Vec::new(),
        };
        let profile = generate_profile(&command, &policy);
        assert!(profile.contains("(deny file-read* (regex #\"^/Users/[^/]+/\\.ssh(/|$)\"))"));
        assert!(profile.contains("(allow file-read* (subpath \"/Users/test/workspace\"))"));
        assert!(profile.contains("(allow network*)"));
    }

    #[test]
    fn profile_keeps_nested_processes_inside_work_runtime_and_write_roots() {
        let policy = WorkSandboxPolicy {
            cwd: PathBuf::from("/tmp/workspace"),
            read_write: vec![PathBuf::from("/tmp/workspace")],
            read_only: vec![PathBuf::from("/usr/bin")],
            hidden: vec![PathBuf::from("/Users/test/.ssh")],
            network: NetworkPolicy::Inherit,
            env: BTreeMap::new(),
        };
        let command = ExecutionCommand {
            program: PathBuf::from("/usr/bin/sh"),
            args: vec![OsString::from("-c"), OsString::from("sh -c true")],
            current_dir: PathBuf::from("/tmp/workspace"),
            envs: Vec::new(),
        };

        let profile = generate_profile(&command, &policy);
        assert!(profile.contains("(allow process-fork)"));
        assert!(profile.contains("(allow process-exec (literal \"/usr/bin/sh\"))"));
        assert!(profile.contains("(allow process-exec (subpath \"/usr/bin\"))"));
        assert!(profile.contains("(deny file-write* (subpath \"/\"))"));
        assert!(profile.contains("(allow file-write* (subpath \"/tmp/workspace\"))"));
    }

    #[test]
    fn profile_denies_network_for_work_by_default() {
        let policy = WorkSandboxPolicy {
            cwd: PathBuf::from("/tmp/workspace"),
            read_write: vec![PathBuf::from("/tmp/workspace")],
            read_only: vec![PathBuf::from("/usr/bin")],
            hidden: Vec::new(),
            network: NetworkPolicy::Deny,
            env: BTreeMap::new(),
        };
        let command = ExecutionCommand {
            program: PathBuf::from("/usr/bin/true"),
            args: Vec::new(),
            current_dir: PathBuf::from("/tmp/workspace"),
            envs: Vec::new(),
        };

        let profile = generate_profile(&command, &policy);
        assert!(!profile.contains("(allow network*)"));
    }

    #[test]
    fn profile_allows_provider_outbound_network_without_inbound_access() {
        let policy = WorkSandboxPolicy {
            cwd: PathBuf::from("/tmp/workspace"),
            read_write: vec![PathBuf::from("/tmp/workspace")],
            read_only: vec![PathBuf::from("/usr/bin")],
            hidden: Vec::new(),
            network: NetworkPolicy::Provider,
            env: BTreeMap::new(),
        };
        let command = ExecutionCommand {
            program: PathBuf::from("/usr/bin/true"),
            args: Vec::new(),
            current_dir: PathBuf::from("/tmp/workspace"),
            envs: Vec::new(),
        };

        let profile = generate_profile(&command, &policy);
        assert!(profile.contains("(allow network-outbound)"));
        assert!(!profile.contains("(allow network*)"));
    }
}
