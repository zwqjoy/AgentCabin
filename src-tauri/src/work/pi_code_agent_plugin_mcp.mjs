import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

function aliases(names) {
  const used = new Set();
  return new Map(names.map((name) => {
    const slug = name.replace(/[^A-Za-z0-9_-]+/g, "_").replace(/^[_-]+|[_-]+$/g, "") || "server";
    for (let attempt = 0; ; attempt += 1) {
      const suffix = createHash("sha256").update(`${name}\0${attempt}`).digest("hex").slice(0, 16);
      const alias = `ac_${slug.slice(0, 60)}_${suffix}`;
      if (!used.has(alias)) { used.add(alias); return [name, alias]; }
    }
  }));
}

export function buildCodePluginMcpConfig(raw, bridge) {
  const url = new URL(bridge?.baseUrl);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port ||
      url.username || url.password || url.pathname !== "/" || url.search || url.hash ||
      typeof bridge.token !== "string" || !bridge.token || bridge.token.length > 512 ||
      /[\u0000-\u001f\u007f]/.test(bridge.token)) {
    throw new Error("Code Agent Plugin MCP requires an authenticated loopback Host bridge");
  }
  const servers = raw?.mcpServers;
  if (!servers || typeof servers !== "object" || Array.isArray(servers)) {
    return { servers: [], errors: [], autoEnableCodemode: false };
  }
  const names = Object.keys(servers);
  const projected = aliases(names);
  return {
    servers: names.map((name) => {
      if (!name.startsWith("agent-plugin--") || name.length > 180 || /[\u0000-\u001f\u007f]/.test(name)) {
        throw new Error("Invalid Agent Plugin MCP metadata");
      }
      return {
        name: projected.get(name), source: "agentcabin-code-host", scope: "extension",
        config: {
          type: "http", url: `${url.origin}/internal/code/mcp/${encodeURIComponent(name)}`,
          headers: { Authorization: `Bearer ${bridge.token}` }, enabled: true,
          description: `AgentCabin Plugin MCP: ${name}`, exposure: "deferred", timeout: 600,
        },
      };
    }),
    errors: [], autoEnableCodemode: false,
  };
}

export default async function codeAgentPluginMcp(pi) {
  const profile = path.resolve(process.env.PI_CODING_AGENT_DIR || "");
  if (!process.env.PI_CODING_AGENT_DIR) throw new Error("Code Plugin MCP requires a managed profile");
  const file = path.join(profile, "agent-plugin-mcp.json");
  const raw = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : { mcpServers: {} };
  const config = buildCodePluginMcpConfig(raw, {
    baseUrl: `http://127.0.0.1:${process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_PORT}`,
    token: process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN,
  });
  const entry = process.env.AGENTCABIN_PI_CODING_AGENT_ENTRY;
  if (!entry || !path.isAbsolute(entry)) throw new Error("Missing bundled Pi native API entry");
  const { createMcpExtension, createToolSearchExtension } = await import(pathToFileURL(entry).href);
  createToolSearchExtension()(pi);
  return createMcpExtension({
    loadConfig: () => config,
    updateConfig: () => { throw new Error("Host-managed Agent Plugin MCP config is read-only"); },
  })(pi);
}
