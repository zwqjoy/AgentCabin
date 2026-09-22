use crate::work::models::WorkExecutionRuntime;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RuntimeDescriptor {
    pub binary_path: PathBuf,
    pub runtime_read_roots: Vec<PathBuf>,
}

pub struct RuntimeResolver;

impl RuntimeResolver {
    /// Resolve runtime descriptor containing executable binary and runtime library read roots.
    pub fn resolve_runtime(runtime: WorkExecutionRuntime) -> Result<RuntimeDescriptor, String> {
        let binary_path = Self::resolve_binary(runtime)?;
        let mut runtime_read_roots = Vec::new();

        // If the binary has a parent directory (e.g. .../bin/python3), determine its installation prefix
        if let Ok(canonical_bin) = binary_path.canonicalize() {
            if let Some(bin_dir) = canonical_bin.parent() {
                runtime_read_roots.push(bin_dir.to_path_buf());
                if let Some(prefix) = bin_dir.parent() {
                    // Include prefix libraries (e.g. prefix/lib, prefix/include, prefix/share)
                    runtime_read_roots.push(prefix.to_path_buf());
                }
            }
        }

        Ok(RuntimeDescriptor {
            binary_path,
            runtime_read_roots,
        })
    }

    /// Resolve trusted binary path for Python or Node on the host OS.
    pub fn resolve_binary(runtime: WorkExecutionRuntime) -> Result<PathBuf, String> {
        let binary_names = match runtime {
            WorkExecutionRuntime::Python => &["python3", "python"][..],
            WorkExecutionRuntime::Node => &["node"][..],
        };

        // Standard system binary directories
        let search_dirs = ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin", "/bin"];

        for &name in binary_names {
            for &dir in &search_dirs {
                let candidate = PathBuf::from(dir).join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }

            // Also check PATH environment variable
            if let Ok(path_var) = std::env::var("PATH") {
                for path_entry in std::env::split_paths(&path_var) {
                    let candidate = path_entry.join(name);
                    if candidate.is_file() {
                        return Ok(candidate);
                    }
                }
            }

            // Also check augmented PATH (e.g. nvm, fnm, volta, mise, homebrew in GUI apps)
            let aug_path = crate::agent::claude_stream::augmented_path();
            for path_entry in std::env::split_paths(&aug_path) {
                let candidate = path_entry.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        Err(format!(
            "Runtime binary for '{:?}' not found on host system",
            runtime
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_available_host_runtimes() {
        let python_res = RuntimeResolver::resolve_runtime(WorkExecutionRuntime::Python);
        let node_res = RuntimeResolver::resolve_runtime(WorkExecutionRuntime::Node);

        // At least one of python or node should exist on standard dev environments
        assert!(python_res.is_ok() || node_res.is_ok());
        if let Ok(desc) = python_res {
            assert!(desc.binary_path.exists());
        }
    }
}
