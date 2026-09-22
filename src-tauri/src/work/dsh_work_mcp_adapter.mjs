// AgentCabin Work tool bridge for DSH's MCP client.
//
// DSH owns the model loop, but Work owns Workspace paths, policy, approvals,
// Inbox, artifacts, apps, and configured MCP servers. This process only speaks
// MCP over stdio and forwards every consequential operation through the
// authenticated Work bridge.

import { ComputerUseV2Session } from "./computer_use_v2_runtime.mjs";

const objectSchema = (properties = {}, required = []) => ({
  type: "object",
  properties,
  ...(required.length ? { required } : {}),
  additionalProperties: false,
});

const string = (description = "") => ({ type: "string", ...(description ? { description } : {}) });
const integer = (description = "") => ({ type: "integer", ...(description ? { description } : {}) });

export const tools = [
  { name: "work_workspace_info", description: "Inspect the current Work Workspace boundary and its read/write areas.", inputSchema: objectSchema() },
  { name: "work_list_tools", description: "List the Work tools available in this DSH session.", inputSchema: objectSchema({ intent: string("Optional task intent used to filter the catalog.") }) },
  { name: "work_discover_capabilities", description: "Find Work tools relevant to an intent before using them.", inputSchema: objectSchema({ intent: string("The task intent to match."), max_results: integer("Maximum number of matches.") }, ["intent"]) },
  { name: "work_activate_tools", description: "Keep optional Work tools active for this DSH session. DSH exposes the safe Work catalog up front.", inputSchema: objectSchema({ names: { type: "array", items: string() }, mode: { type: "string", enum: ["merge", "replace"] } }, ["names"]) },
  { name: "ask_questions", description: "Ask the root user structured questions and wait for the durable Inbox answer.", inputSchema: objectSchema({ title: string(), questions: { type: "array", minItems: 1, items: { type: "object", additionalProperties: true } } }, ["questions"]) },
  { name: "work_set_goal", description: "Set the durable objective for the current Work Run.", inputSchema: objectSchema({ goal: string() }, ["goal"]) },
  { name: "work_replace_plan", description: "Replace the durable Work execution plan.", inputSchema: objectSchema({ steps: { type: "array", items: { type: "object", additionalProperties: true } } }, ["steps"]) },
  { name: "work_update_step", description: "Update one durable Work plan step.", inputSchema: objectSchema({ id: string(), status: { type: "string", enum: ["pending", "in_progress", "completed"] }, text: string() }, ["id", "status"]) },
  { name: "work_save_checkpoint", description: "Save a durable Work checkpoint for Resume and Continue.", inputSchema: objectSchema({ summary: string(), current_step_id: string() }, ["summary"]) },
  { name: "work_read_file", description: "Read a UTF-8 file through the Work Host path boundary.", inputSchema: objectSchema({ path: string(), max_chars: integer() }, ["path"]) },
  { name: "work_list_files", description: "List files through the Work Host path boundary.", inputSchema: objectSchema({ path: string(), area: string(), prefix: string(), max_entries: integer() }) },
  { name: "work_propose_context_update", description: "Propose a Workspace knowledge update; Work asks for explicit confirmation before saving it.", inputSchema: objectSchema({ path: string(), content: string(), overwrite: { type: "boolean" } }, ["path", "content"]) },
  { name: "work_write_file", description: "Write a UTF-8 file through the Work Host policy and approval boundary.", inputSchema: objectSchema({ path: string(), content: string(), overwrite: { type: "boolean" } }, ["path", "content"]) },
  { name: "work_edit_file", description: "Apply one exact text replacement through the Work Host policy boundary.", inputSchema: objectSchema({ path: string(), old_text: string(), new_text: string() }, ["path", "old_text", "new_text"]) },
  { name: "work_register_artifact", description: "Register an output file as a Work Artifact.", inputSchema: objectSchema({ path: string(), title: string(), artifact_type: string() }, ["path"]) },
  { name: "work_list_artifacts", description: "List registered Work Artifacts and lifecycle status.", inputSchema: objectSchema() },
  { name: "work_validate_artifact", description: "Validate a registered Work Artifact.", inputSchema: objectSchema({ id: string(), path: string() }) },
  { name: "library_list", description: "List durable Library entries available to the Workspace.", inputSchema: objectSchema({ collection: string(), category: string(), max_results: integer() }) },
  { name: "library_search", description: "Search durable Library entries available to the Workspace.", inputSchema: objectSchema({ query: string(), collection: string(), max_results: integer() }, ["query"]) },
  { name: "library_read", description: "Read a durable Library entry through the Work Host.", inputSchema: objectSchema({ id: string(), max_chars: integer() }, ["id"]) },
  { name: "work_deliver", description: "Mark a validated output Artifact as delivered.", inputSchema: objectSchema({ id: string(), path: string() }) },
  { name: "work_command_info", description: "Preflight a host command before running it.", inputSchema: objectSchema({ command: string() }, ["command"]) },
  { name: "work_run_command", description: "Run a non-GUI argv command through the Work Host command policy and approval boundary. Never use this for desktop interaction; use Computer Use tools instead.", inputSchema: objectSchema({ command: string(), args: { type: "array", items: string() }, cwd: string(), timeout_seconds: integer(), expected_outputs: { type: "array", items: string() } }, ["command"]) },
  { name: "work_run_connector_cli", description: "Run one trusted Connector Package CLI operation through the Work Host.", inputSchema: objectSchema({ package_id: string(), operation: string(), args: { type: "array", items: string() } }, ["package_id", "operation"]) },
  { name: "work_request_directory_access", description: "Request access to an external absolute directory through Work Inbox.", inputSchema: objectSchema({ path: string(), writable: { type: "boolean" }, purpose: string() }, ["path"]) },
  { name: "work_list_apps", description: "List connected external apps and their available tools.", inputSchema: objectSchema() },
  { name: "work_call_app", description: "Call a connected external app through the Work Host MCP bridge.", inputSchema: objectSchema({ app_id: string(), tool_name: string(), arguments: { type: "object", additionalProperties: true }, account_id: string() }, ["app_id", "tool_name"]) },
  { name: "work_mcp_list_tools", description: "List tools exposed by one configured Work MCP server through the authenticated Work Bridge.", inputSchema: objectSchema({ server: string() }, ["server"]) },
  { name: "work_mcp_call", description: "Call one configured Work MCP tool through the authenticated Work Bridge. The Work Host remains responsible for server policy and approval.", inputSchema: objectSchema({ server: string(), name: string(), arguments: { type: "object", additionalProperties: true } }, ["server", "name"]) },
];

export const browserTools = [
  { name: "web_search", description: "Search the public web through Work's configured web provider.", inputSchema: objectSchema({ query: string(), max_results: integer() }, ["query"]) },
  { name: "web_open", description: "Fetch a public web page through Work's configured web provider.", inputSchema: objectSchema({ url: string() }, ["url"]) },
  { name: "browser_navigate", description: "Navigate the active Work browser page. Generated HTML may use file:// only when it is under the current WorkRun output/ directory; arbitrary local files and private network URLs remain blocked.", inputSchema: objectSchema({ url: string() }, ["url"]) },
  { name: "browser_snapshot", description: "Capture the active Work browser page accessibility tree.", inputSchema: objectSchema() },
  { name: "browser_take_screenshot", description: "Capture the active Work browser page screenshot.", inputSchema: objectSchema({ filename: string(), full_page: { type: "boolean" } }) },
  { name: "browser_wait_for", description: "Wait for the active Work browser page.", inputSchema: objectSchema({ ms: { type: "number" }, load_state: string() }) },
  { name: "browser_tabs", description: "List or manage Work browser tabs.", inputSchema: objectSchema({ action: string(), index: integer(), url: string() }, ["action"]) },
  { name: "browser_close", description: "Close the active Work browser context.", inputSchema: objectSchema() },
  { name: "browser_click", description: "Click an active Work browser element by ref or selector.", inputSchema: objectSchema({ ref: string(), selector: string(), button: string(), double_click: { type: "boolean" } }) },
  { name: "browser_type", description: "Type into an active Work browser element.", inputSchema: objectSchema({ ref: string(), selector: string(), text: string(), clear: { type: "boolean" }, press_enter: { type: "boolean" } }, ["text"]) },
  { name: "browser_select_option", description: "Select an option in the active Work browser page.", inputSchema: objectSchema({ ref: string(), selector: string(), value: string() }, ["value"]) },
  { name: "browser_scroll", description: "Scroll the active Work browser page.", inputSchema: objectSchema({ direction: string(), amount: integer(), ref: string() }) },
];

export const desktopTools = [
  { name: "launch_app", description: "Launch a native application and return its first immutable UI state.", inputSchema: objectSchema({ name: string(), bundleId: string(), createsNewApplicationInstance: { type: "boolean" } }) },
  { name: "find_roots", description: "Find a bounded set of controllable desktop roots.", inputSchema: objectSchema({ text: string(), app: string() }) },
  { name: "observe_ui", description: "Observe one desktop root and return an immutable UI state.", inputSchema: objectSchema({ root: string(), mode: { type: "string", enum: ["semantic", "visual", "fused"] } }) },
  { name: "search_ui", description: "Search a cached desktop UI state.", inputSchema: objectSchema({ stateId: string(), text: string(), role: string() }, ["stateId"]) },
  { name: "expand_ui", description: "Expand bounded context around a cached UI element.", inputSchema: objectSchema({ stateId: string(), ref: string(), depth: integer() }, ["stateId", "ref"]) },
  { name: "inspect_ui", description: "Inspect one exact element in a cached UI state.", inputSchema: objectSchema({ stateId: string(), ref: string() }, ["stateId", "ref"]) },
  { name: "act_ui", description: "Execute checked dependent UI actions and return the successor state.", inputSchema: objectSchema({ stateId: string(), actions: { type: "array", items: { type: "object", additionalProperties: true }, minItems: 1, maxItems: 20 }, expect: { type: "object", additionalProperties: true } }, ["stateId", "actions"]) },
  { name: "read_text", description: "Read text from an exact element in an immutable UI state.", inputSchema: objectSchema({ stateId: string(), ref: string(), offset: integer() }, ["stateId", "ref"]) },
  { name: "wait_for", description: "Wait for a scoped desktop UI condition and return the successor state.", inputSchema: objectSchema({ stateId: string(), ref: string(), text: string(), role: string(), value: string(), until: { type: "string", enum: ["present", "absent"] }, timeoutMs: integer() }, ["stateId"]) },
];

const port = Number(process.env.AGENTCABIN_WORK_BRIDGE_PORT || 0);
const token = String(process.env.AGENTCABIN_WORK_BRIDGE_TOKEN || "").trim();
const baseUrl = port > 0 ? `http://127.0.0.1:${port}` : "";
const browserEnabled = process.env.AGENTCABIN_WORK_BROWSER_ENABLED === "1";
const browserUseEnabled = process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED === "1";
const desktopUseEnabled = process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED === "1";
const workspaceRoot = String(process.env.AGENTCABIN_WORKSPACE_ROOT || ".").trim() || ".";

let externalMcpTools = [];
let externalMcpByName = new Map();
let computerUseSession;
let computerUseTaskActive = false;

function getComputerUseSession() {
  if (!computerUseSession) {
    computerUseSession = new ComputerUseV2Session(async (toolCallId, toolName, action, params, signal) => {
      return callPipeline(toolCallId, toolName, action, params, signal);
    });
  }
  return computerUseSession;
}

async function callComputerUseTool(name, args, id, signal) {
  computerUseTaskActive = true;
  const session = getComputerUseSession();
  const methodName = name.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
  const method = session[methodName];
  if (typeof method !== "function") throw new Error(`Unsupported Computer Use tool: ${name}`);
  return method.call(session, id, args || {}, signal);
}

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function mcpResult(text, isError = false, structuredContent = undefined) {
  const value = { isError, content: [{ type: "text", text: String(text) }] };
  if (structuredContent !== undefined) value.structuredContent = structuredContent;
  return value;
}

function bridgeConfigured() {
  return Boolean(baseUrl && token);
}

function isGuiAutomationCommand(args) {
  const joined = [args?.command, ...(Array.isArray(args?.args) ? args.args : [])]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return /(osascript|swiftc?|automator|cliclick|system events|axuielement|cgevent|nsevent|nsapplescript|accessibility api|calculator\.app|tell application|key code|keystroke)/i.test(joined);
}

async function bridgeRequest(path, method = "GET", body = undefined, signal = undefined) {
  if (!bridgeConfigured()) throw new Error("AgentCabin Work bridge is not configured for this DSH session.");
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, ...(body === undefined ? {} : { "content-type": "application/json" }) },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal,
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(payload?.error || `AgentCabin Work bridge failed (${response.status})`);
  return payload;
}

function interactionId(result) {
  return result?.interactionId || result?.interaction_id || result?.inboxItemId || result?.inbox_item_id || null;
}

async function waitForInbox(itemId, signal) {
  for (;;) {
    const state = await bridgeRequest(`/internal/work/inbox_status/${encodeURIComponent(itemId)}`, "GET", undefined, signal);
    const status = String(state?.status || "").toLowerCase();
    if (!["pending", "delivering"].includes(status)) return state;
    await new Promise((resolve, reject) => {
      const timer = setTimeout(resolve, 250);
      if (signal) {
        if (signal.aborted) {
          clearTimeout(timer);
          reject(new Error("Work tool call was cancelled."));
        } else {
          signal.addEventListener("abort", () => {
            clearTimeout(timer);
            reject(new Error("Work tool call was cancelled."));
          }, { once: true });
        }
      }
    });
  }
}

async function callPipeline(toolCallId, toolName, action, args, signal) {
  const body = {
    toolCallId: String(toolCallId ?? `dsh-work-${Date.now()}`),
    toolName,
    action,
    arguments: args || {},
  };
  let result = await bridgeRequest("/internal/work/tool_pipeline", "POST", body, signal);
  for (let attempt = 0; attempt < 3 && ["waiting_approval", "waiting_input"].includes(result?.status); attempt += 1) {
    const itemId = interactionId(result);
    if (!itemId) throw new Error(`${toolName} entered ${result.status} without a durable Inbox item.`);
    const resolution = await waitForInbox(itemId, signal);
    const status = String(resolution?.status || "").toLowerCase();
    if (!["approved", "answered", "resolved"].includes(status)) {
      throw new Error(`${toolName} was ${status || "cancelled"} in Work Inbox.`);
    }
    result = await bridgeRequest("/internal/work/tool_pipeline", "POST", body, signal);
  }
  return result;
}

function toolText(result) {
  if (result?.success === true) return result.stdout || (result.outputs?.length ? JSON.stringify(result.outputs) : "(empty)");
  return result?.stderr || result?.error || result?.status || "Work tool failed";
}

function fromToolResult(result) {
  const text = toolText(result);
  return mcpResult(text, result?.success !== true, result);
}

function fromComputerUseResult(result) {
  const ok = result?.details?.ok === true;
  if (ok) {
    return {
      isError: false,
      content: result.content || [],
      structuredContent: result.structuredContent || result.details,
    };
  }
  const text = result?.content?.map((block) => block?.text).filter(Boolean).join("\n") || "Computer Use tool failed";
  return mcpResult(text, true, result);
}

function localWorkspaceInfo() {
  const areas = Object.fromEntries(["input", "scratch", "output", "context"].map((area) => [area, `${workspaceRoot}/${area}`]));
  return { ok: true, root: workspaceRoot, areas, note: "路径、策略、审批和成果状态由 Work Host 维护。" };
}

export function localCatalog() {
  const webTools = browserTools.filter((tool) => !tool.name.startsWith("browser_"));
  const browserOperatorTools = browserTools.filter((tool) => tool.name.startsWith("browser_"));
  return [...tools, ...(browserEnabled ? webTools : []), ...(browserUseEnabled ? browserOperatorTools : []), ...(desktopUseEnabled ? desktopTools : []), ...externalMcpTools];
}

function filterCatalog(intent) {
  const query = String(intent || "").trim().toLowerCase();
  const catalog = localCatalog();
  return query ? catalog.filter((tool) => `${tool.name} ${tool.description}`.toLowerCase().includes(query)) : catalog;
}

function normalizeExternalName(serverKey, toolName) {
  return `mcp_${serverKey}_${toolName}`.replace(/[^A-Za-z0-9_-]/g, "_");
}

async function refreshExternalMcpTools(signal) {
  externalMcpTools = [];
  externalMcpByName = new Map();
  let servers = [];
  try { servers = JSON.parse(process.env.AGENTCABIN_WORK_MCP_SERVERS || "[]"); } catch { servers = []; }
  for (const entry of Array.isArray(servers) ? servers : []) {
    const serverKey = String(entry?.key || "").trim();
    const server = String(entry?.server || "").trim();
    if (!serverKey || !server) continue;
    try {
      const response = await bridgeRequest(`/internal/work/mcp/${encodeURIComponent(server)}`, "POST", { jsonrpc: "2.0", id: `list-${serverKey}`, method: "tools/list", params: {} }, signal);
      const listed = response?.result?.tools;
      if (!Array.isArray(listed)) continue;
      for (const item of listed) {
        const rawName = String(item?.name || "").trim();
        if (!rawName) continue;
        const publicName = normalizeExternalName(serverKey, rawName);
        const tool = {
          name: publicName,
          description: `[MCP ${server}] ${String(item?.description || rawName)}`,
          inputSchema: item?.inputSchema || objectSchema({}, []),
        };
        externalMcpTools.push(tool);
        externalMcpByName.set(publicName, { server, rawName });
      }
    } catch {
      // External MCP is optional. Core Work tools remain available when one server is down.
    }
  }
}

async function callExternalMcp(name, args, id, signal) {
  const target = externalMcpByName.get(name);
  if (!target) throw new Error(`Unknown Work MCP tool: ${name}`);
  const response = await bridgeRequest(`/internal/work/mcp/${encodeURIComponent(target.server)}`, "POST", {
    jsonrpc: "2.0",
    id: String(id ?? `call-${Date.now()}`),
    method: "tools/call",
    params: { name: target.rawName, arguments: args || {} },
  }, signal);
  if (response?.error) throw new Error(response.error.message || "Work MCP call failed");
  return response?.result || mcpResult(JSON.stringify(response));
}

async function listMcpTools(server, signal) {
  const key = String(server || "").trim();
  if (!key) throw new Error("MCP server is required.");
  const response = await bridgeRequest(`/internal/work/mcp/${encodeURIComponent(key)}`, "POST", {
    jsonrpc: "2.0",
    id: `list-${Date.now()}`,
    method: "tools/list",
    params: {},
  }, signal);
  if (response?.error) throw new Error(response.error.message || "Work MCP tools/list failed");
  return response?.result || { tools: [] };
}

async function callMcpTool(server, name, args, id, signal) {
  const serverKey = String(server || "").trim();
  const toolName = String(name || "").trim();
  if (!serverKey || !toolName) throw new Error("MCP server and tool name are required.");
  const response = await bridgeRequest(`/internal/work/mcp/${encodeURIComponent(serverKey)}`, "POST", {
    jsonrpc: "2.0",
    id: String(id ?? `call-${Date.now()}`),
    method: "tools/call",
    params: { name: toolName, arguments: args || {} },
  }, signal);
  if (response?.error) throw new Error(response.error.message || "Work MCP tools/call failed");
  return response?.result || mcpResult(JSON.stringify(response));
}

async function callApp(args, id, signal) {
  const payload = await bridgeRequest("/internal/work/apps/call", "POST", {
    appId: String(args?.app_id || "").trim(),
    toolName: String(args?.tool_name || "").trim(),
    arguments: args?.arguments || {},
    accountId: args?.account_id || null,
    toolUseId: String(id ?? `app-${Date.now()}`),
  }, signal);
  if (payload?.status === "success") return mcpResult(JSON.stringify(payload.result ?? payload), false, payload);
  return mcpResult(payload?.message || payload?.error || "External app call was not completed.", true, payload);
}

export async function callTool(name, args, id, signal) {
  if (desktopUseEnabled && name === "work_run_command" && isGuiAutomationCommand(args)) {
    return mcpResult(
      "GUI automation commands are not allowed through work_run_command. Use AgentCabin Computer Use tools for desktop interaction.",
      true,
    );
  }
  if (name === "work_run_command" && computerUseTaskActive) {
    return mcpResult(
      "Desktop Computer Use is active for this Work session. Do not use work_run_command as a GUI fallback; report the Computer Use error instead.",
      true,
    );
  }
  if (name === "work_workspace_info") return mcpResult(JSON.stringify(localWorkspaceInfo(), null, 2), false, localWorkspaceInfo());
  if (name === "work_list_tools") {
    await refreshExternalMcpTools(signal);
    const listed = filterCatalog(args?.intent).map((tool) => ({ name: tool.name, description: tool.description }));
    return mcpResult(JSON.stringify({ tools: listed }, null, 2), false, { tools: listed });
  }
  if (name === "work_discover_capabilities") {
    await refreshExternalMcpTools(signal);
    const matches = filterCatalog(args?.intent).slice(0, Math.min(Number(args?.max_results || 20), 100));
    return mcpResult(JSON.stringify(matches, null, 2), false, { matches });
  }
  if (name === "work_activate_tools") return mcpResult(JSON.stringify({ ok: true, active: localCatalog().map((tool) => tool.name), accepted: args?.names || [], missing: [] }), false);
  if (name === "work_set_goal") {
    const payload = await bridgeRequest("/internal/work/task_state/update", "POST", { goal: args?.goal }, signal);
    return mcpResult(JSON.stringify(payload?.workTaskState || payload?.work_task_state || payload), false, payload);
  }
  if (name === "work_replace_plan") {
    const payload = await bridgeRequest("/internal/work/task_state/update", "POST", { plan: Array.isArray(args?.steps) ? args.steps : [] }, signal);
    return mcpResult(JSON.stringify(payload?.workTaskState || payload?.work_task_state || payload), false, payload);
  }
  if (name === "work_update_step") {
    const payload = await bridgeRequest("/internal/work/task_state/update", "POST", { step: { id: args?.id, status: args?.status, text: args?.text } }, signal);
    return mcpResult(JSON.stringify(payload?.workTaskState || payload?.work_task_state || payload), false, payload);
  }
  if (name === "work_save_checkpoint") {
    const payload = await bridgeRequest("/internal/work/task_state/update", "POST", { checkpoint: { summary: args?.summary, currentStepId: args?.current_step_id || null, createdAt: new Date().toISOString() } }, signal);
    return mcpResult(JSON.stringify(payload?.workTaskState || payload?.work_task_state || payload), false, payload);
  }
  if (name === "work_list_files") {
    const payload = await bridgeRequest("/internal/work/files/list", "POST", { path: args?.path, area: args?.area, prefix: args?.prefix, maxEntries: args?.max_entries }, signal);
    return mcpResult(JSON.stringify(payload?.files || payload, null, 2), false, payload);
  }
  if (name === "work_list_apps") {
    const payload = await bridgeRequest("/internal/work/apps/list", "GET", undefined, signal);
    return mcpResult(JSON.stringify({ apps: payload?.apps || [], tools: payload?.tools || [] }, null, 2), false, payload);
  }
  if (name === "work_call_app") return callApp(args, id, signal);
  if (name === "work_mcp_list_tools") {
    const result = await listMcpTools(args?.server, signal);
    return mcpResult(JSON.stringify(result, null, 2), false, result);
  }
  if (name === "work_mcp_call") {
    return callMcpTool(args?.server, args?.name, args?.arguments, id, signal);
  }
  if (name === "web_search") {
    const payload = await bridgeRequest("/internal/work/web/search", "POST", { query: String(args?.query || "").trim(), maxResults: args?.max_results, toolUseId: String(id ?? `web-${Date.now()}`) }, signal);
    return mcpResult(JSON.stringify(payload?.results ?? payload, null, 2), payload?.ok === false, payload);
  }
  if (name === "web_open") {
    const payload = await bridgeRequest("/internal/work/web/fetch", "POST", { url: String(args?.url || "").trim(), toolUseId: String(id ?? `web-${Date.now()}`) }, signal);
    return mcpResult(payload?.text ? `${payload.title || payload.finalUrl || args?.url || ""}\n\n${payload.text}` : JSON.stringify(payload), payload?.ok === false, payload);
  }
  if (["launch_app", "find_roots", "observe_ui", "search_ui", "expand_ui", "inspect_ui", "act_ui", "read_text", "wait_for"].includes(name)) {
    return fromComputerUseResult(await callComputerUseTool(name, args, id, signal));
  }
  if (name.startsWith("browser_")) {
    const result = await callPipeline(id, name, name.replace(/^.*?_/, ""), args, signal);
    return fromToolResult(result);
  }
  if (externalMcpByName.has(name)) return callExternalMcp(name, args, id, signal);

  const action = name === "work_run_connector_cli" ? String(args?.operation || "run") : ({
    ask_questions: "ask",
    work_propose_context_update: "update",
    work_write_file: "write",
    work_edit_file: "edit",
    work_register_artifact: "register",
    work_list_artifacts: "list",
    work_validate_artifact: "validate",
    library_list: "list",
    library_search: "search",
    library_read: "read",
    work_deliver: "deliver",
    work_command_info: "inspect",
    work_run_command: "run",
    work_request_directory_access: "request_access",
  }[name] || "run");
  const result = await callPipeline(id, name, action, args, signal);
  return fromToolResult(result);
}

async function handle(request) {
  const method = request?.method;
  const id = request?.id;
  if (method === "notifications/initialized") return;
  if (method === "initialize") {
    write({ jsonrpc: "2.0", id, result: { protocolVersion: request?.params?.protocolVersion || "2024-11-05", capabilities: { tools: { listChanged: false } }, serverInfo: { name: "AgentCabin Work", version: "1.0.0" } } });
    return;
  }
  if (method === "ping") { write({ jsonrpc: "2.0", id, result: {} }); return; }
  if (method === "tools/list") {
    await refreshExternalMcpTools();
    write({ jsonrpc: "2.0", id, result: { tools: localCatalog() } });
    return;
  }
  if (method === "tools/call") {
    try {
      await refreshExternalMcpTools();
      const name = String(request?.params?.name || "");
      const available = new Set(localCatalog().map((tool) => tool.name));
      if (!available.has(name)) throw new Error(`Unknown AgentCabin Work tool: ${name}`);
      write({ jsonrpc: "2.0", id, result: await callTool(name, request?.params?.arguments || {}, id) });
    } catch (error) {
      write({ jsonrpc: "2.0", id, result: mcpResult(error instanceof Error ? error.message : String(error), true) });
    }
    return;
  }
  if (id !== undefined) write({ jsonrpc: "2.0", id, error: { code: -32601, message: `Unsupported MCP method: ${method}` } });
}

// The same bridge core is imported by the native Cordis Work plugin. Only a
// dedicated MCP stdio process owns stdin; importing this module must be side
// effect free so DSH can register the native tools in its own process.
if (process.env.AGENTCABIN_WORK_MCP_STDIO === "1") {
  let buffered = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => {
    buffered += chunk;
    let newline;
    while ((newline = buffered.indexOf("\n")) >= 0) {
      const line = buffered.slice(0, newline).trim();
      buffered = buffered.slice(newline + 1);
      if (!line) continue;
      try { void handle(JSON.parse(line)); } catch { /* malformed MCP input is ignored */ }
    }
  });
}
