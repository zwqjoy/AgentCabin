import test from "node:test";
import assert from "node:assert/strict";
import { registerComputerUseProxy } from "./pi_computer_use_proxy.mjs";

test("Proxy registers all upstream tool definitions from JSON", () => {
  const registered = new Map();
  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async () => ({ success: true, result: {} }),
  });

  const expectedTools = [
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
  ];

  for (const name of expectedTools) {
    assert.ok(registered.has(name), `Proxy must register tool ${name}`);
    const tool = registered.get(name);
    assert.equal(tool.name, name);
    assert.ok(tool.description.length > 0);
    assert.ok(tool.parameters);
    assert.equal(typeof tool.execute, "function");
  }
});

test("Proxy forwards tool call through ToolPipeline and returns structured result", async () => {
  const registered = new Map();
  const pipelineCalls = [];

  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async (toolCallId, toolName, action, params) => {
      pipelineCalls.push({ toolCallId, toolName, action, params });
      return {
        success: true,
        result: {
          content: [{ type: "text", text: "Found 1 root" }],
          details: { tool: "find_roots", windows: [{ rootRef: "@r1", pid: 100 }] },
        },
      };
    },
  });

  const findRoots = registered.get("find_roots");
  const result = await findRoots.execute("call-1", { text: "Notes" });

  assert.equal(pipelineCalls.length, 1);
  assert.equal(pipelineCalls[0].toolCallId, "call-1");
  assert.equal(pipelineCalls[0].toolName, "find_roots");
  assert.deepEqual(pipelineCalls[0].params, { text: "Notes" });
  assert.equal(result.content[0].text, "Found 1 root");
  assert.equal(result.details.windows[0].rootRef, "@r1");
});

test("Proxy throws when ToolPipeline returns failure", async () => {
  const registered = new Map();
  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async () => ({
      success: false,
      stderr: "State 's_old' is stale or was evicted.",
    }),
  });

  const actUi = registered.get("act_ui");
  await assert.rejects(
    () => actUi.execute("call-act-1", { stateId: "s_old", actions: [{ action: "press", ref: "@e1" }] }),
    /State 's_old' is stale/
  );
});

test("Proxy handles Work Inbox approval workflow for act_ui", async () => {
  const registered = new Map();
  let pipelineAttempts = 0;
  const approvals = [];

  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async (toolCallId, toolName, action, params) => {
      pipelineAttempts++;
      if (pipelineAttempts === 1) {
        return { status: "waiting_approval", interactionId: "inbox-item-42" };
      }
      return {
        success: true,
        result: {
          content: [{ type: "text", text: "Action executed" }],
          details: { tool: "act_ui", outcome: "worked", stateId: "s2" },
        },
      };
    },
    waitForWorkInboxResolution: async (interactionId) => {
      approvals.push(interactionId);
      return { status: "approved" };
    },
  });

  const actUi = registered.get("act_ui");
  const result = await actUi.execute("call-act-2", {
    stateId: "s1",
    actions: [{ action: "click", ref: "@e2" }],
  });

  assert.equal(pipelineAttempts, 2, "Must retry pipeline after approval");
  assert.deepEqual(approvals, ["inbox-item-42"]);
  assert.equal(result.details.outcome, "worked");
  assert.equal(result.details.stateId, "s2");
});

test("Proxy fails closed when Work Inbox approval is rejected", async () => {
  const registered = new Map();
  let pipelineAttempts = 0;

  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async () => {
      pipelineAttempts++;
      return { status: "waiting_approval", interactionId: "inbox-reject" };
    },
    waitForWorkInboxResolution: async () => ({ status: "rejected" }),
  });

  const actUi = registered.get("act_ui");
  await assert.rejects(
    () => actUi.execute("call-act-3", { stateId: "s1", actions: [{ action: "press", ref: "@e5" }] }),
    /rejected/
  );

  assert.equal(pipelineAttempts, 1, "Must never re-attempt execution after rejection");
});

test("Proxy releases desktop lease on session shutdown or agent end", async () => {
  const registered = new Map();
  const eventHandlers = new Map();
  const pipelineCalls = [];

  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: (event, handler) => {
      eventHandlers.set(event, handler);
    },
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async (toolCallId, toolName, action, params) => {
      pipelineCalls.push({ toolName, action });
      return { success: true, result: {} };
    },
  });

  // Before any desktop use, shutdown shouldn't invoke release
  const shutdown = eventHandlers.get("session_shutdown");
  assert.ok(shutdown, "Must register session_shutdown listener");
  await shutdown();
  assert.equal(pipelineCalls.length, 0);

  // Now use desktop
  const findRoots = registered.get("find_roots");
  await findRoots.execute("call-roots", {});
  assert.equal(pipelineCalls.length, 1);

  // Now agent_end triggers release
  const agentEnd = eventHandlers.get("agent_end");
  assert.ok(agentEnd, "Must register agent_end listener");
  await agentEnd();
  assert.equal(pipelineCalls.length, 2);
  assert.equal(pipelineCalls[1].toolName, "desktop_release");
});

test("Proxy registers launch_app compatibility tool and routes through ToolPipeline", async () => {
  const registered = new Map();
  const pipelineCalls = [];

  const fakePi = {
    registerTool: (tool) => {
      registered.set(tool.name, tool);
    },
    on: () => {},
  };

  registerComputerUseProxy(fakePi, {
    callToolPipeline: async (toolCallId, toolName, action, params) => {
      pipelineCalls.push({ toolCallId, toolName, action, params });
      return {
        success: true,
        result: {
          content: [{ type: "text", text: "App launched" }],
          details: { tool: "launch_app", pid: 12345 },
        },
      };
    },
  });

  assert.ok(registered.has("launch_app"), "launch_app must be registered");
  const launchApp = registered.get("launch_app");
  const result = await launchApp.execute("call-launch-1", { name: "Calculator" });

  assert.equal(pipelineCalls.length, 1);
  assert.equal(pipelineCalls[0].toolCallId, "call-launch-1");
  assert.equal(pipelineCalls[0].toolName, "launch_app");
  assert.deepEqual(pipelineCalls[0].params, { name: "Calculator" });
  assert.equal(result.details.pid, 12345);
});

