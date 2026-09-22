import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const TYPEBOX_STUB = `
export const Type = {
  Array: () => ({}),
  Any: () => ({}),
  Boolean: () => ({}),
  Integer: () => ({}),
  Literal: (value) => value,
  Number: () => ({}),
  Object: () => ({}),
  Optional: (value) => value,
  Record: () => ({}),
  String: () => ({}),
  Union: () => ({}),
};
`;

const PREFLIGHT_STUB = `
import path from "node:path";
export async function resolveSubagentLaunchContract(input) {
  if (String(input.task || "").includes("fail-preflight")) {
    return { ok: false, message: "Preflight intentional failure for test" };
  }
  const runtimeProfile = path.resolve(process.env.PI_CODING_AGENT_DIR || process.env.AGENTCABIN_WORK_PROFILE_DIR);
  const workProfile = path.resolve(process.env.AGENTCABIN_WORK_PROFILE_DIR || process.env.PI_CODING_AGENT_DIR);
  const tools = input.agent === "agentcabin-worker"
    ? ["read", "write", "edit", "bash"]
    : input.agent === "agentcabin-researcher"
      ? ["read", "web_search", "web_open", "web_extract", "web_cite"]
      : ["read"];
  const runtime = ["/runtime/pi-subagents-runtime.mjs"];
  const core = path.join(workProfile, "extensions", "agentcabin-work-core.mjs");
  return {
    ok: true,
    contract: {
      agent: {
        name: input.agent,
        source: "user",
        filePath: path.join(runtimeProfile, "agents", input.agent + ".md"),
        definitionDigest: "definition-digest",
        shadowedCandidates: [],
      },
      tools: {
        effectiveAllowlist: tools,
        requiredChildTools: tools,
        effectiveMcpTools: [],
        toolExtensionPaths: [],
        runtimeExtensions: runtime,
        configuredExtensions: [core],
        extensionArgs: [...runtime, core],
        disableAmbientExtensions: true,
        fanoutAuthorized: false,
      },
      launchContractDigest: "launch-contract-digest",
      digest: "contract-digest",
    },
  };
}
`;

const CAPABILITY_CEILING_STUB = `
let current;
export function registerSubagentCapabilityCeiling(options) {
  current = {
    version: 1,
    allowedAgents: [...options.ceiling.allowedAgents].sort(),
    allowedTools: [...options.ceiling.allowedTools].sort(),
    denyExtensions: options.ceiling.denyExtensions,
    sources: [options.source],
  };
  return { dispose() { current = undefined; } };
}
export function resolveCurrentSubagentCapabilityCeiling() { return current; }
`;

async function loadAdapter(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  const subagentsDir = path.join(tempRoot, "node_modules", "pi-subagents");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.mkdirSync(subagentsDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n', "utf8");
  fs.writeFileSync(
    path.join(tempRoot, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
    "utf8",
  );
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB, "utf8");
  fs.writeFileSync(
    path.join(subagentsDir, "package.json"),
    '{"type":"module","exports":{"./preflight":"./preflight.js","./capability-ceiling":"./capability-ceiling.js"}}\n',
    "utf8",
  );
  fs.writeFileSync(path.join(subagentsDir, "preflight.js"), PREFLIGHT_STUB, "utf8");
  fs.writeFileSync(path.join(subagentsDir, "capability-ceiling.js"), CAPABILITY_CEILING_STUB, "utf8");
  fs.copyFileSync(
    new URL("./pi_subagents_adapter.mjs", import.meta.url),
    path.join(extensionDir, "agentcabin-work-subagents-adapter.mjs"),
  );
  fs.copyFileSync(
    new URL("./pi_workspace_paths.mjs", import.meta.url),
    path.join(extensionDir, "pi_workspace_paths.mjs"),
  );
  fs.copyFileSync(
    new URL("./work_subagent_policy.mjs", import.meta.url),
    path.join(extensionDir, "work_subagent_policy.mjs"),
  );
  fs.copyFileSync(
    new URL("./work_subagent_tools.mjs", import.meta.url),
    path.join(extensionDir, "work_subagent_tools.mjs"),
  );

  const module = await import(
    `${pathToFileURL(path.join(extensionDir, "agentcabin-work-subagents-adapter.mjs"))}?test=${Date.now()}`
  );
  return module.default;
}

function preparePreflightProfile(tempRoot) {
  const profile = path.join(tempRoot, "profile");
  fs.mkdirSync(path.join(profile, "agents"), { recursive: true });
  fs.mkdirSync(path.join(profile, "extensions"), { recursive: true });
  for (const name of ["agentcabin-researcher", "agentcabin-worker", "agentcabin-reviewer"]) {
    fs.writeFileSync(path.join(profile, "agents", `${name}.md`), `---\nname: ${name}\n---\n`, "utf8");
  }
  fs.writeFileSync(path.join(profile, "extensions", "agentcabin-work-core.mjs"), "export default {};\n", "utf8");
  return profile;
}

function systemAgentFileDigests(profile) {
  return Object.fromEntries(
    ["agentcabin-researcher", "agentcabin-worker", "agentcabin-reviewer"].map((name) => [
      name,
      crypto.createHash("sha256").update(fs.readFileSync(path.join(profile, "agents", `${name}.md`))).digest("hex"),
    ]),
  );
}

function createMockPi(options = {}) {
  const tools = new Map();
  const eventHandlers = new Map();
  const lifecycleHandlers = new Map();
  const rpcMethods = [];
  const rpcCalls = [];
  let spawnCount = 0;

  return {
    tools,
    rpcMethods,
    rpcCalls,
    lifecycleHandlers,
    emitLifecycle(event, ...args) {
      const handler = lifecycleHandlers.get(event);
      if (handler) return handler(...args);
    },
    registerTool(tool) {
      tools.set(tool.name, tool);
    },
    on(event, handler) {
      lifecycleHandlers.set(event, handler);
    },
    events: {
      handlers: eventHandlers,
      on(event, handler) {
        let handlers = eventHandlers.get(event);
        if (!handlers) {
          handlers = [];
          eventHandlers.set(event, handlers);
        }
        handlers.push(handler);
        return () => {
          const list = eventHandlers.get(event) || [];
          eventHandlers.set(event, list.filter((h) => h !== handler));
        };
      },
      emit(event, data) {
        if (event === "subagents:rpc:v1:request") {
          const req = data;
          rpcMethods.push(req.method);
          rpcCalls.push(req);
          const replyEvent = `subagents:rpc:v1:reply:${req.requestId}`;
          setTimeout(() => {
            const handlers = eventHandlers.get(replyEvent) || [];
            if (typeof options.rpcHandler === "function") {
              const custom = options.rpcHandler(req, { spawnCount });
              if (custom !== undefined) {
                if (req.method === "spawn" && custom.success) spawnCount += 1;
                for (const h of handlers) {
                  h({ version: 1, requestId: req.requestId, method: req.method, ...custom });
                }
                return;
              }
            }
            let payload = { text: "ok", details: { runId: "subagent-run-1" } };
            if (req.method === "spawn") {
              spawnCount += 1;
              payload = { text: "ok", details: { runId: `subagent-run-${spawnCount}` } };
            } else if (req.method === "status") {
              payload = { text: "Subagent completed", details: { state: "completed", result: "ok result" } };
            } else if (req.method === "stop") {
              payload = { text: "Subagent stopped", details: { state: "stopped" } };
            } else if (req.method === "steer") {
              payload = { text: "Steered", details: { delivered: true } };
            }
            for (const h of handlers) {
              h({ version: 1, requestId: req.requestId, method: req.method, success: true, data: payload });
            }
          }, 5);
        } else {
          for (const handler of eventHandlers.get(event) || []) handler(data);
        }
      },
    },
  };
}

test("Subagents adapter exits without registering tools in child subagent mode", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-child-"));
  const origEnv = process.env.PI_SUBAGENT_CHILD;
  process.env.PI_SUBAGENT_CHILD = "1";
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);
    assert.equal(mockPi.tools.size, 0, "No tools should be registered in child mode");
  } finally {
    if (origEnv === undefined) delete process.env.PI_SUBAGENT_CHILD;
    else process.env.PI_SUBAGENT_CHILD = origEnv;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Subagents adapter registers safe Work delegation tools in Root mode", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-root-"));
  const origEnv = process.env.PI_SUBAGENT_CHILD;
  delete process.env.PI_SUBAGENT_CHILD;
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    assert(mockPi.tools.has("work_delegate"), "work_delegate should be registered");
    assert(mockPi.tools.has("work_research_swarm"), "work_research_swarm should be registered");
    assert(mockPi.tools.has("work_implement_review_fix"), "work_implement_review_fix should be registered");
    assert(mockPi.tools.has("work_agent_status"), "work_agent_status should be registered");
    assert(mockPi.tools.has("work_agent_wait"), "work_agent_wait should be registered");
    assert(mockPi.tools.has("work_agent_steer"), "work_agent_steer should be registered");
    assert(mockPi.tools.has("work_agent_stop"), "work_agent_stop should be registered");
    assert(!mockPi.tools.has("subagent"), "raw subagent tool must NOT be registered");
  } finally {
    if (origEnv !== undefined) process.env.PI_SUBAGENT_CHILD = origEnv;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Subagent delegation fails fast when the Work RPC bridge is not ready", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-not-ready-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);
    await mockPi.emitLifecycle("session_start", {});

    const result = await mockPi.tools.get("work_delegate").execute(
      "call-not-ready",
      { role: "researcher", task: "Investigate auth" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(result.details.ok, false);
    assert.match(result.details.message, /RPC bridge is not ready/i);
    assert.deepEqual(mockPi.rpcMethods, []);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_delegate rejects invalid roles and executes valid roles via RPC spawn", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-delegate-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");

    // 1. Invalid role
    const invalidRes = await delegateTool.execute("call-1", { role: "admin", task: "do stuff" });
    assert.equal(invalidRes.details.ok, false);
    assert.match(invalidRes.details.message, /Invalid role/);

    // 2. Empty task
    const emptyTaskRes = await delegateTool.execute("call-2", { role: "researcher", task: "" });
    assert.equal(emptyTaskRes.details.ok, false);
    assert.match(emptyTaskRes.details.message, /empty/i);

    // 3. Valid spawn
    const validRes = await delegateTool.execute(
      "call-3",
      { role: "researcher", task: "Investigate auth" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(validRes.details.ok, true);
    assert.equal(validRes.details.agent_id, "subagent-run-1");
    assert.equal(validRes.details.role, "researcher");
    assert.equal(validRes.details.status, "running");

    // 4. Second spawn while first is active succeeds under bounded concurrency (1 researcher + 1 worker <= 3)
    const secondRes = await delegateTool.execute(
      "call-4",
      { role: "worker", task: "Fix bug" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(secondRes.details.ok, true);
    assert.equal(secondRes.details.agent_id, "subagent-run-2");
    assert.equal(secondRes.details.role, "worker");
    assert.equal(secondRes.details.status, "running");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_delegate binds agent definitions to the per-run Pi runtime directory", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-runtime-profile-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const workProfile = preparePreflightProfile(temp);
  const runtimeProfile = path.join(temp, "runtime-profile");
  fs.mkdirSync(path.join(runtimeProfile, "agents"), { recursive: true });
  for (const name of ["agentcabin-researcher", "agentcabin-worker", "agentcabin-reviewer"]) {
    fs.copyFileSync(
      path.join(workProfile, "agents", `${name}.md`),
      path.join(runtimeProfile, "agents", `${name}.md`),
    );
  }
  process.env.PI_CODING_AGENT_DIR = runtimeProfile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = workProfile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(workProfile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({
    ok: true,
    status: 200,
    async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; },
  });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    const result = await mockPi.tools.get("work_delegate").execute(
      "call-runtime-profile",
      { role: "researcher", task: "Investigate auth" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(result.details.ok, true);
    assert.equal(result.details.status, "running");
    assert.deepEqual(mockPi.rpcMethods, ["spawn"]);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_agent_status, work_agent_steer, and work_agent_stop execute properly", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-control-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    // Status
    const statusTool = mockPi.tools.get("work_agent_status");
    const statusRes = await statusTool.execute("call-1", { agent_id: "agent-123" });
    assert.equal(statusRes.details.ok, true);

    // Steer
    const steerTool = mockPi.tools.get("work_agent_steer");
    const steerRes = await steerTool.execute("call-2", { agent_id: "agent-123", message: "Focus on jwt.rs" });
    assert.equal(steerRes.details.ok, true);

    // Stop
    const stopTool = mockPi.tools.get("work_agent_stop");
    const stopRes = await stopTool.execute("call-3", { agent_id: "agent-123" });
    assert.equal(stopRes.details.ok, true);
    assert.equal(stopRes.details.status, "stopped");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_delegate fails closed without a real parent session ID", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-no-session-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true }; } });
  try {
    const mockPi = createMockPi();
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const response = await mockPi.tools.get("work_delegate").execute("call-1", {
      role: "researcher",
      task: "Should not spawn",
    }, undefined, undefined, { cwd: temp });
    assert.equal(response.details.ok, false);
    assert.match(response.details.message, /session ID/i);
    assert.deepEqual(mockPi.rpcMethods, []);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("async-complete updates the Work bridge without requiring work_agent_wait", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-async-complete-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  const bridgeBodies = [];
  globalThis.fetch = async (_url, options) => {
    bridgeBodies.push(JSON.parse(options.body));
    return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } };
  };
  try {
    const mockPi = createMockPi();
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const response = await mockPi.tools.get("work_delegate").execute("call-1", {
      role: "researcher",
      task: "Complete asynchronously",
    }, undefined, undefined, { cwd: temp, sessionManager: { getSessionId: () => "root-session" } });
    assert.equal(response.details.agent_id, "subagent-run-1");
    mockPi.events.emit("subagent:async-complete", {
      runId: "subagent-run-1",
      status: "completed",
      summary: "Finished without a wait tool call",
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.deepEqual(bridgeBodies.map((body) => body.status), [undefined, "completed"]);
    const second = await mockPi.tools.get("work_delegate").execute("call-2", {
      role: "reviewer",
      task: "The prior child is terminal",
    }, undefined, undefined, { cwd: temp, sessionManager: { getSessionId: () => "root-session" } });
    assert.equal(second.details.ok, true);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("register_spawn failure stops the spawned child as compensation", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-bridge-failure-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: false, status: 500, async json() { return { error: "registry unavailable" }; } });
  try {
    const mockPi = createMockPi();
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const response = await mockPi.tools.get("work_delegate").execute("call-1", {
      role: "worker",
      task: "Must be compensated",
    }, undefined, undefined, { cwd: temp, sessionManager: { getSessionId: () => "root-session" } });
    assert.equal(response.details.ok, false);
    assert.match(response.details.message, /compensation|stopped/i);
    assert.deepEqual(mockPi.rpcMethods, ["spawn", "stop"]);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

function setupFullEnv(temp) {
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  return profile;
}

function restoreEnv(previous) {
  if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
  if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
  if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
  if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
  if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
  if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
  globalThis.fetch = previous.fetch;
}

// Blocker 1: paused / unknown states must NOT write a terminal fact; rejected maps to failed.
test("async-complete with paused and unknown states does not write terminal fact; rejected maps to failed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-nonterminal-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  const bridgeUpdateStatuses = [];
  globalThis.fetch = async (_url, options) => {
    const body = JSON.parse(options.body);
    if (body.status !== undefined) {
      bridgeUpdateStatuses.push(body.status);
    }
    return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } };
  };

  try {
    const mockPi = createMockPi();
    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    // Spawn a child so activeChildId is set.
    const spawnRes = await mockPi.tools.get("work_delegate").execute(
      "call-1",
      { role: "researcher", task: "Should not auto-complete on paused" },
      undefined, undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(spawnRes.details.ok, true);
    const agentId = spawnRes.details.agent_id;

    // Emit paused – must NOT trigger a bridge update_status call.
    mockPi.events.emit("subagent:async-complete", { runId: agentId, status: "paused" });
    await new Promise((r) => setTimeout(r, 20));
    assert.deepEqual(bridgeUpdateStatuses, [], "paused must not write a terminal status");

    // Emit rejected – must write failed.
    mockPi.events.emit("subagent:async-complete", { runId: agentId, status: "rejected" });
    await new Promise((r) => setTimeout(r, 20));
    assert.deepEqual(bridgeUpdateStatuses, ["failed"], "rejected must map to failed");

  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

// Blocker 2: async-complete arrives in the spawn/register race window – must be buffered and drained.
test("async-complete arriving before activeChildId is committed is buffered and processed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-race-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  const bridgeUpdateStatuses = [];
  const mockPi = createMockPi();

  // Stall register_spawn and emit async-complete during the stall window.
  // The fetch mock holds a reference to mockPi so it can fire the event.
  let registerResolve;
  const registerBlocked = new Promise((r) => { registerResolve = r; });

  globalThis.fetch = async (_url, options) => {
    const body = JSON.parse(options.body);
    if (body.agentId && body.launchContractDigest) {
      // register_spawn call: fire async-complete synchronously then stall briefly.
      // At this point pendingSpawnId === agentId but activeChildId is still null.
      mockPi.events.emit("subagent:async-complete", {
        runId: "subagent-run-1",
        status: "complete",
        summary: "Finished during race window",
      });
      // Wait briefly to ensure the event handler ran before we return.
      await new Promise((r) => setTimeout(r, 5));
    }
    if (body.status !== undefined) {
      bridgeUpdateStatuses.push(body.status);
    }
    return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } };
  };

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const spawnRes = await mockPi.tools.get("work_delegate").execute(
      "call-1",
      { role: "researcher", task: "Race window test" },
      undefined, undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    // Allow the buffered terminal flush to complete.
    await new Promise((r) => setTimeout(r, 50));

    assert.equal(spawnRes.details.ok, true, "spawn should succeed");
    assert.deepEqual(
      bridgeUpdateStatuses, ["completed"],
      "Buffered completion during race window must be written to bridge after registration",
    );
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Bounded concurrency allows 3 parallel researchers and rejects 4th", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-parallel-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  let spawnCounter = 0;
  const mockPi = createMockPi();
  mockPi.customSpawnHandler = (workflowScript) => {
    spawnCounter += 1;
    return { ok: true, details: { runId: `subagent-run-${spawnCounter}` } };
  };

  globalThis.fetch = async () => ({
    ok: true,
    status: 200,
    async json() {
      return { ok: true, bootstrapToken: "test-bootstrap-token" };
    },
  });

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const delegate = mockPi.tools.get("work_delegate");
    const ctx = { cwd: temp, sessionManager: { getSessionId: () => "root-session" } };

    // Spawn 1st, 2nd, 3rd researcher -> all should succeed
    const r1 = await delegate.execute("c1", { role: "researcher", task: "T1" }, undefined, undefined, ctx);
    assert.equal(r1.details.ok, true);
    assert.equal(r1.details.agent_id, "subagent-run-1");

    const r2 = await delegate.execute("c2", { role: "researcher", task: "T2" }, undefined, undefined, ctx);
    assert.equal(r2.details.ok, true);
    assert.equal(r2.details.agent_id, "subagent-run-2");

    const r3 = await delegate.execute("c3", { role: "reviewer", task: "T3" }, undefined, undefined, ctx);
    assert.equal(r3.details.ok, true);
    assert.equal(r3.details.agent_id, "subagent-run-3");

    // Attempt 4th spawn -> must fail because MAX_CONCURRENT_CHILDREN is 3
    const r4 = await delegate.execute("c4", { role: "researcher", task: "T4" }, undefined, undefined, ctx);
    assert.equal(r4.details.ok, false);
    assert.match(r4.details.message, /Subagents limit reached/);
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Worker concurrency constraint permits only 1 active worker subagent", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-worker-limit-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  let spawnCounter = 0;
  const mockPi = createMockPi();
  mockPi.customSpawnHandler = () => {
    spawnCounter += 1;
    return { ok: true, details: { runId: `subagent-run-${spawnCounter}` } };
  };

  globalThis.fetch = async () => ({
    ok: true,
    status: 200,
    async json() {
      return { ok: true, bootstrapToken: "test-bootstrap-token" };
    },
  });

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const delegate = mockPi.tools.get("work_delegate");
    const ctx = { cwd: temp, sessionManager: { getSessionId: () => "root-session" } };

    // 1st: Worker 1 spawned -> succeeds
    const w1 = await delegate.execute("c1", { role: "worker", task: "Implement feature" }, undefined, undefined, ctx);
    assert.equal(w1.details.ok, true);

    // 2nd: Researcher spawned alongside worker -> succeeds (1 worker + 1 researcher <= 3)
    const r1 = await delegate.execute("c2", { role: "researcher", task: "Analyze logs" }, undefined, undefined, ctx);
    assert.equal(r1.details.ok, true);

    // 3rd: Attempt 2nd Worker -> must be rejected because max 1 active worker
    const w2 = await delegate.execute("c3", { role: "worker", task: "Second worker" }, undefined, undefined, ctx);
    assert.equal(w2.details.ok, false);
    assert.match(w2.details.message, /Worker subagent .* is currently running/);
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_agent_wait requires explicit agent_id when multiple subagents are active", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-wait-disambiguation-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  let spawnCounter = 0;
  const mockPi = createMockPi();
  mockPi.customSpawnHandler = () => {
    spawnCounter += 1;
    return { ok: true, details: { runId: `subagent-run-${spawnCounter}` } };
  };

  globalThis.fetch = async () => ({
    ok: true,
    status: 200,
    async json() {
      return { ok: true, bootstrapToken: "test-bootstrap-token" };
    },
  });

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const delegate = mockPi.tools.get("work_delegate");
    const wait = mockPi.tools.get("work_agent_wait");
    const ctx = { cwd: temp, sessionManager: { getSessionId: () => "root-session" } };

    await delegate.execute("c1", { role: "researcher", task: "R1" }, undefined, undefined, ctx);
    await delegate.execute("c2", { role: "reviewer", task: "R2" }, undefined, undefined, ctx);

    // Call work_agent_wait without agent_id -> must fail closed prompting for explicit agent_id
    const waitRes = await wait.execute("w1", {}, undefined);
    assert.equal(waitRes.details.ok, false);
    assert.match(waitRes.details.message, /当前有多个正在运行的子代理/);

    // Call work_agent_wait with explicit agent_id -> proceeds
    const waitSpecific = await wait.execute("w2", { agent_id: "subagent-run-1" }, undefined);
    assert.equal(waitSpecific.details.ok, true);
    assert.equal(waitSpecific.details.agent_id, "subagent-run-1");
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_agent_status recovers interrupted subagent from Host Bridge when RPC fails", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-recover-status-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  const mockPi = createMockPi();
  // Mock RPC status throwing an error (e.g. disconnected child process)
  mockPi.events.emit = (event, data) => {
    if (event === "subagents:rpc:v1:request") {
      const replyEvent = `subagents:rpc:v1:reply:${data.requestId}`;
      setTimeout(() => {
        // Emit failure
        for (const handler of (mockPi.events.handlers?.get(replyEvent) || [])) {
          handler({ version: 1, requestId: data.requestId, method: data.method, success: false, error: "Connection lost" });
        }
      }, 5);
    }
  };

  // Mock Host Bridge record endpoint returning interrupted record
  globalThis.fetch = async (url) => {
    if (url.includes("/internal/work/subagents/record")) {
      return {
        ok: true,
        status: 200,
        async json() {
          return {
            record: {
              agent_id: "interrupted-subagent-1",
              role: "researcher",
              status: "interrupted",
              error: "App restarted before completion",
            },
          };
        },
      };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const statusTool = mockPi.tools.get("work_agent_status");

    const statusRes = await statusTool.execute("s1", { agent_id: "interrupted-subagent-1" });
    assert.equal(statusRes.details.ok, true);
    assert.equal(statusRes.details.status, "interrupted");
    assert.equal(statusRes.details.interrupted, true);
    assert.equal(statusRes.details.recoverable, true);
    assert.match(statusRes.content[0].text, /interrupted/i);
    assert.match(statusRes.content[0].text, /重新委派/);
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_agent_wait detects interrupted subagent from Host Bridge and returns immediately", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-subagents-recover-wait-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  setupFullEnv(temp);

  const mockPi = createMockPi();
  mockPi.events.emit = (event, data) => {
    if (event === "subagents:rpc:v1:request") {
      const replyEvent = `subagents:rpc:v1:reply:${data.requestId}`;
      setTimeout(() => {
        for (const handler of (mockPi.events.handlers?.get(replyEvent) || [])) {
          handler({ version: 1, requestId: data.requestId, method: data.method, success: false, error: "Child process died" });
        }
      }, 5);
    }
  };

  globalThis.fetch = async (url) => {
    if (url.includes("/internal/work/subagents/record")) {
      return {
        ok: true,
        status: 200,
        async json() {
          return {
            record: {
              agent_id: "dead-child-1",
              role: "worker",
              status: "interrupted",
              error: "Process terminated unexpectedly",
            },
          };
        },
      };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const waitTool = mockPi.tools.get("work_agent_wait");

    const startTime = Date.now();
    const waitRes = await waitTool.execute("w1", { agent_id: "dead-child-1", timeout_seconds: 30 });
    const duration = Date.now() - startTime;

    // Should return in < 500ms without waiting for 30s timeout
    assert(duration < 2000, `Expected fast exit on interrupted subagent, took ${duration}ms`);
    assert.equal(waitRes.details.ok, true);
    assert.equal(waitRes.details.status, "interrupted");
    assert.equal(waitRes.details.interrupted, true);
    assert.match(waitRes.content[0].text, /interrupted/);
    assert.match(waitRes.content[0].text, /重新委派/);
  } finally {
    restoreEnv(previous);
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Adapter and Host Bootstrap Credential Integration Test (Mocked Subagent Runner)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-bootstrap-integration-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "input", "secret.txt"), "classified data\n", "utf8");

  const ROOT_BEARER_TOKEN = "root-secret-token-do-not-leak";
  const issuedBootstrapTokens = new Map(); // agentId -> bootstrapToken
  const consumedBootstrapTokens = new Set();
  const activeScopedTokens = new Map(); // token -> { agentId, role }

  // Mock Host Internal Bridge
  const bridgeHost = {
    registerSpawn(body) {
      const bootstrapToken = `bootstrap-token-${body.agentId}-${crypto.randomUUID().slice(0, 8)}`;
      issuedBootstrapTokens.set(body.agentId, {
        token: bootstrapToken,
        role: body.role,
        agentId: body.agentId,
      });
      return {
        ok: true,
        agentId: body.agentId,
        role: body.role,
        bootstrapToken,
      };
    },
    exchangeToken(authHeader, body) {
      const token = (authHeader || "").replace(/^Bearer\s+/i, "").trim();
      if (!token) return { status: 401, data: { error: "Missing token" } };

      let registered;
      if (token === ROOT_BEARER_TOKEN) {
        // v0.51 compatibility path: match registered child by candidate ID
        registered = Array.from(issuedBootstrapTokens.values()).find(
          (b) => b.agentId === body.agentId || b.agentId === body.providerRunId
        );
        if (!registered) {
          return { status: 404, data: { error: "Subagent not registered" } };
        }
      } else {
        // Strict bootstrap path: check single-use atomic consumption
        if (consumedBootstrapTokens.has(token)) {
          return { status: 403, data: { error: "Bootstrap token already consumed" } };
        }
        registered = Array.from(issuedBootstrapTokens.values()).find((b) => b.token === token);
        if (!registered) {
          return { status: 403, data: { error: "Invalid bootstrap token" } };
        }
        if (registered.agentId !== body.agentId && registered.agentId !== body.providerRunId) {
          return { status: 403, data: { error: "Agent ID mismatch" } };
        }
        consumedBootstrapTokens.add(token);
      }

      const scopedToken = `scoped-token-${registered.role}-${registered.agentId}`;
      activeScopedTokens.set(scopedToken, { agentId: registered.agentId, role: registered.role });

      return {
        status: 200,
        data: {
          token: scopedToken,
          agentId: registered.agentId,
          role: registered.role,
          subject: {
            type: "Subagent",
            agent_id: registered.agentId,
            role: registered.role,
          },
        },
      };
    },
  };

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    fetch: globalThis.fetch,
  };

  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "65430";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = ROOT_BEARER_TOKEN;
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "ws-smoke-test";

  globalThis.fetch = async (url, options = {}) => {
    const urlStr = String(url);
    const authHeader = options.headers?.["Authorization"] || options.headers?.["authorization"] || "";
    if (urlStr.includes("/internal/work/subagents/register_spawn")) {
      assert.equal(authHeader, `Bearer ${ROOT_BEARER_TOKEN}`, "register_spawn must be authorized with Root token");
      const body = JSON.parse(options.body || "{}");
      return { ok: true, status: 200, async json() { return bridgeHost.registerSpawn(body); } };
    }
    if (urlStr.includes("/internal/work/subagents/token")) {
      const body = JSON.parse(options.body || "{}");
      const res = bridgeHost.exchangeToken(authHeader, body);
      return {
        ok: res.status === 200,
        status: res.status,
        async json() { return res.data; },
      };
    }
    if (urlStr.includes("/internal/work/tool_pipeline")) {
      const token = authHeader.replace(/^Bearer\s+/i, "").trim();
      const sub = activeScopedTokens.get(token);
      if (!sub) return { ok: false, status: 403, async json() { return { error: "Forbidden" }; } };
      return { ok: true, status: 200, async json() { return { success: true, status: "success", exitCode: 0, stdout: "executed" }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    let spawnCounter = 0;
    const mockPi = {
      tools: new Map(),
      events: {
        handlers: new Map(),
        on(event, handler) {
          let list = this.handlers.get(event) || [];
          list.push(handler);
          this.handlers.set(event, list);
          return () => { this.handlers.set(event, (this.handlers.get(event) || []).filter((h) => h !== handler)); };
        },
        emit(event, data) {
          if (event === "subagents:rpc:v1:request") {
            const replyEvent = `subagents:rpc:v1:reply:${data.requestId}`;
            setTimeout(() => {
              if (data.method === "spawn") {
                spawnCounter += 1;
                const runId = `subagent-run-${spawnCounter}`;
                const reply = { version: 1, requestId: data.requestId, method: "spawn", success: true, data: { details: { runId } } };
                for (const h of this.handlers.get(replyEvent) || []) h(reply);
              }
            }, 5);
          }
        },
      },
      registerTool(tool) { this.tools.set(tool.name, tool); },
      on() {},
    };

    const adapter = await loadAdapter(temp);
    adapter(mockPi);
    const delegate = mockPi.tools.get("work_delegate");
    assert(delegate, "work_delegate must be registered");

    const sessionCtx = { cwd: temp, sessionManager: { getSessionId: () => "root-session" } };

    // Delegate Researcher A
    const resA = await delegate.execute("d1", { role: "researcher", task: "Analyze architecture" }, undefined, undefined, sessionCtx);
    assert.equal(resA.details.ok, true);
    const idA = resA.details.agent_id;

    // Delegate Researcher B
    const resB = await delegate.execute("d2", { role: "researcher", task: "Analyze security" }, undefined, undefined, sessionCtx);
    assert.equal(resB.details.ok, true);
    const idB = resB.details.agent_id;

    // Delegate Reviewer C
    const resC = await delegate.execute("d3", { role: "reviewer", task: "Review findings" }, undefined, undefined, sessionCtx);
    assert.equal(resC.details.ok, true);
    const idC = resC.details.agent_id;

    assert.equal(issuedBootstrapTokens.size, 3, "Host must have issued 3 distinct bootstrap tokens");

    // 2. Simulate Child A, B, C starting up and exchanging their bootstrap credentials
    const childConfigs = [
      { id: idA, role: "agentcabin-researcher", bootstrapToken: issuedBootstrapTokens.get(idA).token },
      { id: idB, role: "agentcabin-researcher", bootstrapToken: issuedBootstrapTokens.get(idB).token },
      { id: idC, role: "agentcabin-reviewer", bootstrapToken: issuedBootstrapTokens.get(idC).token },
    ];

    for (const child of childConfigs) {
      const childTemp = fs.mkdtempSync(path.join(os.tmpdir(), `agentcabin-child-runtime-${child.id}-`));
      process.env.PI_SUBAGENT_CHILD = "1";
      process.env.PI_SUBAGENT_CHILD_AGENT = child.role;
      process.env.PI_SUBAGENT_RUN_ID = child.id;
      process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = child.bootstrapToken;

      const extensionDir = path.join(childTemp, "extensions");
      const typeboxDir = path.join(childTemp, "node_modules", "typebox");
      fs.mkdirSync(extensionDir, { recursive: true });
      fs.mkdirSync(typeboxDir, { recursive: true });
      fs.writeFileSync(path.join(childTemp, "package.json"), '{"type":"module"}\n', "utf8");
      fs.writeFileSync(path.join(typeboxDir, "package.json"), '{"type":"module","exports":"./index.js"}\n', "utf8");
      fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB, "utf8");
      fs.copyFileSync(
        new URL("./pi_core_extension.mjs", import.meta.url),
        path.join(extensionDir, "agentcabin-work-core.mjs"),
      );
      fs.copyFileSync(
        new URL("./pi_workspace_paths.mjs", import.meta.url),
        path.join(extensionDir, "pi_workspace_paths.mjs"),
      );
      fs.copyFileSync(
        new URL("./pi_browser_operator_adapter.mjs", import.meta.url),
        path.join(extensionDir, "pi_browser_operator_adapter.mjs"),
      );
      fs.copyFileSync(
        new URL("./pi_browser_adapter.mjs", import.meta.url),
        path.join(extensionDir, "pi_browser_adapter.mjs"),
      );
      fs.copyFileSync(
        new URL("./agentcabin_computer_use_v2_adapter.mjs", import.meta.url),
        path.join(extensionDir, "agentcabin_computer_use_v2_adapter.mjs"),
      );
      fs.copyFileSync(
        new URL("./computer_use_v2_runtime.mjs", import.meta.url),
        path.join(extensionDir, "computer_use_v2_runtime.mjs"),
      );
      fs.copyFileSync(
        new URL("./computer_use_v3_models.mjs", import.meta.url),
        path.join(extensionDir, "computer_use_v3_models.mjs"),
      );
      fs.copyFileSync(
        new URL("./desktop_computer_use_backend.mjs", import.meta.url),
        path.join(extensionDir, "desktop_computer_use_backend.mjs"),
      );
      fs.copyFileSync(
        new URL("./cdp_computer_use_backend.mjs", import.meta.url),
        path.join(extensionDir, "cdp_computer_use_backend.mjs"),
      );
      fs.copyFileSync(
        new URL("./visual_grounding_backend.mjs", import.meta.url),
        path.join(extensionDir, "visual_grounding_backend.mjs"),
      );
      const runtimeBridgeDir = path.join(extensionDir, "runtime_bridge");
      fs.mkdirSync(runtimeBridgeDir, { recursive: true });
      for (const moduleName of ["bridge_client.mjs", "work_tool_catalog.mjs"]) {
        fs.copyFileSync(
          new URL(`./runtime_bridge/${moduleName}`, import.meta.url),
          path.join(runtimeBridgeDir, moduleName),
        );
      }
      const mod = await import(
        `${pathToFileURL(path.join(extensionDir, "agentcabin-work-core.mjs"))}?test=${Date.now()}-${Math.random()}`
      );
      const coreExt = mod.default;

      const eventHandlers = new Map();
      const tools = new Map();
      coreExt({
        registerTool(t) { tools.set(t.name, t); },
        setActiveTools() {},
        on(ev, fn) { eventHandlers.set(ev, fn); },
      });

      // before_agent_start triggers bootstrap exchange
      assert(eventHandlers.has("before_agent_start"));
      await eventHandlers.get("before_agent_start")({ systemPrompt: "init" });

      // Verify child's bridge token was updated to its scoped token
      const currentToken = process.env.AGENTCABIN_WORK_BRIDGE_TOKEN;
      assert(currentToken.startsWith("scoped-token-"), `Child ${child.id} must hold its scoped token`);
      assert.equal(activeScopedTokens.has(currentToken), true);

      // Verify that replaying the consumed bootstrap token is now rejected
      const replayRes = bridgeHost.exchangeToken(`Bearer ${child.bootstrapToken}`, { agentId: child.id, role: child.role });
      assert.equal(replayRes.status, 403, "Replaying a consumed bootstrap token must return 403 Forbidden");

      fs.rmSync(childTemp, { recursive: true, force: true });
    }

    // 3. Verify Root token can be used in v0.51 compatibility mode for registered child
    const rootExchangeAttempt = bridgeHost.exchangeToken(`Bearer ${ROOT_BEARER_TOKEN}`, { agentId: idA, role: "agentcabin-researcher" });
    assert.equal(rootExchangeAttempt.status, 200, "Root token must be able to exchange for registered child in v0.51 compatibility mode");

    // But unregistered child fails with 404
    const unregAttempt = bridgeHost.exchangeToken(`Bearer ${ROOT_BEARER_TOKEN}`, { agentId: "unregistered-id", role: "agentcabin-researcher" });
    assert.equal(unregAttempt.status, 404, "Root token cannot exchange for unregistered subagent ID");

    assert.equal(consumedBootstrapTokens.size, 3, "All 3 bootstrap tokens must be consumed and retired");
    assert.equal(activeScopedTokens.size, 3, "All 3 children must have active scoped tokens");
  } finally {
    restoreEnv(previous);
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm rejects invalid input schemas, duplicate labels, and task count violations", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-schema-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    assert(swarmTool, "work_research_swarm must be registered");

    // 1. Empty objective
    const emptyObj = await swarmTool.execute("c1", {
      objective: "",
      tasks: [{ label: "a", task: "task a" }, { label: "b", task: "task b" }],
    });
    assert.equal(emptyObj.details.ok, false);
    assert.match(emptyObj.details.message, /objective cannot be empty/i);

    // 2. 1 task (less than 2)
    const oneTask = await swarmTool.execute("c2", {
      objective: "investigate",
      tasks: [{ label: "a", task: "task a" }],
    });
    assert.equal(oneTask.details.ok, false);
    assert.match(oneTask.details.message, /2 to 3/);

    // 3. 4 tasks (greater than 3)
    const fourTasks = await swarmTool.execute("c3", {
      objective: "investigate",
      tasks: [
        { label: "a", task: "task a" },
        { label: "b", task: "task b" },
        { label: "c", task: "task c" },
        { label: "d", task: "task d" },
      ],
    });
    assert.equal(fourTasks.details.ok, false);
    assert.match(fourTasks.details.message, /2 to 3/);

    // 4. Empty label
    const emptyLabel = await swarmTool.execute("c4", {
      objective: "investigate",
      tasks: [
        { label: "", task: "task a" },
        { label: "b", task: "task b" },
      ],
    });
    assert.equal(emptyLabel.details.ok, false);
    assert.match(emptyLabel.details.message, /non-empty label/i);

    // 5. Duplicate label
    const dupLabel = await swarmTool.execute("c5", {
      objective: "investigate",
      tasks: [
        { label: "arch", task: "task a" },
        { label: "arch", task: "task b" },
      ],
    });
    assert.equal(dupLabel.details.ok, false);
    assert.match(dupLabel.details.message, /duplicate task label 'arch'/i);

    // 6. Empty task description
    const emptyTask = await swarmTool.execute("c6", {
      objective: "investigate",
      tasks: [
        { label: "arch", task: "task a" },
        { label: "sec", task: "  " },
      ],
    });
    assert.equal(emptyTask.details.ok, false);
    assert.match(emptyTask.details.message, /empty task description/i);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm preflights all tasks before any spawn and fails closed if one fails", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-preflight-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Security analysis",
        tasks: [
          { label: "auth", task: "Analyze auth system" },
          { label: "crypto", task: "Analyze crypto fail-preflight" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, false);
    assert.match(res.details.message, /Preflight failed for research dimension 'crypto'/);
    // Must NOT have spawned ANY child RPC!
    assert.equal(mockPi.rpcMethods.filter((m) => m === "spawn").length, 0, "No spawn RPC must be sent if preflight fails");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm executes true parallel launch order before waiting", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-parallel-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const adapter = await loadAdapter(temp);
    const mockPi = createMockPi();
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Architecture risk analysis",
        tasks: [
          { label: "dim_a", task: "Analyze dimension A" },
          { label: "dim_b", task: "Analyze dimension B" },
          { label: "dim_c", task: "Analyze dimension C" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "completed");
    assert.equal(res.details.findings.length, 3);

    // Verify RPC method ordering: spawn, spawn, spawn before any status
    const methods = mockPi.rpcMethods;
    const firstStatusIdx = methods.indexOf("status");
    const spawns = methods.slice(0, 3);
    assert.deepEqual(spawns, ["spawn", "spawn", "spawn"], "All 3 spawns must happen before wait phase");
    assert(firstStatusIdx >= 3, "No status check must occur before all 3 spawns complete");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm preserves input dimension order despite out-of-order child completion", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-order-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    let pollCountRun1 = 0;
    let pollCountRun2 = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-3") {
            // Finishes first
            return { success: true, data: { text: "Finding C", details: { state: "completed", result: "Finding C" } } };
          }
          if (id === "subagent-run-1") {
            pollCountRun1 += 1;
            if (pollCountRun1 >= 2) {
              return { success: true, data: { text: "Finding A", details: { state: "completed", result: "Finding A" } } };
            }
            return { success: true, data: { text: "running", details: { state: "running" } } };
          }
          if (id === "subagent-run-2") {
            pollCountRun2 += 1;
            if (pollCountRun2 >= 3) {
              return { success: true, data: { text: "Finding B", details: { state: "completed", result: "Finding B" } } };
            }
            return { success: true, data: { text: "running", details: { state: "running" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Multi-dimension audit",
        tasks: [
          { label: "dim_A", task: "Task A" },
          { label: "dim_B", task: "Task B" },
          { label: "dim_C", task: "Task C" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "completed");
    const findings = res.details.findings;
    assert.equal(findings.length, 3);
    // Even though C completed first, the output must strictly preserve [dim_A, dim_B, dim_C]
    assert.equal(findings[0].label, "dim_A");
    assert.equal(findings[0].agent_id, "subagent-run-1");
    assert.equal(findings[0].result, "Finding A");

    assert.equal(findings[1].label, "dim_B");
    assert.equal(findings[1].agent_id, "subagent-run-2");
    assert.equal(findings[1].result, "Finding B");

    assert.equal(findings[2].label, "dim_C");
    assert.equal(findings[2].agent_id, "subagent-run-3");
    assert.equal(findings[2].result, "Finding C");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm handles partial failure without losing completed findings", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-partial-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Findings for A", details: { state: "completed", result: "Findings for A" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "Error during B", details: { state: "failed", error: "Fatal error in B" } } };
          }
          if (id === "subagent-run-3") {
            return { success: true, data: { text: "Findings for C", details: { state: "completed", result: "Findings for C" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Partial test",
        tasks: [
          { label: "dim_A", task: "Task A" },
          { label: "dim_B", task: "Task B" },
          { label: "dim_C", task: "Task C" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "partial", "Overall status must be partial when some succeed and some fail");
    const findings = res.details.findings;
    assert.equal(findings[0].status, "completed");
    assert.equal(findings[0].result, "Findings for A");

    assert.equal(findings[1].status, "failed");
    assert.match(findings[1].result || findings[1].error, /Fatal error in B/);

    assert.equal(findings[2].status, "completed");
    assert.equal(findings[2].result, "Findings for C");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm handles timeout without auto-stopping or marking completed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-timeout-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Findings A", details: { state: "completed", result: "Findings A" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "Still running", details: { state: "running" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Timeout test",
        tasks: [
          { label: "dim_A", task: "Task A" },
          { label: "dim_B", task: "Task B" },
        ],
        timeout_seconds: 1,
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete", "Overall status must be incomplete on timeout");
    const findings = res.details.findings;
    assert.equal(findings[0].status, "completed");
    assert.equal(findings[1].status, "running");
    assert.equal(findings[1].timeout, true);

    // Timeout child must NOT be stopped automatically
    assert.equal(mockPi.rpcMethods.filter((m) => m === "stop").length, 0, "Timeout subagent must not be auto-stopped");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm performs compensating stops on batch spawn failure", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-comp-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";

  const bridgeUpdates = [];
  globalThis.fetch = async (url, options) => {
    if (String(url).includes("/internal/work/subagents/update_status")) {
      bridgeUpdates.push(JSON.parse(options.body));
      return { ok: true, status: 200, async json() { return { ok: true }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } };
  };

  try {
    let spawnAttempts = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnAttempts += 1;
          if (spawnAttempts === 3) {
            // 3rd spawn fails
            return { success: false, error: { message: "Simulated spawn failure on child 3" } };
          }
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnAttempts}` } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Rollback test",
        tasks: [
          { label: "dim_1", task: "Task 1" },
          { label: "dim_2", task: "Task 2" },
          { label: "dim_3", task: "Task 3" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, false);
    assert.equal(res.details.status, "spawn_failed");
    assert.equal(res.details.compensated, true);
    assert.deepEqual(res.details.spawned_children, ["subagent-run-1", "subagent-run-2"]);

    // Verify stop RPC was called for child 1 and 2
    const stops = mockPi.rpcCalls.filter((c) => c.method === "stop").map((c) => c.params?.id);
    assert.deepEqual(stops, ["subagent-run-1", "subagent-run-2"], "Compensating stop must be called for already spawned children");

    // Verify bridge status updates recorded stopped for both
    const stoppedChildren = bridgeUpdates.filter((u) => u.status === "stopped").map((u) => u.agentId);
    assert.deepEqual(stoppedChildren, ["subagent-run-1", "subagent-run-2"]);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm full integration with 3 parallel researchers and bridge authority", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-e2e-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;

  const registeredSpawns = [];
  const statusUpdates = [];
  const outputFiles = new Map();

  globalThis.fetch = async (url, options) => {
    const urlStr = String(url);
    if (urlStr.includes("/internal/work/subagents/register_spawn")) {
      const body = JSON.parse(options.body);
      registeredSpawns.push(body);
      return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: `bootstrap-${body.agentId}` }; } };
    }
    if (urlStr.includes("/internal/work/subagents/update_status")) {
      const body = JSON.parse(options.body);
      statusUpdates.push(body);
      return { ok: true, status: 200, async json() { return { ok: true }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    const mockPi = createMockPi({
      rpcHandler(req, { spawnCount: currentSpawnCount }) {
        if (req.method === "spawn") {
          const id = `subagent-run-${currentSpawnCount + 1}`;
          const outputMatch = String(req.params?.workflowScript || "").match(/output: ("[^"]+")/);
          assert.ok(outputMatch, "research child must receive an in-workspace output path");
          const outputPath = path.resolve(temp, JSON.parse(outputMatch[1]));
          fs.mkdirSync(path.dirname(outputPath), { recursive: true });
          fs.writeFileSync(outputPath, `Full child result for ${id}`, "utf8");
          outputFiles.set(id, outputPath);
          return {
            success: true,
            data: { text: "spawned", details: { runId: id } },
          };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          return {
            success: true,
            data: {
              // pi-subagents exposes the lifecycle state in the formatted
              // status text; details contains mode/results but no state.
              text: `Run: ${id}\nState: complete\nMode: workflow\nDetailed evidence for ${id}`,
              details: {
                mode: "single",
                results: [],
              },
            },
          };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Comprehensive Work Architecture Audit",
        tasks: [
          { label: "authority", task: "Check bridge token and authority boundary" },
          { label: "lifecycle", task: "Check subagent lifecycle and recovery" },
          { label: "concurrency", task: "Check concurrency gate and isolation" },
        ],
        timeout_seconds: 5,
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.mode, "research_swarm");
    assert.equal(res.details.status, "completed");
    assert.equal(res.details.findings.length, 3);
    assert.equal(
      res.details.findings.every((finding) => finding.result.includes("Full child result")),
      true,
      "Completed child output must be embedded without reading the private async directory",
    );

    // 3 registered spawns on bridge
    assert.equal(registeredSpawns.length, 3);
    assert.equal(registeredSpawns[0].role, "researcher");
    assert.equal(registeredSpawns[1].role, "researcher");
    assert.equal(registeredSpawns[2].role, "researcher");
    assert.equal(registeredSpawns[0].agentId, "subagent-run-1");
    assert.equal(registeredSpawns[1].agentId, "subagent-run-2");
    assert.equal(registeredSpawns[2].agentId, "subagent-run-3");

    // 3 terminal updates on bridge
    assert.equal(statusUpdates.length, 3);
    assert.equal(statusUpdates.every((u) => u.status === "completed"), true);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("waitForChild and work_agent_wait fail closed when Host Bridge update_status fails", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-authority-err-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";

  globalThis.fetch = async (url) => {
    const urlStr = String(url);
    if (urlStr.includes("/internal/work/subagents/register_spawn")) {
      return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-1" }; } };
    }
    if (urlStr.includes("/internal/work/subagents/update_status")) {
      // Simulate Host Bridge failure on finalization
      return { ok: false, status: 500, async json() { return { ok: false, error: "Database lock error" }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "status") {
          return { success: true, data: { text: "Completed payload", details: { state: "completed", result: "Completed payload" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const waitTool = mockPi.tools.get("work_agent_wait");

    // Spawn child
    const spawnRes = await delegateTool.execute(
      "c1",
      { role: "researcher", task: "Investigation" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(spawnRes.details.ok, true);

    // Wait on child -> Host Bridge status update will fail
    const waitRes = await waitTool.execute("c2", { agent_id: "subagent-run-1" });
    assert.equal(waitRes.details.ok, false, "work_agent_wait must fail if Host Bridge status update fails");
    assert.match(waitRes.details.message, /记录子代理终态失败/);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm reports honest compensation status when batch stop RPC fails", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-stop-fail-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";

  const bridgeUpdates = [];
  globalThis.fetch = async (url, options) => {
    const urlStr = String(url);
    if (urlStr.includes("/internal/work/subagents/update_status")) {
      bridgeUpdates.push(JSON.parse(options.body));
      return { ok: true, status: 200, async json() { return { ok: true }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } };
  };

  try {
    let spawnAttempts = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnAttempts += 1;
          if (spawnAttempts === 3) {
            return { success: false, error: { message: "Simulated 3rd spawn fail" } };
          }
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnAttempts}` } } };
        }
        if (req.method === "stop") {
          if (req.params?.id === "subagent-run-1") {
            // Stop RPC for subagent-run-1 fails!
            return { success: false, error: { message: "Network timeout on stop RPC" } };
          }
          return { success: true, data: { text: "stopped", details: { state: "stopped" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const statusTool = mockPi.tools.get("work_agent_status");

    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Stop RPC failure test",
        tasks: [
          { label: "dim_1", task: "Task 1" },
          { label: "dim_2", task: "Task 2" },
          { label: "dim_3", task: "Task 3" },
        ],
      },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, false);
    assert.equal(res.details.status, "spawn_failed");
    assert.equal(res.details.compensated, false, "compensated must be false if any stop RPC fails");

    const comp1 = res.details.compensation.find((c) => c.agent_id === "subagent-run-1");
    const comp2 = res.details.compensation.find((c) => c.agent_id === "subagent-run-2");
    assert.equal(comp1.stopped, false);
    assert.match(comp1.error, /Network timeout on stop RPC/);
    assert.equal(comp2.stopped, true);

    // subagent-run-1 must NOT have been finalized to stopped on Host Bridge
    assert.equal(bridgeUpdates.some((u) => u.agentId === "subagent-run-1"), false);
    // subagent-run-2 must have been finalized to stopped on Host Bridge
    assert.equal(bridgeUpdates.some((u) => u.agentId === "subagent-run-2" && u.status === "stopped"), true);

    // subagent-run-1 remains manageable in activeChildren
    const statusRes = await statusTool.execute("c2");
    assert.equal(statusRes.details.active_children.some((c) => c.id === "subagent-run-1"), true);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_research_swarm handles wait signal abort with incomplete status and retains running child", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-swarm-abort-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "test-bootstrap-token" }; } });
  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "status") {
          return { success: true, data: { text: "running", details: { state: "running" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const swarmTool = mockPi.tools.get("work_research_swarm");
    const statusTool = mockPi.tools.get("work_agent_status");

    // Aborted signal passed to swarm execute
    const abortController = new AbortController();
    abortController.abort();

    const res = await swarmTool.execute(
      "c1",
      {
        objective: "Abort test",
        tasks: [
          { label: "dim_1", task: "Task 1" },
          { label: "dim_2", task: "Task 2" },
        ],
      },
      abortController.signal,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete", "Aborted swarm must be incomplete");
    assert.equal(res.details.findings[0].wait_aborted, true);
    assert.equal(res.details.findings[0].status, "running");

    // Children are still running and manageable
    const statusRes = await statusTool.execute("c2");
    assert.equal(statusRes.details.active_children.length, 2);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

// =========================================================================
// P2.2 Implement → Review → Fix Test Cases (1 to 13)
// =========================================================================

test("P2.2 Case 1: Happy path without fix (Worker -> Reviewer PASS -> passed)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c1-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    const spawnedRuns = [];
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          const runId = `subagent-run-${spawnedRuns.length + 1}`;
          spawnedRuns.push({ runId, script: req.params?.workflowScript });
          return { success: true, data: { text: "ok", details: { runId } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker implementation report: all done", details: { state: "completed", result: "Worker implementation report: all done" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "VERDICT: PASS\n\nSUMMARY: Implementation is clean and tested.", details: { state: "completed", result: "VERDICT: PASS\n\nSUMMARY: Implementation is clean and tested." } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Add secure password validation function", review_focus: "Check regex safety" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.mode, "implement_review_fix");
    assert.equal(res.details.status, "passed");
    assert.equal(res.details.final_review_verdict, "pass");
    assert.equal(res.details.fix_rounds_used, 0);
    assert.equal(res.details.exhausted_fix_rounds, false);
    assert.equal(res.details.stages.length, 2);
    assert.equal(res.details.stages[0].kind, "implementation");
    assert.equal(res.details.stages[0].role, "worker");
    assert.equal(res.details.stages[1].kind, "review");
    assert.equal(res.details.stages[1].role, "reviewer");
    assert.equal(res.details.stages[1].verdict, "pass");
    assert.equal(spawnedRuns.length, 2);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 2: Review -> Fix -> Re-review PASS (4 sequential spawns)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c2-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    const spawnedRuns = [];
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          const runId = `subagent-run-${spawnedRuns.length + 1}`;
          spawnedRuns.push({ runId, script: req.params?.workflowScript });
          return { success: true, data: { text: "ok", details: { runId } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Initial implementation done", details: { state: "completed", result: "Initial implementation done" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Missing null check in auth header parser", details: { state: "completed", result: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Missing null check in auth header parser" } } };
          }
          if (id === "subagent-run-3") {
            return { success: true, data: { text: "Fixed null check and added regression test", details: { state: "completed", result: "Fixed null check and added regression test" } } };
          }
          if (id === "subagent-run-4") {
            return { success: true, data: { text: "VERDICT: PASS\n\nSUMMARY: All blocking issues resolved.", details: { state: "completed", result: "VERDICT: PASS\n\nSUMMARY: All blocking issues resolved." } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Refactor auth token parser" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "passed");
    assert.equal(res.details.final_review_verdict, "pass");
    assert.equal(res.details.fix_rounds_used, 1);
    assert.equal(res.details.exhausted_fix_rounds, false);
    assert.equal(res.details.stages.length, 4);

    assert.equal(res.details.stages[0].kind, "implementation");
    assert.equal(res.details.stages[0].role, "worker");
    assert.equal(res.details.stages[1].kind, "review");
    assert.equal(res.details.stages[1].role, "reviewer");
    assert.equal(res.details.stages[1].verdict, "needs_changes");
    assert.equal(res.details.stages[2].kind, "fix");
    assert.equal(res.details.stages[2].role, "worker");
    assert.equal(res.details.stages[3].kind, "re_review");
    assert.equal(res.details.stages[3].role, "reviewer");
    assert.equal(res.details.stages[3].verdict, "pass");

    assert.equal(spawnedRuns.length, 4);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 3: Re-review still NEEDS_CHANGES stops at exactly 4 spawns", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c3-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1" || id === "subagent-run-3") {
            return { success: true, data: { text: "worker work", details: { state: "completed", result: "worker work" } } };
          }
          if (id === "subagent-run-2" || id === "subagent-run-4") {
            return { success: true, data: { text: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Critical flaw remains", details: { state: "completed", result: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Critical flaw remains" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Complex cryptographic refactoring" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "needs_changes");
    assert.equal(res.details.final_review_verdict, "needs_changes");
    assert.equal(res.details.fix_rounds_used, 1);
    assert.equal(res.details.exhausted_fix_rounds, true);
    assert.equal(res.details.stages.length, 4);
    assert.equal(spawnCount, 4, "Must strictly stop at 4 spawns without attempting a 5th spawn");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 4: Malformed review output fails closed (review_error, no fix spawned)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c4-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Done", details: { state: "completed", result: "Done" } } };
          }
          if (id === "subagent-run-2") {
            // Unstructured feedback without VERDICT line
            return { success: true, data: { text: "Looks good to me, but please check lines 10-20.", details: { state: "completed", result: "Looks good to me, but please check lines 10-20." } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Update styling" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert.equal(res.details.final_review_verdict, null);
    assert.equal(spawnCount, 2, "Must stop immediately on unparseable review verdict without spawning fix");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 5: Implementation timeout (incomplete, worker remains active, no reviewer spawned)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c5-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          return { success: true, data: { text: "still compiling", details: { state: "running" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const statusTool = mockPi.tools.get("work_agent_status");

    const res = await irfTool.execute(
      "c1",
      { task: "Long running build task", timeout_seconds: 1 },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete");
    assert.equal(res.details.stages[0].timeout, true);
    assert.equal(spawnCount, 1, "Reviewer must NOT be spawned when worker implementation times out");

    // Worker remains active in activeChildren
    const statusRes = await statusTool.execute("c2");
    assert.equal(statusRes.details.active_children.length, 1);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 6: Review timeout (incomplete, reviewer remains active, no fix spawned)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c6-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "Reviewer reading files...", details: { state: "running" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const statusTool = mockPi.tools.get("work_agent_status");

    const res = await irfTool.execute(
      "c1",
      { task: "Review timeout task", timeout_seconds: 1 },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete");
    assert.equal(res.details.stages.length, 2);
    assert.equal(res.details.stages[1].timeout, true);
    assert.equal(spawnCount, 2, "No fix worker must be spawned on reviewer timeout");

    const statusRes = await statusTool.execute("c2");
    assert.equal(statusRes.details.active_children.length, 1);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 7: Wait abort (incomplete, child remains running, no next stage)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c7-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  const abortController = new AbortController();

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          abortController.abort();
          return { success: true, data: { text: "running", details: { state: "running" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");

    const res = await irfTool.execute(
      "c1",
      { task: "Aborted task" },
      abortController.signal,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete");
    assert.equal(res.details.stages[0].wait_aborted, true);
    assert.equal(spawnCount, 1, "No further stage spawned after abort");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 8: Host authority failure (authority_error, no next stage spawned)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c8-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";

  globalThis.fetch = async (url) => {
    const urlStr = String(url);
    if (urlStr.includes("/internal/work/subagents/register_spawn")) {
      return { ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-1" }; } };
    }
    if (urlStr.includes("/internal/work/subagents/update_status")) {
      return { ok: false, status: 500, async json() { return { ok: false, error: "Host bridge DB write failed" }; } };
    }
    return { ok: true, status: 200, async json() { return { ok: true }; } };
  };

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Authority fail test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "authority_error");
    assert.equal(spawnCount, 1, "Reviewer must NOT be spawned if worker terminal status failed to write to Host");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 9: Worker implementation failure (implementation_failed, no reviewer)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c9-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          return { success: true, data: { text: "Fatal syntax error in generated code", details: { state: "failed", error: "SyntaxError" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Worker fail test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "implementation_failed");
    assert.equal(spawnCount, 1, "Reviewer must NOT be spawned when worker fails");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 10: Fix failure (fix_failed, no re-review spawned)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c10-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Initial worker done", details: { state: "completed", result: "Initial worker done" } } };
          }
          if (id === "subagent-run-2") {
            return { success: true, data: { text: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Fix bug", details: { state: "completed", result: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Fix bug" } } };
          }
          if (id === "subagent-run-3") {
            return { success: true, data: { text: "Fix crashed with test failure", details: { state: "failed", error: "Test failure" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Fix fail test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "fix_failed");
    assert.equal(spawnCount, 3, "Re-review must NOT be spawned when fix worker fails");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 11: No active children invariant (rejects when child is active, unharmed)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c11-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    const mockPi = createMockPi();
    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const statusTool = mockPi.tools.get("work_agent_status");

    // Spawn a researcher child
    const delRes = await delegateTool.execute(
      "c1",
      { role: "researcher", task: "Ongoing background investigation" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(delRes.details.ok, true);

    // Calling IRF must be rejected
    const irfRes = await irfTool.execute(
      "c2",
      { task: "Attempt to run IRF with active subagent" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(irfRes.details.ok, false);
    assert.match(irfRes.details.message, /Cannot start Implement → Review → Fix while other subagents are active/);

    // Existing active child is preserved
    const statusRes = await statusTool.execute("c3");
    assert.equal(statusRes.details.active_children.length, 1);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 12: Full budget reservation (fails closed if total run budget < 4)", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c12-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          return { success: true, data: { text: "completed", details: { state: "completed", result: "completed" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const waitTool = mockPi.tools.get("work_agent_wait");
    const irfTool = mockPi.tools.get("work_implement_review_fix");

    // Spawn and complete 5 subagents in separate turns
    for (let i = 0; i < 5; i++) {
      // Simulate new turn by resetting turnSpawnCount via turn_start event
      mockPi.emitLifecycle("turn_start");
      const dRes = await delegateTool.execute(
        `d${i}`,
        { role: "researcher", task: `Investigation ${i}` },
        undefined,
        undefined,
        { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
      );
      assert.equal(dRes.details.ok, true);
      const wRes = await waitTool.execute(`w${i}`, { agent_id: `subagent-run-${i + 1}` });
      assert.equal(wRes.details.ok, true);
    }

    // Now totalSpawnCount is 5. Max total is 8. Available budget is 3, which is < 4.
    mockPi.emitLifecycle("turn_start");
    const irfRes = await irfTool.execute(
      "irf1",
      { task: "Should be rejected due to budget" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(irfRes.details.ok, false);
    assert.match(irfRes.details.message, /insufficient total run spawn budget/);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Case 13: Generic P1/P2.1 delegation and swarm still respect MAX_CHILDREN_PER_TURN = 3", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-c13-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          return { success: true, data: { text: "done", details: { state: "completed", result: "done" } } };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const waitTool = mockPi.tools.get("work_agent_wait");

    // Spawn 3 delegates in same turn
    for (let i = 0; i < 3; i++) {
      const dRes = await delegateTool.execute(
        `d${i}`,
        { role: "researcher", task: `Investigation ${i}` },
        undefined,
        undefined,
        { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
      );
      assert.equal(dRes.details.ok, true);
      const wRes = await waitTool.execute(`w${i}`, { agent_id: `subagent-run-${i + 1}` });
      assert.equal(wRes.details.ok, true);
    }

    // 4th delegate in same turn must be rejected by MAX_CHILDREN_PER_TURN = 3
    const fourthRes = await delegateTool.execute(
      "d4",
      { role: "researcher", task: "Fourth investigation in same turn" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(fourthRes.details.ok, false);
    assert.match(fourthRes.details.message, /Exceeded max child spawns per turn/);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Hardening: Reviewer verdict in output file is parsed correctly despite wrapper status text", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-outputfile-"));
  const outputDir = path.join(temp, "output");
  fs.mkdirSync(outputDir, { recursive: true });
  const outputFile = path.join(outputDir, "reviewer_output.md");
  fs.writeFileSync(outputFile, "VERDICT: PASS\n\nSUMMARY: Code quality meets all standards.", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker implementation done", details: { state: "completed", result: "Worker implementation done" } } };
          }
          if (id === "subagent-run-2") {
            // Realistic pi-subagents status text wrapping output file path
            return {
              success: true,
              data: {
                text: `Subagent completed. Output saved to ${outputFile}`,
                details: {
                  state: "completed",
                  outputFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Implement feature with output file review" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "passed", "Must pass by extracting VERDICT: PASS from output file");
    assert.equal(res.details.final_review_verdict, "pass");
    assert.equal(spawnCount, 2);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Hardening: Stage transition cancellation prevents spawning subsequent Reviewer", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-cancel-transition-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  const abortController = new AbortController();

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            // Trigger abort when Worker completes, simulating cancel right at stage transition
            abortController.abort();
            return { success: true, data: { text: "Worker implementation done", details: { state: "completed", result: "Worker implementation done" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Stage boundary cancellation test" },
      abortController.signal,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete");
    assert.equal(spawnCount, 1, "Must NOT spawn Reviewer after user cancellation at stage boundary");
    assert.equal(res.details.stages.length, 1, "Only Worker stage should be recorded");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("P2.2 Hardening: Stage transition cancellation after Reviewer NEEDS_CHANGES prevents Fix Worker spawn", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-irf-cancel-fix-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  const abortController = new AbortController();

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            // Trigger abort when Reviewer completes with NEEDS_CHANGES
            abortController.abort();
            return { success: true, data: { text: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Issue 1", details: { state: "completed", result: "VERDICT: NEEDS_CHANGES\n\nISSUES:\n- Issue 1" } } };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Cancel before fix" },
      abortController.signal,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "incomplete");
    assert.equal(spawnCount, 2, "Must NOT spawn Fix Worker after cancellation at stage boundary");
    assert.equal(res.details.stages.length, 2);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

// =========================================================================
// Release Hardening: Subagent Output File Authority Boundary Tests
// =========================================================================

test("Release Hardening Case 2: Outside path in outputFile is rejected and never read", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case2-"));
  const outsideDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-outside-secret-"));
  const outsideFile = path.join(outsideDir, "secret_output.md");
  fs.writeFileSync(outsideFile, "VERDICT: PASS\n\nSECRET_DATA: highly_confidential", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: `Reviewer finished. Output saved to ${outsideFile}`,
                details: {
                  state: "completed",
                  outputFile: outsideFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Outside path rejection test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    // Because outside file was NOT read, output_content is null, and status text has no verdict -> review_error
    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert.equal(res.details.final_review_verdict, null);
    assert(!JSON.stringify(res).includes("SECRET_DATA"), "Secret data from outside file must NEVER be leaked or embedded");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
    fs.rmSync(outsideDir, { recursive: true, force: true });
  }
});

test("Release Hardening Case 3: Text-injected path outside workspace is rejected", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case3-"));
  const outsideDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-outside-text-"));
  const outsideFile = path.join(outsideDir, "injected_output.md");
  fs.writeFileSync(outsideFile, "VERDICT: PASS\n\nSECRET: injected_file_content", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            // Text mentions outside path via regex matching pattern
            return {
              success: true,
              data: {
                text: `Review completed. File is located at ${outsideFile}`,
                details: { state: "completed" },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Text regex injection test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert(!JSON.stringify(res).includes("injected_file_content"), "Text-injected outside path must NOT be read");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
    fs.rmSync(outsideDir, { recursive: true, force: true });
  }
});

test("Release Hardening Case 4: Symlink escaping workspace is rejected", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case4-"));
  const outsideDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-outside-symlink-"));
  const outsideSecret = path.join(outsideDir, "target_secret.txt");
  fs.writeFileSync(outsideSecret, "VERDICT: PASS\n\nSYMLINK_ESCAPE_PAYLOAD", "utf8");

  // Create symlink inside legitimate output/ workspace area pointing outside
  const linkDir = path.join(temp, "output", "safe_links");
  fs.mkdirSync(linkDir, { recursive: true });
  const symlinkFile = path.join(linkDir, "link_output.md");
  fs.symlinkSync(outsideSecret, symlinkFile);

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review completed.",
                details: {
                  state: "completed",
                  outputFile: symlinkFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Symlink escape test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert(!JSON.stringify(res).includes("SYMLINK_ESCAPE_PAYLOAD"), "Symlink escape target must NEVER be read");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
    fs.rmSync(outsideDir, { recursive: true, force: true });
  }
});

test("Release Hardening Case 5: Nested legitimate output file in workspace is accepted", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case5-"));
  const nestedDir = path.join(temp, "output", "subagents", "run-123");
  fs.mkdirSync(nestedDir, { recursive: true });
  const nestedFile = path.join(nestedDir, "reviewer_output.md");
  fs.writeFileSync(nestedFile, "VERDICT: PASS\n\nSUMMARY: Nested file verified successfully.", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review complete.",
                details: {
                  state: "completed",
                  outputFile: nestedFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Nested file test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "passed");
    assert.equal(res.details.final_review_verdict, "pass");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 6: Missing output file candidate handles gracefully without crash", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case6-"));
  const nonExistentFile = path.join(temp, "output", "does_not_exist_output.md");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          return { success: true, data: { text: "ok", details: { runId: "subagent-run-1" } } };
        }
        if (req.method === "status") {
          return {
            success: true,
            data: {
              text: "Researcher investigation findings.",
              details: {
                state: "completed",
                outputFile: nonExistentFile,
              },
            },
          };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const waitTool = mockPi.tools.get("work_agent_wait");

    const delRes = await delegateTool.execute(
      "c1",
      { role: "researcher", task: "Investigation with missing output file" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );
    assert.equal(delRes.details.ok, true);

    const waitRes = await waitTool.execute("c2", { agent_id: "subagent-run-1" });
    assert.equal(waitRes.details.ok, true);
    assert.equal(waitRes.details.status, "completed");
    assert.equal(waitRes.details.result, "Researcher investigation findings.");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 7: Oversized output file exceeds limit and is safely skipped", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case7-"));
  const outputDir = path.join(temp, "output");
  fs.mkdirSync(outputDir, { recursive: true });
  const largeFile = path.join(outputDir, "huge_output.md");
  // Write 1.5 MB of data
  fs.writeFileSync(largeFile, Buffer.alloc(1.5 * 1024 * 1024, "a"));

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          return { success: true, data: { text: "ok", details: { runId: "subagent-run-1" } } };
        }
        if (req.method === "status") {
          return {
            success: true,
            data: {
              text: "Summary text.",
              details: {
                state: "completed",
                outputFile: largeFile,
              },
            },
          };
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const delegateTool = mockPi.tools.get("work_delegate");
    const waitTool = mockPi.tools.get("work_agent_wait");

    await delegateTool.execute(
      "c1",
      { role: "researcher", task: "Oversized file test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    const waitRes = await waitTool.execute("c2", { agent_id: "subagent-run-1" });
    assert.equal(waitRes.details.ok, true);
    assert.equal(waitRes.details.status, "completed");
    // Oversized content is NOT embedded; summary text is retained
    assert.equal(waitRes.details.result, "Summary text.");
    assert.equal(waitRes.details.output_content ?? null, null);
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 9: Workspace root file outside input/scratch/output/context is rejected", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case9-"));
  const rootOutputFile = path.join(temp, "root_reviewer_output.md");
  fs.writeFileSync(rootOutputFile, "VERDICT: PASS\n\nDATA: should_not_be_read", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review complete.",
                details: {
                  state: "completed",
                  outputFile: rootOutputFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Workspace root file test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    // Root file is rejected by normalizeWorkPath -> review_error
    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert(!JSON.stringify(res).includes("should_not_be_read"));
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 10: Profile non-skills file is rejected", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case10-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  const privateProfileFile = path.join(profile, "private_profile.md");
  fs.writeFileSync(privateProfileFile, "VERDICT: PASS\n\nSECRET: profile_secret", "utf8");

  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review complete.",
                details: {
                  state: "completed",
                  outputFile: privateProfileFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Profile non-skills file test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
    assert(!JSON.stringify(res).includes("profile_secret"));
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 11: Missing AGENTCABIN_WORKSPACE_ROOT fails closed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case11-"));
  const outputFile = path.join(temp, "reviewer_output.md");
  fs.writeFileSync(outputFile, "VERDICT: PASS\n\nSUMMARY: ok", "utf8");

  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(temp);
  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  // Explicitly delete AGENTCABIN_WORKSPACE_ROOT
  delete process.env.AGENTCABIN_WORKSPACE_ROOT;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review complete.",
                details: {
                  state: "completed",
                  outputFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Missing workspace root test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    // Fail closed -> review_error
    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "review_error");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Release Hardening Case 12: Trusted <profile>/skills/.../output.md is accepted", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case12-ws-"));
  const profileDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-h-case12-profile-"));
  const previous = {
    child: process.env.PI_SUBAGENT_CHILD,
    piDir: process.env.PI_CODING_AGENT_DIR,
    workProfile: process.env.AGENTCABIN_WORK_PROFILE_DIR,
    expectedDigests: process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    fetch: globalThis.fetch,
  };
  delete process.env.PI_SUBAGENT_CHILD;
  const profile = preparePreflightProfile(profileDir);
  const skillDir = path.join(profile, "skills", "review_skill");
  fs.mkdirSync(skillDir, { recursive: true });
  const skillOutputFile = path.join(skillDir, "reviewer_output.md");
  fs.writeFileSync(skillOutputFile, "VERDICT: PASS\n\nSUMMARY: Trusted skill review passed.", "utf8");

  process.env.PI_CODING_AGENT_DIR = profile;
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
  process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = JSON.stringify(systemAgentFileDigests(profile));
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.AGENTCABIN_WORKSPACE_ROOT = temp;
  globalThis.fetch = async () => ({ ok: true, status: 200, async json() { return { ok: true, bootstrapToken: "token-test" }; } });

  try {
    let spawnCount = 0;
    const mockPi = createMockPi({
      rpcHandler(req) {
        if (req.method === "spawn") {
          spawnCount += 1;
          return { success: true, data: { text: "ok", details: { runId: `subagent-run-${spawnCount}` } } };
        }
        if (req.method === "status") {
          const id = req.params?.id;
          if (id === "subagent-run-1") {
            return { success: true, data: { text: "Worker done", details: { state: "completed", result: "Worker done" } } };
          }
          if (id === "subagent-run-2") {
            return {
              success: true,
              data: {
                text: "Review complete.",
                details: {
                  state: "completed",
                  outputFile: skillOutputFile,
                },
              },
            };
          }
        }
        return undefined;
      },
    });

    const adapter = await loadAdapter(temp);
    adapter(mockPi);

    const irfTool = mockPi.tools.get("work_implement_review_fix");
    const res = await irfTool.execute(
      "c1",
      { task: "Trusted skills output test" },
      undefined,
      undefined,
      { cwd: temp, sessionManager: { getSessionId: () => "root-session" } },
    );

    assert.equal(res.details.ok, true);
    assert.equal(res.details.status, "passed");
    assert.equal(res.details.final_review_verdict, "pass");
  } finally {
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.piDir === undefined) delete process.env.PI_CODING_AGENT_DIR; else process.env.PI_CODING_AGENT_DIR = previous.piDir;
    if (previous.workProfile === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR; else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.workProfile;
    if (previous.expectedDigests === undefined) delete process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS; else process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS = previous.expectedDigests;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
    fs.rmSync(profileDir, { recursive: true, force: true });
  }
});
