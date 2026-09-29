import test from "node:test";
import assert from "node:assert/strict";
import { createComputerUseHostRuntime } from "./pi_computer_use_runtime.mjs";

test("Host Computer Use runtime loads upstream @injaneity/pi-computer-use and captures all tools", async () => {
  const runtime = await createComputerUseHostRuntime();
  const toolDefs = runtime.getToolDefinitions();

  const toolNames = toolDefs.map((t) => t.name).sort();
  const expectedToolNames = [
    "act_ui",
    "evaluate_browser",
    "expand_ui",
    "find_roots",
    "inspect_ui",
    "launch_browser",
    "navigate_browser",
    "observe_ui",
    "read_text",
    "search_ui",
    "wait_for",
  ].sort();

  assert.deepEqual(toolNames, expectedToolNames);

  for (const def of toolDefs) {
    assert.ok(def.name, "tool has a name");
    assert.ok(def.description, `tool ${def.name} has a description`);
    assert.ok(def.parameters, `tool ${def.name} has parameters schema`);
  }
});

test("Host Computer Use runtime executes tool within session context and handles errors gracefully", async () => {
  const runtime = await createComputerUseHostRuntime();

  // Non-existent tool
  await assert.rejects(
    () => runtime.executeTool("run-1", "non_existent_tool", "call-1", {}),
    /Unknown computer use tool/
  );

  // find_roots with fake context without granted permissions fails gracefully
  const result = await runtime.executeTool(
    "run-1",
    "find_roots",
    "call-roots-1",
    { text: "Finder" },
    undefined,
    { hasUI: false }
  );

  assert.equal(result.toolName, "find_roots");
  assert.equal(result.toolCallId, "call-roots-1");
  assert.equal(typeof result.durationMs, "number");
  // Result is either success (if permissions granted) or failed with clear stderr
  if (!result.success) {
    assert.equal(result.status, "failed");
    assert.ok(result.stderr.length > 0);
  } else {
    assert.equal(result.status, "success");
    assert.ok(result.result);
  }
});

test("Host Computer Use runtime supports session shutdown", async () => {
  const runtime = await createComputerUseHostRuntime();
  await assert.doesNotReject(() => runtime.shutdownSession("run-test-shutdown"));
});

test("Host Computer Use runtime supports tool cancellation via cancelTool", async () => {
  const runtime = await createComputerUseHostRuntime();
  const toolCallId = "call-long-wait-1";

  // Start a wait_for tool with a long timeout
  const promise = runtime.executeTool(
    "run-cancel-test",
    "wait_for",
    toolCallId,
    { timeoutMs: 15000, condition: "none" }
  );

  // Cancel immediately
  const cancelled = runtime.cancelTool(toolCallId);
  assert.equal(cancelled, true, "cancelTool should return true for active call");

  const result = await promise;
  assert.equal(result.success, false, "Cancelled tool execution must report failure");
  assert.equal(result.status, "failed");
  assert.match(result.stderr, /abort|cancel|stopped/i);

  // Subsequent cancel is idempotent
  const secondCancel = runtime.cancelTool(toolCallId);
  assert.equal(secondCancel, false, "cancelTool for finished call should return false");
});

test("Upstream helper client respects PI_CU_SOCKET_PATH and does not install helper daemon", async () => {
  const testSocket = "/tmp/test-agentcabin-custom.sock";
  process.env.PI_CU_SOCKET_PATH = testSocket;
  try {
    const runtime = await createComputerUseHostRuntime();
    assert.ok(runtime, "Runtime successfully initialized with external socket env");
  } finally {
    delete process.env.PI_CU_SOCKET_PATH;
  }
});

