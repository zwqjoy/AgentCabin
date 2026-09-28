import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const TYPEBOX_STUB = `
export const Type = {
  Array: () => ({}),
  Boolean: () => ({}),
  Integer: () => ({}),
  Literal: (v) => v,
  Number: () => ({}),
  Object: () => ({}),
  Optional: (v) => v,
  String: () => ({}),
  Union: (...args) => args,
};
`;

async function loadAdapter(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n');
  fs.writeFileSync(
    path.join(tempRoot, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
  );
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB);
  fs.copyFileSync(
    new URL("./pi_browser_operator_adapter.mjs", import.meta.url),
    path.join(extensionDir, "adapter.mjs"),
  );
  return import(`${pathToFileURL(path.join(extensionDir, "adapter.mjs"))}?test=${Date.now()}`);
}

test("pi_browser_operator_adapter registers Browser Operator tools and executes through ToolPipeline", async (t) => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_"),
  );
  try {
    const mod = await loadAdapter(tempDir);
    const registeredTools = new Map();
    const mockRegisterWorkTool = (tool) => {
      registeredTools.set(tool.name, tool);
    };

    let executedToolName = null;
    let executedAction = null;
    let executedArgs = null;

    const mockCallToolPipeline = async (toolCallId, toolName, action, args) => {
      executedToolName = toolName;
      executedAction = action;
      executedArgs = args;
      if (toolName === "browser_navigate") {
        return {
          success: true,
          stdout: JSON.stringify({
            url: args.url,
            title: "Example Domain",
          }),
        };
      }
      if (toolName === "browser_snapshot") {
        return {
          success: true,
          stdout: JSON.stringify({
            url: "https://example.com",
            title: "Example Domain",
            tree: '[ref=e1] heading "Example Domain"\n[ref=e2] textbox "Email"\n[ref=e3] button "Submit"',
            refsCount: 3,
          }),
        };
      }
      if (toolName === "browser_click") {
        return {
          success: true,
          stdout: JSON.stringify({
            ok: true,
            action: "click",
            target: { ref: args.ref, role: "button", name: "Submit" },
            before: { revision: 1, url: "https://example.com" },
            execution: { performed: true },
            after: { revision: 2, url: "https://example.com/after-click" },
            observation: {
              snapshotType: "delta",
              revision: 2,
              title: "Submitted",
              url: "https://example.com/after-click",
              added: [{ ref: "e4", role: "status", name: "Submitted" }],
              changed: [],
              removed: [],
            },
          }),
        };
      }
      if (toolName === "browser_type") {
        return {
          success: true,
          stdout: JSON.stringify({ ok: true, action: "type", execution: { performed: true }, observation: { snapshotType: "unchanged", revision: 3, url: "https://example.com" } }),
        };
      }
      if (toolName === "browser_select_option") {
        return {
          success: true,
          stdout: JSON.stringify({ ok: true, action: "select_option", execution: { performed: true, selected: true, selectedValue: args.value, selectedLabel: "Option" }, observation: { snapshotType: "unchanged", revision: 3, url: "https://example.com" } }),
        };
      }
      if (toolName === "browser_scroll") {
        return {
          success: true,
          stdout: JSON.stringify({
            direction: args.direction,
            amount: args.amount,
          }),
        };
      }
      if (toolName === "browser_take_screenshot") {
        return {
          success: true,
          stdout: JSON.stringify({
            path: args.filename || "output.png",
          }),
        };
      }
      if (toolName === "browser_tabs") {
        return {
          success: true,
          stdout: JSON.stringify({
            action: args.action,
            tabs: [{ id: 0, title: "Example" }],
          }),
        };
      }
      return { success: true, stdout: "{}" };
    };

    mod.registerBrowserOperatorTools({}, {
      registerWorkTool: mockRegisterWorkTool,
      callToolPipeline: mockCallToolPipeline,
    });

    // 1. Check the Browser Operator tools are registered
    assert.equal(registeredTools.has("browser_navigate"), true);
    assert.equal(registeredTools.has("browser_snapshot"), true);
    assert.equal(registeredTools.has("browser_take_screenshot"), true);
    assert.equal(registeredTools.has("browser_wait_for"), true);
    assert.equal(registeredTools.has("browser_tabs"), true);
    assert.equal(registeredTools.has("browser_close"), true);
    assert.equal(registeredTools.has("browser_click"), true);
    assert.equal(registeredTools.has("browser_type"), true);
    assert.equal(registeredTools.has("browser_select_option"), true);
    assert.equal(registeredTools.has("browser_scroll"), true);
    assert.equal(registeredTools.has("browser_press_key"), true);

    // 2. Test navigate execution
    const nav = registeredTools.get("browser_navigate");
    const navRes = await nav.execute("call-1", {
      url: "https://example.com",
      wait_ms: 100,
    });
    assert.equal(executedToolName, "browser_navigate");
    assert.equal(executedAction, "navigate");
    assert.deepEqual(executedArgs, { url: "https://example.com", wait_ms: 100 });
    assert.match(navRes.content[0].text, /Navigated to https:\/\/example\.com/);

    // 3. Test click execution
    const click = registeredTools.get("browser_click");
    const clickRes = await click.execute("call-2", {
      ref: "e3",
    }, undefined, undefined, { model: { input: ["text", "image"] } });
    assert.equal(executedToolName, "browser_click");
    assert.equal(executedArgs.ref, "e3");
    assert.match(clickRes.content[0].text, /Click action executed on \[ref=e3\]/);
    assert.match(clickRes.content[0].text, /URL: https:\/\/example\.com\/after-click/);
    assert.match(clickRes.content[0].text, /Title: Submitted/);
    assert.match(clickRes.content[0].text, /1 added, 0 changed, 0 removed/);
    assert.equal(clickRes.details.observation.snapshotType, "delta");

    const textOnlyClickRes = await click.execute("call-2-text-only", { ref: "e3" }, undefined, undefined, {
      model: { input: ["text"] },
    });
    assert.equal(textOnlyClickRes.content.length, 1);
    assert.equal(textOnlyClickRes.content[0].type, "text");
    assert.match(textOnlyClickRes.content[0].text, /1 added, 0 changed, 0 removed/);

    // 4. Test type execution
    const typeTool = registeredTools.get("browser_type");
    const typeRes = await typeTool.execute("call-3", {
      ref: "e2",
      text: "admin@example.com",
      press_enter: true,
    });
    assert.equal(executedToolName, "browser_type");
    assert.equal(executedArgs.text, "admin@example.com");
    assert.match(typeRes.content[0].text, /Text input action executed on \[ref=e2\]/);

    // 5. Test select execution
    const select = registeredTools.get("browser_select_option");
    const selectRes = await select.execute("call-4", {
      ref: "e4",
      value: "opt1",
    });
    assert.equal(executedToolName, "browser_select_option");
    assert.match(selectRes.content[0].text, /Selected dropdown option/);

    // 6. Test scroll execution
    const scroll = registeredTools.get("browser_scroll");
    const scrollRes = await scroll.execute("call-5", {
      direction: "down",
      amount: 400,
    });
    assert.equal(executedToolName, "browser_scroll");
    assert.match(scrollRes.content[0].text, /Scrolled down/);

    // 7. Test screenshot execution
    const screenshot = registeredTools.get("browser_take_screenshot");
    const shotRes = await screenshot.execute("call-6", {
      filename: "output.png",
    });
    assert.equal(executedToolName, "browser_take_screenshot");
    assert.match(shotRes.content[0].text, /Screenshot saved to output\.png/);

    // 8. Test tabs execution
    const tabs = registeredTools.get("browser_tabs");
    const tabsRes = await tabs.execute("call-7", {
      action: "new",
      url: "https://example.com",
    });
    assert.equal(executedToolName, "browser_tabs");
    assert.match(tabsRes.content[0].text, /1 open tab\(s\)/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("adapter carries the latest snapshot identity and semantic locator with ref actions", async (t) => {
  const tempDir = fs.mkdtempSync(path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_identity_"));
  try {
    const mod = await loadAdapter(tempDir);
    const tools = new Map();
    let actionArgs;
    mod.registerBrowserOperatorTools({}, {
      registerWorkTool: (tool) => tools.set(tool.name, tool),
      callToolPipeline: async (_id, name, _action, args) => {
        if (name === "browser_snapshot") return { success: true, stdout: JSON.stringify({
          ok: true, snapshotType: "full", workerInstanceId: "bw-1", documentId: "doc-1", targetId: "tab-1", revision: 12,
          refs: { e17: { ref: "e17", role: "button", name: "Continue" } }, tree: "[ref=e17] button Continue",
        }) };
        actionArgs = args;
        return { success: true, stdout: JSON.stringify({ ok: true, action: "click", execution: { performed: true } }) };
      },
    });
    await tools.get("browser_snapshot").execute("snapshot", {}, undefined, undefined, { model: { input: ["text"] } });
    await tools.get("browser_click").execute("click", { ref: "e17" });
    assert.deepEqual(actionArgs.targetIdentity, { workerInstanceId: "bw-1", documentId: "doc-1", revision: 12, targetId: "tab-1" });
    assert.deepEqual(actionArgs.locator, { role: "button", name: "Continue" });
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("action observations refresh Browser refs, revision, worker, document, and tab identity", async (t) => {
  const tempDir = fs.mkdtempSync(path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_observation_"));
  try {
    const mod = await loadAdapter(tempDir);
    const tools = new Map();
    const calls = [];
    mod.registerBrowserOperatorTools({}, {
      registerWorkTool: (tool) => tools.set(tool.name, tool),
      callToolPipeline: async (_id, name, _action, args) => {
        calls.push({ name, args });
        if (name === "browser_snapshot") {
          return { success: true, stdout: JSON.stringify({
            ok: true,
            snapshotType: "full",
            workerInstanceId: "worker-1",
            documentId: "document-A",
            targetId: "tab-1",
            revision: 1,
            url: "https://example.test/a",
            refs: {
              e1: { ref: "e1", role: "button", name: "Open B" },
              old: { ref: "old", role: "button", name: "Old document" },
            },
          }) };
        }
        let observation;
        let execution = { performed: true };
        if (name === "browser_click" && args.ref === "e1") {
          observation = {
            snapshotType: "full", workerInstanceId: "worker-1", documentId: "document-B", targetId: "tab-1",
            revision: 2, url: "https://example.test/b", title: "Document B",
            refs: {
              e2: { ref: "e2", role: "button", name: "Updated" },
              e3: { ref: "e3", role: "button", name: "Remove me" },
            },
          };
        } else if (name === "browser_type") {
          observation = {
            snapshotType: "delta", workerInstanceId: "worker-1", documentId: "document-B", targetId: "tab-1",
            revision: 3, url: "https://example.test/b",
            added: [{ ref: "e4", role: "status", name: "Added" }],
            changed: [{ ref: "e2", after: { role: "button", name: "Updated after delta" } }],
            removed: ["e3"],
          };
        } else if (name === "browser_select_option") {
          execution = { performed: true, selected: true, selectedValue: "yes", selectedLabel: "Yes" };
          observation = {
            snapshotType: "unchanged", workerInstanceId: "worker-1", documentId: "document-B", targetId: "tab-1",
            revision: 4, url: "https://example.test/b", title: "Still B",
          };
        } else if (name === "browser_press_key") {
          observation = {
            snapshotType: "full", workerInstanceId: "worker-2", documentId: "document-B", targetId: "tab-1",
            revision: 1, url: "https://example.test/b", refs: { e5: { ref: "e5", role: "button", name: "Worker 2" } },
          };
        } else if (name === "browser_click" && args.ref === "e5") {
          observation = {
            snapshotType: "full", workerInstanceId: "worker-2", documentId: "document-B", targetId: "tab-2",
            revision: 1, url: "https://example.test/other-tab", refs: { e6: { ref: "e6", role: "button", name: "Tab 2" } },
          };
        }
        return { success: true, stdout: JSON.stringify({ ok: true, execution, ...(observation ? { observation } : {}) }) };
      },
    });

    const snapshot = tools.get("browser_snapshot");
    const click = tools.get("browser_click");
    const type = tools.get("browser_type");
    const select = tools.get("browser_select_option");
    const pressKey = tools.get("browser_press_key");
    await snapshot.execute("snapshot-A", {});
    await click.execute("click-unknown-ref", { ref: "not-in-latest-snapshot" });
    assert.equal(calls.at(-1).args.targetIdentity, undefined);
    assert.equal(calls.at(-1).args.locator, undefined);

    await click.execute("click-to-B", { ref: "e1" });
    await click.execute("click-in-B", { ref: "e2" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-1", documentId: "document-B", revision: 2, targetId: "tab-1",
    });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Updated" });

    await type.execute("type-delta", { ref: "e2", text: "value" });
    await click.execute("click-changed", { ref: "e2" });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Updated after delta" });
    await click.execute("click-added", { ref: "e4" });
    assert.deepEqual(calls.at(-1).args.locator, { role: "status", name: "Added" });
    await click.execute("click-removed", { ref: "e3" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-1", documentId: "document-B", revision: 2, targetId: "tab-1",
    });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Remove me" });

    await select.execute("select-unchanged", { ref: "e2", value: "yes" });
    await click.execute("click-after-unchanged", { ref: "e2" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-1", documentId: "document-B", revision: 4, targetId: "tab-1",
    });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Updated after delta" });

    await pressKey.execute("press-key-worker-change", { key: "Enter" });
    await click.execute("click-old-worker-ref", { ref: "e2" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-1", documentId: "document-B", revision: 4, targetId: "tab-1",
    });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Updated after delta" });

    await click.execute("click-to-tab-2", { ref: "e5" });
    await click.execute("click-old-tab-ref", { ref: "e5" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-2", documentId: "document-B", revision: 1, targetId: "tab-1",
    });
    assert.deepEqual(calls.at(-1).args.locator, { role: "button", name: "Worker 2" });
    await click.execute("click-new-tab-ref", { ref: "e6" });
    assert.deepEqual(calls.at(-1).args.targetIdentity, {
      workerInstanceId: "worker-2", documentId: "document-B", revision: 1, targetId: "tab-2",
    });
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("pi_browser_operator_adapter handles waiting_approval and resolves after grant", async () => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_approval_"),
  );
  try {
    const mod = await loadAdapter(tempDir);
    const registeredTools = new Map();
    let attempt = 0;
    const mockCallToolPipeline = async (toolCallId, toolName, action, args) => {
      attempt += 1;
      if (attempt === 1) {
        return {
          success: false,
          status: "waiting_approval",
          interactionId: "interaction-123",
          stderr: "Requires user approval in Inbox",
        };
      }
      return {
        success: true,
        status: "success",
        stdout: JSON.stringify({ ok: true, clicked: true }),
      };
    };

    let waitCalled = false;
    const mockWaitForInbox = async () => {
      waitCalled = true;
      return { status: "approved" };
    };

    mod.registerBrowserOperatorTools({}, {
      registerWorkTool: (tool) => {
        registeredTools.set(tool.name, tool);
      },
      callToolPipeline: mockCallToolPipeline,
      waitForWorkInboxResolution: mockWaitForInbox,
    });

    // Verify tools can be invoked and handle approval cycle
    assert(registeredTools.has("browser_click"));
    const click = registeredTools.get("browser_click");
    const clickRes = await click.execute("call-retry-1", { ref: "e1" });
    assert(waitCalled, "waitForWorkInboxResolution must be called on waiting_approval");
    assert.equal(attempt, 2, "Should retry calling pipeline after approval");
    assert.equal(clickRes.details.ok, true);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("pi_browser_operator_adapter uses the shared Browser Runtime bridge for Code", async () => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_code_")
  );
  try {
    const mod = await loadAdapter(tempDir);
    const registeredTools = new Map();
    let runtimeCall = null;
    mod.registerBrowserOperatorTools({}, {
      registerTool: (tool) => registeredTools.set(tool.name, tool),
      callBrowserRuntime: async (toolCallId, toolName, params) => {
        runtimeCall = { toolCallId, toolName, params };
        return {
          success: true,
          stdout: JSON.stringify({ url: params.url, title: "Example Domain" }),
        };
      },
    });

    const navigate = registeredTools.get("browser_navigate");
    const result = await navigate.execute("code-call-1", { url: "https://example.com" });
    assert.deepEqual(runtimeCall, {
      toolCallId: "code-call-1",
      toolName: "browser_navigate",
      params: { url: "https://example.com" },
    });
    assert.match(result.content[0].text, /Navigated to https:\/\/example\.com/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("createBrowserInvoker rejects structured browser failures instead of returning stale success", async () => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_browser_failure_"),
  );
  try {
    const mod = await loadAdapter(tempDir);
    const invoker = mod.createBrowserInvoker({
      callToolPipeline: async () => ({
        success: true,
        stdout: JSON.stringify({ ok: false, error: "stale_browser_target: target closed" }),
      }),
    });
    await assert.rejects(
      () => invoker("failure-call", "browser_snapshot", {}),
      /stale_browser_target: target closed/,
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});
