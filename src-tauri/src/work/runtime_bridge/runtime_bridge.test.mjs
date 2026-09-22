import assert from "node:assert/strict";
import test from "node:test";

import {
  buildWorkToolPipelinePayload,
  callWorkToolPipeline,
  createWorkBridgeClient,
} from "./bridge_client.mjs";
import {
  createDefaultActiveWorkTools,
  createWorkToolCatalog,
  createWorkToolDefinition,
} from "./work_tool_catalog.mjs";

test("Work tool catalog is independent from the selected runtime", () => {
  const base = createWorkToolCatalog({
    mcpEnabled: false,
    browserEnabled: false,
    browserUseEnabled: true,
    subagentChild: false,
  });
  const browserDisabled = createWorkToolCatalog({
    mcpEnabled: false,
    browserEnabled: false,
    browserUseEnabled: false,
    subagentChild: false,
  });
  const optional = createWorkToolCatalog({
    mcpEnabled: true,
    browserEnabled: true,
    browserUseEnabled: true,
    subagentChild: true,
  });

  assert.ok(base.some((tool) => tool.name === "work_read_file"));
  assert.ok(base.some((tool) => tool.name === "browser_snapshot"));
  const desktop = createWorkToolCatalog({
    mcpEnabled: false,
    browserEnabled: false,
    browserUseEnabled: false,
    desktopUseEnabled: true,
    subagentChild: false,
  });
  assert.ok(desktop.some((tool) => tool.name === "desktop_observe"));
  assert.ok(!browserDisabled.some((tool) => tool.name === "desktop_observe"));
  assert.ok(!browserDisabled.some((tool) => tool.name === "browser_snapshot"));
  assert.ok(!base.some((tool) => tool.name === "mcp"));
  assert.ok(optional.some((tool) => tool.name === "mcp"));
  assert.ok(optional.some((tool) => tool.name === "web_search"));
  assert.ok(!optional.some((tool) => tool.name === "work_delegate"));
  assert.ok(createDefaultActiveWorkTools({ subagentChild: false }).has("work_run_command"));
});

test("Work bridge promotes expected output obligations to the pipeline payload", () => {
  assert.deepEqual(
    buildWorkToolPipelinePayload("call-1", "work_run_command", "run", {
      command: "soffice",
      expected_outputs: ["output/report.xlsx"],
    }),
    {
      toolCallId: "call-1",
      toolName: "work_run_command",
      action: "run",
      arguments: {
        command: "soffice",
        expected_outputs: ["output/report.xlsx"],
      },
      expectedOutputs: ["output/report.xlsx"],
    },
  );
});

test("Fake registrar executes a catalog-defined Work bridge tool", async () => {
  const requests = [];
  const env = {
    AGENTCABIN_WORK_BRIDGE_PORT: "49321",
    AGENTCABIN_WORK_BRIDGE_TOKEN: "runtime-neutral-token",
  };
  const client = createWorkBridgeClient({
    env,
    fetchImpl: async (url, init) => {
      requests.push({ url, init });
      return {
        ok: true,
        status: 200,
        async json() {
          return { ok: true, status: "success", outputs: ["scratch/result.md"] };
        },
      };
    },
  });
  const catalogEntry = createWorkToolCatalog({
    mcpEnabled: false,
    browserEnabled: false,
    subagentChild: false,
  }).find((tool) => tool.name === "work_read_file");
  const registered = new Map();
  const fakeRegistrar = {
    registerTool(tool) {
      registered.set(tool.name, tool);
    },
  };

  fakeRegistrar.registerTool(
    createWorkToolDefinition(
      catalogEntry,
      (toolCallId, args, signal) =>
        callWorkToolPipeline(client, toolCallId, "work_read_file", "read", args, signal),
      {},
    ),
  );

  const response = await registered.get("work_read_file").execute("fake-call", { path: "input/a.md" });
  assert.equal(response.status, "success");
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "http://127.0.0.1:49321/internal/work/tool_pipeline");
  assert.equal(requests[0].init.headers.Authorization, "Bearer runtime-neutral-token");
  assert.deepEqual(JSON.parse(requests[0].init.body), {
    toolCallId: "fake-call",
    toolName: "work_read_file",
    action: "read",
    arguments: { path: "input/a.md" },
    expectedOutputs: [],
  });
});
