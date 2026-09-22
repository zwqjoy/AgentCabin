//! Helpers for loading optional Pi packages without duplicating native packages.
//!
//! Pi automatically loads packages listed in the active profile's
//! `settings.json` (`~/.pi/agent` for the unmanaged CLI, or an AgentCabin
//! profile for Pi Code/Work). The host may still need to load a package
//! explicitly for a fresh profile, but passing the same package with
//! `-e npm:...` causes Pi to load it twice and reject duplicate tools/flags.

use crate::storage;
use serde_json::Value;
use std::path::PathBuf;

fn native_settings_path() -> Option<PathBuf> {
    storage::home_dir().map(|home| PathBuf::from(home).join(".pi/agent/settings.json"))
}

fn settings_path_for_agent_dir(agent_dir: Option<&str>) -> Option<PathBuf> {
    agent_dir
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("settings.json"))
        .or_else(native_settings_path)
}

fn package_spec_matches(spec: &str, package: &str) -> bool {
    let spec = spec.strip_prefix("npm:").unwrap_or(spec);
    let package = package.strip_prefix("npm:").unwrap_or(package);
    spec == package
        || spec
            .strip_prefix(package)
            .is_some_and(|rest| rest.starts_with('@'))
}

fn native_settings_has_package(settings: &Value, package: &str) -> bool {
    settings
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|spec| package_spec_matches(spec, package))
}

/// Return whether the host should pass a package through `-e`.
///
/// If native settings cannot be read, keep the historical behavior and load
/// the package explicitly.  This keeps a fresh or temporarily incomplete Pi
/// profile functional while avoiding duplicate loads for valid native settings.
pub(crate) fn should_load_explicitly_for_agent_dir(agent_dir: Option<&str>, package: &str) -> bool {
    let Some(path) = settings_path_for_agent_dir(agent_dir) else {
        return true;
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return true;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&contents) else {
        return true;
    };
    !native_settings_has_package(&settings, package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn package_spec_matches_npm_and_versioned_specs() {
        assert!(package_spec_matches(
            "npm:@narumitw/pi-plan-mode",
            "npm:@narumitw/pi-plan-mode"
        ));
        assert!(package_spec_matches(
            "npm:@narumitw/pi-plan-mode@0.40.1",
            "npm:@narumitw/pi-plan-mode"
        ));
        assert!(!package_spec_matches(
            "npm:@narumitw/pi-goal",
            "npm:@narumitw/pi-plan-mode"
        ));
    }

    #[test]
    fn native_settings_detects_configured_packages() {
        let settings = json!({
            "packages": [
                "npm:@narumitw/pi-plan-mode",
                "npm:@narumitw/pi-goal@0.41.0"
            ]
        });
        assert!(native_settings_has_package(
            &settings,
            "npm:@narumitw/pi-plan-mode"
        ));
        assert!(native_settings_has_package(
            &settings,
            "npm:@narumitw/pi-goal"
        ));
        assert!(!native_settings_has_package(
            &settings,
            "npm:@gotgenes/pi-permission-system"
        ));
    }

    #[test]
    fn work_agent_dir_is_used_for_package_detection() {
        let temp = tempfile::TempDir::new().unwrap();
        let settings_path = temp.path().join("settings.json");
        std::fs::write(&settings_path, r#"{"packages":["npm:@narumitw/pi-goal"]}"#).unwrap();

        assert!(!should_load_explicitly_for_agent_dir(
            Some(temp.path().to_str().unwrap()),
            "npm:@narumitw/pi-goal"
        ));
        assert!(should_load_explicitly_for_agent_dir(
            Some(temp.path().to_str().unwrap()),
            "npm:@narumitw/pi-plan-mode"
        ));
    }
}
