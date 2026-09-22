import assert from "node:assert/strict";
import { test } from "node:test";

process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "test-token";
process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED = "1";

const { callTool, desktopTools } = await import(`./dsh_work_mcp_adapter.mjs?test=${Date.now()}`);

test("DSH exposes the Computer Use V2 stateful schema", () => {
  const schemas = new Map(desktopTools.map((tool) => [tool.name, tool.inputSchema]));
  assert.deepEqual(schemas.get("act_ui").required, ["stateId", "actions"]);
  assert.deepEqual(schemas.get("read_text").required, ["stateId", "ref"]);
  assert.equal(schemas.get("observe_ui").properties.root.type, "string");
  assert.equal(schemas.get("observe_ui").properties.root_ref, undefined);
});

test("DSH rejects GUI scripting through work_run_command", async () => {
  const result = await callTool("work_run_command", { command: "osascript", args: ["-e", "tell application Calculator"] }, "dsh-test-gui-fallback");
  assert.equal(result.isError, true);
  assert.match(result.content[0].text, /GUI automation commands are not allowed/);
  const probe = await callTool("work_run_command", { command: "which", args: ["cliclick"] }, "dsh-test-gui-probe");
  assert.equal(probe.isError, true);
  assert.match(probe.content[0].text, /GUI automation commands are not allowed/);
});

test("DSH Computer Use routes through the shared V2 session and legacy host backend", async () => {
  const calls = [];
  const previousFetch = globalThis.fetch;
  globalThis.fetch = async (_url, options = {}) => {
    if (!options.body) return new Response("Not found", { status: 404 });
    const request = JSON.parse(options.body);
    calls.push(request.toolName);
    const structuredContent = request.toolName === "desktop_open_app"
      ? { pid: 123, window_id: 7, root_ref: "ax:calculator", app_name: "Calculator", title: "Calculator" }
      : request.toolName === "desktop_list_apps"
        ? { windows: [{ pid: 123, window_id: 7, root_ref: "ax:calculator", app_name: "Calculator", title: "Calculator" }] }
        : request.toolName === "desktop_act_batch"
          ? { observation: { pid: 123, window_id: 7, root_ref: "ax:calculator", elements: [{ ref: "ax:result", role: "AXStaticText", title: "42", value: "42" }] }, verification: { outcome: "worked" } }
        : { pid: 123, window_id: 7, root_ref: "ax:calculator", elements: [{ ref: "ax:result", role: "AXStaticText", title: "42", value: "42" }] };
    return new Response(JSON.stringify({ success: true, structuredContent }), { status: 200 });
  };

  try {
    const result = await callTool("launch_app", { name: "Calculator" }, "dsh-test-launch");
    assert.equal(result.isError, false, JSON.stringify(result));
    const observed = await callTool("observe_ui", {}, "dsh-test-observe");
    assert.equal(observed.isError, false, JSON.stringify(observed));
    const acted = await callTool("act_ui", {
      stateId: result.structuredContent.stateId,
      actions: [{ action: "click", ref: result.structuredContent.elements[0].ref }],
    }, "dsh-test-act");
    assert.equal(acted.isError, false, JSON.stringify(acted));
    assert.deepEqual(calls, ["desktop_open_app", "desktop_list_apps", "desktop_observe", "desktop_observe", "desktop_act_batch"]);
    const fallback = await callTool("work_run_command", { command: "node", args: ["-e", "console.log(1)"] }, "dsh-test-fallback");
    assert.equal(fallback.isError, true);
    assert.match(fallback.content[0].text, /Computer Use is active/);
  } finally {
    globalThis.fetch = previousFetch;
  }
});
