import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

const PI_MCP_SERVER_NAME = /^[A-Za-z0-9_-]{1,80}$/;

function piServerNames(hostNames, packageNames) {
  const names = new Map();
  const reserved = new Set(hostNames.filter((name) => !packageNames.has(name) && PI_MCP_SERVER_NAME.test(name)));
  const used = new Set(reserved);
  for (const hostName of hostNames) {
    if (!packageNames.has(hostName) && PI_MCP_SERVER_NAME.test(hostName)) {
      names.set(hostName, hostName);
      continue;
    }
    const slug = hostName.replace(/[^A-Za-z0-9_-]+/g, "_").replace(/^[_-]+|[_-]+$/g, "") || "server";
    for (let attempt = 0; ; attempt += 1) {
      const digest = createHash("sha256").update(`${hostName}\0${attempt}`).digest("hex");
      const suffix = digest.slice(0, 16);
      const alias = `ac_${slug.slice(0, 60)}_${suffix}`;
      if (!used.has(alias)) {
        names.set(hostName, alias);
        used.add(alias);
        break;
      }
    }
  }
  return names;
}

function configuredServers(raw) {
  const servers = raw?.mcpServers ?? raw?.mcp_servers;
  return servers && typeof servers === "object" && !Array.isArray(servers) ? servers : {};
}

/** Only Host-owned loopback transports may reach the Pi MCP client. */
export function buildWorkNativeMcpConfig(raw, packages, bridge) {
  const url = new URL(bridge?.baseUrl);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port ||
      url.username || url.password || url.pathname !== "/" || url.search || url.hash ||
      typeof bridge.token !== "string" || !bridge.token || bridge.token.length > 512 ||
      /[\u0000-\u001f\u007f]/.test(bridge.token)) {
    throw new Error("Work MCP requires an authenticated loopback Host bridge");
  }
  const entries = new Map([...Object.entries(configuredServers(raw)), ...Object.entries(configuredServers(packages))]);
  const projectedNames = piServerNames([...entries.keys()], new Set(Object.keys(configuredServers(packages))));
  return {
    servers: [...entries].map(([name, server]) => {
      if (!name || /[\u0000-\u001f\u007f]/.test(name) || name.length > 200 ||
          !server || typeof server !== "object" || Array.isArray(server)) {
        throw new Error("Invalid Work MCP server metadata");
      }
      // Free-form connector descriptions can embed credentials or endpoints.
      // Use a bounded Host-owned label until metadata has an explicit safe-text contract.
      return {
        name: projectedNames.get(name), source: "agentcabin-work-host", scope: "extension",
        config: {
          type: "http", url: `${url.origin}/internal/work/mcp/${encodeURIComponent(name)}`,
          headers: { Authorization: `Bearer ${bridge.token}` },
          enabled: server.disabled !== true && server.enabled !== false,
          description: `AgentCabin connector: ${name}`,
          exposure: "deferred", timeout: 600,
        },
      };
    }),
    errors: [], autoEnableCodemode: false,
  };
}

export default async function workNativeMcp(pi) {
  const profile = path.resolve(process.env.AGENTCABIN_WORK_PROFILE_DIR || "");
  if (!process.env.AGENTCABIN_WORK_PROFILE_DIR) throw new Error("Work MCP requires an isolated profile");
  const read = (env, filename, optional = false) => {
    const file = path.resolve(process.env[env] || path.join(profile, filename));
    if (file !== path.join(profile, filename)) throw new Error("Work MCP config must remain inside its profile");
    if (optional && !fs.existsSync(file)) return {};
    const raw = JSON.parse(fs.readFileSync(file, "utf8"));
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("Invalid Work MCP config");
    return raw;
  };
  const config = buildWorkNativeMcpConfig(
    read("AGENTCABIN_WORK_MCP_CONFIG", "mcp.json"),
    read("AGENTCABIN_WORK_PACKAGE_MCP_CONFIG", "connector-package-mcp.json", true),
    { baseUrl: `http://127.0.0.1:${process.env.AGENTCABIN_WORK_BRIDGE_PORT}`, token: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN },
  );
  // The runtime locator points at the same bundled Host that launches Work.
  // Never install a second Pi to resolve this external extension's imports.
  const entry = process.env.AGENTCABIN_PI_CODING_AGENT_ENTRY;
  if (!entry || !path.isAbsolute(entry)) throw new Error("Missing bundled Pi native API entry");
  const { createMcpExtension, createToolSearchExtension } = await import(pathToFileURL(entry).href);
  createToolSearchExtension()(pi);
  return createMcpExtension({
    loadConfig: () => config,
    updateConfig: () => { throw new Error("Host-managed MCP config cannot be modified by Pi"); },
  })(pi);
}
