import test from "node:test";
import assert from "node:assert/strict";
import { buildCodePluginMcpConfig } from "./pi_code_agent_plugin_mcp.mjs";

test("Code Plugin MCP exposes only bounded aliases and authenticated Host routes", () => {
  const ids = ["agent-plugin--github--mcp--github", "agent-plugin--foo.bar--mcp--service", "agent-plugin--foo-bar--mcp--service"];
  const secrets = Object.fromEntries(ids.map((id) => [id, {
    command: "SECRET_COMMAND", env: { TOKEN: "SECRET_ENV" },
    headers: { Authorization: "Bearer SECRET_HEADER" }, url: "https://secret.invalid",
  }]));
  const bridge = { baseUrl: "http://127.0.0.1:43210", token: "session-token" };
  const config = buildCodePluginMcpConfig({ mcpServers: secrets }, bridge);
  assert.deepEqual(config, buildCodePluginMcpConfig({ mcpServers: secrets }, bridge));
  assert.equal(new Set(config.servers.map((server) => server.name)).size, ids.length);
  assert.equal(JSON.stringify(config).includes("SECRET"), false);
  for (const [index, server] of config.servers.entries()) {
    assert.match(server.name, /^[A-Za-z0-9_-]{1,80}$/);
    assert.equal(server.config.url, `http://127.0.0.1:43210/internal/code/mcp/${encodeURIComponent(ids[index])}`);
    assert.equal(server.config.headers.Authorization, "Bearer session-token");
  }
});

test("Code Plugin MCP rejects non-loopback bridge and non-plugin entries", () => {
  assert.throws(() => buildCodePluginMcpConfig({ mcpServers: {} }, {
    baseUrl: "https://example.com", token: "token",
  }), /authenticated loopback/);
  assert.throws(() => buildCodePluginMcpConfig({ mcpServers: { github: {} } }, {
    baseUrl: "http://127.0.0.1:43210", token: "token",
  }), /Invalid Agent Plugin/);
});
