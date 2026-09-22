use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use super::{NetworkPolicy, WorkSandboxPolicy};
use crate::work::sandbox::{ConfinedCommand, ExecutionCommand, SandboxEnforcement, SandboxError};

static BWRAP_PATH: OnceLock<Result<PathBuf, String>> = OnceLock::new();

pub(super) fn confine(
    command: &ExecutionCommand,
    policy: &WorkSandboxPolicy,
) -> Result<ConfinedCommand, SandboxError> {
    let bwrap = bwrap_path(policy)?;
    let mut args = vec![
        OsString::from("--die-with-parent"),
        OsString::from("--unshare-all"),
    ];
    if matches!(
        policy.network,
        NetworkPolicy::Inherit | NetworkPolicy::ProxyOnly | NetworkPolicy::Provider
    ) {
        args.push(OsString::from("--share-net"));
    }
    args.extend([
        OsString::from("--proc"),
        OsString::from("/proc"),
        OsString::from("--dev"),
        OsString::from("/dev"),
        OsString::from("--tmpfs"),
        OsString::from("/tmp"),
    ]);

    // These are the minimum read-only system roots needed by common Node/Python
    // runtimes. The user's home is not mounted wholesale; connector-specific
    // HOME directories are mounted below from policy roots.
    for root in ["/usr", "/bin", "/lib", "/lib64", "/etc", "/opt"] {
        let path = Path::new(root);
        if path.exists() {
            args.push(OsString::from("--ro-bind"));
            args.push(path.as_os_str().to_os_string());
            args.push(path.as_os_str().to_os_string());
        }
    }

    let mut mounted = Vec::new();
    for root in &policy.read_only {
        if covered_by_system_root(root)
            || policy
                .read_write
                .iter()
                .any(|writable| writable.starts_with(root))
            || mounted
                .iter()
                .any(|existing: &PathBuf| root.starts_with(existing))
        {
            continue;
        }
        add_parent_dirs(&mut args, root, &mut mounted);
        args.push(OsString::from("--ro-bind"));
        args.push(root.as_os_str().to_os_string());
        args.push(root.as_os_str().to_os_string());
        mounted.push(root.clone());
    }
    for root in &policy.read_write {
        if covered_by_system_root(root) || mounted.iter().any(|existing| existing == root) {
            continue;
        }
        add_parent_dirs(&mut args, root, &mut mounted);
        args.push(OsString::from("--bind"));
        args.push(root.as_os_str().to_os_string());
        args.push(root.as_os_str().to_os_string());
        mounted.push(root.clone());
    }

    args.push(OsString::from("--chdir"));
    args.push(policy.cwd.as_os_str().to_os_string());
    args.push(OsString::from("--"));
    args.push(command.program.as_os_str().to_os_string());
    args.extend(command.args.iter().cloned());

    Ok(ConfinedCommand {
        program: bwrap,
        args,
        current_dir: policy.cwd.clone(),
        envs: command.envs.clone(),
        enforcement: SandboxEnforcement::Full,
        denial_signatures: vec![
            "operation not permitted".into(),
            "permission denied".into(),
            "no such file or directory".into(),
        ],
        runner_failure_signatures: vec![
            "bwrap:".into(),
            "bubblewrap:".into(),
            "creating new namespace failed".into(),
        ],
    })
}

fn bwrap_path(policy: &WorkSandboxPolicy) -> Result<PathBuf, SandboxError> {
    if let Some(cached) = BWRAP_PATH.get() {
        return cached.clone().map_err(SandboxError::Unavailable);
    }
    let path_value = policy
        .env
        .get("PATH")
        .cloned()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let path = path_value
        .split(':')
        .map(|dir| Path::new(dir).join("bwrap"))
        .find(|candidate| candidate.is_file())
        .or_else(|| {
            ["/usr/bin/bwrap", "/bin/bwrap"]
                .into_iter()
                .map(PathBuf::from)
                .find(|candidate| candidate.is_file())
        });
    let result = path.ok_or_else(|| "bubblewrap (bwrap) was not found".to_string());
    if result.is_ok() {
        let _ = BWRAP_PATH.set(result.clone());
    }
    result.map_err(SandboxError::Unavailable)
}

fn covered_by_system_root(path: &Path) -> bool {
    ["/usr", "/bin", "/lib", "/lib64", "/etc", "/opt"]
        .iter()
        .map(Path::new)
        .any(|root| path.starts_with(root))
}

fn add_parent_dirs(args: &mut Vec<OsString>, path: &Path, mounted: &mut Vec<PathBuf>) {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if current == "/" || covered_by_system_root(&current) || mounted.contains(&current) {
            continue;
        }
        args.push(OsString::from("--dir"));
        args.push(current.as_os_str().to_os_string());
        mounted.push(current.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn system_roots_are_not_rebound() {
        assert!(covered_by_system_root(Path::new("/usr/bin/node")));
        assert!(!covered_by_system_root(Path::new("/home/user/work")));
    }

    #[test]
    fn parent_dirs_are_created_for_user_mounts() {
        let mut args = Vec::new();
        let mut mounted = Vec::new();
        add_parent_dirs(&mut args, Path::new("/home/user/workspace"), &mut mounted);
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--dir" && pair[1] == "/home"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--dir" && pair[1] == "/home/user"));
    }

    #[allow(dead_code)]
    fn _policy() -> WorkSandboxPolicy {
        WorkSandboxPolicy {
            cwd: PathBuf::from("/tmp"),
            read_write: Vec::new(),
            read_only: Vec::new(),
            hidden: Vec::new(),
            network: NetworkPolicy::Inherit,
            env: BTreeMap::new(),
        }
    }
}
