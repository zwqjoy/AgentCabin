import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT_BEARER_TOKEN = "root-work-token-smoke";

const WORK_DIR = import.meta.dirname;

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

function setupChildEnvironment(tempDir) {
  const extensionDir = path.join(tempDir, "extensions");
  const typeboxDir = path.join(tempDir, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempDir, "package.json"), '{"type":"module"}\n', "utf8");
  fs.writeFileSync(
    path.join(tempDir, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
    "utf8",
  );
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB, "utf8");

  const extDst = path.join(extensionDir, "agentcabin-work-core.mjs");
  const pathsDst = path.join(extensionDir, "pi_workspace_paths.mjs");
  fs.copyFileSync(path.join(WORK_DIR, "pi_core_extension.mjs"), extDst);
  fs.copyFileSync(path.join(WORK_DIR, "pi_workspace_paths.mjs"), pathsDst);
  fs.copyFileSync(path.join(WORK_DIR, "pi_browser_adapter.mjs"), path.join(extensionDir, "pi_browser_adapter.mjs"));
  fs.copyFileSync(path.join(WORK_DIR, "pi_browser_operator_adapter.mjs"), path.join(extensionDir, "pi_browser_operator_adapter.mjs"));
  fs.copyFileSync(
    path.join(WORK_DIR, "agentcabin_computer_use_v2_adapter.mjs"),
    path.join(extensionDir, "agentcabin_computer_use_v2_adapter.mjs"),
  );
  fs.copyFileSync(path.join(WORK_DIR, "computer_use_v2_runtime.mjs"), path.join(extensionDir, "computer_use_v2_runtime.mjs"));
  fs.copyFileSync(path.join(WORK_DIR, "computer_use_v3_models.mjs"), path.join(extensionDir, "computer_use_v3_models.mjs"));
  fs.copyFileSync(path.join(WORK_DIR, "desktop_computer_use_backend.mjs"), path.join(extensionDir, "desktop_computer_use_backend.mjs"));
  fs.copyFileSync(path.join(WORK_DIR, "cdp_computer_use_backend.mjs"), path.join(extensionDir, "cdp_computer_use_backend.mjs"));
  fs.copyFileSync(path.join(WORK_DIR, "visual_grounding_backend.mjs"), path.join(extensionDir, "visual_grounding_backend.mjs"));
  const runtimeBridgeDir = path.join(extensionDir, "runtime_bridge");
  fs.mkdirSync(runtimeBridgeDir, { recursive: true });
  for (const moduleName of ["bridge_client.mjs", "work_tool_catalog.mjs"]) {
    fs.copyFileSync(path.join(WORK_DIR, "runtime_bridge", moduleName), path.join(runtimeBridgeDir, moduleName));
  }
  return extDst;
}

/**
 * Spins up a real local HTTP server implementing the Work Bridge internal contract.
 */
function createSmokeBridgeServer(workspaceRoot) {
  const registry = new Map();
  const activeTokens = new Map(); // token -> { subject, workspaceId }
  const executionLogs = [];

  // Seed root token
  activeTokens.set(ROOT_BEARER_TOKEN, {
    subject: { type: "Root" },
    workspaceId: "ws-smoke",
    runId: "smoke-run-1",
  });

  const server = http.createServer(async (req, res) => {
    const authHeader = req.headers["authorization"] || "";
    const bearer = authHeader.replace(/^Bearer\s+/i, "").trim();
    const url = new URL(req.url, `http://localhost:${server.address().port}`);

    // Read JSON body if present
    let body = {};
    if (req.method === "POST") {
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      const text = Buffer.concat(chunks).toString("utf8");
      if (text) {
        try {
          body = JSON.parse(text);
        } catch {
          res.writeHead(400, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "Invalid JSON" }));
          return;
        }
      }
    }

    if (url.pathname === "/internal/work/subagents/register_spawn") {
      if (bearer !== ROOT_BEARER_TOKEN) {
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Forbidden: Root only" }));
        return;
      }
      registry.set(body.agentId, {
        agentId: body.agentId,
        providerRunId: body.providerRunId || body.agentId,
        role: body.role,
        status: "running",
      });
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, bootstrapToken: `bootstrap-${body.agentId}` }));
      return;
    }

    if (url.pathname === "/internal/work/subagents/token") {
      const tokenInfo = activeTokens.get(bearer);
      if (!tokenInfo || tokenInfo.subject.type !== "Root") {
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Forbidden: Root token required for compatibility exchange" }));
        return;
      }

      const candidateId = body.agentId || body.providerRunId;
      const record = registry.get(candidateId);
      if (!record) {
        res.writeHead(404, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Subagent not found in registry" }));
        return;
      }

      const reqRole = (body.role || "").toLowerCase().replace("agentcabin-", "");
      const recRole = (record.role || "").toLowerCase().replace("agentcabin-", "");
      if (reqRole !== recRole) {
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Role mismatch" }));
        return;
      }

      if (record.status !== "running") {
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Subagent not running" }));
        return;
      }

      const childToken = `wbt-child-${record.agentId}-${Date.now()}`;
      activeTokens.set(childToken, {
        subject: { type: "Subagent", agent_id: record.agentId, role: record.role },
        workspaceId: "ws-smoke",
        runId: "smoke-run-1",
      });

      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          token: childToken,
          agentId: record.agentId,
          role: record.role,
          subject: { type: "Subagent", agent_id: record.agentId, role: record.role },
        }),
      );
      return;
    }

    if (url.pathname === "/internal/work/tool_pipeline") {
      const tokenInfo = activeTokens.get(bearer);
      if (!tokenInfo) {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Unauthorized" }));
        return;
      }

      const toolName = body.toolName;
      const subject = tokenInfo.subject;

      // Role check
      if (subject.type === "Subagent") {
        const role = subject.role.toLowerCase().replace("agentcabin-", "");
        if (role === "researcher" || role === "reviewer") {
          if (toolName !== "work_read_file") {
            res.writeHead(403, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ error: `Forbidden: role '${role}' cannot execute '${toolName}'` }));
            return;
          }
        } else if (role === "worker") {
          if (["work_delegate", "work_set_goal", "work_replace_plan"].includes(toolName)) {
            res.writeHead(403, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ error: `Forbidden: worker cannot execute root tool '${toolName}'` }));
            return;
          }
        }
      }

      if (toolName === "work_write_file" && workspaceRoot && body.arguments?.path) {
        const target = path.join(workspaceRoot, body.arguments.path);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, body.arguments.content || "", "utf8");
      }

      executionLogs.push({ toolName, subject, args: body.arguments });
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ success: true, status: "success", stdout: "executed-ok", exitCode: 0, outputs: [body.arguments?.path].filter(Boolean) }));
      return;
    }

    if (url.pathname === "/internal/work/web/search" || url.pathname === "/internal/work/web/fetch") {
      const tokenInfo = activeTokens.get(bearer);
      if (!tokenInfo || tokenInfo.subject.type !== "Subagent") {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Subagent role token required" }));
        return;
      }
      const role = tokenInfo.subject.role.toLowerCase().replace("agentcabin-", "");
      if (role !== "researcher") {
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: `Forbidden: role '${role}' cannot use Web research` }));
        return;
      }
      executionLogs.push({
        toolName: url.pathname.endsWith("search") ? "web_search" : "web_open",
        subject: tokenInfo.subject,
        args: body,
      });
      res.writeHead(200, { "Content-Type": "application/json" });
      if (url.pathname.endsWith("search")) {
        res.end(JSON.stringify({
          ok: true,
          results: [{
            title: "AgentCabin Web research smoke source",
            url: "https://example.com/subagent-web-smoke",
            snippet: "A researcher child reached the Work Web bridge.",
          }],
        }));
      } else {
        res.end(JSON.stringify({
          ok: true,
          finalUrl: body.url,
          title: "AgentCabin Web research smoke source",
          contentType: "text/html",
          text: "A researcher child reached the Work Web bridge and produced citable evidence.",
        }));
      }
      return;
    }

    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "Not found" }));
  });

  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({
        port: server.address().port,
        close: () => new Promise((r) => server.close(r)),
        registry,
        activeTokens,
        executionLogs,
      });
    });
  });
}

/**
 * Runner script that executes inside the real child subprocess.
 */
function createChildRunnerScript(workspaceRoot, extensionPath, action) {
  return `
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const action = ${JSON.stringify(action)};
const workspaceRoot = ${JSON.stringify(workspaceRoot)};
const extensionPath = ${JSON.stringify(extensionPath)};

const extMod = await import(pathToFileURL(extensionPath).href);
const extension = extMod.default;

const tools = new Map();
const eventHandlers = new Map();

extension({
  registerTool(tool) {
    tools.set(tool.name, tool);
  },
  setActiveTools() {},
  on(event, handler) {
    eventHandlers.set(event, handler);
  },
});

// 1. Fire before_agent_start to trigger real token exchange
if (eventHandlers.has("before_agent_start")) {
  await eventHandlers.get("before_agent_start")({ systemPrompt: "initial prompt" });
}

// 2. Perform requested action
if (action.type === "researcher_read") {
  const readTool = tools.get("read");
  if (!readTool) throw new Error("read tool not registered");
  const res = await readTool.execute("r1", { path: action.path });
  if (!res.details.ok) throw new Error("Read failed: " + JSON.stringify(res));
  
  // Try write tool (should be denied)
  const writeTool = tools.get("write");
  const writeRes = await writeTool.execute("w1", { path: "output/denied.txt", content: "bad" });
  if (writeRes.details.ok) throw new Error("Write should have been denied for researcher");

  const searchTool = tools.get("web_search");
  const openTool = tools.get("web_open");
  const extractTool = tools.get("web_extract");
  const citeTool = tools.get("web_cite");
  if (!searchTool || !openTool || !extractTool || !citeTool) {
    throw new Error("Researcher Web tool chain is not registered");
  }
  const searchRes = await searchTool.execute("web-s1", { query: "AgentCabin subagent Web smoke" });
  if (!searchRes.details.ok) throw new Error("Search failed: " + JSON.stringify(searchRes));
  const openRes = await openTool.execute("web-o1", { source_id: searchRes.details.results[0].source_id });
  if (!openRes.details.ok) throw new Error("Open failed: " + JSON.stringify(openRes));
  const extractRes = await extractTool.execute("web-e1", { page_id: openRes.details.page_id, query: "citable evidence" });
  if (!extractRes.details.ok || extractRes.details.passages.length === 0) {
    throw new Error("Extract failed: " + JSON.stringify(extractRes));
  }
  const citeRes = await citeTool.execute("web-c1", {
    page_id: openRes.details.page_id,
    passage_ids: [extractRes.details.passages[0].passage_id],
  });
  if (!citeRes.details.ok || !citeRes.content[0].text.includes("https://example.com/subagent-web-smoke")) {
    throw new Error("Citation failed: " + JSON.stringify(citeRes));
  }

} else if (action.type === "worker_write_and_bash") {
  const writeTool = tools.get("write");
  if (!writeTool) throw new Error("write tool not registered");
  const writeRes = await writeTool.execute("w1", { path: action.writePath, content: action.content });
  if (!writeRes.details.ok) throw new Error("Write failed: " + JSON.stringify(writeRes));

  const bashTool = tools.get("bash");
  if (!bashTool) throw new Error("bash tool not registered");
  const bashRes = await bashTool.execute("b1", { command: action.command });
  if (!bashRes.details.ok) throw new Error("Bash failed: " + JSON.stringify(bashRes));

} else if (action.type === "parallel_child_action") {
  const readTool = tools.get("read");
  if (!readTool) throw new Error("read tool not registered");
  const res = await readTool.execute("p1", { path: action.path });
  if (!res.details.ok) throw new Error("Read failed: " + JSON.stringify(res));
}

// Exit cleanly
process.exit(0);
`;
}

function runChildSubprocess(env, scriptContent, tempDir) {
  const scriptPath = path.join(tempDir, `child-runner-${Date.now()}-${Math.random().toString(36).slice(2)}.mjs`);
  fs.writeFileSync(scriptPath, scriptContent, "utf8");

  return new Promise((resolve, reject) => {
    const proc = spawn(process.execPath, [scriptPath], {
      cwd: tempDir,
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    proc.stdout.on("data", (d) => (stdout += d));
    proc.stderr.on("data", (d) => (stderr += d));

    proc.on("close", (code) => {
      if (code === 0) {
        resolve({ code, stdout, stderr });
      } else {
        reject(new Error(`Subprocess exited with code ${code}.\nStdout: ${stdout}\nStderr: ${stderr}`));
      }
    });
  });
}

test("Real Multi-Process Smoke Case 1: Researcher Child performs token exchange, read, and Web citation chain", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "smoke-researcher-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "input", "subagent-smoke.txt"), "researcher data content\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const bridge = await createSmokeBridgeServer(workspaceRoot);
  const extDst = setupChildEnvironment(temp);

  // Register in Bridge
  bridge.registry.set("smoke-res-1", {
    agentId: "smoke-res-1",
    providerRunId: "smoke-res-1",
    role: "researcher",
    status: "running",
  });

  const childEnv = {
    AGENTCABIN_WORKSPACE_ROOT: workspaceRoot,
    AGENTCABIN_WORKSPACE_ID: "ws-smoke",
    AGENTCABIN_WORK_BRIDGE_PORT: String(bridge.port),
    AGENTCABIN_WORK_BRIDGE_TOKEN: ROOT_BEARER_TOKEN, // Inherited Root Token
    AGENTCABIN_WORK_BROWSER_ENABLED: "1",
    AGENTCABIN_WORK_BROWSER_RUN_DIR: path.join(temp, "browser"),
    PI_SUBAGENT_CHILD: "1",
    PI_SUBAGENT_CHILD_AGENT: "agentcabin-researcher",
    PI_SUBAGENT_RUN_ID: "smoke-res-1",
    PI_SUBAGENT_REQUIRED_CHILD_TOOLS: JSON.stringify([
      "read",
      "web_search",
      "web_open",
      "web_extract",
      "web_cite",
    ]),
  };

  const script = createChildRunnerScript(workspaceRoot, extDst, {
    type: "researcher_read",
    path: "input/subagent-smoke.txt",
  });

  try {
    const result = await runChildSubprocess(childEnv, script, temp);
    assert.equal(result.code, 0);

    // Verify bridge issued a scoped token and received authorized tool executions
    const childTokens = Array.from(bridge.activeTokens.entries()).filter(
      ([, info]) => info.subject.type === "Subagent" && info.subject.agent_id === "smoke-res-1",
    );
    assert.equal(childTokens.length, 1, "Child must have exchanged and received a scoped token");
    assert.equal(childTokens[0][1].subject.role, "researcher");
    assert.deepEqual(
      bridge.executionLogs.filter((entry) => entry.toolName.startsWith("web_")).map((entry) => entry.toolName),
      ["web_search", "web_open"],
    );
    const childLedger = path.join(temp, "browser", "subagents", "smoke-res-1", "browser-ledger.json");
    const ledger = JSON.parse(fs.readFileSync(childLedger, "utf8"));
    assert.equal(ledger.searches.length, 1);
    assert.equal(ledger.pages.length, 1);
    assert.equal(ledger.citations.length, 1);
  } finally {
    await bridge.close();
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Real Multi-Process Smoke Case 2: Worker Child performs real write and bash", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "smoke-worker-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const bridge = await createSmokeBridgeServer(workspaceRoot);
  const extDst = setupChildEnvironment(temp);

  // Register in Bridge
  bridge.registry.set("smoke-worker-1", {
    agentId: "smoke-worker-1",
    providerRunId: "smoke-worker-1",
    role: "worker",
    status: "running",
  });

  const childEnv = {
    AGENTCABIN_WORKSPACE_ROOT: workspaceRoot,
    AGENTCABIN_WORKSPACE_ID: "ws-smoke",
    AGENTCABIN_WORK_BRIDGE_PORT: String(bridge.port),
    AGENTCABIN_WORK_BRIDGE_TOKEN: ROOT_BEARER_TOKEN, // Inherited Root Token
    PI_SUBAGENT_CHILD: "1",
    PI_SUBAGENT_CHILD_AGENT: "agentcabin-worker",
    PI_SUBAGENT_RUN_ID: "smoke-worker-1",
    PI_SUBAGENT_REQUIRED_CHILD_TOOLS: JSON.stringify(["read", "write", "edit", "bash"]),
  };

  const script = createChildRunnerScript(workspaceRoot, extDst, {
    type: "worker_write_and_bash",
    writePath: "output/worker-smoke.txt",
    content: "worker payload data\n",
    command: "echo worker_done",
  });

  try {
    const result = await runChildSubprocess(childEnv, script, temp);
    assert.equal(result.code, 0);

    const childTokens = Array.from(bridge.activeTokens.entries()).filter(
      ([, info]) => info.subject.type === "Subagent" && info.subject.agent_id === "smoke-worker-1",
    );
    assert.equal(childTokens.length, 1);
    assert.equal(childTokens[0][1].subject.role, "worker");

    const written = fs.readFileSync(path.join(workspaceRoot, "output/worker-smoke.txt"), "utf8");
    assert.equal(written, "worker payload data\n");
  } finally {
    await bridge.close();
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Real Multi-Process Smoke Case 3: 3 Parallel Children concurrently exchange tokens and execute", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "smoke-parallel-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "input", "shared.txt"), "shared info\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const bridge = await createSmokeBridgeServer(workspaceRoot);
  const extDst = setupChildEnvironment(temp);

  const children = [
    { id: "par-res-1", role: "agentcabin-researcher", reqRole: "researcher" },
    { id: "par-res-2", role: "agentcabin-researcher", reqRole: "researcher" },
    { id: "par-rev-3", role: "agentcabin-reviewer", reqRole: "reviewer" },
  ];

  for (const c of children) {
    bridge.registry.set(c.id, {
      agentId: c.id,
      providerRunId: c.id,
      role: c.reqRole,
      status: "running",
    });
  }

  try {
    const promises = children.map((c) => {
      const childEnv = {
        AGENTCABIN_WORKSPACE_ROOT: workspaceRoot,
        AGENTCABIN_WORKSPACE_ID: "ws-smoke",
        AGENTCABIN_WORK_BRIDGE_PORT: String(bridge.port),
        AGENTCABIN_WORK_BRIDGE_TOKEN: ROOT_BEARER_TOKEN,
        PI_SUBAGENT_CHILD: "1",
        PI_SUBAGENT_CHILD_AGENT: c.role,
        PI_SUBAGENT_RUN_ID: c.id,
        PI_SUBAGENT_REQUIRED_CHILD_TOOLS: JSON.stringify(["read"]),
      };
      const script = createChildRunnerScript(workspaceRoot, extDst, {
        type: "parallel_child_action",
        path: "input/shared.txt",
      });
      return runChildSubprocess(childEnv, script, temp);
    });

    const results = await Promise.all(promises);
    for (const r of results) {
      assert.equal(r.code, 0);
    }

    // Verify all 3 children got distinct scoped tokens
    const scopedTokens = Array.from(bridge.activeTokens.entries()).filter(
      ([, info]) => info.subject.type === "Subagent",
    );
    assert.equal(scopedTokens.length, 3, "All 3 parallel children must have distinct scoped tokens");
    const assignedIds = new Set(scopedTokens.map(([, info]) => info.subject.agent_id));
    assert.equal(assignedIds.has("par-res-1"), true);
    assert.equal(assignedIds.has("par-res-2"), true);
    assert.equal(assignedIds.has("par-rev-3"), true);
  } finally {
    await bridge.close();
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
