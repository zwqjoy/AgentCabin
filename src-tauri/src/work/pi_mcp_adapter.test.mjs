import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

test("Work MCP adapter exposes only authenticated Host bridge definitions", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-mcp-adapter-"));
  const profile = path.join(root, "profile");
  fs.mkdirSync(profile, { recursive: true });
  const configPath = path.join(profile, "mcp.json");
  const packageConfigPath = path.join(profile, "connector-package-mcp.json");
  const secretsPath = path.join(profile, "mcp-secrets.json");
  const adapterPath = path.join(root, "agentcabin-work-mcp-adapter.mjs");
  const permissionsPath = path.join(root, "agentcabin-work-mcp-permissions.mjs");
  const fakeAdapterPath = path.join(root, "fake-mcp-adapter.mjs");
  fs.copyFileSync(path.resolve("src-tauri/src/work/pi_mcp_adapter.mjs"), adapterPath);
  fs.copyFileSync(path.resolve("src-tauri/src/work/pi_mcp_permissions.mjs"), permissionsPath);
  fs.writeFileSync(configPath, JSON.stringify({
    settings: { directTools: true, scriptMode: true, approveTools: true, agentPluginPaths: ["/unsafe"] },
    mcpServers: {
      docs: {
        command: "node",
        args: ["server.mjs"],
        env: { API_KEY: "__AGENTCABIN_WORK_SECRET__" },
        headers: { "X-Api-Key": "__AGENTCABIN_WORK_SECRET__" },
      },
    },
  }));
  fs.writeFileSync(packageConfigPath, JSON.stringify({
    mcpServers: {
      package_docs: { url: "https://connector.example.test/mcp", auth: "oauth" },
    },
  }));
  fs.writeFileSync(secretsPath, JSON.stringify({ connectors: { docs: { env: { API_KEY: "must-not-enter-pi" } } } }));
  fs.writeFileSync(fakeAdapterPath, `
    export function createMcpAdapter(options = {}) {
      globalThis.__agentcabinWorkMcpTestOptions = options;
      return (pi) => pi.registerTool({ name: "mcp", async execute() { return { content: [] }; } });
    }
  `);

  const envNames = [
    "AGENTCABIN_WORK_PROFILE_DIR",
    "AGENTCABIN_WORK_MCP_CONFIG",
    "AGENTCABIN_WORK_MCP_SECRETS",
    "AGENTCABIN_WORK_PACKAGE_MCP_CONFIG",
    "AGENTCABIN_WORK_BRIDGE_PORT",
    "AGENTCABIN_WORK_BRIDGE_TOKEN",
    "AGENTCABIN_PI_SYSTEM_MCP_ADAPTER_ENTRY",
    "MCP_DIRECT_TOOLS",
    "MCP_OAUTH_DIR",
  ];
  const previous = Object.fromEntries(envNames.map((name) => [name, process.env[name]]));
  try {
    process.env.AGENTCABIN_WORK_PROFILE_DIR = profile;
    process.env.AGENTCABIN_WORK_MCP_CONFIG = configPath;
    process.env.AGENTCABIN_WORK_MCP_SECRETS = secretsPath;
    process.env.AGENTCABIN_WORK_PACKAGE_MCP_CONFIG = packageConfigPath;
    process.env.AGENTCABIN_WORK_BRIDGE_PORT = "49321";
    process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "host-bridge-token";
    process.env.AGENTCABIN_PI_SYSTEM_MCP_ADAPTER_ENTRY = fakeAdapterPath;

    const moduleUrl = `${pathToFileURL(adapterPath).href}?test=${Date.now()}`;
    const adapterModule = await import(moduleUrl);
    const registeredTools = [];
    const pi = {
      on() {},
      events: { on() {} },
      registerTool(tool) { registeredTools.push(tool); },
    };
    adapterModule.default(pi);

    const options = globalThis.__agentcabinWorkMcpTestOptions;
    assert.ok(options?.config);
    assert.deepEqual(Object.keys(options.config.mcpServers).sort(), ["docs", "package_docs"]);
    for (const server of Object.values(options.config.mcpServers)) {
      assert.equal(server.type, "streamable-http");
      assert.equal(server.httpTransport, "streamable-http");
      assert.equal(server.auth, false);
      assert.equal(server.oauth, false);
      assert.equal(server.directTools, false);
      assert.equal(server.approveTools, false);
      assert.equal(server.exposeResources, false);
      assert.equal(server.requestTimeoutMs, 600000);
      assert.equal("command" in server, false);
      assert.equal("args" in server, false);
      assert.equal("env" in server, false);
      assert.deepEqual(server.headers, { Authorization: "Bearer host-bridge-token" });
      assert.match(server.url, /^http:\/\/127\.0\.0\.1:49321\/internal\/work\/mcp\//);
    }
    assert.equal(options.config.settings.directTools, false);
    assert.equal(options.config.settings.scriptMode, false);
    assert.equal(options.config.settings.approveTools, false);
    assert.deepEqual(options.config.settings.agentPluginPaths, []);
    assert.equal(process.env.MCP_DIRECT_TOOLS, "__none__");
    assert.equal(JSON.stringify(options).includes("must-not-enter-pi"), false);
    assert.equal(registeredTools.some((tool) => tool.name === "mcp"), true);
  } finally {
    for (const name of envNames) {
      if (previous[name] === undefined) delete process.env[name];
      else process.env[name] = previous[name];
    }
    delete globalThis.__agentcabinWorkMcpTestOptions;
    fs.rmSync(root, { recursive: true, force: true });
  }
});
