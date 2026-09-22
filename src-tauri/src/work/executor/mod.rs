pub mod command_runner;
pub mod host_executor;
pub mod request;
pub mod runtime;

#[cfg(test)]
pub mod tests;

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

pub use host_executor::RestrictedHostExecutor;
pub use request::{
    ExecutionFailureKind, WorkExecutionRequest, WorkExecutionResult, WorkExecutionStatus,
};
pub use runtime::RuntimeResolver;

use crate::work::models::WorkResourceManifest;
use crate::work::paths::WorkPaths;
use crate::work::sandbox::NativeSandboxProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorBackendKind {
    RestrictedHost,
    LimaVM,
    Wsl2VM,
    Container,
}

#[async_trait]
pub trait WorkExecutorBackend: Send + Sync {
    fn kind(&self) -> ExecutorBackendKind;
    async fn health(&self) -> Result<String, String>;
    async fn execute(
        &self,
        paths: &WorkPaths,
        resource_manifest: &WorkResourceManifest,
        resource_dir: &Path,
        request: &WorkExecutionRequest,
        is_builtin_trusted: bool,
    ) -> Result<WorkExecutionResult, String>;
}

pub struct WorkExecutor {
    backend: Box<dyn WorkExecutorBackend>,
}

impl WorkExecutor {
    pub fn new_restricted_host() -> Self {
        Self {
            backend: Box::new(DefaultHostBackend { sandbox: None }),
        }
    }

    pub fn new_restricted_host_with_sandbox(sandbox: Box<dyn NativeSandboxProvider>) -> Self {
        Self {
            backend: Box::new(DefaultHostBackend {
                sandbox: Some(Arc::from(sandbox)),
            }),
        }
    }

    pub async fn execute(
        &self,
        paths: &WorkPaths,
        resource_manifest: &WorkResourceManifest,
        resource_dir: &Path,
        request: &WorkExecutionRequest,
        is_builtin_trusted: bool,
    ) -> Result<WorkExecutionResult, String> {
        self.backend
            .execute(
                paths,
                resource_manifest,
                resource_dir,
                request,
                is_builtin_trusted,
            )
            .await
    }
}

struct DefaultHostBackend {
    sandbox: Option<Arc<dyn NativeSandboxProvider>>,
}

#[async_trait]
impl WorkExecutorBackend for DefaultHostBackend {
    fn kind(&self) -> ExecutorBackendKind {
        ExecutorBackendKind::RestrictedHost
    }

    async fn health(&self) -> Result<String, String> {
        Ok("RestrictedHostExecutor active (trusted built-in capabilities only)".to_string())
    }

    async fn execute(
        &self,
        paths: &WorkPaths,
        resource_manifest: &WorkResourceManifest,
        resource_dir: &Path,
        request: &WorkExecutionRequest,
        is_builtin_trusted: bool,
    ) -> Result<WorkExecutionResult, String> {
        if let Some(sandbox) = &self.sandbox {
            RestrictedHostExecutor::execute_with_sandbox(
                paths,
                resource_manifest,
                resource_dir,
                request,
                is_builtin_trusted,
                sandbox.as_ref(),
            )
            .await
        } else {
            RestrictedHostExecutor::execute(
                paths,
                resource_manifest,
                resource_dir,
                request,
                is_builtin_trusted,
            )
            .await
        }
    }
}
