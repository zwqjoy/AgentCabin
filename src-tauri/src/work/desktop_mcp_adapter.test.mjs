import assert from "node:assert/strict";
import { createInterface } from "node:readline";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const adapterPath = fileURLToPath(new URL("./desktop_mcp_adapter.mjs", import.meta.url));

test("desktop MCP adapter advertises the Computer Use V2 contract", async () => {
  const child = spawn(process.execPath, [adapterPath], {
    stdio: ["pipe", "pipe", "ignore"],
    env: { PATH: process.env.PATH || "" },
  });
  const lines = createInterface({ input: child.stdout });
  const next = () => new Promise((resolve, reject) => {
    const onLine = (line) => {
      cleanup();
      resolve(JSON.parse(line));
    };
    const onError = (error) => {
      cleanup();
      reject(error);
    };
    const cleanup = () => {
      lines.off("line", onLine);
      child.off("error", onError);
    };
    lines.on("line", onLine);
    child.once("error", onError);
  });

  try {
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })}\n`);
    const initialize = await next();
    assert.equal(initialize.result.serverInfo.name, "AgentCabin Computer Use");

    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} })}\n`);
    const listed = await next();
    const names = listed.result.tools.map((tool) => tool.name);
    assert.deepEqual(names, ["launch_app", "find_roots", "observe_ui", "search_ui", "expand_ui", "inspect_ui", "act_ui", "read_text", "wait_for"]);

    child.stdin.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: { name: "observe_ui", arguments: {} },
    })}\n`);
    const unavailable = await next();
    assert.equal(unavailable.result.isError, true);
  } finally {
    lines.close();
    child.kill();
  }
});
