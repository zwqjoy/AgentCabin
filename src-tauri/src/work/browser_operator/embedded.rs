//! Registry for Electron-owned embedded browser views and their CDP relays.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedTarget {
    pub target_id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub url: String,
    /// A tab may use its own authenticated relay while sharing a run with the
    /// other tabs in the same Electron-owned browser surface.
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedRegistration {
    pub endpoint: String,
    pub token: String,
    #[serde(default)]
    pub targets: Vec<EmbeddedTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEmbeddedTarget {
    pub endpoint: String,
    pub token: String,
    pub target_id: String,
    pub url: String,
}

#[derive(Debug, Default)]
pub struct EmbeddedRegistry {
    inner: RwLock<HashMap<String, EmbeddedRegistration>>,
}

impl EmbeddedRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, registration: EmbeddedRegistration) {
        let mut guard = self.inner.write().expect("embedded registry poisoned");
        guard.insert(registration.endpoint.clone(), registration);
    }

    pub fn unregister(&self, endpoint: &str) {
        let mut guard = self.inner.write().expect("embedded registry poisoned");
        guard.remove(endpoint);
    }

    pub fn resolve(&self, run_id: &str) -> Option<ResolvedEmbeddedTarget> {
        let guard = self.inner.read().expect("embedded registry poisoned");
        guard.values().find_map(|registration| {
            let targets: Vec<_> = registration
                .targets
                .iter()
                .filter(|target| target.run_id.as_deref() == Some(run_id))
                .collect();
            let target = targets
                .iter()
                .copied()
                .find(|target| target.active)
                .or_else(|| targets.first().copied())?;
            Some(ResolvedEmbeddedTarget {
                endpoint: target
                    .endpoint
                    .clone()
                    .unwrap_or_else(|| registration.endpoint.clone()),
                token: target
                    .token
                    .clone()
                    .unwrap_or_else(|| registration.token.clone()),
                target_id: target.target_id.clone(),
                url: target.url.clone(),
            })
        })
    }

    pub fn list(&self) -> Vec<EmbeddedRegistration> {
        let guard = self.inner.read().expect("embedded registry poisoned");
        guard.values().cloned().collect()
    }
}

static REGISTRY: OnceLock<EmbeddedRegistry> = OnceLock::new();

pub fn embedded_registry() -> &'static EmbeddedRegistry {
    REGISTRY.get_or_init(EmbeddedRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(run_id: Option<&str>) -> EmbeddedRegistration {
        EmbeddedRegistration {
            endpoint: "127.0.0.1:51234".to_string(),
            token: "tok_a".to_string(),
            targets: vec![EmbeddedTarget {
                target_id: "T1".to_string(),
                run_id: run_id.map(str::to_string),
                url: "https://example.com".to_string(),
                endpoint: None,
                token: None,
                active: true,
            }],
        }
    }

    #[test]
    fn registers_and_resolves_a_run_bound_target() {
        let registry = EmbeddedRegistry::new();
        registry.register(registration(Some("run-1")));
        let resolved = registry.resolve("run-1").expect("target should resolve");
        assert_eq!(resolved.endpoint, "127.0.0.1:51234");
        assert_eq!(resolved.target_id, "T1");
    }

    #[test]
    fn unregistering_clears_bindings() {
        let registry = EmbeddedRegistry::new();
        registry.register(registration(Some("run-1")));
        registry.unregister("127.0.0.1:51234");
        assert!(registry.resolve("run-1").is_none());
    }

    #[test]
    fn unbound_targets_do_not_resolve() {
        let registry = EmbeddedRegistry::new();
        registry.register(registration(None));
        assert!(registry.resolve("run-1").is_none());
    }
}
