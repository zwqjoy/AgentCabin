//! Application-owned runtime closure resolution.
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const PI_VERSION: &str = "0.85.1";

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "agentcabinVersion")]
    pub agentcabin_version: String,
    pub node: Version,
    pub runtimes: Runtimes,
    pub pnpm: Version,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub version: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Runtimes {
    pub pi: Package,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub package: String,
    pub version: String,
    pub entrypoint: String,
}
#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub manifest: RuntimeManifest,
    pub node: PathBuf,
    pub pi: PathBuf,
    pub pnpm: PathBuf,
}

/// Directories that may hold the app-pinned Pi extension packages, i.e. the
/// `extensions/node_modules` roots that contain one directory per package.
///
/// Packaged builds ship them as `extraResources` at
/// `<resources>/runtime/extensions`, while Electron/Tauri development keeps the
/// same checked-in tree at `src-tauri/runtime/extensions`. Both have to be
/// searched, because a Pi extension that cannot be found on disk is fetched
/// from the npm registry on every session start.
pub fn extension_node_modules_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            result.push(parent.join("../Resources/runtime/extensions/node_modules"));
            result.push(parent.join("../runtime/extensions/node_modules"));
        }
    }
    if let Ok(root) = std::env::var("AGENTCABIN_RUNTIME_ROOT") {
        if !root.trim().is_empty() {
            let root = PathBuf::from(root);
            // `<resources>/runtimes` is a sibling of `<resources>/runtime/extensions`.
            result.push(root.join("../runtime/extensions/node_modules"));
            // Some layouts ship the extensions inside the runtime root itself.
            result.push(root.join("extensions/node_modules"));
        }
    }
    // Development layout: the checked-in extension tree next to the crate.
    result.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/extensions/node_modules"));
    result
}

fn roots() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(root) = std::env::var("AGENTCABIN_RUNTIME_ROOT") {
        if !root.trim().is_empty() {
            result.push(root.into());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            result.push(parent.join("../Resources/runtimes"));
            result.push(parent.join("runtimes"));
        }
    }
    // The generated runtime-build is used by Electron development and by
    // packaged smoke tests; keep the checked-in extension runtime separate.
    result.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime-build"));
    result.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    result
}
pub fn packaged() -> bool {
    matches!(
        std::env::var("AGENTCABIN_PACKAGED").as_deref(),
        Ok("1" | "true")
    )
}
pub fn bundled() -> Result<RuntimePaths, String> {
    let root = roots()
        .into_iter()
        .find(|p| p.join("runtime-manifest.json").is_file())
        .ok_or_else(|| "RuntimeMissing: bundled runtime closure was not found".to_string())?;
    let manifest: RuntimeManifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("runtime-manifest.json"))
            .map_err(|e| format!("RuntimeManifestInvalid: {e}"))?,
    )
    .map_err(|e| format!("RuntimeManifestInvalid: {e}"))?;
    if manifest.runtimes.pi.version != PI_VERSION {
        return Err(format!(
            "RuntimeVersionMismatch: Pi is {}, expected {PI_VERSION}",
            manifest.runtimes.pi.version
        ));
    }
    let bin = |name: &str| {
        root.join(name).join("bin").join(if cfg!(windows) {
            format!("{name}.cmd")
        } else {
            name.to_string()
        })
    };
    Ok(RuntimePaths {
        node: root.join("node").join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        }),
        pi: bin("pi"),
        pnpm: bin("pnpm"),
        root,
        manifest,
    })
}
pub fn resolve_pi() -> Result<String, String> {
    let p = bundled()?.pi;
    if p.is_file() {
        Ok(p.to_string_lossy().into())
    } else {
        Err(format!("RuntimeClosureInvalid: missing {}", p.display()))
    }
}
pub fn resolve_node() -> Result<String, String> {
    let p = bundled()?.node;
    if p.is_file() {
        Ok(p.to_string_lossy().into())
    } else {
        Err(format!("RuntimeClosureInvalid: missing {}", p.display()))
    }
}
pub fn resolve_npm() -> Result<String, String> {
    let root = bundled()?.root;
    let npm = if cfg!(windows) {
        root.join("node").join("npm.cmd")
    } else {
        root.join("node").join("bin").join("npm")
    };
    if npm.is_file() {
        Ok(npm.to_string_lossy().into())
    } else {
        Err(format!("RuntimeClosureInvalid: missing {}", npm.display()))
    }
}
pub fn resolve_npm_cli() -> Result<String, String> {
    let root = bundled()?.root;
    let candidates = [
        root.join("node")
            .join("lib")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
        root.join("node")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
    ];
    for cli in candidates {
        if cli.is_file() {
            return Ok(cli.to_string_lossy().into());
        }
    }
    Err(format!(
        "RuntimeClosureInvalid: missing npm-cli.js in {}",
        root.join("node").display()
    ))
}
pub fn resolve_pnpm() -> Result<String, String> {
    let p = bundled()?.pnpm;
    if p.is_file() {
        Ok(p.to_string_lossy().into())
    } else {
        Err(format!("RuntimeClosureInvalid: missing {}", p.display()))
    }
}
#[allow(dead_code)]
pub fn is_within(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pi_is_pinned() {
        assert_eq!(PI_VERSION, "0.85.1");
    }

    #[test]
    fn bundled_runtime_resolutions() {
        if let Ok(paths) = bundled() {
            assert!(paths.node.is_file(), "node must exist");
            assert!(resolve_node().is_ok(), "resolve_node must succeed");
            assert!(resolve_npm().is_ok(), "resolve_npm must succeed");
            assert!(resolve_npm_cli().is_ok(), "resolve_npm_cli must succeed");
            assert!(resolve_pnpm().is_ok(), "resolve_pnpm must succeed");
            assert!(resolve_pi().is_ok(), "resolve_pi must succeed");
        }
    }

    #[test]
    fn development_layout_exposes_pinned_pi_extensions() {
        let dirs = extension_node_modules_dirs();
        assert!(
            dirs.iter().any(|dir| dir.is_dir()),
            "at least one extension root must exist in a checked-out tree: {dirs:?}"
        );
        assert!(
            crate::agent::claude_stream::bundled_pi_package_path("pi-mono-multi-edit").is_some(),
            "the pinned multi-edit extension must resolve locally so Pi never installs it online"
        );
    }
}
