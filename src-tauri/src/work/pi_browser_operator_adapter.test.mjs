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

test("pi_browser_operator_adapter registers all 10 tools and executes through ToolPipeline", async (t) => {
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
            ref: args.ref,
            button: "left",
            url: "https://example.com/after-click",
            title: "Submitted",
            tree: '[ref=e1] heading "Submitted"',
            screenshot: "data:image/png;base64,aGVsbG8=",
          }),
        };
      }
      if (toolName === "browser_type") {
        return {
          success: true,
          stdout: JSON.stringify({
            ref: args.ref,
            textLength: args.text.length,
          }),
        };
      }
      if (toolName === "browser_select_option") {
        return {
          success: true,
          stdout: JSON.stringify({
            ref: args.ref,
            selected: [args.value],
          }),
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

    // 1. Check all 10 tools are registered
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
    assert.equal(clickRes.content[1].type, "image");
    assert.equal(clickRes.content[1].mimeType, "image/png");
    assert.equal(clickRes.content[1].data, "aGVsbG8=");
    assert.match(clickRes.content[0].text, /URL: https:\/\/example\.com\/after-click/);
    assert.match(clickRes.content[0].text, /Title: Submitted/);
    assert.equal(clickRes.details.tree, '[ref=e1] heading "Submitted"');
    assert.equal(clickRes.details.screenshot, "data:image/png;base64,aGVsbG8=");

    const textOnlyClickRes = await click.execute("call-2-text-only", { ref: "e3" }, undefined, undefined, {
      model: { input: ["text"] },
    });
    assert.equal(textOnlyClickRes.content.length, 1);
    assert.equal(textOnlyClickRes.content[0].type, "text");
    assert.match(textOnlyClickRes.content[0].text, /Page snapshot:\n\[ref=e1\] heading "Submitted"/);
    assert.equal(textOnlyClickRes.details.screenshot, "data:image/png;base64,aGVsbG8=");

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
    assert.match(selectRes.content[0].text, /Selected 'opt1'/);

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
    assert.match(tabsRes.content[0].text, /Tab action 'new' completed/);
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
