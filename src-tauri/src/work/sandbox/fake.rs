use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::work::sandbox::policy::{
    SandboxEnforcement, SandboxExecutionPolicy, SandboxRequirement,
};
use crate::work::sandbox::{
    ConfinedCommand, ExecutionCommand, NativeSandboxProvider, SandboxError,
};

#[derive(Clone)]
pub struct FakeSandboxProvider {
    name: &'static str,
    enforcement: SandboxEnforcement,
    should_fail_probe: Arc<AtomicBool>,
    should_fail_confine: Arc<AtomicBool>,
}

impl FakeSandboxProvider {
    pub fn new_full() -> Self {
        Self {
            name: "fake-full",
            enforcement: SandboxEnforcement::Full,
            should_fail_probe: Arc::new(AtomicBool::new(false)),
            should_fail_confine: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn new_partial() -> Self {
        Self {
            name: "fake-partial",
            enforcement: SandboxEnforcement::Partial,
            should_fail_probe: Arc::new(AtomicBool::new(false)),
            should_fail_confine: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn new_unavailable() -> Self {
        Self {
            name: "fake-unavailable",
            enforcement: SandboxEnforcement::None,
            should_fail_probe: Arc::new(AtomicBool::new(true)),
            should_fail_confine: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_fail_probe(&self, fail: bool) {
        self.should_fail_probe.store(fail, Ordering::SeqCst);
    }

    pub fn set_fail_confine(&self, fail: bool) {
        self.should_fail_confine.store(fail, Ordering::SeqCst);
    }
}

impl Default for FakeSandboxProvider {
    fn default() -> Self {
        Self::new_full()
    }
}

impl NativeSandboxProvider for FakeSandboxProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn probe(&self) -> Result<SandboxEnforcement, SandboxError> {
        if self.should_fail_probe.load(Ordering::SeqCst) {
            return Err(SandboxError::Unavailable(
                "Fake provider simulated probe failure".to_string(),
            ));
        }
        Ok(self.enforcement)
    }

    fn confine(
        &self,
        command: &ExecutionCommand,
        policy: &SandboxExecutionPolicy,
    ) -> Result<ConfinedCommand, SandboxError> {
        if self.should_fail_confine.load(Ordering::SeqCst) {
            return Err(SandboxError::RunnerFailed(
                "Fake provider simulated confinement failure".to_string(),
            ));
        }

        let enforcement = self.probe()?;
        if policy.requirement == SandboxRequirement::FullRequired
            && enforcement != SandboxEnforcement::Full
        {
            return Err(SandboxError::EnforcementInsufficient {
                required: policy.requirement,
                actual: enforcement,
            });
        }

        Ok(ConfinedCommand {
            program: command.program.clone(),
            args: command.args.clone(),
            current_dir: command.current_dir.clone(),
            envs: command.envs.clone(),
            enforcement,
            denial_signatures: vec![
                "permission denied".to_string(),
                "operation not permitted".to_string(),
            ],
            runner_failure_signatures: vec!["fake-runner-failed".to_string()],
        })
    }
}
