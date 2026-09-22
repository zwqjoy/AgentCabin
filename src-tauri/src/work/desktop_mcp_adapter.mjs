// AgentCabin Computer Use V2 MCP projection for Code providers that do not
// load Pi extensions. State and scheduling stay in this session-local process;
// every live operation is forwarded to the authenticated Host bridge.

import { ComputerUseV2Session } from "./computer_use_v2_runtime.mjs";

const object = (properties, required = []) => ({ type: "object", properties, required, additionalProperties: false });
const stateId = { type: "string", description: "Immutable state owning each @e ref." };
const condition = {
  ref: { type: "string" },
  text: { type: "string" },
  role: { type: "string" },
  value: { type: "string" },
  until: { enum: ["present", "absent"] },
  timeoutMs: { type: "number", minimum: 100, maximum: 60000 },
};
const action = {
  oneOf: [
    object({ action: { const: "press" }, ref: { type: "string" } }, ["action", "ref"]),
    object({ action: { const: "click" }, ref: { type: "string" }, x: { type: "number" }, y: { type: "number" }, button: { enum: ["left", "right", "middle"] }, clickCount: { type: "integer", minimum: 1, maximum: 3 } }, ["action"]),
    object({ action: { const: "setText" }, ref: { type: "string" }, text: { type: "string" } }, ["action", "ref", "text"]),
    object({ action: { const: "typeText" }, ref: { type: "string" }, text: { type: "string" } }, ["action", "text"]),
    object({ action: { const: "keypress" }, ref: { type: "string" }, keys: { type: "array", items: { type: "string" }, minItems: 1 } }, ["action", "keys"]),
    object({ action: { const: "scroll" }, ref: { type: "string" }, scrollX: { type: "number" }, scrollY: { type: "number" } }, ["action"]),
  ],
};

const tools = [
  { name: "launch_app", description: "Launch a native application and return its first immutable UI state.", inputSchema: object({ name: { type: "string" }, bundleId: { type: "string" }, createsNewApplicationInstance: { type: "boolean" } }) },
  { name: "find_roots", description: "Find controllable desktop roots with stable root refs.", inputSchema: object({ text: { type: "string" }, app: { type: "string" } }) },
  { name: "observe_ui", description: "Observe one root and return a compact outline plus immutable stateId.", inputSchema: object({ root: { type: "string" }, mode: { enum: ["semantic", "visual", "fused"] } }) },
  { name: "search_ui", description: "Search a complete cached UI state.", inputSchema: object({ stateId, text: { type: "string" }, role: { type: "string" } }, ["stateId"]) },
  { name: "expand_ui", description: "Expand bounded context around one cached UI element.", inputSchema: object({ stateId, ref: { type: "string" }, depth: { type: "integer", minimum: 1, maximum: 8 } }, ["stateId", "ref"]) },
  { name: "inspect_ui", description: "Inspect one exact cached UI element.", inputSchema: object({ stateId, ref: { type: "string" } }, ["stateId", "ref"]) },
  { name: "act_ui", description: "Execute checked UI actions and return the successor state.", inputSchema: object({ stateId, actions: { type: "array", items: action, minItems: 1, maxItems: 20 }, expect: object(condition) }, ["stateId", "actions"]) },
  { name: "read_text", description: "Read text from one exact cached UI element.", inputSchema: object({ stateId, ref: { type: "string" }, offset: { type: "integer", minimum: 0 } }, ["stateId", "ref"]) },
  { name: "wait_for", description: "Wait for a scoped UI condition and return the successor state.", inputSchema: object({ stateId, ...condition }, ["stateId"]) },
];

const toolNames = new Set(tools.map((tool) => tool.name));
const port = Number(process.env.AGENTCABIN_DESKTOP_BRIDGE_PORT || 0);
const token = String(process.env.AGENTCABIN_DESKTOP_BRIDGE_TOKEN || "").trim();

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function textResult(text, isError = false) {
  return { isError, content: [{ type: "text", text: String(text) }] };
}

async function callDesktop(toolCallId, name, args, signal) {
  if (!port || !token) return { success: false, stderr: "AgentCabin Computer Use runtime is not configured for this session." };
  try {
    const response = await fetch(`http://127.0.0.1:${port}/internal/desktop/call`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ toolCallId, method: name, params: args || {} }),
      signal,
    });
    const payload = await response.json();
    return response.ok ? payload : { success: false, stderr: payload?.error || `Computer Use runtime failed (${response.status}).` };
  } catch (error) {
    return { success: false, stderr: error instanceof Error ? error.message : String(error) };
  }
}

const browserPort = Number(process.env.AGENTCABIN_BROWSER_BRIDGE_PORT || 0);
const browserToken = String(process.env.AGENTCABIN_BROWSER_BRIDGE_TOKEN || "").trim();

async function callBrowser(toolCallId, toolName, params, signal) {
  if (!browserPort || !browserToken) {
    throw new Error("Shared Browser Runtime is not configured for this session.");
  }
  const response = await fetch(`http://127.0.0.1:${browserPort}/internal/browser/call`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${browserToken}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({ toolCallId, method: toolName, params: params || {} }),
    signal,
  });
  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload?.error || `Browser Runtime failed (${response.status}).`);
  }
  if (payload?.stdout && typeof payload.stdout === "string") {
    try {
      return JSON.parse(payload.stdout);
    } catch {
      return payload;
    }
  }
  return payload;
}

const session = new ComputerUseV2Session(
  (toolCallId, toolName, _action, params, signal) => callDesktop(toolCallId, toolName, params, signal),
  {
    invokeBrowser: (browserPort && browserToken) ? callBrowser : undefined,
  }
);
const handlers = {
  launch_app: session.launchApp,
  find_roots: session.findRoots,
  observe_ui: session.observeUi,
  search_ui: session.searchUi,
  expand_ui: session.expandUi,
  inspect_ui: session.inspectUi,
  act_ui: session.actUi,
  read_text: session.readText,
  wait_for: session.waitFor,
};

async function handle(request) {
  const method = request?.method;
  const id = request?.id;
  if (method === "notifications/initialized") return;
  if (method === "initialize") {
    write({ jsonrpc: "2.0", id, result: { protocolVersion: request?.params?.protocolVersion || "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "AgentCabin Computer Use", version: "2.0.0" } } });
    return;
  }
  if (method === "ping") {
    write({ jsonrpc: "2.0", id, result: {} });
    return;
  }
  if (method === "tools/list") {
    write({ jsonrpc: "2.0", id, result: { tools } });
    return;
  }
  if (method === "tools/call") {
    const name = request?.params?.name;
    if (!toolNames.has(name)) {
      write({ jsonrpc: "2.0", id, result: textResult(`Unknown Computer Use tool: ${name}`, true) });
      return;
    }
    const output = await handlers[name].call(session, id == null ? null : String(id), request?.params?.arguments || {}, undefined);
    write({ jsonrpc: "2.0", id, result: { isError: output?.details?.ok === false, content: output?.content || [], structuredContent: output?.details || {} } });
    return;
  }
  if (id !== undefined) write({ jsonrpc: "2.0", id, error: { code: -32601, message: `Unsupported MCP method: ${method}` } });
}

let buffered = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffered += chunk;
  let newline;
  while ((newline = buffered.indexOf("\n")) >= 0) {
    const line = buffered.slice(0, newline).trim();
    buffered = buffered.slice(newline + 1);
    if (!line) continue;
    let request;
    try {
      request = JSON.parse(line);
    } catch {
      continue;
    }
    void handle(request).catch((error) => {
      if (request.id !== undefined) write({ jsonrpc: "2.0", id: request.id, error: { code: -32000, message: String(error) } });
    });
  }
});
