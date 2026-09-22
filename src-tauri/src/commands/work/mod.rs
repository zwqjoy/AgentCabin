use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::{profile, session};

pub(crate) static INBOX_DELIVERY_LOCK: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));

pub(crate) fn ensure_work_enabled() -> Result<(), String> {
    profile::ensure_enabled()
}

pub(crate) async fn resume_work_session_after_approval(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    session_id: &str,
    message: &str,
) -> Result<(), String> {
    let actor_is_alive = {
        let map = sessions.lock().await;
        map.contains_key(session_id)
    };

    if actor_is_alive {
        session::send_message(emitter, sessions, session_id, message, None).await
    } else {
        session::resume(
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            session_id,
            Some(message),
            None,
        )
        .await
        .map(|_| ())
    }
}

pub mod artifacts;
pub mod capabilities;
pub mod conversations;
pub mod interactions;
pub mod tasks;
pub mod workspaces;

pub use artifacts::*;
pub use capabilities::*;
pub use conversations::*;
pub use interactions::*;
pub use tasks::*;
pub use workspaces::*;

#[cfg(test)]
mod tests {
    use super::interactions::resolve_inbox_session_run;
    use super::tasks::{attach_work_session, mark_work_run_failed};
    use crate::work::{
        inbox::InboxManager,
        models::{InboxItemPayload, InboxItemType, WorkRunTrigger},
        paths::WorkPaths,
        tasks::TaskManager,
    };
    use tempfile::TempDir;

    #[test]
    fn attach_work_session_reports_binding_failures() {
        let temp = TempDir::new().unwrap();
        let manager = TaskManager::new(WorkPaths::new(temp.path().join("data")));
        let task = manager
            .create_task("ws-attach", "Attach test", "", None)
            .unwrap();

        let error = attach_work_session(&manager, &task.id, "missing-run", "session-1")
            .expect_err("missing WorkRun must not be silently ignored");
        assert!(error.contains("Failed to attach Work session 'session-1'"));
        assert!(error.contains("missing-run"));
    }

    #[test]
    fn session_binding_cleanup_reports_persistence_failures() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let task = manager
            .create_task("ws-attach", "Attach test", "", None)
            .unwrap();

        let error = mark_work_run_failed(
            &paths,
            &task.id,
            "missing-run",
            "session binding failed".to_string(),
        );
        assert!(error.contains("session binding failed"));
        assert!(error.contains("failed to persist WorkRun failure"));
        assert!(error.contains("missing-run"));
    }

    #[test]
    fn mark_work_run_failed_persists_terminal_state() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let task = manager
            .create_task("ws-failure", "Failure test", "", None)
            .unwrap();
        let run = manager
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let error =
            mark_work_run_failed(&paths, &task.id, &run.id, "runtime unavailable".to_string());

        assert_eq!(error, "runtime unavailable");
        let persisted = manager.get_run(&task.id, &run.id).unwrap();
        assert_eq!(persisted.status, crate::work::models::WorkRunStatus::Failed);
        assert_eq!(
            persisted.error_message.as_deref(),
            Some("runtime unavailable")
        );
    }

    #[test]
    fn inbox_session_lookup_resolves_work_run_to_chat_run() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let task = manager
            .create_task("ws-inbox", "Inbox test", "", None)
            .unwrap();
        let work_run = manager
            .start_run(&task.id, Some("chat-session-1"), WorkRunTrigger::Manual)
            .unwrap();
        let item = InboxManager::new(paths.clone())
            .create_item(
                &task.id,
                &work_run.id,
                "ws-inbox",
                InboxItemType::QuestionElicitation,
                "Need an answer",
                "Details",
                InboxItemPayload::default(),
            )
            .unwrap();

        assert_eq!(
            resolve_inbox_session_run(&paths, &item.id).unwrap(),
            Some("chat-session-1".to_string())
        );
    }

    #[test]
    fn inbox_session_lookup_resolves_standalone_run() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let standalone_run_id = "standalone-chat-session-456";
        let item = InboxManager::new(paths.clone())
            .create_item(
                standalone_run_id,
                standalone_run_id,
                "",
                InboxItemType::PermissionRequest,
                "Permission Needed",
                "Details",
                InboxItemPayload::default(),
            )
            .unwrap();

        // Without storage::runs it returns None since run doesn't exist
        assert_eq!(resolve_inbox_session_run(&paths, &item.id).unwrap(), None);
    }
}
