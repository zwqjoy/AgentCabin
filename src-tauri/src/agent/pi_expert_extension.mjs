import fs from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const xml = (value) => String(value ?? "").replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");

// Read per turn: the expert can change while the Pi RPC process stays alive.
export function applyExpertContext(systemPrompt, context) {
  const base = systemPrompt.replace(/<available_skills>[\s\S]*?<\/available_skills>/g, "");
  const skills = (context.skills ?? []).filter((skill) => !skill.id.includes("--expert--") && !skill.id.includes("--expert-team--"));
  const catalog = skills.length ? `\n<available_skills>\n${skills.map((skill) => `<skill><name>${xml(skill.name)}</name><description>${xml(skill.description)}</description><location>${xml(path.join(skill.path, "SKILL.md"))}</location></skill>`).join("\n")}\n</available_skills>\nRead the relevant SKILL.md before using a skill. Resolve its relative resources from its directory.` : "";
  const role = context.expert && context.systemPrompt
    ? context.systemPrompt
    : "## 当前会话专家\n当前未选择专家。停止遵循历史轮次中的专家角色和专家团指令；按普通助手身份处理当前任务。";
  return `${base}\n${catalog}\n${role}`;
}

export function memberArguments(argv, prompt) {
  const output = [];
  for (let i = 0; i < argv.length; i++) {
    if (["--mode", "--session", "--fork"].includes(argv[i])) { i++; continue; }
    if (["--resume", "--continue", "--no-session", "--print", "-p", "-c", "-r"].includes(argv[i])) continue;
    output.push(argv[i]);
  }
  return [...output, "--mode", "json", "--print", "--no-session", prompt];
}

export async function runExpertMember(agent, prompt, expertId, signal, onUpdate) {
  const args = memberArguments(process.argv.slice(2), prompt);
  const child = spawn(process.execPath, [...process.execArgv, process.argv[1], ...args], {
    cwd: process.cwd(), env: { ...process.env, AGENTCABIN_EXPERT_MEMBER: agent, AGENTCABIN_EXPERT_MEMBER_PLUGIN: expertId },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let buffer = "";
  let error = "";
  let result = "";
  let turns = 0;
  const abort = () => child.kill("SIGTERM");
  signal?.addEventListener("abort", abort, { once: true });
  const timeout = setTimeout(abort, 5 * 60 * 1000);
  child.stderr.on("data", chunk => { error = (error + chunk).slice(-4000); });
  child.stdout.on("data", chunk => {
    buffer += chunk;
    let newline;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline); buffer = buffer.slice(newline + 1);
      let event; try { event = JSON.parse(line); } catch { continue; }
      if (event.type === "turn_start" && ++turns > 50) abort();
      if (event.type === "message_end" && event.message?.role === "assistant") {
        const text = (event.message.content ?? []).filter(item => item.type === "text").map(item => item.text).join("\n");
        if (text) result = text;
      }
      if (event.type === "tool_execution_start") onUpdate?.({ content: [{ type: "text", text: `${agent} 正在执行 ${event.toolName}` }] });
    }
  });
  try {
    const code = await new Promise((resolve, reject) => { child.once("error", reject); child.once("close", resolve); });
    if (signal?.aborted) throw new Error("成员任务已取消");
    if (code !== 0 || !result) throw new Error(`专家团成员 ${agent} 执行失败: ${error || "未返回结果"}`);
    return { content: [{ type: "text", text: result }], details: { agent, expertId, turns } };
  } finally { clearTimeout(timeout); signal?.removeEventListener("abort", abort); }
}

export default async function expertExtension(pi) {
  // Use Pi's native consumer in this extension, before our reconciliation
  // handlers. Pi orders built-ins after file extensions regardless of -e order.
  const nativePath = path.join(path.dirname(process.argv[1]), path.basename(path.dirname(process.argv[1])) === "bundle" ? "../extensions/mcp/index.js" : "extensions/mcp/index.js");
  const nativeMcp = await import(pathToFileURL(nativePath).href);
  await nativeMcp.default(pi);
  const contextPath = path.join(path.dirname(fileURLToPath(import.meta.url)), "expert-context.json");
  const registered = new Map();
  let sessionActive = false;
  const serviceChanges = [];
  pi.on("session_start", async () => { sessionActive = true; });
  // This handler follows the native MCP consumer in the managed extension
  // order, so completion means connection/removal has finished.
  pi.on("mcp_servers_change", async () => { serviceChanges.shift()?.(); });
  async function syncServices(context) {
    const track = () => sessionActive ? new Promise(resolve => serviceChanges.push(resolve)) : Promise.resolve();
    const next = context.mcpServers ?? {};
    for (const name of registered.keys()) {
      if (!Object.hasOwn(next, name)) {
        const changed = track();
        pi.unregisterMcpServer(name);
        registered.delete(name);
        await changed;
      }
    }
    for (const [name, config] of Object.entries(next)) {
      const identity = JSON.stringify(config);
      if (registered.get(name) === identity) continue;
      const changed = track();
      pi.registerMcpServer(name, config);
      registered.set(name, identity);
      await changed;
    }
  }
  const readContext = () => JSON.parse(fs.readFileSync(contextPath, "utf8"));
  await syncServices(readContext());
  // Reconcile before Pi rebuilds its system prompt. The native MCP hook can
  // then await newly selected direct servers on this very turn.
  pi.on("input", async () => {
    await syncServices(readContext());
    return { action: "continue" };
  });
  pi.registerTool({
    name: "AgentTool", label: "委派专家成员", description: "将独立任务交给当前专家团的指定成员，执行真实的独立会话并返回结果。",
    defaultActive: false,
    parameters: { type: "object", properties: { agent: { type: "string" }, prompt: { type: "string" } }, required: ["agent", "prompt"], additionalProperties: false },
    async execute(_id, { agent, prompt }, signal, onUpdate) {
      const context = readContext();
      if (process.env.AGENTCABIN_EXPERT_MEMBER || !context.expert?.isTeam || !Object.hasOwn(context.members ?? {}, agent)) {
        throw new Error("只能委派给当前专家团声明的成员");
      }
      if (signal?.aborted) throw new Error("成员任务已取消");
      return runExpertMember(agent, prompt, context.expert.id, signal, onUpdate);
    },
  });
  pi.on("before_agent_start", async (event) => {
    // Fail closed on a corrupt projection instead of silently running with the wrong role.
    const context = readContext();
    await syncServices(context);
    const member = process.env.AGENTCABIN_EXPERT_MEMBER;
    if (member) {
      if (context.expert?.id !== process.env.AGENTCABIN_EXPERT_MEMBER_PLUGIN || !Object.hasOwn(context.members ?? {}, member)) {
        throw new Error("专家团配置已更改，停止旧成员任务");
      }
      context.systemPrompt = context.members[member];
    }
    const tools = pi.getActiveTools().filter(name => name !== "AgentTool");
    if (!member && context.expert?.isTeam && Object.keys(context.members ?? {}).length) tools.push("AgentTool");
    pi.setActiveTools(tools);
    return { systemPrompt: applyExpertContext(event.systemPrompt, context) };
  });
}
