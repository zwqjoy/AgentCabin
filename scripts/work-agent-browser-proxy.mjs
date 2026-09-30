#!/usr/bin/env node
// Work-only, pinned agent-browser transport. The host chooses the executable.
import process from "node:process";

const port = Number(process.env.AGENTCABIN_WORK_BRIDGE_PORT);
const token = process.env.AGENTCABIN_WORK_BRIDGE_TOKEN;
if (!Number.isInteger(port) || port < 1 || port > 65535 || !token) {
  process.stderr.write("Work browser host bridge is unavailable\n");
  process.exit(1);
}

const base = `http://127.0.0.1:${port}/internal/work`;
const headers = { authorization: `Bearer ${token}`, "content-type": "application/json" };
const chunks = [];
let size = 0;
for await (const chunk of process.stdin) {
  size += chunk.length;
  if (size > 1024 * 1024) {
    process.stderr.write("Browser input exceeds 1 MiB\n");
    process.exit(1);
  }
  chunks.push(chunk);
}

const env = Object.fromEntries(
  Object.entries(process.env).filter(([key]) =>
    ["AGENT_BROWSER_NAMESPACE", "AGENT_BROWSER_DEFAULT_TIMEOUT", "AGENT_BROWSER_IDLE_TIMEOUT_MS", "AGENT_BROWSER_CONFIG", "AGENT_BROWSER_USER_AGENT", "AGENT_BROWSER_RESTORE", "AGENT_BROWSER_AUTOSAVE_INTERVAL_MS"].includes(key),
  ),
);
const body = JSON.stringify({
  args: process.argv.slice(2),
  cwd: process.cwd(),
  stdin: Buffer.concat(chunks).toString("utf8"),
  env,
});

try {
  while (true) {
    const response = await fetch(`${base}/browser_host_command`, {
      method: "POST", headers, body,
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || `Browser host bridge HTTP ${response.status}`);
    if (result.status === "waiting_approval") {
      const id = encodeURIComponent(result.inboxItemId);
      const deadline = Date.now() + 300_000;
      while (true) {
        if (Date.now() >= deadline) throw new Error("Browser host approval timed out");
        await new Promise((resolve) => setTimeout(resolve, 500));
        const statusResponse = await fetch(`${base}/inbox_status/${id}`, { headers });
        if (!statusResponse.ok) throw new Error(`Browser approval status HTTP ${statusResponse.status}`);
        const item = await statusResponse.json();
        if (item.status === "approved") break;
        if (item.status !== "pending") throw new Error(`Browser host execution ${item.status}`);
      }
      continue;
    }
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    process.exitCode = result.exitCode ?? 1;
    break;
  }
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
}
