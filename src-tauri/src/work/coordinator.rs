//! Run-global Execution Coordinator for Work Mode.
//!
//! Enforces per-WorkRun execution concurrency rules:
//! - ParallelSafe (e.g. read_file, list_files, workspace_info): multiple concurrent readers up to read_pool limit.
//! - Serial / Exclusive (e.g. write_file, edit_file, run_command, work_execute): run-global exclusive execution across the entire WorkRun.
//!
//! Important Invariant:
//! Execution permits are acquired ONLY after policy checks and approval grants are satisfied,
//! immediately before executing the underlying action. They are never held while waiting on user approvals.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{RwLock, Semaphore};

use crate::work::models::ToolConcurrencyClass;

const DEFAULT_PARALLEL_READ_LIMIT: usize = 4;

pub struct RunExecutionGate {
    gate: RwLock<()>,
    read_pool: Semaphore,
}

impl Default for RunExecutionGate {
    fn default() -> Self {
        Self {
            gate: RwLock::new(()),
            read_pool: Semaphore::new(DEFAULT_PARALLEL_READ_LIMIT),
        }
    }
}

pub enum ExecutionPermit<'a> {
    Read {
        _gate: tokio::sync::RwLockReadGuard<'a, ()>,
        _read_permit: tokio::sync::SemaphorePermit<'a>,
    },
    Exclusive {
        _gate: tokio::sync::RwLockWriteGuard<'a, ()>,
    },
}

#[derive(Default)]
pub struct WorkExecutionCoordinator {
    runs: Mutex<HashMap<String, Arc<RunExecutionGate>>>,
}

static COORDINATOR: OnceLock<WorkExecutionCoordinator> = OnceLock::new();

pub fn coordinator() -> &'static WorkExecutionCoordinator {
    COORDINATOR.get_or_init(WorkExecutionCoordinator::default)
}

impl WorkExecutionCoordinator {
    pub fn get_or_create_gate(&self, run_key: &str) -> Arc<RunExecutionGate> {
        let mut lock = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        lock.entry(run_key.to_string())
            .or_insert_with(|| Arc::new(RunExecutionGate::default()))
            .clone()
    }

    pub fn cleanup_run(&self, run_key: &str) {
        let mut lock = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        lock.remove(run_key);
    }
}

impl RunExecutionGate {
    pub async fn acquire(&self, concurrency: ToolConcurrencyClass) -> ExecutionPermit<'_> {
        match concurrency {
            ToolConcurrencyClass::ParallelSafe => {
                let gate = self.gate.read().await;
                let read_permit = self
                    .read_pool
                    .acquire()
                    .await
                    .expect("Semaphore is never closed");
                ExecutionPermit::Read {
                    _gate: gate,
                    _read_permit: read_permit,
                }
            }
            ToolConcurrencyClass::Serial | ToolConcurrencyClass::Exclusive => {
                let gate = self.gate.write().await;
                ExecutionPermit::Exclusive { _gate: gate }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_parallel_reads_run_concurrently() {
        let gate = Arc::new(RunExecutionGate::default());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let counter_clone = counter.clone();
            let gate_clone = gate.clone();
            handles.push(tokio::spawn(async move {
                let _permit = gate_clone.acquire(ToolConcurrencyClass::ParallelSafe).await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }));
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
        // All 3 readers should have acquired concurrently
        assert_eq!(counter.load(Ordering::SeqCst), 3);

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_exclusive_write_blocks_reads_and_other_writes() {
        let gate = Arc::new(RunExecutionGate::default());
        let in_flight_write = Arc::new(AtomicUsize::new(0));
        let read_after_write_started = Arc::new(AtomicUsize::new(0));

        // Start exclusive write
        let gate1 = gate.clone();
        let write_flag = in_flight_write.clone();
        let write_handle = tokio::spawn(async move {
            let _permit = gate1.acquire(ToolConcurrencyClass::Serial).await;
            write_flag.store(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(60)).await;
            write_flag.store(0, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(15)).await;
        assert_eq!(in_flight_write.load(Ordering::SeqCst), 1);

        // Try reading while write is in progress
        let gate2 = gate.clone();
        let write_flag_for_read = in_flight_write.clone();
        let read_counter = read_after_write_started.clone();
        let read_handle = tokio::spawn(async move {
            let _permit = gate2.acquire(ToolConcurrencyClass::ParallelSafe).await;
            // When read finally acquires, the write must have finished!
            assert_eq!(write_flag_for_read.load(Ordering::SeqCst), 0);
            read_counter.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        // Reader should still be waiting
        assert_eq!(read_after_write_started.load(Ordering::SeqCst), 0);

        write_handle.await.unwrap();
        read_handle.await.unwrap();

        assert_eq!(read_after_write_started.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_separate_runs_have_independent_gates() {
        let coord = WorkExecutionCoordinator::default();
        let gate_run1 = coord.get_or_create_gate("run-1");
        let gate_run2 = coord.get_or_create_gate("run-2");

        let run1_write_started = Arc::new(AtomicUsize::new(0));
        let run2_read_completed = Arc::new(AtomicUsize::new(0));

        let g1 = gate_run1.clone();
        let r1_flag = run1_write_started.clone();
        let h1 = tokio::spawn(async move {
            let _permit = g1.acquire(ToolConcurrencyClass::Serial).await;
            r1_flag.store(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(60)).await;
        });

        tokio::time::sleep(Duration::from_millis(15)).await;
        assert_eq!(run1_write_started.load(Ordering::SeqCst), 1);

        // Run 2 read should NOT be blocked by Run 1 write
        let g2 = gate_run2.clone();
        let r2_counter = run2_read_completed.clone();
        let h2 = tokio::spawn(async move {
            let _permit = g2.acquire(ToolConcurrencyClass::ParallelSafe).await;
            r2_counter.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(run2_read_completed.load(Ordering::SeqCst), 1);

        h1.await.unwrap();
        h2.await.unwrap();

        coord.cleanup_run("run-1");
        coord.cleanup_run("run-2");
    }
}
