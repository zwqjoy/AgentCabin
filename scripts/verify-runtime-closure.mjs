#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(fileURLToPath(import.meta.url), "../..");
const runtime = resolve(process.env.AGENTCABIN_RUNTIME_OUTPUT || join(root, "runtime-build"));
const mpath = join(runtime, "runtime-manifest.json");
if (!existsSync(mpath)) throw new Error(`RuntimeClosureInvalid: missing ${mpath}`);
const m = JSON.parse(readFileSync(mpath, "utf8"));
if (m.runtimes?.pi?.version !== "0.85.1") throw new Error("RuntimeVersionMismatch: Pi must be 0.85.1");
const exe = process.platform === "win32" ? { node: "node/node.exe", pi: "pi/bin/pi.cmd", dsh: "dsh/bin/dsh.cmd", pnpm: "pnpm/bin/pnpm.cmd" } : { node: "node/bin/node", pi: "pi/bin/pi", dsh: "dsh/bin/dsh", pnpm: "pnpm/bin/pnpm" };
for (const rel of [exe.node, exe.pi, exe.dsh, exe.pnpm, `pi/node_modules/${m.runtimes.pi.package}/${m.runtimes.pi.entrypoint}`, `dsh/node_modules/${m.runtimes.dsh.package}/${m.runtimes.dsh.entrypoint}`, "pnpm/node_modules/pnpm/bin/pnpm.cjs"]) if (!existsSync(join(runtime, rel))) throw new Error(`RuntimeClosureInvalid: missing ${rel}`);
console.log(`runtime closure valid: Pi ${m.runtimes.pi.version}, DSH ${m.runtimes.dsh.version}, Node ${m.node.version}, pnpm ${m.pnpm.version}`);
