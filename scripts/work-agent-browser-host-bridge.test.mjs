import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import test from "node:test";
import { runAgentBrowserProcess } from "../src-tauri/runtime/extensions/node_modules/pi-agent-browser-native/dist/extensions/agent-browser/lib/process.js";
import {
  isWorkBrowserHostProxyEnabled,
  runWorkBrowserHostCommand,
} from "./work-agent-browser-host-bridge.mjs";

async function withServer(handler, run) {
  const server = createServer(handler);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    await run(server.address().port);
  } finally {
    server.close();
  }
}

test("Work native Browser extension transports commands directly through the authenticated host bridge", async () => {
  let received;
  await withServer(
    async (req, res) => {
      let body = "";
      for await (const chunk of req) body += chunk;
      received = { url: req.url, authorization: req.headers.authorization, body: JSON.parse(body) };
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify({ exitCode: 0, stdout: "structured browser result", stderr: "" }));
    },
    async (port) => {
      const result = await runWorkBrowserHostCommand({
        args: ["open", "https://example.com"],
        cwd: "/tmp/work",
        env: {
          AGENTCABIN_WORK_BRIDGE_PORT: String(port),
          AGENTCABIN_WORK_BRIDGE_TOKEN: "test-token",
          AGENTCABIN_WORK_BROWSER_HOST_PROXY: "1",
          AGENT_BROWSER_NAMESPACE: "work-session",
          SECRET_SHOULD_NOT_FORWARD: "no",
        },
        stdin: "input",
      });

      assert.equal(result.exitCode, 0);
      assert.equal(result.stdout, "structured browser result");
      assert.equal(result.agentBrowserStarted, true);
      assert.equal(received.url, "/internal/work/browser_host_command");
      assert.equal(received.authorization, "Bearer test-token");
      assert.deepEqual(received.body, {
        args: ["open", "https://example.com"],
        cwd: "/tmp/work",
        stdin: "input",
        env: { AGENT_BROWSER_NAMESPACE: "work-session" },
      });
    },
  );
});

test("Work native Browser host transport waits for Inbox approval and retries once approved", async () => {
  let attempts = 0;
  await withServer(
    (req, res) => {
      res.setHeader("content-type", "application/json");
      if (req.url === "/internal/work/browser_host_command") {
        attempts += 1;
        res.end(
          JSON.stringify(
            attempts === 1
              ? { status: "waiting_approval", inboxItemId: "interaction-1" }
              : { status: "completed", exitCode: 0, stdout: "approved", stderr: "" },
          ),
        );
        return;
      }
      assert.equal(req.url, "/internal/work/inbox_status/interaction-1");
      res.end(JSON.stringify({ status: "approved" }));
    },
    async (port) => {
      const result = await runWorkBrowserHostCommand(
        {
          args: ["open", "https://example.com"],
          cwd: "/tmp/work",
          env: {
            AGENTCABIN_WORK_BRIDGE_PORT: String(port),
            AGENTCABIN_WORK_BRIDGE_TOKEN: "test-token",
          },
          timeoutMs: 5_000,
        },
        { wait: async () => {} },
      );

      assert.equal(result.exitCode, 0);
      assert.equal(result.stdout, "approved");
      assert.equal(attempts, 2);
    },
  );
});

test("Work native Browser host transport is enabled only for a Work-marked process", () => {
  assert.equal(isWorkBrowserHostProxyEnabled({ AGENTCABIN_WORK_BROWSER_HOST_PROXY: "1" }), true);
  assert.equal(isWorkBrowserHostProxyEnabled({}), false);
});

test("prepared native Browser process routes Work commands through the host bridge without spawning", async () => {
  const previousEnv = Object.fromEntries(
    [
      "AGENTCABIN_WORK_BROWSER_HOST_PROXY",
      "AGENTCABIN_WORK_BRIDGE_PORT",
      "AGENTCABIN_WORK_BRIDGE_TOKEN",
    ].map((name) => [name, process.env[name]]),
  );
  const originalFetch = globalThis.fetch;
  const requests = [];
  process.env.AGENTCABIN_WORK_BROWSER_HOST_PROXY = "1";
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "43210";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "integration-token";
  globalThis.fetch = async (url, init) => {
    requests.push({
      url: String(url),
      authorization: init.headers.authorization,
      body: JSON.parse(init.body),
    });
    return new Response(
      JSON.stringify({ status: "completed", exitCode: 0, stdout: "bridge reached", stderr: "" }),
      {
        status: 200,
        headers: { "content-type": "application/json" },
      },
    );
  };
  try {
    const result = await runAgentBrowserProcess({
      args: ["open", "https://example.com"],
      cwd: process.cwd(),
      env: {},
    });
    assert.equal(result.exitCode, 0);
    assert.equal(result.stdout, "bridge reached");
    assert.equal(requests.length, 1);
    assert.equal(requests[0].url, "http://127.0.0.1:43210/internal/work/browser_host_command");
    assert.equal(requests[0].authorization, "Bearer integration-token");
    assert.deepEqual(requests[0].body.args, ["open", "https://example.com"]);
  } finally {
    globalThis.fetch = originalFetch;
    for (const [name, value] of Object.entries(previousEnv)) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  }
});
