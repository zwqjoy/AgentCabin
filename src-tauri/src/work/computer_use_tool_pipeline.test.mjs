import assert from "node:assert/strict";
import test from "node:test";

import { createComputerUseToolPipelineInvoker } from "./computer_use_tool_pipeline.mjs";

test("Computer Use waits for Host Inbox approval and retries through ToolPipeline", async () => {
  const pipelineCalls = [];
  const approvalWaits = [];
  const invoke = createComputerUseToolPipelineInvoker({
    callToolPipeline: async (...args) => {
      pipelineCalls.push(args);
      if (pipelineCalls.length === 1) {
        return { status: "waiting_approval", interactionId: "inbox-cu-1" };
      }
      return { success: true, structuredContent: { windows: [{ pid: 42, window_id: 7 }] } };
    },
    waitForWorkInboxResolution: async (interactionId) => {
      approvalWaits.push(interactionId);
      return { status: "approved" };
    },
    callDesktopRuntime: async () => {
      assert.fail("Computer Use must not bypass ToolPipeline after approval");
    },
  });

  const action = { app: "Calculator" };
  const params = { app: "Calculator" };
  const result = await invoke("cu-call-1", "desktop_list_apps", action, params);

  assert.equal(result.success, true);
  assert.deepEqual(approvalWaits, ["inbox-cu-1"]);
  assert.equal(pipelineCalls.length, 2);
  assert.deepEqual(pipelineCalls[0], pipelineCalls[1]);
});

test("Computer Use rejection stops before retrying ToolPipeline", async () => {
  let pipelineCalls = 0;
  const invoke = createComputerUseToolPipelineInvoker({
    callToolPipeline: async () => {
      pipelineCalls++;
      return { status: "waiting_approval", interactionId: "inbox-cu-reject" };
    },
    waitForWorkInboxResolution: async () => ({ status: "rejected" }),
    callDesktopRuntime: async () => {
      assert.fail("Rejected Computer Use must not reach the native runtime");
    },
  });

  const result = await invoke("cu-call-reject", "desktop_list_apps", {}, {});

  assert.equal(result.success, false);
  assert.match(result.stderr, /rejected/);
  assert.equal(pipelineCalls, 1);
});
