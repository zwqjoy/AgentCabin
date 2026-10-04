# Bundled Work MCP

Work uses Pi 1.0.2 native `createMcpExtension` and `createToolSearchExtension` through `work/pi_mcp_adapter.mjs`. Every connector is projected as an authenticated `127.0.0.1` HTTP server with `scope: extension`, `exposure: deferred`, and a 600-second timeout. The shim supplies its own `loadConfig`; Pi's default user/project MCP discovery is never used in Work. Config writes fail closed. Connector commands, endpoints, credentials, OAuth, sandboxing, SSE compatibility, policy and Inbox approval remain owned by the Rust Host.

`tool_search` discovers connected MCP tools and loads matching `mcp__<server>__<tool>` definitions. `work_discover_capabilities` discovers AgentCabin resources; `work_list_tools` and `work_activate_tools` continue to manage optional Work tools. The old `mcp` proxy and Pi-side adapter approval plane are removed. Retired package references are blocked in old profiles rather than installed.

The trusted `AGENTCABIN_PI_CODING_AGENT_ENTRY` comes from the bundled runtime locator and resolves the same Pi Host's `dist/index.js`. The extension closure must contain neither `pi-mcp-adapter` nor another Pi Host. Regenerate its lock with `npm install --package-lock-only --ignore-scripts --legacy-peer-deps --prefix src-tauri/runtime/extensions`; omitting `--legacy-peer-deps` can install extension peer dependencies as a second Host.

## Verification

```sh
npm run prepare:pi-extensions
npm run prepare:runtimes
npm run verify:runtimes
node scripts/smoke-work-native-mcp.mjs
cargo test --manifest-path src-tauri/Cargo.toml native_pi_search_calls -- --ignored --nocapture
npm run electron:package:dir
```

The deterministic smoke uses 58 fixture tools to verify the initial deferred surface, native search activation, preservation through Work's next agent-start event, direct calls, absence of resource tools, and resume. The explicit Cargo integration test runs bundled Node/Pi against the authenticated production Host bridge and real ToolPipeline, with exactly one Inbox approval for mutation before the real server is called. The regular Cargo suite also covers Host-only read and approval/resume behavior.

Pi 1.0.2's SDK initializes its default active-tool loadout when opening a session. Resume is tested by reopening Pi's native transcript, discovering tools again through native search, and calling them through the current bridge. AgentCabin does not add a tool-state cache or patch Pi to change this behavior. Work's core retains native tool activations within a running session. Host call IDs include the hashed process-scoped bridge token so the native client's restarted JSON-RPC counter does not collide with a previous Pi launch; retries within one process retain their identity.
