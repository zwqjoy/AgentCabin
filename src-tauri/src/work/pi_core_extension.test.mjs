import assert from "node:assert/strict";
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

async function loadExtension(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n', "utf8");
  fs.writeFileSync(
    path.join(tempRoot, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
    "utf8",
  );
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
  fs.copyFileSync(
    new URL("./runtime_bridge/bridge_client.mjs", import.meta.url),
    path.join(runtimeBridgeDir, "bridge_client.mjs"),
  );
  fs.copyFileSync(
    new URL("./runtime_bridge/work_tool_catalog.mjs", import.meta.url),
    path.join(runtimeBridgeDir, "work_tool_catalog.mjs"),
  );

  const module = await import(
    `${pathToFileURL(path.join(extensionDir, "agentcabin-work-core.mjs"))}?test=${Date.now()}-${Math.random()}`,
  );
  return module.default;
}

function mockPipelineFetch(workspaceRoot) {
  return async (url, init = {}) => {
    const u = String(url);
    if (u.includes("/internal/work/task_state/update")) {
      const body = JSON.parse(init.body || "{}");
      const runDir = process.env.AGENTCABIN_WORK_RUN_DIR || workspaceRoot;
      const stateFile = path.join(runDir, "work-task-state.json");
      let state = { version: 1, revision: 0, goal: null, plan: [], checkpoint: null, pendingApproval: null, updatedAt: new Date().toISOString() };
      try { state = JSON.parse(fs.readFileSync(stateFile, "utf8")); } catch (_) {}
      if (body.goal !== undefined) state.goal = body.goal;
      if (body.plan !== undefined) state.plan = body.plan;
      if (body.checkpoint !== undefined) state.checkpoint = body.checkpoint;
      if (body.step) {
        const target = state.plan.find(s => s.id === body.step.id);
        if (target) {
          if (body.step.status) target.status = body.step.status;
          if (body.step.text) target.text = body.step.text;
        }
      }
      state.revision += 1;
      state.updatedAt = new Date().toISOString();
      fs.mkdirSync(path.dirname(stateFile), { recursive: true });
      fs.writeFileSync(stateFile, JSON.stringify(state, null, 2), "utf8");
      return {
        ok: true,
        status: 200,
        async json() {
          return { ok: true, workTaskState: state, work_task_state: state };
        },
      };
    }
    if (u.includes("/internal/work/tool_pipeline")) {
      const body = JSON.parse(init.body || "{}");
      if (body.toolName === "work_write_file") {
        const relPath = body.arguments.path;
        const filePath = path.join(workspaceRoot, relPath);
        fs.mkdirSync(path.dirname(filePath), { recursive: true });
        fs.writeFileSync(filePath, body.arguments.content, "utf8");
        let artifact = null;
        if (relPath.startsWith("output/")) {
          const artifactsFile = path.join(workspaceRoot, "context", "artifacts.json");
          let registry = { artifacts: [] };
          try { registry = JSON.parse(fs.readFileSync(artifactsFile, "utf8")); } catch (_) {}
          const runId = process.env.AGENTCABIN_WORK_RUN_ID || null;
          let existing = registry.artifacts.find(a => a.path === relPath && (a.run_id || null) === runId);
          if (!existing) {
            existing = { id: "mock-" + Date.now(), path: relPath, created_at: new Date().toISOString() };
            registry.artifacts.push(existing);
          }
          existing.run_id = runId;
          existing.title = path.basename(relPath);
          existing.artifact_type = path.extname(relPath).slice(1) || "file";
          existing.status = "delivered";
          fs.mkdirSync(path.dirname(artifactsFile), { recursive: true });
          fs.writeFileSync(artifactsFile, JSON.stringify(registry, null, 2), "utf8");
          artifact = existing;
        }
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, outputs: [relPath], artifact };
          },
        };
      } else if (body.toolName === "work_edit_file") {
        const filePath = path.join(workspaceRoot, body.arguments.path);
        let content = fs.readFileSync(filePath, "utf8");
        content = content.replace(body.arguments.old_text, body.arguments.new_text);
        fs.writeFileSync(filePath, content, "utf8");
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
          },
        };
      } else if (body.toolName === "work_register_artifact") {
        const artifactsFile = path.join(workspaceRoot, "context", "artifacts.json");
        let registry = { artifacts: [] };
        try { registry = JSON.parse(fs.readFileSync(artifactsFile, "utf8")); } catch (_) {}
        const relPath = body.arguments.path;
        let existing = registry.artifacts.find(a => a.path === relPath && (!a.run_id || a.run_id === process.env.AGENTCABIN_WORK_RUN_ID));
        if (!existing) {
          existing = { id: "mock-" + Date.now(), path: relPath, created_at: new Date().toISOString() };
          registry.artifacts.push(existing);
        }
        existing.run_id = process.env.AGENTCABIN_WORK_RUN_ID || null;
        existing.title = body.arguments.title || path.basename(relPath);
        existing.status = "delivered";
        fs.mkdirSync(path.dirname(artifactsFile), { recursive: true });
        fs.writeFileSync(artifactsFile, JSON.stringify(registry, null, 2), "utf8");
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, stdout: JSON.stringify(existing), outputs: [relPath] };
          },
        };
      } else if (body.toolName === "work_list_artifacts") {
        const artifactsFile = path.join(workspaceRoot, "context", "artifacts.json");
        let registry = { artifacts: [] };
        try { registry = JSON.parse(fs.readFileSync(artifactsFile, "utf8")); } catch (_) {}
        const runId = process.env.AGENTCABIN_WORK_RUN_ID;
        const filtered = runId ? registry.artifacts.filter(a => a.run_id === runId) : registry.artifacts;
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, stdout: JSON.stringify(filtered), outputs: [] };
          },
        };
      } else if (body.toolName === "work_validate_artifact" || body.toolName === "work_deliver") {
        const artifactsFile = path.join(workspaceRoot, "context", "artifacts.json");
        let registry = { artifacts: [] };
        try { registry = JSON.parse(fs.readFileSync(artifactsFile, "utf8")); } catch (_) {}
        const id = body.arguments.id;
        const p = body.arguments.path;
        const artifact = registry.artifacts.find(a => (id && a.id === id) || (p && a.path === p));
        if (artifact && body.toolName === "work_deliver") {
          artifact.status = "delivered";
          fs.writeFileSync(artifactsFile, JSON.stringify(registry, null, 2), "utf8");
        }
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, stdout: JSON.stringify(artifact || {}), outputs: [] };
          },
        };
      }
    }
    return { ok: true, status: 200, async json() { return { success: true, status: "success" }; } };
  };
}

test("Work refuses to start when Pi cannot enforce dynamic tool activation", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-capability-gate-"));
  try {
    const extension = await loadExtension(temp);
    assert.throws(
      () =>
        extension({
          registerTool() {},
          on() {},
        }),
      /setActiveTools.*refusing to start unsafely/,
    );
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work refuses to start when a native tool guard cannot be installed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-native-guard-fail-"));
  try {
    const extension = await loadExtension(temp);
    // Pi throws when trying to register over a locked/pre-registered native tool.
    assert.throws(
      () =>
        extension({
          registerTool() {
            throw new Error("Tool 'bash' is locked and cannot be re-registered");
          },
          setActiveTools() {},
          on() {},
        }),
      /fail-closed guard for native tool 'bash'.*refusing to start/,
    );
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work refuses to start when getTool reveals the guard did not replace the native tool", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-native-guard-retain-"));
  try {
    const extension = await loadExtension(temp);
    const realGuard = [];
    assert.throws(
      () =>
        extension({
          registerTool(tool) {
            realGuard.push(tool);
          },
          setActiveTools() {},
          on() {},
          getTool() {
            // Pi retained the original native implementation, returning a different object
            return { name: "bash", label: "bash", execute() {} };
          },
        }),
      /guard for 'bash' was not installed.*refusing to start/i,
    );
    assert.equal(realGuard.length, 1);
    assert.equal(realGuard[0].name, "bash");
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work defers dynamic tool activation until Pi runtime initialization", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-runtime-init-"));
  const handlers = new Map();
  let runtimeReady = false;
  let activeTools = null;
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool() {},
      setActiveTools(names) {
        assert.equal(runtimeReady, true, "setActiveTools must run after Pi binds the extension runtime");
        activeTools = names;
      },
      on(event, handler) {
        handlers.set(event, handler);
      },
    });

    assert.equal(activeTools, null, "extension loading must not call runtime action methods");
    runtimeReady = true;
    await handlers.get("session_start")({}, {});
    assert.ok(activeTools.includes("work_workspace_info"));
    assert.ok(activeTools.includes("work_write_file"));
    assert.ok(activeTools.includes("work_edit_file"));
    assert.ok(activeTools.includes("work_run_command"));
    assert.ok(!activeTools.includes("work_execute"));
    for (const subagentTool of ["work_delegate", "work_agent_wait", "work_agent_status", "work_agent_steer", "work_agent_stop"]) {
      assert.ok(activeTools.includes(subagentTool), `${subagentTool} should be active in Root mode`);
    }
    for (const compatibilityName of ["read", "write", "edit", "bash"]) {
      assert.ok(activeTools.includes(compatibilityName));
    }
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Pi native tool names delegate to the confined Work tool pipeline", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-native-compat-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "input", "source.md"), "input content\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "compat-fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  const pipelineCalls = [];
  globalThis.fetch = async (url, init = {}) => {
    const body = JSON.parse(init.body || "{}");
    pipelineCalls.push(body);
    if (body.toolName === "work_write_file") {
      const filePath = path.join(workspaceRoot, body.arguments.path);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, body.arguments.content, "utf8");
      return { ok: true, status: 200, async json() { return { success: true, status: "success", exitCode: 0 }; } };
    }
    if (body.toolName === "work_edit_file") {
      const filePath = path.join(workspaceRoot, body.arguments.path);
      const content = fs.readFileSync(filePath, "utf8").replace(body.arguments.old_text, body.arguments.new_text);
      fs.writeFileSync(filePath, content, "utf8");
      return { ok: true, status: 200, async json() { return { success: true, status: "success", exitCode: 0 }; } };
    }
    if (body.toolName === "work_run_command") {
      return { ok: true, status: 200, async json() { return { success: true, status: "success", exitCode: 0, stdout: "compat bash\n" }; } };
    }
    return { ok: true, status: 200, async json() { return { success: true, status: "success" }; } };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const read = await tools.get("read").execute("compat-read", { file_path: "input/source.md" });
    assert.equal(read.details.ok, true);
    assert.match(read.content[0].text, /input content/);

    const write = await tools.get("write").execute("compat-write", { path: "scratch/compat.md", content: "before\n" });
    assert.equal(write.details.ok, true);
    const edit = await tools.get("edit").execute("compat-edit", {
      path: "scratch/compat.md",
      edits: [{ oldText: "before", newText: "after" }],
    });
    assert.equal(edit.details.ok, true);
    assert.equal(fs.readFileSync(path.join(workspaceRoot, "scratch", "compat.md"), "utf8"), "after\n");

    const bash = await tools.get("bash").execute("compat-bash", { command: "printf 'compat bash'", cwd: "scratch" });
    assert.equal(bash.details.ok, true);
    const defaultCwdBash = await tools.get("bash").execute("compat-bash-root", { command: "pwd" });
    assert.equal(defaultCwdBash.details.ok, true);
    assert.deepEqual(pipelineCalls[3].arguments, {
      command: "pwd",
      args: [],
      cwd: ".",
      expected_outputs: [],
      timeout_seconds: 120,
    });

    const heredoc = await tools.get("bash").execute("compat-heredoc", {
      command: "python3 << 'EOF'\nprint('content | && $ stays in the script')\nEOF",
      cwd: ".",
    });
    assert.equal(heredoc.details.ok, true);
    const heredocWrite = pipelineCalls[4];
    const heredocRun = pipelineCalls[5];
    assert.equal(heredocWrite.toolName, "work_write_file");
    assert.match(heredocWrite.arguments.path, /^scratch\/.agentcabin-bash-[0-9a-f-]+\.py$/);
    assert.equal(heredocWrite.arguments.content, "print('content | && $ stays in the script')\n");
    assert.deepEqual(heredocRun.arguments, {
      command: "python3",
      args: [heredocWrite.arguments.path],
      cwd: ".",
      expected_outputs: [],
      timeout_seconds: 120,
    });
    assert.equal(
      fs.readFileSync(path.join(workspaceRoot, heredocWrite.arguments.path), "utf8"),
      heredocWrite.arguments.content,
    );

    const unsupportedHeredoc = await tools.get("bash").execute("compat-unsupported-heredoc", {
      command: "cat << 'EOF'\nnot a script\nEOF",
    });
    assert.equal(unsupportedHeredoc.details.ok, false);
    assert.match(unsupportedHeredoc.content[0].text, /approved script interpreters/i);
    assert.equal(pipelineCalls.length, 6, "unsupported heredoc must never reach the Work pipeline");

    const pipelineCountBeforeUnsafeBash = pipelineCalls.length;
    const unsafeBash = await tools.get("bash").execute("compat-unsafe-bash", {
      command: "printf safe | cat /etc/passwd",
      cwd: "scratch",
    });
    assert.equal(unsafeBash.details.ok, false);
    assert.match(unsafeBash.content[0].text, /shell operators/i);
    assert.equal(pipelineCalls.length, pipelineCountBeforeUnsafeBash, "rejected shell syntax must never reach the Work pipeline");

    assert.deepEqual(
      pipelineCalls.map((call) => call.toolName),
      ["work_write_file", "work_edit_file", "work_run_command", "work_run_command", "work_write_file", "work_run_command"],
    );
    assert.deepEqual(pipelineCalls[2].arguments, {
      command: "printf",
      args: ["compat bash"],
      cwd: "scratch",
      expected_outputs: [],
      timeout_seconds: 120,
    });
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work tools read an authorized external directory and reject other paths", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-extension-"));
  const workspaceRoot = path.join(temp, "workspace");
  const externalRoot = path.join(temp, "external");
  const siblingRoot = path.join(temp, "sibling");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(externalRoot, { recursive: true });
  fs.mkdirSync(siblingRoot, { recursive: true });
  fs.writeFileSync(path.join(externalRoot, "notes.md"), "authorized content\n", "utf8");
  fs.writeFileSync(path.join(siblingRoot, "secret.md"), "not authorized\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "{}\n", "utf8");
  fs.writeFileSync(
    path.join(workspaceRoot, "manifest.json"),
    JSON.stringify({ accessRoots: [] }),
    "utf8",
  );

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    const initiallyDenied = await tools.get("work_read_file").execute("read-0", {
      path: path.join(externalRoot, "notes.md"),
    });
    assert.equal(initiallyDenied.details.ok, false);

    fs.writeFileSync(
      path.join(workspaceRoot, "manifest.json"),
      JSON.stringify({ accessRoots: [{ path: externalRoot, writable: false }] }),
      "utf8",
    );
    const read = await tools.get("work_read_file").execute("read-1", {
      path: path.join(externalRoot, "notes.md"),
    });
    assert.equal(read.details.ok, true);
    assert.equal(read.content[0].text, "authorized content\n");

    const listed = await tools.get("work_list_files").execute("list-1", {
      path: externalRoot,
    });
    assert.equal(listed.details.ok, true);
    assert.deepEqual(listed.details.files, [fs.realpathSync(path.join(externalRoot, "notes.md"))]);

    const hiddenContext = await tools.get("work_list_files").execute("list-2", {
      area: "context",
    });
    assert.equal(hiddenContext.details.ok, true);
    assert.deepEqual(hiddenContext.details.files, []);

    const protectedRegistry = await tools.get("work_write_file").execute("write-0", {
      path: "context/artifacts.json",
      content: "should not overwrite\n",
    });
    assert.equal(protectedRegistry.details.ok, false);

    const protectedRead = await tools.get("work_read_file").execute("read-internal", {
      path: "context/artifacts.json",
    });
    assert.equal(protectedRead.details.ok, false);

    const denied = await tools.get("work_read_file").execute("read-2", {
      path: path.join(siblingRoot, "secret.md"),
    });
    assert.equal(denied.details.ok, false);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work tools can read the isolated Work Profile skill tree without granting host access", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-profile-skills-"));
  const workspaceRoot = path.join(temp, "workspace");
  const profileRoot = path.join(temp, "profile");
  const skillRoot = path.join(profileRoot, "skills");
  const skillDir = path.join(skillRoot, "pptx");
  const connectorSkillRoot = path.join(profileRoot, "connectors", "feishu", "skills");
  const connectorSkillDir = path.join(connectorSkillRoot, "lark-contact");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(skillDir, { recursive: true });
  fs.mkdirSync(connectorSkillDir, { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  fs.writeFileSync(path.join(skillDir, "SKILL.md"), "# PPTX skill\n", "utf8");
  fs.writeFileSync(path.join(skillDir, "reference.md"), "reference content\n", "utf8");
  fs.writeFileSync(path.join(connectorSkillDir, "SKILL.md"), "# Feishu contact skill\n", "utf8");
  fs.writeFileSync(path.join(connectorSkillDir, "reference.md"), "connector reference\n", "utf8");
  fs.writeFileSync(path.join(profileRoot, "profile-secret.json"), "must stay private\n", "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    profileDir: process.env.AGENTCABIN_WORK_PROFILE_DIR,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_PROFILE_DIR = profileRoot;
  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    const skill = await tools.get("work_read_file").execute("read-skill", {
      path: path.join(skillDir, "SKILL.md"),
    });
    assert.equal(skill.details.ok, true);
    assert.equal(skill.content[0].text, "# PPTX skill\n");

    const listed = await tools.get("work_list_files").execute("list-skills", {
      path: skillRoot,
    });
    assert.equal(listed.details.ok, true);
    assert.deepEqual(
      listed.details.files,
      [path.join(skillDir, "SKILL.md"), path.join(skillDir, "reference.md")]
        .map((file) => fs.realpathSync(file))
        .sort((left, right) => left.localeCompare(right)),
    );

    const connectorListed = await tools.get("work_list_files").execute("list-connector-skills", {
      path: connectorSkillRoot,
    });
    assert.equal(connectorListed.details.ok, true);
    assert.deepEqual(
      connectorListed.details.files,
      [path.join(connectorSkillDir, "SKILL.md"), path.join(connectorSkillDir, "reference.md")]
        .map((file) => fs.realpathSync(file))
        .sort((left, right) => left.localeCompare(right)),
    );

    const profileSecret = await tools.get("work_read_file").execute("read-profile-secret", {
      path: path.join(profileRoot, "profile-secret.json"),
    });
    assert.equal(profileSecret.details.ok, false);

    const writeSkill = await tools.get("work_write_file").execute("write-skill", {
      path: path.join(skillDir, "generated.md"),
      content: "must be rejected\n",
    });
    assert.equal(writeSkill.details.ok, false);
    assert.equal(fs.existsSync(path.join(skillDir, "generated.md")), false);

    const requestSkillAccess = await tools.get("work_request_directory_access").execute("request-skill", {
      path: skillRoot,
      writable: true,
      purpose: "should be rejected as unnecessary",
    });
    assert.equal(requestSkillAccess.details.ok, false);
    assert.match(requestSkillAccess.content[0].text, /already readable/i);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.profileDir === undefined) delete process.env.AGENTCABIN_WORK_PROFILE_DIR;
    else process.env.AGENTCABIN_WORK_PROFILE_DIR = previous.profileDir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Workspace knowledge updates require confirmation and never touch internal artifacts", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-context-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "{}\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";

  const pipelineCalls = [];
  let inboxResolution = "rejected";
  globalThis.fetch = async (url, init = {}) => {
    const u = String(url);
    if (u.includes("/internal/work/tool_pipeline")) {
      const body = JSON.parse(init.body || "{}");
      pipelineCalls.push(body);
      if (inboxResolution === "rejected") {
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: false, status: "waiting_approval", interactionId: "inbox-ctx-1" };
          },
        };
      }
      if (inboxResolution === "approved") {
        if (pipelineCalls.filter((c) => c.toolCallId === body.toolCallId).length === 1) {
          return {
            ok: true,
            status: 200,
            async json() {
              return { success: false, status: "waiting_approval", interactionId: "inbox-ctx-2" };
            },
          };
        }
        const filePath = path.join(workspaceRoot, body.arguments.path);
        fs.mkdirSync(path.dirname(filePath), { recursive: true });
        fs.writeFileSync(filePath, body.arguments.content, "utf8");
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
          },
        };
      }
    }
    if (u.includes("/internal/work/inbox_status/")) {
      return {
        ok: true,
        status: 200,
        async json() {
          return { status: inboxResolution };
        },
      };
    }
    return { ok: false, status: 404, async json() { return {}; } };
  };

  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    const propose = tools.get("work_propose_context_update");

    const declined = await propose.execute(
      "context-1",
      { path: "context/decisions.md", content: "# Decision\n\nKeep the boundary.\n" },
    );
    assert.equal(declined.details.ok, false);
    assert.equal(declined.details.confirmed, false);
    assert.equal(fs.existsSync(path.join(workspaceRoot, "context", "decisions.md")), false);
    assert.equal(pipelineCalls[0].toolName, "work_update_context");
    assert.equal(pipelineCalls[0].arguments.path, "context/decisions.md");

    inboxResolution = "approved";
    const saved = await propose.execute(
      "context-2",
      {
        path: "context/decisions.md",
        content: "# Decision\n\nKeep the boundary.\n",
        overwrite: true,
      },
    );
    assert.equal(saved.details.ok, true);
    assert.equal(saved.details.confirmed, true);
    assert.equal(
      fs.readFileSync(path.join(workspaceRoot, "context", "decisions.md"), "utf8"),
      "# Decision\n\nKeep the boundary.\n",
    );

    const callsBefore = pipelineCalls.length;
    const protectedUpdate = await propose.execute(
      "context-3",
      { path: "context/artifacts.json", content: "overwrite\n", overwrite: true },
    );
    assert.equal(protectedUpdate.details.ok, false);
    assert.equal(pipelineCalls.length, callsBefore);
    assert.equal(fs.readFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "utf8"), "{}\n");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT;
    else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN;
    else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work Harness tools persist goal, plan, step progress, and checkpoint in the current Run", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-harness-"));
  const workspaceRoot = path.join(temp, "workspace");
  const runRoot = path.join(temp, "run");
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(runRoot, { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runRoot: process.env.AGENTCABIN_WORK_RUN_DIR,
    localFallback: process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_RUN_DIR = runRoot;
  process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = "1";
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const goal = await tools.get("work_set_goal").execute("goal-1", { goal: "交付带引用的报告" });
    assert.equal(goal.details.work_task_state.goal, "交付带引用的报告");
    assert.equal(goal.details.work_task_state.revision, 1);

    const plan = await tools.get("work_replace_plan").execute("plan-1", {
      steps: [
        { id: "research", text: "搜索官方来源" },
        { id: "write", text: "撰写报告" },
      ],
    }, undefined, undefined, {
      hasUI: true,
      ui: { select: async () => { throw new Error("计划不应触发审批"); } },
    });
    assert.equal(plan.details.work_task_state.plan.length, 2);
    assert.equal(plan.details.work_task_state.revision, 2);
    assert.equal(plan.details.work_task_state.pendingApproval, null);

    const step = await tools.get("work_update_step").execute("step-1", {
      id: "research",
      status: "in_progress",
    });
    assert.equal(step.details.work_task_state.plan[0].status, "in_progress");

    const checkpoint = await tools.get("work_save_checkpoint").execute("checkpoint-1", {
      summary: "已确定官方来源",
      current_step_id: "research",
    });
    assert.equal(checkpoint.details.work_task_state.revision, 4);
    assert.equal(checkpoint.details.work_task_state.checkpoint.currentStepId, "research");

    const stored = JSON.parse(fs.readFileSync(path.join(runRoot, "work-task-state.json"), "utf8"));
    assert.deepEqual(stored, checkpoint.details.work_task_state);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runRoot === undefined) delete process.env.AGENTCABIN_WORK_RUN_DIR; else process.env.AGENTCABIN_WORK_RUN_DIR = previous.runRoot;
    if (previous.localFallback === undefined) delete process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK; else process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = previous.localFallback;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work Harness tools delegate task state mutations to Work Bridge when configured", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-bridge-task-state-"));
  const workspaceRoot = path.join(temp, "workspace");
  const runRoot = path.join(temp, "run");
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(runRoot, { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runRoot: process.env.AGENTCABIN_WORK_RUN_DIR,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_RUN_DIR = runRoot;
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  let bridgeUpdateCalled = 0;
  const originalMock = mockPipelineFetch(workspaceRoot);
  globalThis.fetch = async (url, init = {}) => {
    if (String(url).includes("/internal/work/task_state/update")) {
      bridgeUpdateCalled += 1;
    }
    return originalMock(url, init);
  };
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const goal = await tools.get("work_set_goal").execute("goal-1", { goal: "完成架构收敛" });
    assert.equal(goal.details.work_task_state.goal, "完成架构收敛");
    assert.equal(bridgeUpdateCalled, 1, "work_set_goal must call internal bridge");

    const plan = await tools.get("work_replace_plan").execute("plan-1", {
      steps: [
        { id: "step-1", text: "清理冗余代码" },
        { id: "step-2", text: "集成验证" },
      ],
    });
    assert.equal(plan.details.work_task_state.plan.length, 2);
    assert.equal(bridgeUpdateCalled, 2, "work_replace_plan must call internal bridge");

    const step = await tools.get("work_update_step").execute("step-update-1", {
      id: "step-1",
      status: "in_progress",
    });
    assert.equal(step.details.work_task_state.plan[0].status, "in_progress");
    assert.equal(bridgeUpdateCalled, 3, "work_update_step must call internal bridge");

    const checkpoint = await tools.get("work_save_checkpoint").execute("cp-1", {
      summary: "第一步执行中",
      current_step_id: "step-1",
    });
    assert.equal(checkpoint.details.work_task_state.checkpoint.summary, "第一步执行中");
    assert.equal(bridgeUpdateCalled, 4, "work_save_checkpoint must call internal bridge");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runRoot === undefined) delete process.env.AGENTCABIN_WORK_RUN_DIR; else process.env.AGENTCABIN_WORK_RUN_DIR = previous.runRoot;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work plan replacement records the plan without opening an approval prompt", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-plan-gate-"));
  const workspaceRoot = path.join(temp, "workspace");
  const runRoot = path.join(temp, "run");
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(runRoot, { recursive: true });
  fs.writeFileSync(
    path.join(workspaceRoot, "manifest.json"),
    JSON.stringify({ accessRoots: [] }),
    "utf8",
  );
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runRoot: process.env.AGENTCABIN_WORK_RUN_DIR,
    localFallback: process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_RUN_DIR = runRoot;
  process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = "1";
  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    let selectCalls = 0;
    const plan = await tools.get("work_replace_plan").execute(
      "plan-gate-1",
      { steps: [{ id: "test", text: "运行测试" }] },
      undefined,
      undefined,
      {
        hasUI: true,
        ui: {
          select: async () => {
            selectCalls += 1;
            return "取消";
          },
        },
      },
    );

    assert.equal(selectCalls, 0);
    assert.equal(plan.details.work_task_state.pendingApproval, null);
    assert.equal(plan.details.work_task_state.plan[0].text, "运行测试");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runRoot === undefined) delete process.env.AGENTCABIN_WORK_RUN_DIR;
    else process.env.AGENTCABIN_WORK_RUN_DIR = previous.runRoot;
    if (previous.localFallback === undefined) delete process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK;
    else process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = previous.localFallback;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work plan replacement does not leave a pending approval when UI is unavailable", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-pending-approval-"));
  const workspaceRoot = path.join(temp, "workspace");
  const runRoot = path.join(temp, "run");
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.mkdirSync(runRoot, { recursive: true });
  fs.writeFileSync(
    path.join(workspaceRoot, "manifest.json"),
    JSON.stringify({ accessRoots: [] }),
    "utf8",
  );
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runRoot: process.env.AGENTCABIN_WORK_RUN_DIR,
    localFallback: process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_RUN_DIR = runRoot;
  process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = "1";
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    const plan = await tools.get("work_replace_plan").execute("pending-1", {
      steps: [{ id: "test", text: "运行测试" }],
    });
    assert.equal(plan.details.work_task_state.pendingApproval, null);
    assert.equal(plan.details.work_task_state.plan[0].text, "运行测试");

    const stored = JSON.parse(fs.readFileSync(path.join(runRoot, "work-task-state.json"), "utf8"));
    assert.equal(stored.pendingApproval, null);
    assert.equal(stored.plan[0].text, "运行测试");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runRoot === undefined) delete process.env.AGENTCABIN_WORK_RUN_DIR;
    else process.env.AGENTCABIN_WORK_RUN_DIR = previous.runRoot;
    if (previous.localFallback === undefined) delete process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK;
    else process.env.AGENTCABIN_WORK_TEST_LOCAL_FALLBACK = previous.localFallback;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work file writes run without a second prompt inside the Work trust boundary", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-write-permission-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = mockPipelineFetch(workspaceRoot);
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });
    const write = tools.get("work_write_file");
    const first = await write.execute(
      "write-1",
      { path: "scratch/draft.md", content: "draft\n" },
      undefined,
      undefined,
      undefined,
    );
    assert.equal(first.details.ok, true);
    assert.equal(fs.readFileSync(path.join(workspaceRoot, "scratch", "draft.md"), "utf8"), "draft\n");

    const second = await write.execute(
      "write-2",
      { path: "scratch/draft.md", content: "draft\n" },
      undefined,
      undefined,
      undefined,
    );
    assert.equal(second.details.ok, true);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("FullAccess writes a read-only external path without a second confirmation", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-full-access-write-"));
  const workspaceRoot = path.join(temp, "workspace");
  const externalDir = path.join(temp, "external-read-only");
  const externalFile = path.join(externalDir, "full-access.md");
  const permissionPath = path.join(temp, "task.json");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.mkdirSync(externalDir, { recursive: true });
  fs.writeFileSync(
    path.join(workspaceRoot, "manifest.json"),
    JSON.stringify({ accessRoots: [{ path: externalDir, writable: false }] }),
    "utf8",
  );
  fs.writeFileSync(permissionPath, JSON.stringify({ policy: { executionMode: "full_access" } }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    permissionPath: process.env.AGENTCABIN_WORK_PERMISSION_PATH,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "full-access-fixture";
  process.env.AGENTCABIN_WORK_PERMISSION_PATH = permissionPath;
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async (_url, init = {}) => {
    const body = JSON.parse(init.body || "{}");
    if (body.toolName === "work_write_file") {
      fs.mkdirSync(path.dirname(body.arguments.path), { recursive: true });
      fs.writeFileSync(body.arguments.path, body.arguments.content, "utf8");
    }
    return {
      ok: true,
      status: 200,
      async json() {
        return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
      },
    };
  };
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const written = await tools.get("work_write_file").execute(
      "full-access-write",
      { path: externalFile, content: "full access\n" },
      undefined,
      undefined,
      undefined,
    );
    assert.equal(written.details.ok, true);
    assert.equal(fs.readFileSync(externalFile, "utf8"), "full access\n");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.permissionPath === undefined) delete process.env.AGENTCABIN_WORK_PERMISSION_PATH; else process.env.AGENTCABIN_WORK_PERMISSION_PATH = previous.permissionPath;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Writing an output file automatically registers a delivered Artifact", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-output-artifact-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = mockPipelineFetch(workspaceRoot);
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });
    const context = { hasUI: true, ui: { confirm: async () => true } };
    const written = await tools.get("work_write_file").execute(
      "write-output",
      { path: "output/report.md", content: "# Report\n" },
      undefined,
      undefined,
      context,
    );
    assert.equal(written.details.ok, true);
    assert.equal(written.details.artifact.status, "delivered");
    assert.equal(written.details.artifact.path, "output/report.md");
    const registry = JSON.parse(fs.readFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "utf8"));
    assert.equal(registry.artifacts[0].artifact_type, "md");
    assert.equal(registry.artifacts[0].type, undefined);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Writing a reserved output area works when a local-folder project uses managed state", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-managed-output-write-"));
  const workspaceRoot = path.join(temp, "project");
  const managedStateRoot = path.join(temp, "managed-state");
  fs.mkdirSync(workspaceRoot, { recursive: true });
  fs.mkdirSync(path.join(managedStateRoot, "output"), { recursive: true });
  fs.mkdirSync(path.join(managedStateRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(managedStateRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    managedStateDir: process.env.AGENTCABIN_MANAGED_STATE_DIR,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    permissionPath: process.env.AGENTCABIN_WORK_PERMISSION_PATH,
    executionMode: process.env.AGENTCABIN_WORK_EXECUTION_MODE,
    policy: process.env.AGENTCABIN_WORK_POLICY,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  const permissionPath = path.join(temp, "task.json");
  fs.writeFileSync(permissionPath, JSON.stringify({ policy: { executionMode: "default" } }), "utf8");
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_MANAGED_STATE_DIR = managedStateRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "managed-output-fixture";
  process.env.AGENTCABIN_WORK_PERMISSION_PATH = permissionPath;
  delete process.env.AGENTCABIN_WORK_EXECUTION_MODE;
  delete process.env.AGENTCABIN_WORK_POLICY;
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49324";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async (_url, init = {}) => {
    const body = JSON.parse(init.body || "{}");
    if (body.toolName === "work_write_file") {
      const filePath = path.join(managedStateRoot, body.arguments.path);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, body.arguments.content, "utf8");
    }
    return {
      ok: true,
      status: 200,
      async json() {
        return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
      },
    };
  };
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const written = await tools.get("work_write_file").execute(
      "managed-output-write",
      { path: "output/report.md", content: "# Managed output\n" },
    );
    assert.equal(written.details.ok, true);
    assert.equal(written.details.confirmed, true);
    assert.equal(
      fs.readFileSync(path.join(managedStateRoot, "output", "report.md"), "utf8"),
      "# Managed output\n",
    );
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.managedStateDir === undefined) delete process.env.AGENTCABIN_MANAGED_STATE_DIR; else process.env.AGENTCABIN_MANAGED_STATE_DIR = previous.managedStateDir;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.permissionPath === undefined) delete process.env.AGENTCABIN_WORK_PERMISSION_PATH; else process.env.AGENTCABIN_WORK_PERMISSION_PATH = previous.permissionPath;
    if (previous.executionMode === undefined) delete process.env.AGENTCABIN_WORK_EXECUTION_MODE; else process.env.AGENTCABIN_WORK_EXECUTION_MODE = previous.executionMode;
    if (previous.policy === undefined) delete process.env.AGENTCABIN_WORK_POLICY; else process.env.AGENTCABIN_WORK_POLICY = previous.policy;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Writing a reserved output area uses the selected project root in direct artifact mode", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-direct-output-write-"));
  const workspaceRoot = path.join(temp, "project");
  const managedStateRoot = path.join(temp, "managed-state");
  fs.mkdirSync(workspaceRoot, { recursive: true });
  fs.mkdirSync(path.join(managedStateRoot, "context"), { recursive: true });
  fs.writeFileSync(
    path.join(managedStateRoot, "manifest.json"),
    JSON.stringify({
      rootKind: "local_folder",
      primaryWorkRoot: workspaceRoot,
      artifactStorageMode: "primary_work_root",
      accessRoots: [],
    }),
    "utf8",
  );
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    managedStateDir: process.env.AGENTCABIN_MANAGED_STATE_DIR,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_MANAGED_STATE_DIR = managedStateRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "direct-output-fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49325";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async (_url, init = {}) => {
    const body = JSON.parse(init.body || "{}");
    if (body.toolName === "work_write_file") {
      const filePath = path.join(workspaceRoot, body.arguments.path);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, body.arguments.content, "utf8");
    }
    return {
      ok: true,
      status: 200,
      async json() {
        return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
      },
    };
  };
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const written = await tools.get("work_write_file").execute(
      "direct-output-write",
      { path: "output/report.md", content: "# Direct output\n" },
    );
    assert.equal(written.details.ok, true);
    assert.equal(written.details.storage_scope, "primary_output");
    assert.equal(written.details.resolved_path, path.join(fs.realpathSync(workspaceRoot), "output", "report.md"));
    assert.equal(
      fs.readFileSync(path.join(workspaceRoot, "output", "report.md"), "utf8"),
      "# Direct output\n",
    );
    assert.equal(fs.existsSync(path.join(managedStateRoot, "output", "report.md")), false);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.managedStateDir === undefined) delete process.env.AGENTCABIN_MANAGED_STATE_DIR; else process.env.AGENTCABIN_MANAGED_STATE_DIR = previous.managedStateDir;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work edit_file applies an exact edit inside a writable area", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-edit-file-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "scratch", "source.js"), "const value = 1;\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = mockPipelineFetch(workspaceRoot);
  let confirmations = 0;
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });
    const edited = await tools.get("work_edit_file").execute(
      "edit-1",
      { path: "scratch/source.js", old_text: "const value = 1;", new_text: "const value = 2;" },
      undefined,
      undefined,
      { hasUI: true, ui: { confirm: async () => { confirmations += 1; return true; } } },
    );
    assert.equal(edited.details.ok, true);
    assert.equal(confirmations, 0, "edits inside the task trust boundary should not require confirmation");
    assert.equal(fs.readFileSync(path.join(workspaceRoot, "scratch", "source.js"), "utf8"), "const value = 2;\n");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_run_command is registered and work_execute is removed", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-run-command-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = async (url, init = {}) => {
    return {
      ok: true,
      status: 200,
      async json() {
        return { success: true, status: "success", exitCode: 0, stdout: "Command executed" };
      },
    };
  };
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });
    assert.ok(tools.get("work_run_command"), "work_run_command must be registered as the confined shell tool");
    assert.ok(tools.get("work_command_info"), "work_command_info must be registered as the preflight tool");
    assert.equal(tools.get("work_execute"), undefined, "work_execute must no longer be registered");

    let lastPipelineCall = null;
    const pipelineCalls = [];
    globalThis.fetch = async (url, init = {}) => {
      const parsed = JSON.parse(init.body || "{}");
      lastPipelineCall = parsed;
      pipelineCalls.push(parsed);
      return {
        ok: true,
        status: 200,
        async json() {
          return { success: true, status: "success", exitCode: 0, stdout: "ok" };
        },
      };
    };

    // Test work_command_info execution
    await tools.get("work_command_info").execute("call-cmd-info", { command: "soffice" });
    assert.equal(lastPipelineCall.toolName, "work_command_info");
    assert.equal(lastPipelineCall.arguments.command, "soffice");

    // Test work_run_command strips 2>&1 and splits command string
    await tools.get("work_run_command").execute("call-run-cmd", {
      command: "soffice --convert-to xlsx 2>&1",
      args: ["output/report.xlsx", "2>&1"],
    });
    assert.equal(lastPipelineCall.toolName, "work_run_command");
    assert.equal(lastPipelineCall.arguments.command, "soffice");
    assert.deepEqual(lastPipelineCall.arguments.args, ["--convert-to", "xlsx", "output/report.xlsx"]);

    pipelineCalls.length = 0;
    await tools.get("bash").execute("call-chain", {
      command: "cp scratch/recalc/销售汇总.xlsx output/销售汇总.xlsx && ls -la output/",
    });
    assert.deepEqual(
      pipelineCalls.map((call) => [call.toolName, call.arguments.command, call.arguments.args]),
      [
        ["work_run_command", "cp", ["scratch/recalc/销售汇总.xlsx", "output/销售汇总.xlsx"]],
        ["work_run_command", "ls", ["-la", "output/"]],
      ],
    );

    pipelineCalls.length = 0;
    const inlinePython = `print("long script")\n${"# filler\n".repeat(1100)}`;
    await tools.get("work_run_command").execute("call-python-script", {
      command: "python3.12",
      args: ["-c", inlinePython],
      cwd: ".",
    });
    assert.equal(pipelineCalls[0].toolName, "work_write_file");
    assert.match(pipelineCalls[0].arguments.path, /^scratch\/.agentcabin-python-[0-9a-f-]+\.py$/);
    assert.equal(pipelineCalls[0].arguments.content, inlinePython);
    assert.deepEqual(pipelineCalls[1].arguments, {
      command: "python3.12",
      args: [pipelineCalls[0].arguments.path],
      cwd: ".",
      expected_outputs: [],
      timeout_seconds: 120,
    });
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Artifacts are isolated by Work Run and keep previous runs intact", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-artifact-runs-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runId: process.env.AGENTCABIN_WORK_RUN_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = mockPipelineFetch(workspaceRoot);
  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });
    const context = { hasUI: true, ui: { confirm: async () => true } };

    process.env.AGENTCABIN_WORK_RUN_ID = "run-a";
    await tools.get("work_write_file").execute(
      "write-a",
      { path: "output/report.md", content: "run a\n" },
      undefined,
      undefined,
      context,
    );
    process.env.AGENTCABIN_WORK_RUN_ID = "run-b";
    await tools.get("work_write_file").execute(
      "write-b",
      { path: "output/report.md", content: "run b\n" },
      undefined,
      undefined,
      context,
    );

    const registry = JSON.parse(fs.readFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "utf8"));
    assert.equal(registry.artifacts.length, 2);
    assert.deepEqual(
      registry.artifacts.map((artifact) => artifact.run_id).sort(),
      ["run-a", "run-b"],
    );

    const list = tools.get("work_list_artifacts");
    const runB = await list.execute("list-b", {});
    assert.equal(runB.details.artifacts.length, 1);
    assert.equal(runB.details.artifacts[0].run_id, "run-b");
    process.env.AGENTCABIN_WORK_RUN_ID = "run-a";
    const runA = await list.execute("list-a", {});
    assert.equal(runA.details.artifacts.length, 1);
    assert.equal(runA.details.artifacts[0].run_id, "run-a");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runId === undefined) delete process.env.AGENTCABIN_WORK_RUN_ID;
    else process.env.AGENTCABIN_WORK_RUN_ID = previous.runId;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("register claims an unattributed reconciled artifact instead of duplicating it", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-artifact-claim-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "output", "report.md"), "report\n", "utf8");
  // Simulate a Workspace-level reconciliation entry discovered without a run.
  fs.writeFileSync(
    path.join(workspaceRoot, "context", "artifacts.json"),
    JSON.stringify({
      version: 1,
      artifacts: [
        {
          id: "legacy-1",
          workspace_id: "fixture",
          run_id: null,
          artifact_type: "md",
          title: "report.md",
          path: "output/report.md",
          status: "ready",
          size: 7,
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
        },
      ],
    }),
    "utf8",
  );
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    runId: process.env.AGENTCABIN_WORK_RUN_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_RUN_ID = "run-x";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  globalThis.fetch = mockPipelineFetch(workspaceRoot);
  const tools = new Map();
  try {
    const extension = await loadExtension(temp);
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    await tools.get("work_register_artifact").execute(
      "register-x",
      { path: "output/report.md", title: "Q2 Report" },
    );

    const registry = JSON.parse(fs.readFileSync(path.join(workspaceRoot, "context", "artifacts.json"), "utf8"));
    assert.equal(registry.artifacts.length, 1, "register must claim the unattributed entry, not duplicate it");
    assert.equal(registry.artifacts[0].id, "legacy-1");
    assert.equal(registry.artifacts[0].run_id, "run-x");
    assert.equal(registry.artifacts[0].title, "Q2 Report");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.runId === undefined) delete process.env.AGENTCABIN_WORK_RUN_ID;
    else process.env.AGENTCABIN_WORK_RUN_ID = previous.runId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT;
    else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN;
    else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work core exposes Browser tools only when the explicit Browser capability gate is enabled", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-browser-catalog-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    browser: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    let beforeAgentStart;
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on(eventName, handler) {
        if (eventName === "before_agent_start") beforeAgentStart = handler;
      },
    });
    const listed = await tools.get("work_list_tools").execute("catalog-1", {});
    assert.equal(listed.details.tools.some((tool) => tool.name === "web_search"), true);
    assert.equal(listed.details.tools.some((tool) => tool.name === "web_cite"), true);
    const prompt = await beforeAgentStart({ systemPrompt: "base" });
    assert.match(prompt.systemPrompt, /never guess, shorten, rename, or synthesize an ID/);
    assert.match(prompt.systemPrompt, /only after web_cite succeeds/);
    assert.match(prompt.systemPrompt, /never replace missing evidence with model memory/);
    assert.match(prompt.systemPrompt, /```html-preview/);
    assert.match(prompt.systemPrompt, /无需先写文件或登记 Artifact/);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.browser === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.browser;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work core exposes Computer Use V2 tools only when the desktop gate is enabled", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-desktop-catalog-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    desktop: process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED = "1";
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });
    assert.equal(tools.has("desktop_list_apps"), false);
    assert.equal(tools.has("find_roots"), true);
    assert.equal(tools.has("observe_ui"), true);
    assert.equal(tools.has("act_ui"), true);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.desktop === undefined) delete process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED; else process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED = previous.desktop;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("standalone Work prompt does not suggest Workspace access authorization", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-standalone-access-guidance-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    standalone: process.env.AGENTCABIN_WORK_STANDALONE,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  delete process.env.AGENTCABIN_WORKSPACE_ID;
  process.env.AGENTCABIN_WORK_STANDALONE = "1";
  try {
    const extension = await loadExtension(temp);
    let beforeAgentStart;
    extension({
      registerTool() {},
      setActiveTools() {},
      on(eventName, handler) {
        if (eventName === "before_agent_start") beforeAgentStart = handler;
      },
    });

    const prompt = await beforeAgentStart({ systemPrompt: "base" });
    assert.match(prompt.systemPrompt, /standalone Work task without a Workspace/);
    assert.match(prompt.systemPrompt, /external directory authorization is unavailable/i);
    assert.match(prompt.systemPrompt, /do not call work_request_directory_access/);
    assert.match(prompt.systemPrompt, /```html-preview/);
    assert.match(prompt.systemPrompt, /文件路径或 Markdown 数据表不能替代/);
    assert.match(prompt.systemPrompt, /file:\/\/ 或 loopback 访问失败不代表对话内预览不可用/);
    const followUp = await beforeAgentStart({ systemPrompt: "follow-up" });
    assert.match(followUp.systemPrompt, /```html-preview/);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.standalone === undefined) delete process.env.AGENTCABIN_WORK_STANDALONE; else process.env.AGENTCABIN_WORK_STANDALONE = previous.standalone;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work core exposes MCP only when the explicit MCP runtime gate is enabled", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-mcp-catalog-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");
  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    mcp: process.env.AGENTCABIN_WORK_MCP_ENABLED,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  delete process.env.AGENTCABIN_WORK_MCP_ENABLED;
  try {
    async function listedTools() {
      const extension = await loadExtension(temp);
      const tools = new Map();
      extension({
        registerTool(tool) { tools.set(tool.name, tool); },
        setActiveTools() {},
        on() {},
      });
      return tools.get("work_list_tools").execute("catalog-mcp", {});
    }

    const disabled = await listedTools();
    assert.equal(disabled.details.tools.some((tool) => tool.name === "mcp"), false);

    process.env.AGENTCABIN_WORK_MCP_ENABLED = "1";
    const enabled = await listedTools();
    assert.equal(enabled.details.tools.some((tool) => tool.name === "mcp"), true);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.mcp === undefined) delete process.env.AGENTCABIN_WORK_MCP_ENABLED; else process.env.AGENTCABIN_WORK_MCP_ENABLED = previous.mcp;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work compatibility tools preserve the Work boundary and reject /etc/passwd, ../, symlink escape", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-security-"));
  const workspaceRoot = path.join(temp, "workspace");
  const inputDir = path.join(workspaceRoot, "input");
  fs.mkdirSync(inputDir, { recursive: true });
  fs.mkdirSync(path.join(workspaceRoot, "context"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  // Create a symlink in input pointing to outside (/etc/passwd or temp secret)
  const secretPath = path.join(temp, "outside_secret.txt");
  fs.writeFileSync(secretPath, "secret data", "utf8");
  const symlinkPath = path.join(inputDir, "symlink_outside.txt");
  try {
    fs.symlinkSync(secretPath, symlinkPath);
  } catch {
    // Windows non-admin fallback
  }

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "security-fixture";

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    // 1. Native names are compatibility wrappers, not unrestricted Pi implementations.
    for (const nativeName of ["bash", "read", "write", "edit"]) {
      assert.equal(tools.has(nativeName), true, `Compatibility tool '${nativeName}' must be registered`);
      const res = await tools.get(nativeName).execute("id-1", {});
      assert.equal(res.details.ok, false);
      assert.doesNotMatch(res.content[0].text, /unrestricted native/i);
    }

    // 2. /etc/passwd or absolute external path reading attempt
    const etcRes = await tools.get("read").execute("read-passwd", { path: "/etc/passwd" });
    assert.equal(etcRes.details.ok, false);
    assert.match(etcRes.content[0].text, /outside/i);

    // 3. Relative path traversal (../)
    const dotRes = await tools.get("read").execute("read-dotdot", { path: "../outside_secret.txt" });
    assert.equal(dotRes.details.ok, false);
    assert.match(dotRes.content[0].text, /escape/i);

    // 4. Symlink pointing outside workspace (if created)
    if (fs.existsSync(symlinkPath)) {
      const symRes = await tools.get("read").execute("read-sym", { path: "input/symlink_outside.txt" });
      assert.equal(symRes.details.ok, false);
      assert.match(symRes.content[0].text, /outside/i);
    }
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID;
    else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_run_command fails closed on a failed ExecutionResult", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-run-command-bridge-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  const requests = [];
  globalThis.fetch = async (url, init = {}) => {
    requests.push({ url: String(url), init });
    return {
      ok: true,
      status: 200,
      async json() {
        return {
          success: false,
          status: "failed",
          failureKind: "sandbox_denied",
          exitCode: 7,
          stdout: "",
          stderr: "sandbox: denied",
          outputs: [],
        };
      },
    };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const execution = await tools.get("work_run_command").execute(
      "tool-1",
      { command: "echo", args: ["hello"], cwd: "scratch" },
    );
    assert.equal(execution.details.ok, false);
    assert.equal(execution.details.failure_kind, "sandbox_denied");
    assert.equal(execution.details.exit_code, 7);
    assert.equal(execution.details.stderr, "sandbox: denied");
    assert.match(execution.content[0].text, /failed/i);
    assert.equal(requests.length, 1);
    assert.match(requests[0].url, /127\.0\.0\.1:49321\/internal\/work\/tool_pipeline$/);
    assert.equal(requests[0].init.headers.Authorization, "Bearer test-token");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("unattended work_run_command waits on durable Inbox approval and retries", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-run-command-inbox-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "scratch"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49322";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  const requests = [];
  let pipelineCount = 0;
  globalThis.fetch = async (url, init = {}) => {
    const request = { url: String(url), init };
    requests.push(request);
    if (request.url.endsWith("/internal/work/tool_pipeline")) {
      pipelineCount += 1;
      return {
        ok: true,
        status: 200,
        async json() {
          return pipelineCount === 1
            ? { success: false, status: "waiting_approval", interactionId: "inbox-1" }
            : { success: true, status: "success", exitCode: 0, stdout: "done", outputs: [] };
        },
      };
    }
    assert.match(request.url, /\/internal\/work\/inbox_status\/inbox-1$/);
    return {
      ok: true,
      status: 200,
      async json() { return { status: "approved" }; },
    };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const execution = await tools.get("work_run_command").execute(
      "tool-2",
      { command: "echo", args: ["hello"], cwd: "scratch" },
    );
    assert.equal(execution.details.ok, true);
    assert.equal(pipelineCount, 2);
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("work_write_file waits on durable Inbox approval and retries instead of failing early", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-write-file-inbox-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    fetch: globalThis.fetch,
  };
  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "fixture";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49323";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  const requests = [];
  let pipelineCount = 0;
  globalThis.fetch = async (url, init = {}) => {
    const request = { url: String(url), init };
    requests.push(request);
    if (request.url.endsWith("/internal/work/tool_pipeline")) {
      pipelineCount += 1;
      const body = JSON.parse(init.body || "{}");
      if (pipelineCount === 1) {
        return {
          ok: true,
          status: 200,
          async json() {
            return { success: false, status: "waiting_approval", interactionId: "write-inbox-1" };
          },
        };
      }
      const filePath = path.join(workspaceRoot, body.arguments.path);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, body.arguments.content, "utf8");
      return {
        ok: true,
        status: 200,
        async json() {
          return { success: true, status: "success", exitCode: 0, outputs: [body.arguments.path] };
        },
      };
    }
    assert.match(request.url, /\/internal\/work\/inbox_status\/write-inbox-1$/);
    return {
      ok: true,
      status: 200,
      async json() { return { status: "approved" }; },
    };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools() {},
      on() {},
    });

    const execution = await tools.get("work_write_file").execute(
      "write-inbox",
      { path: "output/readmA9.txt", content: "123" },
    );
    assert.equal(execution.details.ok, true);
    assert.equal(execution.details.confirmed, true);
    assert.equal(pipelineCount, 2);
    const written = fs.readFileSync(path.join(workspaceRoot, "output/readmA9.txt"), "utf8");
    assert.equal(written, "123");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Pi Work delegates questions to the native ask_user_question extension", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-native-questions-"));
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    const eventHandlers = new Map();
    let active = [];
    extension({
      registerTool(tool) { tools.set(tool.name, tool); },
      setActiveTools(names) { active = names; },
      on(event, handler) { eventHandlers.set(event, handler); },
    });
    eventHandlers.get("session_start")?.();
    assert(!tools.has("ask_questions"), "Pi Work must not register a duplicate questionnaire");
    assert(active.includes("ask_user_question"), "native Pi questionnaire must remain active");
    assert(active.includes("todo"), "native Pi todo tool must remain active");
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Child subagent mode (researcher) enforces read-only boundary and denies root tools", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-child-researcher-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "input", "data.txt"), "hello world\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    childEnv: process.env.PI_SUBAGENT_CHILD,
    childAgent: process.env.PI_SUBAGENT_CHILD_AGENT,
    requiredTools: process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS,
  };

  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "ws-child-test";
  process.env.PI_SUBAGENT_CHILD = "1";
  process.env.PI_SUBAGENT_CHILD_AGENT = "agentcabin-researcher";
  process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS = JSON.stringify([
    "read",
    "web_search",
    "web_open",
    "web_extract",
    "web_cite",
  ]);

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    let currentActiveTools = [];
    let activeToolSetCalls = 0;
    const eventHandlers = new Map();

    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools(active) {
        activeToolSetCalls += 1;
        currentActiveTools = active;
      },
      on(event, handler) {
        eventHandlers.set(event, handler);
      },
    });

    // Fire session_start
    if (eventHandlers.has("session_start")) {
      eventHandlers.get("session_start")();
    }

    // 1. Root tools must NOT be active or registered
    assert(!tools.has("work_set_goal"), "work_set_goal must not be registered in child mode");
    assert(!tools.has("work_replace_plan"), "work_replace_plan must not be registered in child mode");
    assert(!tools.has("work_activate_tools"), "work_activate_tools must not be registered in child mode");
    assert(!tools.has("work_delegate"), "work_delegate must not be registered in child mode");
    assert(!tools.has("ask_questions"), "ask_questions must not be registered in child mode");

    // 2. Researcher receives the Work-owned, read-only Web research chain.
    for (const name of ["web_search", "web_open", "web_extract", "web_cite"]) {
      assert(tools.has(name), `${name} must be registered for a researcher child`);
    }

    // 3. Child Work Core must not replace the launcher's authoritative tool set.
    assert.deepEqual(currentActiveTools, []);
    assert.equal(activeToolSetCalls, 0, "Child Work Core must never call setActiveTools");

    // 4. Executing read succeeds
    const readCompat = tools.get("read");
    const readRes = await readCompat.execute("r1", { path: "input/data.txt" });
    assert.equal(readRes.details.ok, true);
    assert.equal(readRes.content[0].text, "hello world\n");

    // 5. Executing write or edit or bash through compat wrapper fails with permission denied
    const writeCompat = tools.get("write");
    const writeRes = await writeCompat.execute("w1", { path: "output/evil.txt", content: "evil" });
    assert.equal(writeRes.details.ok, false);
    assert.match(writeRes.content[0].text, /Permission denied/i);

    const editCompat = tools.get("edit");
    const editRes = await editCompat.execute("e1", { path: "input/data.txt", oldText: "hello", newText: "bye" });
    assert.equal(editRes.details.ok, false);
    assert.match(editRes.content[0].text, /Permission denied/i);

    const bashCompat = tools.get("bash");
    const bashRes = await bashCompat.execute("b1", { command: "ls" });
    assert.equal(bashRes.details.ok, false);
    assert.match(bashRes.content[0].text, /Permission denied/i);

    // 6. Privilege widening: calling before_agent_start does NOT restore root tools
    if (eventHandlers.has("before_agent_start")) {
      const promptRes = await eventHandlers.get("before_agent_start")({ systemPrompt: "Initial" });
      assert.match(promptRes.systemPrompt, /AgentCabin Child Subagent Mode/);
      assert.match(promptRes.systemPrompt, /web_search, web_open, web_extract, then web_cite/);
    }
    assert(!currentActiveTools.includes("write"), "write must NOT become active after before_agent_start");
    assert(!currentActiveTools.includes("bash"), "bash must NOT become active after before_agent_start");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.childEnv === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.childEnv;
    if (previous.childAgent === undefined) delete process.env.PI_SUBAGENT_CHILD_AGENT; else process.env.PI_SUBAGENT_CHILD_AGENT = previous.childAgent;
    if (previous.requiredTools === undefined) delete process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS; else process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS = previous.requiredTools;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Child subagent mode (worker) allows read, write, edit, and bash but rejects root harness tools", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-child-worker-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "output"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    childEnv: process.env.PI_SUBAGENT_CHILD,
    childAgent: process.env.PI_SUBAGENT_CHILD_AGENT,
    requiredTools: process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS,
    fetch: globalThis.fetch,
  };

  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "ws-child-worker-test";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "9999";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
  process.env.PI_SUBAGENT_CHILD = "1";
  process.env.PI_SUBAGENT_CHILD_AGENT = "agentcabin-worker";
  process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS = JSON.stringify(["read", "write", "edit", "bash"]);

  globalThis.fetch = async (url, options) => {
    if (options?.body) {
      try {
        const body = JSON.parse(options.body);
        if (body.arguments?.path && body.arguments?.content !== undefined) {
          const filePath = path.join(workspaceRoot, body.arguments.path);
          fs.mkdirSync(path.dirname(filePath), { recursive: true });
          fs.writeFileSync(filePath, body.arguments.content, "utf8");
        }
      } catch (_) {}
    }
    return {
      ok: true,
      status: 200,
      async json() {
        return { success: true, status: "allowed", stdout: "", stderr: "" };
      },
    };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    let currentActiveTools = [];
    let activeToolSetCalls = 0;
    const eventHandlers = new Map();

    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools(active) {
        activeToolSetCalls += 1;
        currentActiveTools = active;
      },
      on(event, handler) {
        eventHandlers.set(event, handler);
      },
    });

    if (eventHandlers.has("session_start")) {
      eventHandlers.get("session_start")();
    }

    // 1. The launcher's --tools read/write/edit/bash set remains authoritative.
    assert.deepEqual(currentActiveTools, []);
    assert.equal(activeToolSetCalls, 0, "Child Work Core must never call setActiveTools");

    // 2. Worker still DOES NOT have Root-only harness tools
    assert(!tools.has("work_set_goal"));
    assert(!tools.has("work_replace_plan"));
    assert(!tools.has("work_activate_tools"));
    assert(!tools.has("work_delegate"));
    assert(!tools.has("ask_questions"));
    assert(!currentActiveTools.includes("work_set_goal"));

    // 3. Worker can write files safely through the wrapper
    const writeCompat = tools.get("write");
    const writeRes = await writeCompat.execute("w1", { path: "output/worker_out.txt", content: "worker content" });
    assert.equal(writeRes.details.ok, true);
    const written = fs.readFileSync(path.join(workspaceRoot, "output/worker_out.txt"), "utf8");
    assert.equal(written, "worker content");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.childEnv === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.childEnv;
    if (previous.childAgent === undefined) delete process.env.PI_SUBAGENT_CHILD_AGENT; else process.env.PI_SUBAGENT_CHILD_AGENT = previous.childAgent;
    if (previous.requiredTools === undefined) delete process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS; else process.env.PI_SUBAGENT_REQUIRED_CHILD_TOOLS = previous.requiredTools;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Child subagent executes lazy token exchange before invoking Work Bridge", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-child-token-exchange-"));
  const workspaceRoot = path.join(temp, "workspace");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.writeFileSync(path.join(workspaceRoot, "input", "data.txt"), "hello world\n", "utf8");
  fs.writeFileSync(path.join(workspaceRoot, "manifest.json"), JSON.stringify({ accessRoots: [] }), "utf8");

  const previous = {
    workspaceRoot: process.env.AGENTCABIN_WORKSPACE_ROOT,
    workspaceId: process.env.AGENTCABIN_WORKSPACE_ID,
    bridgePort: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    bridgeToken: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
    childEnv: process.env.PI_SUBAGENT_CHILD,
    childAgent: process.env.PI_SUBAGENT_CHILD_AGENT,
    childRunId: process.env.PI_SUBAGENT_RUN_ID,
    fetch: globalThis.fetch,
  };

  process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
  process.env.AGENTCABIN_WORKSPACE_ID = "ws-exchange-test";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "65432";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "parent-root-token";
  process.env.PI_SUBAGENT_CHILD = "1";
  process.env.PI_SUBAGENT_CHILD_AGENT = "agentcabin-worker";
  process.env.PI_SUBAGENT_RUN_ID = "subagent-run-exchange-1";

  const fetchCalls = [];
  globalThis.fetch = async (url, options) => {
    fetchCalls.push({ url, options });
    if (url.includes("/internal/work/subagents/token")) {
      return {
        ok: true,
        json: async () => ({
          token: "exchanged-child-token",
          agentId: "subagent-run-exchange-1",
          role: "agentcabin-worker",
        }),
      };
    }
    if (url.includes("/internal/work/tool_pipeline")) {
      return {
        ok: true,
        json: async () => ({
          success: true,
          status: "success",
          stdout: "exchanged ok",
          exitCode: 0,
        }),
      };
    }
    return { ok: false, status: 404, json: async () => ({}) };
  };

  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    extension({
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on() {},
    });

    const runCommand = tools.get("work_run_command");
    assert(runCommand);
    const result = await runCommand.execute("cmd-1", { command: "echo ok" });
    assert.equal(result.details.ok, true);

    // Verify token exchange was called first with parent token, then pipeline called with child token
    assert(fetchCalls.length >= 2);
    assert(fetchCalls[0].url.includes("/internal/work/subagents/token"));
    assert.equal(fetchCalls[0].options.headers["Authorization"], "Bearer parent-root-token");
    assert(fetchCalls[1].url.includes("/internal/work/tool_pipeline"));
    assert.equal(fetchCalls[1].options.headers["Authorization"], "Bearer exchanged-child-token");
  } finally {
    if (previous.workspaceRoot === undefined) delete process.env.AGENTCABIN_WORKSPACE_ROOT; else process.env.AGENTCABIN_WORKSPACE_ROOT = previous.workspaceRoot;
    if (previous.workspaceId === undefined) delete process.env.AGENTCABIN_WORKSPACE_ID; else process.env.AGENTCABIN_WORKSPACE_ID = previous.workspaceId;
    if (previous.bridgePort === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.bridgePort;
    if (previous.bridgeToken === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.bridgeToken;
    if (previous.childEnv === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.childEnv;
    if (previous.childAgent === undefined) delete process.env.PI_SUBAGENT_CHILD_AGENT; else process.env.PI_SUBAGENT_CHILD_AGENT = previous.childAgent;
    if (previous.childRunId === undefined) delete process.env.PI_SUBAGENT_RUN_ID; else process.env.PI_SUBAGENT_RUN_ID = previous.childRunId;
    globalThis.fetch = previous.fetch;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Parallel child token exchange matches exact IDs and uses distinct credentials", async () => {
  const children = [
    { role: "agentcabin-researcher", runId: "subagent-run-res-A", index: 0 },
    { role: "agentcabin-researcher", runId: "subagent-run-res-B", index: 1 },
    { role: "agentcabin-reviewer", runId: "subagent-run-rev-C", index: 2 },
  ];

  const exchangedTokens = new Map();
  globalThis.fetch = async (url, options) => {
    if (url.includes("/internal/work/subagents/token")) {
      const body = JSON.parse(options?.body || "{}");
      const matched = children.find(
        (c) => c.runId === body.agentId || c.runId === body.providerRunId
      );
      if (!matched) {
        return { ok: false, status: 404, json: async () => ({ error: "Not found" }) };
      }
      const token = `scoped-token-${matched.runId}`;
      exchangedTokens.set(matched.runId, token);
      return {
        ok: true,
        json: async () => ({
          token,
          agentId: matched.runId,
          role: matched.role,
        }),
      };
    }
    return { ok: true, json: async () => ({ success: true, status: "allowed" }) };
  };

  try {
    for (const child of children) {
      const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-parallel-child-"));
      const workspaceRoot = path.join(temp, "workspace");
      fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
      const externalDir = path.join(temp, "ext");
      fs.mkdirSync(externalDir, { recursive: true });
      fs.writeFileSync(path.join(externalDir, "doc.txt"), "external data", "utf8");
      fs.writeFileSync(
        path.join(workspaceRoot, "manifest.json"),
        JSON.stringify({ accessRoots: [{ path: externalDir, writable: false }] }),
        "utf8"
      );

      process.env.AGENTCABIN_WORKSPACE_ROOT = workspaceRoot;
      process.env.AGENTCABIN_WORKSPACE_ID = "ws-parallel-test";
      process.env.AGENTCABIN_WORK_BRIDGE_PORT = "65433";
      process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = `bootstrap-token-${child.runId}`;
      process.env.PI_SUBAGENT_CHILD = "1";
      process.env.PI_SUBAGENT_CHILD_AGENT = child.role;
      process.env.PI_SUBAGENT_RUN_ID = child.runId;
      process.env.PI_SUBAGENT_CHILD_INDEX = String(child.index);

      const extension = await loadExtension(temp);
      const tools = new Map();
      const eventHandlers = new Map();
      extension({
        registerTool(tool) { tools.set(tool.name, tool); },
        setActiveTools() {},
        on(event, handler) { eventHandlers.set(event, handler); },
      });

      if (eventHandlers.has("before_agent_start")) {
        await eventHandlers.get("before_agent_start")({ systemPrompt: "Initial prompt" });
      }
      fs.rmSync(temp, { recursive: true, force: true });
    }

    assert.equal(exchangedTokens.size, 3);
    assert.equal(exchangedTokens.get("subagent-run-res-A"), "scoped-token-subagent-run-res-A");
    assert.equal(exchangedTokens.get("subagent-run-res-B"), "scoped-token-subagent-run-res-B");
    assert.equal(exchangedTokens.get("subagent-run-rev-C"), "scoped-token-subagent-run-rev-C");
  } finally {
    delete process.env.AGENTCABIN_WORKSPACE_ROOT;
    delete process.env.AGENTCABIN_WORKSPACE_ID;
    delete process.env.AGENTCABIN_WORK_BRIDGE_PORT;
    delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN;
    delete process.env.PI_SUBAGENT_CHILD;
    delete process.env.PI_SUBAGENT_CHILD_AGENT;
    delete process.env.PI_SUBAGENT_RUN_ID;
    delete process.env.PI_SUBAGENT_CHILD_INDEX;
  }
});

test("Pi Work wraps ask_user_question into a single batch envelope and formats answers for the model", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-ask-batch-"));
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    const eventHandlers = new Map();
    const mockPi = {
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on(event, handler) {
        eventHandlers.set(event, handler);
      },
      getTool(name) {
        return tools.get(name);
      },
    };

    extension(mockPi);

    // Simulate upstream @juicesharp/rpiv-ask-user-question registering its tool
    mockPi.registerTool({
      name: "ask_user_question",
      label: "Ask User Question",
      description: "Ask questions",
      execute: async () => ({ content: [{ type: "text", text: "original" }] }),
    });

    eventHandlers.get("session_start")?.();

    const tool = tools.get("ask_user_question");
    assert(tool, "ask_user_question must be registered");
    assert.equal(tool.__agentCabinBatchWrapped, true);

    let sentEnvelope = null;
    const mockCtx = {
      hasUI: true,
      ui: {
        input: async (envelope) => {
          sentEnvelope = JSON.parse(envelope);
          return JSON.stringify({
            answers: [
              { id: "q1", type: "select", value: "docx", label: "Word 文档 (.docx)", wasCustom: false },
              { id: "q2", type: "select", value: "presentation", label: "对外展示", wasCustom: false },
            ],
          });
        },
      },
    };

    const response = await tool.execute(
      "call-1",
      {
        questions: [
          {
            header: "文档类型",
            question: "您希望我为您创建什么类型的文档？",
            options: [
              { label: "Word 文档 (.docx)", description: "适合长文本" },
              { label: "PPT 演示文稿 (.pptx)", description: "适合汇报" },
            ],
          },
          {
            header: "用途受众",
            question: "这份文档的主要用途和受众是什么？",
            options: [
              { label: "工作汇报", description: "向领导汇报" },
              { label: "对外展示", description: "面向客户" },
            ],
          },
        ],
      },
      null,
      null,
      mockCtx,
    );

    assert(sentEnvelope, "Must send a batch envelope");
    assert.equal(sentEnvelope.__piDeckBatchAsk, 1);
    assert.equal(sentEnvelope.questions.length, 2);
    assert.equal(sentEnvelope.questions[0].header, "文档类型");
    assert.equal(sentEnvelope.questions[1].header, "用途受众");

    assert.equal(response.details.cancelled, false);
    assert.equal(response.details.answers.length, 2);
    assert.equal(response.details.answers[0].answer, "Word 文档 (.docx)");
    assert.equal(response.details.answers[1].answer, "对外展示");

    const text = response.content[0].text;
    assert(text.includes('User has answered your questions:'));
    assert(text.includes('"您希望我为您创建什么类型的文档？"="Word 文档 (.docx)"'));
    assert(text.includes('"这份文档的主要用途和受众是什么？"="对外展示"'));
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Pi Work ask_user_question handles user cancellation as a canonical decline", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-ask-cancel-"));
  try {
    const extension = await loadExtension(temp);
    const tools = new Map();
    const eventHandlers = new Map();
    const mockPi = {
      registerTool(tool) {
        tools.set(tool.name, tool);
      },
      setActiveTools() {},
      on(event, handler) {
        eventHandlers.set(event, handler);
      },
      getTool(name) {
        return tools.get(name);
      },
    };

    extension(mockPi);
    mockPi.registerTool({
      name: "ask_user_question",
      label: "Ask User Question",
      execute: async () => ({ content: [{ type: "text", text: "original" }] }),
    });
    eventHandlers.get("session_start")?.();

    const tool = tools.get("ask_user_question");
    const mockCtx = {
      hasUI: true,
      ui: {
        input: async () => JSON.stringify({ cancelled: true, answers: [] }),
      },
    };

    const response = await tool.execute(
      "call-2",
      {
        questions: [
          {
            header: "文档类型",
            question: "您希望我为您创建什么类型的文档？",
            options: [{ label: "Word 文档" }],
          },
        ],
      },
      null,
      null,
      mockCtx,
    );

    assert.equal(response.details.cancelled, true);
    assert.equal(response.content[0].text, "User declined to answer questions");
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
