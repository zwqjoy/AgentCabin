import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { guardWorkMcpAuthTool } from "./agentcabin-work-mcp-permissions.mjs";

const profileDir = process.env.AGENTCABIN_WORK_PROFILE_DIR?.trim();
const configPath = process.env.AGENTCABIN_WORK_MCP_CONFIG?.trim();
const secretsPath = process.env.AGENTCABIN_WORK_MCP_SECRETS?.trim();
const packageConfigPath = process.env.AGENTCABIN_WORK_PACKAGE_MCP_CONFIG?.trim();
const bridgePort = Number.parseInt(String(process.env.AGENTCABIN_WORK_BRIDGE_PORT || ""), 10);
const bridgeToken = String(process.env.AGENTCABIN_WORK_BRIDGE_TOKEN || "").trim();
const MCP_STATUS_EVENT = "pi-mcp-adapter/status/v1";
const STATUS_WIDGET_KEY = "agentcabin-work-mcp";

if (!profileDir || !configPath) {
  throw new Error("AgentCabin Work MCP adapter 需要独立的 Profile 和配置路径");
}

const resolvedProfileDir = path.resolve(profileDir);
const resolvedConfigPath = path.resolve(configPath);
if (resolvedConfigPath !== path.join(resolvedProfileDir, "mcp.json")) {
  throw new Error("Work MCP adapter 只能读取 Work Profile 内的 mcp.json");
}
const resolvedPackageConfigPath = packageConfigPath ? path.resolve(packageConfigPath) : null;
if (
  resolvedPackageConfigPath &&
  resolvedPackageConfigPath !== path.join(resolvedProfileDir, "connector-package-mcp.json")
) {
  throw new Error("Work MCP adapter 只能读取 Work Profile 内的 Connector Package MCP config");
}
if (!Number.isInteger(bridgePort) || bridgePort < 1 || bridgePort > 65535 || !bridgeToken) {
  throw new Error("AgentCabin Work MCP adapter 需要已认证的 Host bridge");
}
if (bridgeToken.length > 512 || /[\u0000-\u001f\u007f]/.test(bridgeToken)) {
  throw new Error("AgentCabin Work MCP bridge token 无效");
}
process.env.MCP_OAUTH_DIR = path.join(resolvedProfileDir, "mcp-oauth");

function readConfig(filePath, label, optional = false) {
  if (optional && !fs.existsSync(filePath)) return {};
  const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`${label} 必须是 JSON 对象`);
  }
  return parsed;
}

const rawConfig = readConfig(resolvedConfigPath, "AgentCabin Work MCP 配置");
const rawPackageConfig = resolvedPackageConfigPath
  ? readConfig(resolvedPackageConfigPath, "Connector Package MCP config", true)
  : {};

function configuredServers(raw) {
  const servers = raw.mcpServers ?? raw.mcp_servers;
  return servers && typeof servers === "object" && !Array.isArray(servers) ? servers : {};
}

function copyStringArray(server, field) {
  const values = server?.[field];
  if (!Array.isArray(values)) return undefined;
  const strings = values.filter((value) => typeof value === "string").map((value) => value.trim()).filter(Boolean);
  return strings.length > 0 ? strings : undefined;
}

function copySafeServerOptions(server) {
  const result = {};
  if (!server || typeof server !== "object" || Array.isArray(server)) {
    throw new Error("Work MCP server 配置必须是 JSON 对象");
  }
  if (server.disabled === true) result.disabled = true;
  if (["keep-alive", "lazy", "lazy-keep-alive", "eager"].includes(server.lifecycle)) {
    result.lifecycle = server.lifecycle;
  }
  if (Number.isFinite(server.idleTimeout) && server.idleTimeout >= 0) {
    result.idleTimeout = server.idleTimeout;
  }
  for (const field of ["includeTools", "excludeTools"]) {
    const values = copyStringArray(server, field);
    if (values) result[field] = values;
  }
  if (["server", "none", "short", "mcp"].includes(server.toolPrefix)) {
    result.toolPrefix = server.toolPrefix;
  }
  return result;
}

function buildHostBridgeServer(name, server, bridge) {
  const serverOptions = copySafeServerOptions(server);
  return {
    ...serverOptions,
    type: "streamable-http",
    httpTransport: "streamable-http",
    protocolVersion: "legacy",
    url: `${bridge.baseUrl}/internal/work/mcp/${encodeURIComponent(name)}`,
    headers: { Authorization: `Bearer ${bridge.token}` },
    auth: false,
    oauth: false,
    exposeResources: false,
    directTools: false,
    approveTools: false,
    requestTimeoutMs: 600000,
  };
}

/**
 * Build the only MCP configuration visible to Work Pi. External connector
 * URLs, commands, env values, and credentials are deliberately replaced by
 * an authenticated loopback Host bridge; Rust then applies Policy/Inbox and
 * performs the actual connector operation.
 */
export function buildWorkMcpConfig(rawConfig, rawPackageConfig, bridge) {
  if (!bridge || typeof bridge.baseUrl !== "string" || typeof bridge.token !== "string") {
    throw new Error("Work MCP Host bridge configuration is invalid");
  }
  const entries = [
    ...Object.entries(configuredServers(rawConfig)),
    ...Object.entries(configuredServers(rawPackageConfig)),
  ];
  const servers = Object.fromEntries(
    entries.map(([name, server]) => [name, buildHostBridgeServer(name, server, bridge)]),
  );
  const rawSettings = rawConfig?.settings && typeof rawConfig.settings === "object" && !Array.isArray(rawConfig.settings)
    ? rawConfig.settings
    : {};
  const settings = {};
  for (const field of ["toolPrefix", "showStatusIcon", "mcpFooterStatus", "idleTimeout", "outputGuard", "authRequiredMessage"]) {
    if (rawSettings[field] !== undefined) settings[field] = rawSettings[field];
  }
  Object.assign(settings, {
    hostConfigDiscovery: "off",
    agentPluginPaths: [],
    directTools: false,
    scriptMode: false,
    approveTools: false,
    disableProxyTool: false,
    autoAuth: false,
    sampling: false,
    elicitation: false,
    requestTimeoutMs: 600000,
  });
  return { settings, mcpServers: servers };
}

const config = buildWorkMcpConfig(rawConfig, rawPackageConfig, {
  baseUrl: `http://127.0.0.1:${bridgePort}`,
  token: bridgeToken,
});
const resolvedSecretsPath = path.resolve(secretsPath || path.join(resolvedProfileDir, "mcp-secrets.json"));
if (resolvedSecretsPath !== path.join(resolvedProfileDir, "mcp-secrets.json")) {
  throw new Error("Work MCP adapter 只能读取 Work Profile 内的 mcp-secrets.json");
}
// Keep validating the expected secret-store location for compatibility with
// the launch contract, but never read it in Pi. Rust Host resolves credentials
// only for the connector operation that passed through ToolPipeline.
void resolvedSecretsPath;
process.env.MCP_DIRECT_TOOLS = "__none__";
const adapterEntry = path.resolve(
  process.env.AGENTCABIN_PI_SYSTEM_MCP_ADAPTER_ENTRY?.trim() ||
    path.join(resolvedProfileDir, "npm", "node_modules", "pi-mcp-adapter", "index.ts"),
);
if (!fs.existsSync(adapterEntry)) {
  throw new Error(`AgentCabin Pi MCP system adapter 未安装: ${adapterEntry}`);
}
const adapterModule = await import(pathToFileURL(adapterEntry).href);
const createMcpAdapter = adapterModule.createMcpAdapter ?? adapterModule.default;

if (typeof createMcpAdapter !== "function") {
  throw new Error("Work Pi MCP adapter 未导出 createMcpAdapter");
}

const statusLabels = {
  connected: "已连接",
  cached: "有缓存",
  failed: "失败",
  "needs-auth": "需要认证",
  "not-connected": "未连接",
  disabled: "已停用",
};

function renderStatus(snapshot) {
  if (!snapshot || !Array.isArray(snapshot.servers)) return undefined;
  const servers = snapshot.servers;
  if (servers.length === 0) return ["MCP：没有启用的连接器"];
  return [
    `MCP：${snapshot.connectedCount ?? 0}/${servers.length} 已连接 · ${snapshot.totalTools ?? 0} 个工具`,
    ...servers.map((server) => {
      const status = statusLabels[server.status] ?? server.status ?? "未知";
      return `${status} · ${server.name} · ${server.toolCount ?? 0} 个工具`;
    }),
  ];
}

function installWorkStatusBridge(pi) {
  let currentUi;
  pi.on("session_start", (_event, context) => {
    currentUi = context.ui;
  });
  pi.on("session_shutdown", () => {
    currentUi = undefined;
  });
  pi.events.on(MCP_STATUS_EVENT, (snapshot) => {
    currentUi?.setWidget(STATUS_WIDGET_KEY, renderStatus(snapshot), { placement: "aboveEditor" });
  });
}

export default function workMcpAdapter(pi) {
  installWorkStatusBridge(pi);
  const registerTool = pi.registerTool.bind(pi);
  pi.registerTool = (tool) => registerTool(guardWorkMcpAuthTool(tool));
  return createMcpAdapter({ config })(pi);
}
