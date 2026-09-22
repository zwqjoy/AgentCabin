//! Grok Build ACP adapter for AgentCabin's shared session command surface.
//!
//! Grok speaks Agent Client Protocol JSON-RPC over stdio. It intentionally runs
//! in a dedicated actor so the existing Claude/Codex session actor remains
//! unchanged while the integration is validated.

mod actor_loop;
mod attachments;
mod capabilities;
mod commands;
pub(crate) mod config;
pub(crate) mod fork;
mod mcp;
mod models;
mod modes;
mod plugin_dirs;
pub(crate) mod process;
mod todos;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod tests;

pub use process::spawn_actor;
