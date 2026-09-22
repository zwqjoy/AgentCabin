use std::fs;

use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::work::models::WorkProfile;
use crate::work::paths::WorkPaths;

pub fn is_enabled() -> bool {
    crate::storage::settings::get_user_settings().work_mode_enabled
}

pub fn ensure_enabled() -> Result<(), String> {
    if is_enabled() {
        Ok(())
    } else {
        Err("Work Mode is disabled by the current feature flag".into())
    }
}

pub fn default_profile() -> WorkProfile {
    WorkProfile {
        id: "work".into(),
        name: "Work".into(),
        enabled: true,
        runtime: RuntimeProviderKind::Pi,
    }
}

pub fn get_profile() -> Result<WorkProfile, String> {
    let mut profile = get_profile_with_paths(&WorkPaths::app())?;
    profile.enabled = is_enabled();
    Ok(profile)
}

/// Resolve the runtime for a newly created Work session.
///
/// `work_default_runtime` is the current settings authority. The persisted
/// profile runtime remains a read-only migration fallback for installations
/// that predate the setting.
pub fn resolve_new_work_runtime(
    settings: &crate::models::UserSettings,
    legacy_profile: &WorkProfile,
) -> Result<RuntimeProviderKind, String> {
    let configured_runtime = settings
        .work_default_runtime
        .as_deref()
        .map(str::trim)
        .filter(|runtime| !runtime.is_empty());

    match configured_runtime {
        Some(runtime) => RuntimeProviderKind::try_from_agent_str(runtime)
            .map_err(|error| format!("Invalid Work default runtime '{runtime}': {error}")),
        None => Ok(legacy_profile.runtime),
    }
}

/// Resolve the runtime for a newly created Work session from persisted app state.
pub fn resolve_new_work_runtime_from_storage() -> Result<RuntimeProviderKind, String> {
    let settings = crate::storage::settings::get_user_settings();
    let legacy_profile = get_profile()?;
    resolve_new_work_runtime(&settings, &legacy_profile)
}

pub fn get_profile_with_paths(paths: &WorkPaths) -> Result<WorkProfile, String> {
    paths.ensure_layout()?;
    let path = paths.work_profile_dir().join("profile.json");
    if !path.exists() {
        let profile = default_profile();
        let content = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
        fs::write(&path, format!("{content}\n")).map_err(|e| e.to_string())?;
        return Ok(profile);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid Work profile: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(runtime: RuntimeProviderKind) -> WorkProfile {
        WorkProfile {
            id: "work".into(),
            name: "Work".into(),
            enabled: true,
            runtime,
        }
    }

    #[test]
    fn new_work_runtime_prefers_user_settings() {
        let mut settings = crate::models::UserSettings::default();
        settings.work_default_runtime = Some("codex".into());

        let runtime = resolve_new_work_runtime(&settings, &profile(RuntimeProviderKind::Pi))
            .expect("configured runtime should parse");

        assert_eq!(runtime, RuntimeProviderKind::Codex);
    }

    #[test]
    fn new_work_runtime_uses_legacy_profile_when_setting_is_missing_or_blank() {
        let mut settings = crate::models::UserSettings::default();
        settings.work_default_runtime = Some("  ".into());

        let runtime = resolve_new_work_runtime(&settings, &profile(RuntimeProviderKind::Grok))
            .expect("legacy runtime should be accepted");

        assert_eq!(runtime, RuntimeProviderKind::Grok);
    }

    #[test]
    fn new_work_runtime_accepts_dsh_settings() {
        let mut settings = crate::models::UserSettings::default();
        settings.work_default_runtime = Some("dsh".into());

        let runtime = resolve_new_work_runtime(&settings, &profile(RuntimeProviderKind::Pi))
            .expect("dsh runtime should be recognized");

        assert_eq!(runtime, RuntimeProviderKind::Dsh);
    }

    #[test]
    fn new_work_runtime_rejects_unknown_settings() {
        let mut settings = crate::models::UserSettings::default();
        settings.work_default_runtime = Some("unknown_runtime_xyz".into());

        let error = resolve_new_work_runtime(&settings, &profile(RuntimeProviderKind::Pi))
            .expect_err("unknown runtime must fail closed");

        assert!(error.contains("unknown_runtime_xyz"));
    }
}
