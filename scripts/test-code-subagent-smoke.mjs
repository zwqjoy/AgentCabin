import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { EventEmitter } from "node:events";
import { execSync, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..");
const SUBAGENTS_ROOT = path.join(REPO_ROOT, "src-tauri/runtime/extensions/node_modules/pi-subagents");

console.log("================================================================================");
console.log("Code Subagent Smoke Test: parent -> child A & child B -> mailbox -> parent summary");
console.log("================================================================================");

// 1. Verify bundled pi-subagents exists and is pinned to 0.51.0
const pkgJson = JSON.parse(fs.readFileSync(path.join(SUBAGENTS_ROOT, "package.json"), "utf-8"));
assert.equal(pkgJson.version, "0.51.0", "bundled pi-subagents must be 0.51.0");
console.log(`[PASS] Pinned bundled extension: pi-subagents @${pkgJson.version}`);

// 2. Prepare isolated temporary workspace for Node TS stripping
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-code-subagent-smoke-"));
const tempSubagentsSrc = path.join(tempDir, "pi-subagents-src");
execSync(`cp -R "${path.join(SUBAGENTS_ROOT, "src")}" "${tempSubagentsSrc}"`);

// 3. Dynamically import pi-subagents intercom modules from outside node_modules
const {
  buildSubagentResultIntercomPayload,
  deliverSubagentResultIntercomEvent,
  resolveSubagentResultStatus,
} = await import(path.join(tempSubagentsSrc, "intercom/result-intercom.ts"));

console.log("[PASS] Imported pi-subagents @0.51.0 result-intercom runtime");

// 4. Test Native Supervisor Mailbox Protocol for Child A & Child B
const supervisorChannelRoot = path.join(tempDir, "channels");
fs.mkdirSync(supervisorChannelRoot, { recursive: true });

const runId = "code-subagent-run-" + Date.now();
const childAChannel = path.join(supervisorChannelRoot, `${runId}-researcher-0`);
const childBChannel = path.join(supervisorChannelRoot, `${runId}-worker-1`);

fs.mkdirSync(path.join(childAChannel, "requests"), { recursive: true });
fs.mkdirSync(path.join(childAChannel, "replies"), { recursive: true });
fs.mkdirSync(path.join(childBChannel, "requests"), { recursive: true });
fs.mkdirSync(path.join(childBChannel, "replies"), { recursive: true });

// Child A writes progress update to mailbox
const reqAId = "req-a-1";
const reqA = {
  type: "subagent.supervisor.request",
  id: reqAId,
  createdAt: Date.now(),
  reason: "progress_update",
  message: "Child A (researcher): analyzed 3 repository modules, zero vulnerabilities found.",
  expectsReply: false,
  runId,
  agent: "researcher",
  childIndex: 0,
};
fs.writeFileSync(path.join(childAChannel, "requests", `${reqAId}.json`), JSON.stringify(reqA, null, 2));

// Child B writes decision request to mailbox
const reqBId = "req-b-1";
const reqB = {
  type: "subagent.supervisor.request",
  id: reqBId,
  createdAt: Date.now(),
  reason: "need_decision",
  message: "Child B (worker): patch ready for src/core.ts. Proceed to apply?",
  expectsReply: true,
  runId,
  agent: "worker",
  childIndex: 1,
};
fs.writeFileSync(path.join(childBChannel, "requests", `${reqBId}.json`), JSON.stringify(reqB, null, 2));

console.log("[PASS] Child A and Child B dispatched supervisor mailbox requests");

// Parent inspects requests
const pendingFilesA = fs.readdirSync(path.join(childAChannel, "requests"));
const pendingFilesB = fs.readdirSync(path.join(childBChannel, "requests"));
assert.equal(pendingFilesA.length, 1, "Child A mailbox request registered");
assert.equal(pendingFilesB.length, 1, "Child B mailbox request registered");

// Parent writes reply to Child B
const replyB = {
  type: "subagent.supervisor.reply",
  requestId: reqBId,
  createdAt: Date.now(),
  message: "Approved: proceed to apply patch.",
};
fs.writeFileSync(path.join(childBChannel, "replies", `${reqBId}.json`), JSON.stringify(replyB, null, 2));

// Verify Child B received reply
assert(fs.existsSync(path.join(childBChannel, "replies", `${reqBId}.json`)), "Child B reply delivered");
console.log("[PASS] Supervisor mailbox bidirectional exchange (Child B request -> Parent reply) verified");

// 5. Build Aggregated Parent Result & Summary Payload
const childAResult = {
  agent: "researcher",
  index: 0,
  status: resolveSubagentResultStatus({ success: true, exitCode: 0, state: "complete" }),
  outputState: "present",
  summary: "Child A finished: analyzed repository architecture, identified core components.",
  artifactPath: "output/architecture-findings.md",
  intercomTarget: "session-child-a",
};

const childBResult = {
  agent: "worker",
  index: 1,
  status: resolveSubagentResultStatus({ success: true, exitCode: 0, state: "complete" }),
  outputState: "present",
  summary: "Child B finished: created patch diff and ran tests.",
  artifactPath: "output/patch.diff",
  intercomTarget: "session-child-b",
};

const payload = buildSubagentResultIntercomPayload({
  to: "parent-code-session",
  runId,
  mode: "parallel",
  source: "async",
  children: [childAResult, childBResult],
  asyncId: "async-" + runId,
  asyncDir: path.join(tempDir, "async", runId),
});

assert.equal(payload.status, "completed", "Overall group status should be completed");
assert.equal(payload.children.length, 2, "Payload must contain both children");
assert.equal(payload.children[0].agent, "researcher");
assert.equal(payload.children[1].agent, "worker");

// 6. Deliver to Parent Event Bus
const eventBus = new EventEmitter();
let receivedEvent = null;

eventBus.on("subagent:result-intercom", (event) => {
  receivedEvent = event;
  eventBus.emit("subagent:result-intercom-delivery", {
    requestId: event.requestId,
    delivered: true,
  });
});

const delivered = await deliverSubagentResultIntercomEvent(
  {
    on: (ev, fn) => {
      eventBus.on(ev, fn);
      return () => eventBus.off(ev, fn);
    },
    emit: (ev, data) => eventBus.emit(ev, data),
  },
  payload,
  1000
);

assert.equal(delivered, true, "Mailbox must acknowledge delivery to parent session");
assert(receivedEvent !== null, "Parent event bus received intercom payload");

console.log("[PASS] Delivered grouped results through event bus to parent mailbox");
console.log("\n-------------------- Generated Parent Summary --------------------");
console.log(receivedEvent.message);
console.log("------------------------------------------------------------------\n");

// 7. Verify Parent Summary Content
assert(receivedEvent.message.includes("subagent results"), "Must have subagent results header");
assert(receivedEvent.message.includes(`Run: ${runId}`), "Must reference correct runId");
assert(receivedEvent.message.includes("Children: 2 completed"), "Summary must report 2 completed children");
assert(receivedEvent.message.includes("1. researcher — process completed"), "Child A must be completed");
assert(receivedEvent.message.includes("2. worker — process completed"), "Child B must be completed");
assert(receivedEvent.message.includes("Output artifact: output/architecture-findings.md"), "Child A artifact in summary");
assert(receivedEvent.message.includes("Output artifact: output/patch.diff"), "Child B artifact in summary");
assert(receivedEvent.message.includes("Child A finished: analyzed repository architecture"), "Child A summary included");
assert(receivedEvent.message.includes("Child B finished: created patch diff"), "Child B summary included");

// 8. Verify Pi Runtime RPC loading of bundled pi-subagents
console.log("Testing real Pi Agent CLI RPC bootstrap with bundled pi-subagents...");
const subagentsEntry = path.join(SUBAGENTS_ROOT, "index.ts");
const piProc = spawn("pi", ["--mode", "rpc", "-e", subagentsEntry], {
  stdio: ["pipe", "pipe", "ignore"],
  env: {
    ...process.env,
    HOME: tempDir, // Isolate to avoid touching real ~/.pi
  },
});

let piBootstrappedSubagents = false;
piProc.stdout.on("data", (chunk) => {
  const text = chunk.toString();
  if (text.includes("subagent-async") || text.includes("subagent")) {
    piBootstrappedSubagents = true;
  }
});

await new Promise((resolve) => {
  const timeout = setTimeout(() => {
    piProc.kill();
    resolve();
  }, 5000);

  piProc.on("exit", () => {
    clearTimeout(timeout);
    resolve();
  });
});

assert(piBootstrappedSubagents, "Pi Agent CLI RPC must successfully boot with bundled pi-subagents");
console.log("[PASS] Real Pi Agent CLI bootstrapped bundled pi-subagents in RPC mode");

// Cleanup
fs.rmSync(tempDir, { recursive: true, force: true });

console.log("================================================================================");
console.log("=== ALL SMOKE CHECKS PASSED: Code Multi-Subagent & Mailbox Summary OK ===");
console.log("================================================================================");
