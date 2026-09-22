//! AgentCabin-owned Pi context usage extension shared by Code and Work.

use std::fs;
use std::path::PathBuf;

use crate::work::paths::WorkPaths;

const EXTENSION_DIR: &str = "agentcabin-context-usage";
const EXTENSION_FILENAME: &str = "context_usage.mjs";
const EXTENSION_SOURCE: &str = include_str!("work/pi_context_usage_extension.mjs");

/// Provision the built-in context classifier beside AgentCabin's common Pi system packages.
/// The extension is loaded explicitly by both Code and Work, so user Pi extensions cannot
/// change whether the context telemetry exists.
pub fn ensure_context_usage_extension(paths: &WorkPaths) -> Result<PathBuf, String> {
    let dir = paths
        .pi_system_dir()
        .join("node_modules")
        .join(EXTENSION_DIR);
    fs::create_dir_all(&dir).map_err(|error| {
        format!("Failed to create AgentCabin context extension directory: {error}")
    })?;
    let entry = dir.join(EXTENSION_FILENAME);
    fs::write(&entry, EXTENSION_SOURCE).map_err(|error| {
        format!(
            "Failed to provision AgentCabin context extension {}: {error}",
            entry.display()
        )
    })?;
    Ok(entry)
}
