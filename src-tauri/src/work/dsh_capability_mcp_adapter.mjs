// AgentCabin capability bridge for DSH's MCP client.
//
// DSH runs this file as a per-run stdio MCP server. The host keeps all
// credentials and policy decisions behind its authenticated loopback bridge;
// this adapter only translates MCP tool calls into those host calls.

const tools = [
  {
    name: "web_search",
    description: "Search the public web through AgentCabin's configured web provider.",
    inputSchema: { type: "object", properties: { query: { type: "string", minLength: 1 }, max_results: { type: "integer", minimum: 1, maximum: 20 } }, required: ["query"], additionalProperties: false },
  },
  {
    name: "web_fetch",
    description: "Fetch a public HTTP(S) page through AgentCabin's bounded web provider.",
    inputSchema: { type: "object", properties: { url: { type: "string", minLength: 1 } }, required: ["url"], additionalProperties: false },
  },
  {
    name: "work_run_connector_cli",
    description: "Run one trusted, enabled AgentCabin Connector Package operation.",
    inputSchema: { type: "object", properties: { package_id: { type: "string", minLength: 1 }, operation: { type: "string", minLength: 1 }, args: { type: "array", items: { type: "string" }, maxItems: 64 } }, required: ["package_id", "operation"], additionalProperties: false },
  },
  {
    name: "browser_navigate",
    description: "Navigate the active isolated browser page to a public HTTP(S) URL.",
    inputSchema: { type: "object", properties: { url: { type: "string", minLength: 1 } }, required: ["url"], additionalProperties: false },
  },
  {
    name: "browser_snapshot",
    description: "Capture the active browser page accessibility tree with stable element refs.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "browser_take_screenshot",
    description: "Capture a screenshot of the active browser page.",
    inputSchema: { type: "object", properties: { filename: { type: "string" }, full_page: { type: "boolean" } }, additionalProperties: false },
  },
  {
    name: "browser_wait_for",
    description: "Wait for the active browser page to load or for a bounded duration.",
    inputSchema: { type: "object", properties: { ms: { type: "number" }, load_state: { type: "string" } }, additionalProperties: false },
  },
  {
    name: "browser_tabs",
    description: "List, open, switch, or close isolated browser tabs.",
    inputSchema: { type: "object", properties: { action: { type: "string" }, index: { type: "number" }, url: { type: "string" } }, required: ["action"], additionalProperties: false },
  },
  {
    name: "browser_close",
    description: "Close the current isolated browser context.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "browser_click",
    description: "Click an element in the active browser page by semantic ref or selector.",
    inputSchema: { type: "object", properties: { ref: { type: "string" }, selector: { type: "string" }, button: { type: "string" }, double_click: { type: "boolean" } }, additionalProperties: false },
  },
  {
    name: "browser_type",
    description: "Type text into an active browser page input by semantic ref or selector.",
    inputSchema: { type: "object", properties: { ref: { type: "string" }, selector: { type: "string" }, text: { type: "string" }, clear: { type: "boolean" }, press_enter: { type: "boolean" } }, required: ["text"], additionalProperties: false },
  },
  {
    name: "browser_select_option",
    description: "Select an option in the active browser page.",
    inputSchema: { type: "object", properties: { ref: { type: "string" }, selector: { type: "string" }, value: { type: "string" } }, required: ["value"], additionalProperties: false },
  },
  {
    name: "browser_scroll",
    description: "Scroll the active browser page or a semantic container.",
    inputSchema: { type: "object", properties: { direction: { type: "string" }, amount: { type: "number" }, ref: { type: "string" } }, additionalProperties: false },
  },
];

const port = Number(process.env.AGENTCABIN_WORK_BRIDGE_PORT || 0);
const token = String(process.env.AGENTCABIN_WORK_BRIDGE_TOKEN || "").trim();
const connectorPort = Number(process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_PORT || 0);
const connectorToken = String(process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN || "").trim();
const browserPort = Number(process.env.AGENTCABIN_BROWSER_BRIDGE_PORT || 0);
const browserToken = String(process.env.AGENTCABIN_BROWSER_BRIDGE_TOKEN || "").trim();
const webEnabled = process.env.AGENTCABIN_WEB_ENABLED === "1";
const connectorEnabled = process.env.AGENTCABIN_CODE_CONNECTOR_ENABLED === "1";
const browserUseEnabled = process.env.AGENTCABIN_BROWSER_USE_ENABLED === "1";
const browserToolNames = new Set(tools.filter((tool) => tool.name.startsWith("browser_")).map((tool) => tool.name));
const advertisedTools = tools.filter((tool) => {
  if (browserToolNames.has(tool.name)) return browserUseEnabled;
  if (tool.name === "web_search" || tool.name === "web_fetch") return webEnabled;
  if (tool.name === "work_run_connector_cli") return connectorEnabled;
  return true;
});

function write(message) { process.stdout.write(`${JSON.stringify(message)}\n`); }
function result(text, isError = false, structuredContent = undefined) {
  const value = { isError, content: [{ type: "text", text: String(text) }] };
  if (structuredContent !== undefined) value.structuredContent = structuredContent;
  return value;
}

async function post(url, bearer, body, signal) {
  if (!bearer) throw new Error("AgentCabin capability bridge is not configured for this session.");
  const response = await fetch(url, {
    method: "POST",
    headers: { authorization: `Bearer ${bearer}`, "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok || payload?.ok === false) throw new Error(payload?.error || `AgentCabin bridge failed (${response.status})`);
  return payload;
}

async function callBrowser(name, args, id) {
  if (!browserPort || !browserToken) throw new Error("AgentCabin Browser Runtime is not configured for this session.");
  const payload = await post(`http://127.0.0.1:${browserPort}/internal/browser/call`, browserToken, {
    toolCallId: id == null ? undefined : String(id),
    method: name,
    params: args || {},
  });
  if (payload?.success !== true || payload?.status !== "success") {
    throw new Error(payload?.stderr || payload?.error || "Browser operation failed");
  }
  let output = payload?.stdout;
  if (typeof output === "string") {
    try { output = JSON.parse(output); } catch { /* preserve plain text */ }
  }
  return result(typeof output === "string" ? output : JSON.stringify(output ?? payload), false, payload);
}

async function call(name, args, id) {
  if (name === "web_search") {
    if (!port) throw new Error("AgentCabin Web Search bridge is not configured for this session.");
    const payload = await post(`http://127.0.0.1:${port}/internal/work/web/search`, token, {
      query: String(args?.query || "").trim(),
      maxResults: args?.max_results,
      toolUseId: id == null ? undefined : String(id),
    });
    const results = Array.isArray(payload.results) ? payload.results : payload.results?.results || payload.results;
    return result(JSON.stringify(results ?? payload), false, payload);
  }
  if (name === "web_fetch") {
    if (!port) throw new Error("AgentCabin Web Fetch bridge is not configured for this session.");
    const payload = await post(`http://127.0.0.1:${port}/internal/work/web/fetch`, token, {
      url: String(args?.url || "").trim(),
      toolUseId: id == null ? undefined : String(id),
    });
    return result(payload.text ? `${payload.title || payload.finalUrl || payload.url || ""}\n\n${payload.text}` : JSON.stringify(payload), false, payload);
  }
  if (name === "work_run_connector_cli") {
    if (!connectorPort || !connectorToken) throw new Error("AgentCabin Connector bridge is not configured for this session.");
    const payload = await post(`http://127.0.0.1:${connectorPort}/internal/code/connector_cli`, connectorToken, {
      toolCallId: id == null ? undefined : String(id),
      packageId: String(args?.package_id || "").trim(),
      operation: String(args?.operation || "").trim(),
      args: Array.isArray(args?.args) ? args.args.map(String) : [],
    });
    if (payload?.success !== true || payload?.status !== "success") {
      throw new Error(payload?.stderr || payload?.error || "Connector operation failed");
    }
    return result(payload.stdout || "(empty)", false, payload);
  }
  if (name?.startsWith("browser_")) return callBrowser(name, args, id);
  throw new Error(`Unknown AgentCabin capability tool: ${name}`);
}

async function handle(request) {
  const method = request?.method;
  const id = request?.id;
  if (method === "notifications/initialized") return;
  if (method === "initialize") {
    write({ jsonrpc: "2.0", id, result: { protocolVersion: request?.params?.protocolVersion || "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "AgentCabin Capabilities", version: "1.0.0" } } });
    return;
  }
  if (method === "ping") { write({ jsonrpc: "2.0", id, result: {} }); return; }
  if (method === "tools/list") { write({ jsonrpc: "2.0", id, result: { tools: advertisedTools } }); return; }
  if (method === "tools/call") {
    try {
      const name = request?.params?.name;
      if (!advertisedTools.some((tool) => tool.name === name)) throw new Error(`Unknown AgentCabin capability tool: ${name}`);
      write({ jsonrpc: "2.0", id, result: await call(name, request?.params?.arguments || {}, id) });
    } catch (error) {
      write({ jsonrpc: "2.0", id, result: result(error instanceof Error ? error.message : String(error), true) });
    }
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
    try { void handle(JSON.parse(line)); } catch { /* malformed input is ignored */ }
  }
});
