//! Headless AgentCabin Core — host-agnostic runtime extracted from the Tauri shell.
//!
//! `CoreRuntime` owns the same shared state the Tauri `manage()` block wires up
//! (ProcessMap, ActorSessionMap, EventWriter, …) and exposes it through a
//! loopback-only HTTP server so Electron (P4) can connect without going through
//! Tauri.
//!
//! Tauri continues to work unchanged; this module is the parallel entry point.

pub mod runtime;
pub mod server;

pub use runtime::CoreRuntime;
