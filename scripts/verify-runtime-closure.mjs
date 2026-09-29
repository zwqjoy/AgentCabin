#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(fileURLToPath(import.meta.url), "../..");
const runtime = resolve(process.env.AGENTCABIN_RUNTIME_OUTPUT || join(root, "runtime-build"));
const mpath = join(runtime, "runtime-manifest.json");
if (!existsSync(mpath)) throw new Error(`RuntimeClosureInvalid: missing ${mpath}`);
const m = JSON.parse(readFileSync(mpath, "utf8"));
if (m.runtimes?.pi?.version !== "0.87.1") throw new Error("RuntimeVersionMismatch: Pi must be 0.87.1");
if (m.node?.version !== "24.21.0") throw new Error("RuntimeVersionMismatch: Node must be 24.21.0");
// Verify no DSH runtime was accidentally packaged
if (existsSync(join(runtime, "dsh"))) throw new Error("RuntimeClosureInvalid: unexpected DSH runtime directory found in closure");
const exe = process.platform === "win32"
  ? { node: "node/node.exe", pi: "pi/bin/pi.cmd", pnpm: "pnpm/bin/pnpm.cmd" }
  : { node: "node/bin/node", pi: "pi/bin/pi", pnpm: "pnpm/bin/pnpm" };
for (const rel of [
  exe.node,
  exe.pi,
  exe.pnpm,
  `pi/node_modules/${m.runtimes.pi.package}/${m.runtimes.pi.entrypoint}`,
  "pnpm/node_modules/pnpm/bin/pnpm.cjs",
]) {
  if (!existsSync(join(runtime, rel))) throw new Error(`RuntimeClosureInvalid: missing ${rel}`);
}
if (m.runtimes?.agentBrowser) {
  if (m.runtimes.agentBrowser.version !== "0.37.0") throw new Error("RuntimeVersionMismatch: agentBrowser must be 0.37.0");
  const agentBrowserExe = process.platform === "win32" ? "agent-browser/bin/agent-browser.cmd" : "agent-browser/bin/agent-browser";
  if (!existsSync(join(runtime, agentBrowserExe))) throw new Error(`RuntimeClosureInvalid: missing ${agentBrowserExe}`);
  if (!existsSync(join(runtime, `agent-browser/node_modules/${m.runtimes.agentBrowser.package}/${m.runtimes.agentBrowser.entrypoint}`))) {
    throw new Error("RuntimeClosureInvalid: missing agent-browser entrypoint");
  }
}
console.log(`runtime closure valid: Pi ${m.runtimes.pi.version}, Node ${m.node.version}, pnpm ${m.pnpm.version}, agent-browser ${m.runtimes?.agentBrowser?.version}`);
