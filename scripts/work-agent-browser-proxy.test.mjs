import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./work-agent-browser-proxy.mjs", import.meta.url));

test("Work browser proxy carries arguments, stdin and the bearer token", async () => {
  let request;
  const server = createServer(async (req, res) => {
    let body = "";
    for await (const chunk of req) body += chunk;
    request = { url: req.url, authorization: req.headers.authorization, body: JSON.parse(body) };
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify({ status: "completed", exitCode: 0, stdout: "browser output", stderr: "" }));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    const child = spawn(process.execPath, [script, "batch", "--session", "test"], {
      env: {
        ...process.env,
        AGENTCABIN_WORK_BRIDGE_PORT: String(server.address().port),
        AGENTCABIN_WORK_BRIDGE_TOKEN: "test-token",
        AGENT_BROWSER_NAMESPACE: "test-namespace",
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    child.stdin.end('[{"command":"snapshot"}]');
    let stdout = "";
    let stderr = "";
    for await (const chunk of child.stdout) stdout += chunk;
    for await (const chunk of child.stderr) stderr += chunk;
    const [code] = await once(child, "close");
    assert.equal(code, 0, stderr);
    assert.equal(stdout, "browser output");
    assert.equal(request.url, "/internal/work/browser_host_command");
    assert.equal(request.authorization, "Bearer test-token");
    assert.deepEqual(request.body.args, ["batch", "--session", "test"]);
    assert.equal(request.body.stdin, '[{"command":"snapshot"}]');
    assert.equal(request.body.env.AGENT_BROWSER_NAMESPACE, "test-namespace");
  } finally {
    server.close();
  }
});

test("Work browser proxy retries the same command after approval", async () => {
  let attempts = 0;
  const server = createServer((req, res) => {
    res.setHeader("content-type", "application/json");
    if (req.url === "/internal/work/browser_host_command") {
      attempts += 1;
      res.end(JSON.stringify(attempts === 1
        ? { status: "waiting_approval", inboxItemId: "test-interaction" }
        : { status: "completed", exitCode: 0, stdout: "approved", stderr: "" }));
    } else {
      assert.equal(req.url, "/internal/work/inbox_status/test-interaction");
      res.end(JSON.stringify({ status: "approved" }));
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    const child = spawn(process.execPath, [script, "snapshot"], {
      env: {
        ...process.env,
        AGENTCABIN_WORK_BRIDGE_PORT: String(server.address().port),
        AGENTCABIN_WORK_BRIDGE_TOKEN: "test-token",
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    child.stdin.end();
    let stdout = "";
    for await (const chunk of child.stdout) stdout += chunk;
    const [code] = await once(child, "close");
    assert.equal(code, 0);
    assert.equal(stdout, "approved");
    assert.equal(attempts, 2);
  } finally {
    server.close();
  }
});
