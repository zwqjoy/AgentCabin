//! DeepSeek Harness (DSH) session actor module.

pub mod acp_actor;
pub mod actor_loop;
pub mod normalizer;
pub mod process;
pub mod protocol;

pub use process::{spawn_actor, spawn_actor_with_protocol, DshProtocol};
