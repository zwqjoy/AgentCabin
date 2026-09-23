/**
 * AgentCabin's DSH transport bridge.
 *
 * The child process keeps the ACP-shaped stdio contract used by the native
 * actor, while the actual session is owned by the official DSH Web Harness
 * and its Session Controller. This lets Code and Work share the same
 * session/selectModel semantics without carrying a second agent loop.
 */
import { createServer } from "node:net";
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import readline from "node:readline";

const dshBinary = process.env.AGENTCABIN_DSH_BINARY;
const patchPath = process.env.AGENTCABIN_DSH_PATCH;
const initialProvider = process.env.AGENTCABIN_DSH_PROVIDER || "";
const initialModel = process.env.AGENTCABIN_DSH_MODEL || "";
const initialEffort = process.env.AGENTCABIN_DSH_EFFORT || "";
const initialPermissionMode = process.env.AGENTCABIN_DSH_PERMISSION_MODE || "";

if (!dshBinary || !patchPath) {
  throw new Error("AgentCabin DSH Harness bridge requires AGENTCABIN_DSH_BINARY and AGENTCABIN_DSH_PATCH");
}

// The bridge is staged into a per-run managed directory, so bare module
// resolution cannot see the packaged DSH dependencies. Resolve ws from the
// sibling DSH installation instead of relying on the user's cwd or NODE_PATH.
const require = createRequire(import.meta.url);
const dshNodeModules = process.env.AGENTCABIN_DSH_NODE_MODULES || join(dirname(dirname(dshBinary)), "node_modules");
const wsModule = require(join(dshNodeModules, "ws"));
const WebSocket = wsModule.default || wsModule;

const output = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
let bridgeStopped = false;
let web;
let resolveWebReady;
const webReady = new Promise((resolve) => {
  resolveWebReady = resolve;
});
const state = {
  sessionId: "",
  selection: {
    provider: initialProvider,
    model: initialModel,
    ...(initialEffort ? { reasoningEffort: initialEffort } : {}),
  },
  follow: null,
  remoteEvents: null,
  remoteEventClientId: "",
  remoteQuestionRequests: new Map(),
  promptRequests: new Map(),
  toolNames: new Map(),
  rpc: 1,
  stopped: false,
};

// Install the stdin listener before booting the Web Harness. ACP clients may
// send initialize immediately after spawning us; webReady preserves those
// frames until the authenticated Controller is available.
output.on("line", (line) => {
  if (!line.trim()) return;
  try {
    const message = JSON.parse(line);
    void webReady.then(() => {
      if (message && typeof message === "object" && !("method" in message) && "id" in message) {
        void handleResponse(message);
      } else {
        void handleRequest(message);
      }
    });
  } catch (error) {
    process.stderr.write(`[agentcabin-dsh-bridge] ${String(error)}\n`);
  }
});

web = await startHarness();
resolveWebReady(web);

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function response(id, result, error) {
  write({ jsonrpc: "2.0", id, ...(error ? { error } : { result }) });
}

async function call(endpoint, args) {
  const rpcId = `agentcabin-${state.rpc++}-${randomUUID()}`;
  const res = await fetch(`${web.baseUrl}/api/${endpoint}`, {
    method: "POST",
    headers: { "content-type": "application/json", Cookie: web.cookie },
    body: JSON.stringify({
      type: "client-request",
      rpcId,
      method: endpoint,
      payload: { args: { request: args } },
    }),
  });
  if (!res.ok) throw new Error(`DSH Harness ${endpoint} returned HTTP ${res.status}`);
  const envelope = await res.json();
  if (envelope?.type !== "server-response" || envelope.rpcId !== rpcId) {
    throw new Error(`DSH Harness ${endpoint} returned an invalid RPC envelope`);
  }
  if (envelope.result?.ok !== true) {
    throw new Error(envelope.result?.error?.message || `DSH Harness ${endpoint} failed`);
  }
  return envelope.result.value;
}

// Some DSH remotes (notably commands/execute) use the agent-scoped wire
// arguments directly instead of the session-controller request envelope used
// by session/* endpoints.
async function callWithArgs(endpoint, args) {
  const rpcId = `agentcabin-${state.rpc++}-${randomUUID()}`;
  const res = await fetch(`${web.baseUrl}/api/${endpoint}`, {
    method: "POST",
    headers: { "content-type": "application/json", Cookie: web.cookie },
    body: JSON.stringify({
      type: "client-request",
      rpcId,
      method: endpoint,
      payload: { args },
    }),
  });
  if (!res.ok) throw new Error(`DSH Harness ${endpoint} returned HTTP ${res.status}`);
  const envelope = await res.json();
  if (envelope?.type !== "server-response" || envelope.rpcId !== rpcId) {
    throw new Error(`DSH Harness ${endpoint} returned an invalid RPC envelope`);
  }
  if (envelope.result?.ok !== true) {
    throw new Error(envelope.result?.error?.message || `DSH Harness ${endpoint} failed`);
  }
  return envelope.result.value;
}

async function setPermissionPreset(preset) {
  const result = await callWithArgs("commands/execute", {
    agentId: state.sessionId,
    line: `/permission ${preset}`,
    images: [],
  });
  if (result?.result?.kind === "error") {
    throw new Error(result.result.text || `DSH permission preset '${preset}' was rejected`);
  }
  if (result?.result?.kind !== "success") {
    throw new Error(`DSH permission preset '${preset}' was not accepted`);
  }
  return result;
}

function permissionPresetForMode(mode) {
  switch (mode) {
    case "default":
    case "acceptEdits":
      return "workspace-write";
    case "bypassPermissions":
    case "dontAsk":
      return "danger-full-access";
    default:
      return "";
  }
}

async function startSession(params, resume) {
  const args = resume
    ? { sessionId: params.sessionId, cwd: params.cwd }
    : { cwd: params.cwd, ...(params.sessionId ? { sessionId: params.sessionId } : {}) };
  const result = await call("session/create", args);
  state.sessionId = result.sessionId;
  await applySelection();
  const initialPermissionPreset = permissionPresetForMode(initialPermissionMode);
  if (initialPermissionPreset) await setPermissionPreset(initialPermissionPreset);
  await Promise.all([startFollow(), startRemoteEvents()]);
  return result;
}

async function applySelection() {
  if (!state.sessionId || !state.selection.provider || !state.selection.model) return;
  const selected = await call("session/selectModel", {
    sessionId: state.sessionId,
    provider: state.selection.provider,
    model: state.selection.model,
    ...(state.selection.reasoningEffort
      ? { reasoningEffort: state.selection.reasoningEffort }
      : {}),
  });
  state.selection = { ...state.selection, ...selected.selected };
}

function normalizeSelection(model, provider, effort) {
  let selectedProvider = provider || state.selection.provider || initialProvider;
  let selectedModel = model || state.selection.model || initialModel;
  if (selectedModel.includes("/")) {
    const slash = selectedModel.indexOf("/");
    selectedProvider = selectedModel.slice(0, slash);
    selectedModel = selectedModel.slice(slash + 1);
  }
  return {
    provider: selectedProvider,
    model: selectedModel,
    ...(effort ? { reasoningEffort: effort } : state.selection.reasoningEffort ? { reasoningEffort: state.selection.reasoningEffort } : {}),
  };
}

async function selectModel(params) {
  state.selection = normalizeSelection(params.model, params.provider, params.reasoningEffort);
  const result = await call("session/selectModel", {
    sessionId: state.sessionId,
    ...state.selection,
  });
  state.selection = { ...state.selection, ...result.selected };
  return result;
}

async function startFollow() {
  if (!state.sessionId || state.follow) return;
  const ws = new WebSocket(`${web.wsBaseUrl}/api/remote.mux`, {
    headers: { Cookie: web.cookie },
  });
  const streamId = `agentcabin-follow-${randomUUID()}`;
  state.follow = ws;
  await new Promise((resolve, reject) => {
    ws.once("unexpected-response", (_request, response) => {
      reject(new Error(`DSH follow WebSocket returned HTTP ${response.statusCode}`));
    });
    ws.once("open", resolve);
    ws.once("error", reject);
  });
  ws.send(JSON.stringify({
    type: "open",
    streamId,
    endpoint: "session/follow",
    payload: {
      args: {
        request: {
          address: { kind: "session", sessionId: state.sessionId },
          assistantStream: true,
        },
      },
    },
  }));
  ws.on("message", (data) => {
    try {
      const frame = JSON.parse(data.toString());
      if (frame.type === "item" && frame.streamId === streamId) handleFollow(frame.value);
      if (frame.type === "error" && frame.streamId === streamId) failPending(frame.error?.message || "DSH follow failed");
      if (frame.type === "end" && frame.streamId === streamId) failPending("DSH Harness follow stream ended");
    } catch (error) {
      failPending(String(error));
    }
  });
  ws.on("close", () => {
    if (!state.stopped) failPending("DSH Harness follow WebSocket closed");
  });
}

async function startRemoteEvents() {
  if (!state.sessionId || state.remoteEvents) return;
  const ws = new WebSocket(`${web.wsBaseUrl}/api/remote.mux`, {
    headers: { Cookie: web.cookie },
  });
  const streamId = `agentcabin-events-${randomUUID()}`;
  state.remoteEvents = ws;
  await new Promise((resolve, reject) => {
    ws.once("unexpected-response", (_request, response) => {
      reject(new Error(`DSH Remote Events WebSocket returned HTTP ${response.statusCode}`));
    });
    ws.once("open", resolve);
    ws.once("error", reject);
  });
  ws.send(JSON.stringify({
    type: "open",
    streamId,
    endpoint: "$events",
    payload: { args: {} },
  }));
  ws.on("message", (data) => {
    try {
      const frame = JSON.parse(data.toString());
      if (frame.type === "item" && frame.streamId === streamId) {
        handleRemoteEventFrame(frame.value);
      } else if (frame.type === "error" && frame.streamId === streamId) {
        process.stderr.write(`[agentcabin-dsh-bridge] Remote Events failed: ${frame.error?.message || "unknown error"}\n`);
      }
    } catch (error) {
      process.stderr.write(`[agentcabin-dsh-bridge] invalid Remote Events frame: ${String(error)}\n`);
    }
  });
  ws.on("close", () => {
    state.remoteEventClientId = "";
  });
}

function handleRemoteEventFrame(frame) {
  if (!frame || typeof frame !== "object") return;
  if (frame.type === "ready") {
    state.remoteEventClientId = frame.clientId || "";
    return;
  }
  if (frame.type !== "waterfall" || frame.event !== "user-questions/request" || !state.remoteEventClientId) return;
  const requestId = `dsh-question-${randomUUID()}`;
  state.remoteQuestionRequests.set(String(requestId), {
    clientId: state.remoteEventClientId,
    eventId: frame.eventId,
    questions: Array.isArray(frame.request?.questions) ? frame.request.questions : [],
  });
  write({
    jsonrpc: "2.0",
    id: requestId,
    method: "ask_user_question",
    params: frame.request || {},
  });
}

async function handleResponse(message) {
  const pending = state.remoteQuestionRequests.get(String(message.id));
  if (!pending) return;
  state.remoteQuestionRequests.delete(String(message.id));
  const outcome = message.error
    ? { kind: "rejected", error: {
        name: message.error.name || "UserQuestionError",
        code: message.error.code || "ASK_CANCELLED",
        message: message.error.message || "The user question was cancelled",
      } }
    : { kind: "result", value: toDshQuestionAnswer(pending.questions, message.result?.answers) };
  try {
    await callWithArgs("$events/result", {
      clientId: pending.clientId,
      eventId: pending.eventId,
      outcome,
    });
  } catch (error) {
    process.stderr.write(`[agentcabin-dsh-bridge] failed to return user-question answer: ${String(error)}\n`);
  }
}

function toDshQuestionAnswer(questions, rawAnswers) {
  const answers = rawAnswers && typeof rawAnswers === "object" ? rawAnswers : {};
  return {
    answers: questions.map((question, index) => {
      const id = String(question?.id ?? index);
      const raw = answers[id];
      const values = Array.isArray(raw) ? raw.map(String) : raw == null ? [] : [String(raw)];
      const labels = new Set((Array.isArray(question?.options) ? question.options : [])
        .map((option) => typeof option === "string" ? option : option?.label)
        .filter((label) => typeof label === "string"));
      if (labels.size === 0) return { id, selected: [], ...(values.length ? { custom: values.join("\n") } : {}) };
      const selected = values.filter((value) => labels.has(value));
      const custom = values.filter((value) => !labels.has(value)).join("\n");
      return { id, selected, ...(custom ? { custom } : {}) };
    }),
  };
}

function emitUpdate(update) {
  write({
    jsonrpc: "2.0",
    method: "session/update",
    params: { sessionId: state.sessionId, update },
  });
}

function handleFollow(frame) {
  if (!frame || frame.type === "snapshot") return;
  if (frame.type === "assistant-stream") {
    const chunk = frame.frame;
    if (!chunk || (chunk.type !== "chunk" && chunk.type !== "start" && chunk.type !== "end")) return;
    if (chunk.type !== "chunk") return;
    const value = chunk.chunk;
    if (value?.type === "text-delta" && typeof value.text === "string") {
      emitUpdate({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: value.text } });
    } else if (value?.type === "reasoning-delta" && typeof value.text === "string") {
      emitUpdate({ sessionUpdate: "agent_thought_chunk", content: { type: "text", text: value.text } });
    }
    return;
  }
  if (frame.type !== "event") return;
  const event = frame.event;
  const data = event?.data || {};
  if (event?.type === "turn/end") {
    const request = state.promptRequests.values().next().value;
    if (request) {
      state.promptRequests.delete(request.id);
      response(request.id, { stopReason: data.reason?.kind === "error" ? "refusal" : "end_turn" });
    }
  } else if (event?.type === "assistant/message") {
    if (data.usage && typeof data.usage === "object") {
      emitUpdate({ sessionUpdate: "usage_update", usage: data.usage });
    }
  } else if (event?.type === "tool/call") {
    const toolCallId = data.callId || data.toolCallId || "dsh-tool";
    const toolName = data.name || data.toolName || data.message?.source?.name || "DSH tool";
    if (isQuestionTool(toolName)) return;
    state.toolNames.set(toolCallId, toolName);
    emitUpdate({
      sessionUpdate: "tool_call",
      toolCallId,
      title: toolName,
      rawInput: data.arguments || {},
    });
  } else if (event?.type === "tool/result") {
    const toolCallId = data.message?.source?.callId || data.callId || data.toolCallId || "dsh-tool";
    const toolName = data.name || data.toolName || data.message?.source?.name || state.toolNames.get(toolCallId) || "DSH tool";
    if (isQuestionTool(toolName)) {
      state.toolNames.delete(toolCallId);
      return;
    }
    emitUpdate({
      sessionUpdate: "tool_call_update",
      toolCallId,
      title: toolName,
      status: "completed",
      rawOutput: data.message || data,
    });
    state.toolNames.delete(toolCallId);
  }
}

function isQuestionTool(name) {
  return String(name).trim().toLowerCase() === "ask_user_question";
}

function failPending(message) {
  for (const pending of state.promptRequests.values()) {
    response(pending.id, null, { code: -32000, message });
  }
  state.promptRequests.clear();
}

async function handleRequest(message) {
  const { id, method, params = {} } = message;
  try {
    switch (method) {
      case "initialize":
        response(id, {
          protocolVersion: 1,
          agentCapabilities: {
            loadSession: true,
            promptCapabilities: { image: true },
            sessionCapabilities: { model: true, reasoningEffort: true },
          },
          authMethods: [],
        });
        break;
      case "session/new":
        response(id, await startSession(params, false));
        break;
      case "session/resume":
        response(id, await startSession(params, true));
        break;
      case "session/selectModel":
        response(id, await selectModel(params));
        break;
      case "session/set_config_option": {
        if (params.configId === "reasoning_effort") {
          state.selection = normalizeSelection(undefined, undefined, params.value);
          response(id, await selectModel(state.selection));
        } else if (params.configId === "model") {
          response(id, await selectModel({ model: params.value }));
        } else if (params.configId === "provider") {
          response(id, await selectModel({ provider: params.value }));
        } else if (params.configId === "permission_preset") {
          response(id, await setPermissionPreset(params.value));
        } else {
          throw new Error(`Unsupported DSH config option: ${params.configId}`);
        }
        break;
      }
      case "session/prompt": {
        if (!state.sessionId) throw new Error("DSH session is not ready");
        const requestId = `acp-${randomUUID()}`;
        state.promptRequests.set(id, { id });
        const prompt = params.prompt || params.content || params.contentBlocks || [];
        // The SDK-facing ACP wire uses `mimeType`, while the Web Harness
        // Session Controller contract uses `mediaType`. Translate at this
        // bridge boundary instead of leaking one protocol's shape downstream.
        const content = prompt.map((block) => {
          if (block?.type !== "image") return block;
          const { mimeType, data, name } = block;
          return { type: "image", mediaType: mimeType, data, ...(name ? { name } : {}) };
        });
        await call("session/prompt", {
          requestId,
          sessionId: state.sessionId,
          mode: "queue",
          content,
        });
        break;
      }
      case "session/cancel":
        response(id, await call("session/cancel", { sessionId: state.sessionId }));
        break;
      default:
        response(id, null, { code: -32601, message: `Unsupported DSH ACP method '${method}'` });
    }
  } catch (error) {
    response(id, null, { code: -32000, message: error instanceof Error ? error.message : String(error) });
  }
}

async function startHarness() {
  const port = await freePort();
  let resolveLaunchToken;
  let rejectLaunchToken;
  const launchToken = new Promise((resolve, reject) => {
    resolveLaunchToken = resolve;
    rejectLaunchToken = reject;
  });
  // The Web Harness is a second DSH process. Keep the Work boundary explicit
  // across this nested spawn instead of relying on the parent bridge's
  // inherited environment. Without it, the native Work plugin rejects the
  // child during Cordis boot and the ACP side only sees ECONNREFUSED.
  const childEnv = { ...process.env };
  if (process.env.AGENTCABIN_WORK_BRIDGE_TOKEN && !childEnv.AGENTCABIN_WORK_RUNTIME) {
    childEnv.AGENTCABIN_WORK_RUNTIME = "dsh";
  }
  const child = spawn(dshBinary, [
    "--patch", patchPath,
    "--profile", "web",
    "--host", "127.0.0.1",
    "--port", String(port),
    "--no-open",
  ], {
    cwd: process.cwd(),
    env: childEnv,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let launchOutput = "";
  child.stdout.on("data", (chunk) => {
    const text = chunk.toString();
    launchOutput = `${launchOutput}${text}`.slice(-4096);
    process.stderr.write(`[dsh-web] ${text.replace(/([?&]token=)[^\s]+/g, "$1<redacted>")}`);
    const match = launchOutput.match(/[?&]token=([^\s]+)/);
    if (match) resolveLaunchToken(decodeURIComponent(match[1]));
  });
  child.stderr.on("data", (chunk) => process.stderr.write(`[dsh-web] ${chunk}`));
  child.once("exit", (code, signal) => {
    rejectLaunchToken(new Error(`DSH Web Harness exited before authentication (code=${code} signal=${signal})`));
    if (!bridgeStopped) process.stderr.write(`[dsh-web] exited code=${code} signal=${signal}\n`);
  });
  const baseUrl = `http://127.0.0.1:${port}`;
  let token;
  try {
    token = await Promise.race([
      launchToken,
      new Promise((_, reject) => setTimeout(() => reject(new Error("launch token pending")), 30_000)),
    ]);
  } catch (error) {
    child.kill();
    throw new Error(`Timed out waiting for the DSH Harness Web Controller launch token: ${error}`);
  }
  // Work's native plugin tree adds a small amount of boot time after the
  // launch token is printed. Retry loopback readiness so a transient
  // ECONNREFUSED does not turn into a failed ACP session.
  const bootstrap = await fetchWithReadiness(`${baseUrl}/?token=${encodeURIComponent(token)}`, {
    redirect: "manual",
  });
  const setCookies = typeof bootstrap.headers.getSetCookie === "function"
    ? bootstrap.headers.getSetCookie()
    : [bootstrap.headers.get("set-cookie") || ""];
  const cookie = setCookies[0]?.split(";", 1)[0];
  if (bootstrap.status !== 303 || !cookie) {
    child.kill();
    throw new Error(`DSH Harness Web authentication bootstrap failed (HTTP ${bootstrap.status})`);
  }
  const ready = await fetchWithReadiness(`${baseUrl}/`, { headers: { Cookie: cookie } });
  if (!ready.ok) {
    child.kill();
    throw new Error(`DSH Harness Web Controller returned HTTP ${ready.status} after authentication`);
  }
  return { baseUrl, wsBaseUrl: `ws://127.0.0.1:${port}`, cookie, child };
}

async function fetchWithReadiness(url, options, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      return await fetch(url, options);
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }
  throw lastError || new Error(`Timed out waiting for DSH Harness readiness: ${url}`);
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address ? address.port : 0;
      server.close((error) => error ? reject(error) : resolve(port));
    });
  });
}

async function shutdown() {
  bridgeStopped = true;
  state.stopped = true;
  state.follow?.close();
  state.remoteEvents?.close();
  web?.child.kill();
  process.exit(0);
}

process.once("SIGTERM", shutdown);
process.once("SIGINT", shutdown);
