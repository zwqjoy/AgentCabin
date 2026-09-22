/**
 * Runtime-neutral Work tool metadata.
 *
 * A runtime registrar consumes this catalog and supplies the execute
 * functions. Keeping discovery/activation metadata here lets Pi, DSH, and
 * test registrars expose the same Work capability surface.
 */

const BASE_TOOL_CATALOG = [
  ["work_workspace_info", "Inspect the current Work Workspace boundary and its read/write areas.", "read", true],
  ["work_list_tools", "Discover Work tools and active resource summaries without loading large schemas.", "read", true],
  ["work_discover_capabilities", "Find active Work capabilities by the user's intent before loading or activating them.", "read", true],
  ["work_activate_tools", "Activate optional Work tools for the current session.", "read", true],
  ["ask_questions", "Ask the root user one or more structured questions and wait for their answers before continuing.", "user_input", true],
  ["work_set_goal", "Set the durable objective for the current Work Run.", "harness_state", true],
  ["work_replace_plan", "Replace the durable execution plan for the current Work Run.", "harness_state", true],
  ["work_update_step", "Update one durable Work plan step.", "harness_state", true],
  ["work_save_checkpoint", "Save a durable Work checkpoint for Resume and Continue.", "harness_state", true],
  ["work_read_file", "Read a UTF-8 file inside the Workspace, the read-only Work Profile skill tree, an authorized external directory, or any host path in FullAccess.", "read", true],
  ["work_list_files", "List files inside a Work Workspace area, the read-only Work Profile skill tree, an authorized external directory, or any host directory in FullAccess.", "read", true],
  ["work_propose_context_update", "Propose a Workspace knowledge update and save it only after explicit user confirmation.", "context_write", true],
  ["work_write_file", "Write a UTF-8 file to scratch, output, a writable authorized external directory, or any host path in FullAccess.", "local_write", true],
  ["work_edit_file", "Apply an exact text edit inside the Workspace, a writable authorized external directory, or any host path in FullAccess.", "local_write", true],
  ["work_register_artifact", "Register an output file as a Work Artifact.", "local_write", true],
  ["work_list_artifacts", "List registered Work Artifacts and their lifecycle status.", "read", true],
  ["work_validate_artifact", "Validate that a registered output Artifact still exists and is readable.", "read", true],
  ["library_list", "List durable Library entries available to the current Workspace.", "read", true],
  ["library_search", "Search durable Library entries available to the current Workspace.", "read", true],
  ["library_read", "Read a durable Library entry through the Host Work Tool Pipeline.", "read", true],
  ["work_deliver", "Mark a validated output Artifact as delivered.", "local_write", false],
  ["work_command_info", "Preflight a host command (soffice, python, git, node, etc.) to check installation status, resolved path, file type, version, and recommended execution channel before running it.", "read", true],
  ["work_run_command", "Execute a shell command (git, npm, node, python, cargo, etc.) inside the Workspace boundary or any host directory in FullAccess. Connector CLIs such as lark-cli must use work_run_connector_cli. Approval depends on the Work permission mode.", "exec", true],
  ["work_run_connector_cli", "Run one fixed operation from a trusted and enabled Connector Package CLI through the Work command policy. Package CLI credentials and raw secret-shaped output stay on the Host.", "external", true],
  ["work_request_directory_access", "Request authorized access to an external absolute directory on the host filesystem outside the Workspace. Never use for Workspace paths (input/, scratch/, output/, context/) or individual files; Workspace inputs are already directly readable.", "access_root_request", true],
  ["work_list_apps", "List all connected external apps (Gmail, Google Calendar, Google Drive, Slack, GitHub, Notion, etc.) and their available tools.", "read", true],
  ["work_call_app", "Execute a tool/action on a connected external app (e.g. Gmail, Google Calendar, Google Drive, Slack, GitHub, Notion) via Host MCP Bridge.", "external", true],
];

const MCP_TOOL_CATALOG = [
  ["mcp", "Use the configured external MCP connector proxy after the Work MCP adapter is loaded.", "external", true],
];

const BROWSER_TOOL_CATALOG = [
  ["web_search", "Search the web through the explicitly configured Work 网络访问 provider.", "external", true],
  ["web_open", "Fetch a public web page and save a private Work 网络访问 page snapshot.", "external", true],
  ["web_extract", "Extract query-relevant passages from a saved 网络访问 page snapshot.", "read", true],
  ["web_cite", "Create stable citations from 网络访问 page passages in the current run ledger; use page_id plus exact passage_ids, or pass a list of passage_ids when citing multiple opened pages.", "read", true],
];

const BROWSER_OPERATOR_TOOL_CATALOG = [
  ["browser_navigate", "Navigate active browser page to a public URL or a generated file:// HTML under the current WorkRun output/ directory.", "read", true],
  ["browser_snapshot", "Capture current page semantic accessibility tree with stable [ref=eX] identifiers.", "read", true],
  ["browser_take_screenshot", "Capture screenshot of current browser page.", "read", true],
  ["browser_wait_for", "Wait for duration or lifecycle load state in browser.", "read", true],
  ["browser_tabs", "Manage browser tabs (list, new, switch, close).", "read", true],
  ["browser_close", "Close browser context for current WorkRun.", "read", true],
  ["browser_click", "Click an interactive element by semantic ref or selector.", "local_write", true],
  ["browser_type", "Type text into an input field by semantic ref.", "local_write", true],
  ["browser_select_option", "Select option from dropdown by semantic ref.", "local_write", true],
  ["browser_scroll", "Scroll page or container element.", "read", true],
];

const DESKTOP_OPERATOR_TOOL_CATALOG = [
  ["launch_app", "Launch a native application and return its first immutable UI state.", "local_write", true],
  ["find_roots", "Find controllable desktop roots with stable root refs.", "read", true],
  ["observe_ui", "Observe one root and return an immutable stateId with a compact UI outline.", "read", true],
  ["search_ui", "Search the complete cached UI state.", "read", true],
  ["expand_ui", "Expand bounded local context around one cached UI element.", "read", true],
  ["inspect_ui", "Inspect one exact cached UI element.", "read", true],
  ["act_ui", "Execute checked UI actions from one state and return the successor state.", "local_write", true],
  ["read_text", "Read text owned by an immutable UI state.", "read", true],
  ["wait_for", "Wait for a scoped UI condition and return the successor state.", "read", true],
  // Private native backend methods remain pipeline-addressable for the V2 adapter, but are
  // no longer activated as model-facing tools.
  ["desktop_list_apps", "Native backend: list desktop windows.", "read", false],
  ["desktop_probe_app", "Native backend: probe an application.", "read", false],
  ["desktop_open_app", "Native backend: open an application.", "local_write", false],
  ["desktop_observe", "Native backend: observe a desktop window.", "read", false],
  ["desktop_screenshot", "Native backend: capture a window.", "read", false],
  ["desktop_click", "Native backend: click.", "local_write", false],
  ["desktop_type", "Native backend: type text.", "local_write", false],
  ["desktop_key", "Native backend: press keys.", "local_write", false],
  ["desktop_scroll", "Native backend: scroll.", "local_write", false],
  ["desktop_act_batch", "Native backend: execute one checked UI transaction.", "local_write", false],
  ["desktop_release", "Native backend: release its lease.", "read", false],
];

const SUBAGENT_TOOL_CATALOG = [
  ["work_delegate", "Delegate an independent task to a focused Work child subagent.", "delegation", true],
  ["work_research_swarm", "Launch 2-3 parallel read-only researcher subagents for orthogonal investigation dimensions and collect all findings.", "delegation", true],
  ["work_implement_review_fix", "Orchestrate a bounded implementation, independent review, and optional fix loop with worker and reviewer subagents.", "delegation", true],
  ["work_agent_wait", "Wait for a Work child subagent and retrieve its result.", "delegation", true],
  ["work_agent_status", "Inspect Work child subagent status and progress.", "read", true],
  ["work_agent_steer", "Send guidance to a running Work child subagent.", "delegation", true],
  ["work_agent_stop", "Stop a running Work child subagent.", "delegation", true],
];

function materialize(entries) {
  return entries.map(([name, description, effect, defaultActive]) => ({
    name,
    description,
    effect,
    default_active: defaultActive,
  }));
}

export function createWorkToolCatalog({
  mcpEnabled = process.env.AGENTCABIN_WORK_MCP_ENABLED === "1",
  browserEnabled = process.env.AGENTCABIN_WORK_BROWSER_ENABLED === "1",
  browserUseEnabled = process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED === "1",
  desktopUseEnabled = process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED === "1",
  subagentChild = process.env.PI_SUBAGENT_CHILD === "1",
} = {}) {
  return [
    ...materialize(BASE_TOOL_CATALOG),
    ...(browserUseEnabled ? materialize(BROWSER_OPERATOR_TOOL_CATALOG) : []),
    ...(desktopUseEnabled ? materialize(DESKTOP_OPERATOR_TOOL_CATALOG) : []),
    ...(mcpEnabled ? materialize(MCP_TOOL_CATALOG) : []),
    ...(browserEnabled ? materialize(BROWSER_TOOL_CATALOG) : []),
    ...(subagentChild ? [] : materialize(SUBAGENT_TOOL_CATALOG)),
  ];
}

export function createDefaultActiveWorkTools(options = {}) {
  return new Set(
    createWorkToolCatalog(options)
      .filter((tool) => tool.default_active)
      .map((tool) => tool.name),
  );
}

export function createWorkToolDefinition(entry, execute, parameters, overrides = {}) {
  if (!entry?.name || typeof execute !== "function") {
    throw new TypeError("A Work tool definition requires a catalog entry and execute handler.");
  }
  return {
    ...overrides,
    ...entry,
    label: overrides.label || entry.name,
    parameters: parameters ?? overrides.parameters,
    execute,
  };
}

export const WORK_COMPAT_TOOL_NAMES = Object.freeze(["read", "write", "edit", "bash"]);
