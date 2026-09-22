use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmokeConfig {
    #[serde(default = "default_scenario")]
    pub scenario: String,
    pub runtime: String,
    pub model: String,
    pub effort: String,
    #[serde(
        default = "default_timeout",
        alias = "timeoutSecs",
        alias = "timeout_secs",
        alias = "timeout_seconds"
    )]
    pub timeout_secs: u64,
    #[serde(default, alias = "keepWorkspace", alias = "keep_workspace")]
    pub keep_workspace: bool,
    #[serde(default, alias = "reportDir", alias = "report_dir")]
    pub report_dir: Option<PathBuf>,
    #[serde(default, alias = "externalFixtureDir", alias = "external_fixture_dir")]
    pub external_fixture_dir: Option<PathBuf>,
}

fn default_scenario() -> String {
    "core-contract-approval".to_string()
}

fn default_timeout() -> u64 {
    600
}

impl SmokeConfig {
    pub fn validate(&self) -> Result<(), String> {
        let trimmed_runtime = self.runtime.trim().to_ascii_lowercase();
        if trimmed_runtime != "pi" {
            return Err("Real Agent Smoke V1 only supports runtime=pi".to_string());
        }

        if self.model.trim().is_empty() {
            return Err("--model is required".to_string());
        }

        let effort = self.effort.trim().to_ascii_lowercase();
        if !matches!(effort.as_str(), "off" | "low" | "medium" | "high") {
            return Err(format!(
                "Invalid effort '{}'; must be one of: off, low, medium, high",
                self.effort
            ));
        }

        if self.scenario.trim() != "core-contract-approval" {
            return Err(format!(
                "Real Agent Smoke V1 only supports scenario 'core-contract-approval', got '{}'",
                self.scenario
            ));
        }

        if self.timeout_secs == 0 {
            return Err("timeout_secs must be greater than 0".to_string());
        }

        Ok(())
    }

    pub fn effective_report_dir(&self, app_root: &Path) -> PathBuf {
        self.report_dir
            .clone()
            .unwrap_or_else(|| app_root.join("reports").join("work-smoke"))
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            format!(
                "Failed to read smoke config from {}: {e}",
                path.as_ref().display()
            )
        })?;
        let config: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse smoke config JSON: {e}"))?;
        config.validate()?;
        Ok(config)
    }
}
