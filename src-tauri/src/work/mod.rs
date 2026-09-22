pub mod apps;
pub mod artifacts;
pub mod browser;
pub mod browser_operator;
pub mod capability_projection;
pub mod cli_runtime;
pub mod command_info;
pub mod connector_package;
pub mod connector_package_manager;
pub mod connectors;
pub mod context;
pub mod coordinator;
pub mod desktop_operator;
pub mod executor;
pub mod files;
pub mod goal;
pub mod guardian;
pub mod inbox;
pub mod input;
pub mod interaction;
pub mod internal_bridge;
pub mod ledger;
pub mod library;
pub mod lifecycle;
pub mod mcp;
pub mod models;
pub mod paths;
pub mod pipeline;
pub mod policy;
pub mod profile;
pub mod progress;
pub mod projection;
pub mod proxy;
pub mod receipt;
pub mod resources;
pub mod runtime;
pub mod sandbox;
pub mod scheduler;
pub mod session;
pub mod subagents;
pub mod system_packages;
pub mod task_state;
pub mod tasks;
pub mod traits;
pub mod twork_migration;
pub mod web;
// The WorkBench is an executable acceptance harness, not a product runtime
// dependency. Keep it available to Rust tests without shipping a production
// command or compiling the harness into the desktop binary.
#[cfg(test)]
pub mod workbench;
pub mod workspace;

pub mod e2e;

#[cfg(any(test, feature = "work-smoke"))]
pub mod smoke;
