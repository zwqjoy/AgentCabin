#!/usr/bin/env node
// Real bundled Pi RPC subprocess; isolated deterministic provider, no account or external calls.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";

const runtime = resolve(process.env.AGENTCABIN_RUNTIME_ROOT || "runtime-build");
const home = mkdtempSync(join(tmpdir(), "agentcabin-expert-rpc-"));
const requests = [];
const events = [];
let child;
let stderr = "";
const pending = new Map();
let sequence = 0;
const server = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  if (req.method !== "POST") {
    res.writeHead(405);
    res.end();
    return;
  }
  const body = JSON.parse(Buffer.concat(chunks).toString());
  if (req.url.startsWith("/mcp/")) {
    if (body.id === undefined) {
      res.writeHead(202);
      res.end();
      return;
    }
    const result =
      body.method === "initialize"
        ? {
            protocolVersion: "2025-03-26",
            capabilities: { tools: {} },
            serverInfo: { name: "expert-fixture", version: "1.0.0" },
          }
        : body.method === "tools/list"
          ? {
              tools: [
                {
                  name: "expert_probe",
                  description: "Expert fixture probe",
                  inputSchema: { type: "object", properties: {} },
                },
              ],
            }
          : body.method === "tools/call"
            ? { content: [{ type: "text", text: "fixture probe" }] }
            : {};
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ jsonrpc: "2.0", id: body.id, result }));
    return;
  }
  requests.push(body);
  res.writeHead(200, { "Content-Type": "text/event-stream" });
  const base = { id: "expert-smoke", object: "chat.completion.chunk", created: 1, model: "smoke" };
  const system = body.messages
    .filter((m) => m.role === "system")
    .map((m) => m.content)
    .join("\n");
  const isTeam = system.includes("TEAM_LEAD_ROLE");
  const hasMemberResult = body.messages.some(
    (m) => m.role === "tool" && String(m.content).includes("MEMBER_DONE"),
  );
  const chunksToSend =
    isTeam && !hasMemberResult
      ? [
          [
            {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "member-call",
                  type: "function",
                  function: {
                    name: "AgentTool",
                    arguments: JSON.stringify({ agent: "reviewer", prompt: "Review the fixture" }),
                  },
                },
              ],
            },
            null,
          ],
          [{}, "tool_calls"],
        ]
      : [
          [
            {
              role: "assistant",
              content: system.includes("REVIEWER_MEMBER_ROLE")
                ? "MEMBER_DONE"
                : "Expert configuration verified.",
            },
            null,
          ],
          [{}, "stop"],
        ];
  for (const [delta, finish_reason] of chunksToSend) {
    res.write(
      `data: ${JSON.stringify({ ...base, choices: [{ index: 0, delta, finish_reason }] })}\n\n`,
    );
  }
  res.end("data: [DONE]\n\n");
});
const pause = () => new Promise((r) => setTimeout(r, 20));
async function waitFor(predicate, label) {
  const deadline = Date.now() + 15000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error(`Timed out ${label}: ${stderr}`);
    await pause();
  }
}
async function rpc(type, fields = {}) {
  const id = `expert-${++sequence}`;
  let timer;
  const response = new Promise((resolve, reject) => {
    pending.set(id, resolve);
    timer = setTimeout(() => reject(new Error(`RPC timeout ${type}: ${stderr}`)), 15000);
  });
  child.stdin.write(JSON.stringify({ id, type, ...fields }) + "\n");
  try {
    const result = await response;
    assert.equal(result.success, true, JSON.stringify(result));
    return result.data;
  } finally {
    clearTimeout(timer);
    pending.delete(id);
  }
}
function context(id) {
  return id
    ? {
        expert: { id },
        systemPrompt: `${id.toUpperCase()}_ROLE\n${id.toUpperCase()}_PRELOADED_SKILL`,
        skills: [
          { id: `${id}-skill`, name: `${id}-skill`, description: "fixture", path: join(home, id) },
        ],
        mcpServers: {
          [id]: { url: `http://127.0.0.1:${server.address().port}/mcp/${id}`, exposure: "direct" },
        },
      }
    : { expert: null, systemPrompt: null, skills: [], mcpServers: {} };
}
function project(id) {
  writeFileSync(join(home, "expert-context.json"), JSON.stringify(context(id)));
}
async function prompt(message) {
  const mark = events.length;
  const requestMark = requests.length;
  await rpc("prompt", { message });
  await waitFor(() => events.slice(mark).some((e) => e.type === "agent_end"), message);
  await waitFor(() => requests.length > requestMark, "provider request");
  return requests
    .at(-1)
    .messages.filter((m) => m.role === "system")
    .map((m) => m.content)
    .join("\n");
}
try {
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  writeFileSync(
    join(home, "models.json"),
    JSON.stringify({
      providers: {
        "expert-smoke": {
          baseUrl: `http://127.0.0.1:${server.address().port}/v1`,
          api: "openai-completions",
          apiKey: "local-test",
          models: [{ id: "smoke", contextWindow: 32000, maxTokens: 4096 }],
        },
      },
    }),
  );
  writeFileSync(
    join(home, "settings.json"),
    JSON.stringify({ compaction: { enabled: false }, retry: { enabled: false } }),
  );
  const extension = join(home, "expert-extension.mjs");
  copyFileSync(resolve("src-tauri/src/agent/pi_expert_extension.mjs"), extension);
  project("writer");
  child = spawn(
    resolve(runtime, process.platform === "win32" ? "pi/bin/pi.cmd" : "pi/bin/pi"),
    [
      "--mode",
      "rpc",
      "--provider",
      "expert-smoke",
      "--model",
      "smoke",
      "--no-extensions",
      "-e",
      "builtin:tool-search",
      "-e",
      "builtin:codemode",
      "--no-skills",
      "--no-prompt-templates",
      "-e",
      extension,
    ],
    {
      cwd: home,
      env: { ...process.env, PI_CODING_AGENT_DIR: home },
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  let buffer = "";
  child.stderr.on("data", (c) => {
    stderr += c;
  });
  child.stdout.on("data", (c) => {
    buffer += c;
    let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      let event;
      try {
        event = JSON.parse(line);
      } catch {
        continue;
      }
      events.push(event);
      if (event.type === "response" && pending.has(event.id)) pending.get(event.id)(event);
    }
  });
  await rpc("get_state");
  let system = await prompt("First message");
  assert.ok(system.includes("WRITER_ROLE") && system.includes("WRITER_PRELOADED_SKILL"));
  system = await prompt("Second message without reselecting");
  assert.ok(system.includes("WRITER_ROLE"));
  assert.ok(requests.at(-1).tools?.some((t) => t.function.name === "mcp__writer__expert_probe"));
  project("reviewer");
  system = await prompt("Changed expert in the same conversation");
  assert.ok(system.includes("REVIEWER_ROLE") && system.includes("REVIEWER_PRELOADED_SKILL"));
  assert.ok(!system.includes("WRITER_ROLE") && !system.includes("writer-skill"));
  assert.ok(!requests.at(-1).tools?.some((t) => t.function.name === "mcp__writer__expert_probe"));
  assert.ok(
    requests.at(-1).tools?.some((t) => t.function.name === "mcp__reviewer__expert_probe"),
    JSON.stringify({
      tools: requests.at(-1).tools?.map((t) => t.function.name),
      errors: events.filter((e) => e.type.includes("error")),
      stderr,
    }),
  );
  await prompt("Use the changed expert on the following turn");
  assert.ok(requests.at(-1).tools?.some((t) => t.function.name === "mcp__reviewer__expert_probe"));
  project(null);
  system = await prompt("Continue after clicking x");
  assert.ok(system.includes("当前未选择专家"));
  assert.ok(!system.includes("REVIEWER_ROLE") && !system.includes("reviewer-skill"));
  assert.ok(!requests.at(-1).tools?.some((t) => /^mcp__(writer|reviewer)__/.test(t.function.name)));
  writeFileSync(
    join(home, "expert-context.json"),
    JSON.stringify({
      expert: { id: "team", isTeam: true },
      systemPrompt: "TEAM_LEAD_ROLE",
      skills: [],
      mcpServers: {},
      members: { reviewer: "REVIEWER_MEMBER_ROLE\nMEMBER_PRELOADED_SKILL" },
    }),
  );
  const teamMark = events.length;
  system = await prompt("Delegate the review to a real member");
  assert.ok(system.includes("TEAM_LEAD_ROLE"));
  assert.ok(
    requests.some((body) =>
      body.messages.some(
        (m) =>
          m.role === "system" &&
          String(m.content).includes("REVIEWER_MEMBER_ROLE") &&
          String(m.content).includes("MEMBER_PRELOADED_SKILL"),
      ),
    ),
  );
  assert.ok(
    events
      .slice(teamMark)
      .some((e) => e.type === "tool_execution_end" && e.toolName === "AgentTool" && !e.isError),
  );
  console.log(
    "PASS: real Pi RPC persistence, first-prompt expert switch, explicit exit, role/preload isolation, MCP registration/removal, real expert-team member subprocess",
  );
} finally {
  if (child && child.exitCode === null) {
    const exited = new Promise((r) => child.once("exit", r));
    child.kill("SIGTERM");
    await exited;
  }
  server.closeAllConnections();
  await new Promise((r) => server.close(r));
  rmSync(home, { recursive: true, force: true });
}
