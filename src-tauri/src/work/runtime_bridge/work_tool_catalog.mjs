/**
 * Host capability metadata for the Pi Work extension.
 *
 * Pi supplies tool registration; the Host retains execution authority.
 * This catalog centralizes discovery and activation, not runtime selection.
 */

const BASE_TOOL_CATALOG = [
  ["work_workspace_info", "Inspect the current Work Workspace boundary and its read/write areas.", "read", true],
  ["work_list_tools", "Discover Work tools and active resource summaries without loading large schemas.", "read", true],
  ["work_discover_capabilities", "Find active Work capabilities by the user's intent before loading or activating them.", "read", true],
  ["work_activate_tools", "Activate optional Work tools for the current session.", "read", true],
  ["ask_questions", "Ask the root user one or more structured questions and wait for their answers before continuing.", "user_input", true],
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

const BROWSER_TOOL_CATALOG = [
  ["web_search", "Search the web through the explicitly configured Work 网络访问 provider.", "external", true],
  ["web_open", "Fetch a public web page and save a private Work 网络访问 page snapshot.", "external", true],
  ["web_extract", "Extract query-relevant passages from a saved 网络访问 page snapshot.", "read", true],
  ["web_cite", "Create stable citations from 网络访问 page passages in the current run ledger; use page_id plus exact passage_ids, or pass a list of passage_ids when citing multiple opened pages.", "read", true],
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
  browserEnabled = process.env.AGENTCABIN_WORK_BROWSER_ENABLED === "1",
} = {}) {
  return [
    ...materialize(BASE_TOOL_CATALOG),
    ...(browserEnabled ? materialize(BROWSER_TOOL_CATALOG) : []),
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

// Pi built-in names are confined aliases, not historical runtime shims.
export const WORK_COMPAT_TOOL_NAMES = Object.freeze(["read", "write", "edit", "bash"]);
