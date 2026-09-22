//! Pi RPC adapter for the shared SessionActor command contract.
//!
//! Pi has a distinct JSONL wire loop, but it is registered in the common
//! `ActorSessionMap` and consumes the standard `ActorCommand` mailbox.

mod actor_loop;
mod events;
mod process;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::ActorCommand;
use serde_json::Value;
use std::sync::OnceLock;

static SHARED_SESSIONS: OnceLock<ActorSessionMap> = OnceLock::new();

pub use self::process::{spawn_actor, spawn_fork_actor};

pub(super) fn register_session_map(sessions: &ActorSessionMap) {
    let _ = SHARED_SESSIONS.set(sessions.clone());
}

pub(crate) fn shared_sessions() -> Option<&'static ActorSessionMap> {
    SHARED_SESSIONS.get()
}

#[allow(dead_code)]
pub(crate) fn active_pi_run_ids() -> Vec<String> {
    let Some(sessions) = SHARED_SESSIONS.get() else {
        return vec![];
    };
    let Ok(map) = sessions.try_lock() else {
        return vec![];
    };
    map.keys()
        .filter(|run_id| crate::storage::runs::get_run(run_id).is_some_and(|run| run.agent == "pi"))
        .cloned()
        .collect()
}

#[allow(dead_code)]
pub(crate) fn has_active_pi_model(model: &str) -> bool {
    active_pi_run_ids().into_iter().any(|run_id| {
        crate::storage::runs::get_run(&run_id)
            .is_some_and(|run| run.model.as_deref() == Some(model))
    })
}

/// Best-effort compatibility path for legacy UI calls that cannot carry the shared Tauri state.
/// The normal session control API still awaits and reports RPC errors; this queue is only a
/// fallback for old model/thinking handlers shared with Claude.
#[allow(dead_code)]
pub(crate) fn queue_control(run_id: String, request: Value) {
    let Some(sessions) = SHARED_SESSIONS.get().cloned() else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let sender = {
            let map = sessions.lock().await;
            map.get(&run_id).map(|handle| handle.cmd_tx.clone())
        };
        let Some(sender) = sender else {
            return;
        };
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if sender
            .send(ActorCommand::SendControl {
                request,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            log::warn!(
                "[pi_session_actor] compatibility control: actor {} died",
                run_id
            );
            return;
        }
        match reply_rx.await {
            Ok(Ok((_request_id, response_rx))) => {
                if tokio::time::timeout(std::time::Duration::from_secs(5), response_rx)
                    .await
                    .is_err()
                {
                    log::warn!(
                        "[pi_session_actor] compatibility control timed out for {}",
                        run_id
                    );
                }
            }
            Ok(Err(error)) => log::warn!(
                "[pi_session_actor] compatibility control failed for {}: {}",
                run_id,
                error
            ),
            Err(_) => log::warn!(
                "[pi_session_actor] compatibility control reply dropped for {}",
                run_id
            ),
        }
    });
}
