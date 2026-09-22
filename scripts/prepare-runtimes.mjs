#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(import.meta.url), "../..");
const manifest = JSON.parse(readFileSync(join(root, "src-tauri/runtime/runtime-manifest.json"), "utf8"));
const out = resolve(process.env.AGENTCABIN_RUNTIME_OUTPUT || join(root, "runtime-build"));
const nodePlatform = process.platform === "darwin" ? "darwin" : process.platform === "win32" ? "win" : "linux";
const nodeArch = process.arch === "arm64" ? "arm64" : "x64";
const nodeDir = join(out, "node");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });
const mkdir = (dir) => mkdirSync(dir, { recursive: true });

rmSync(out, { recursive: true, force: true });
mkdir(out);
const archive = join(out, `node-v${manifest.node.version}-${nodePlatform}-${nodeArch}.tar.gz`);
const url = `https://nodejs.org/dist/v${manifest.node.version}/node-v${manifest.node.version}-${nodePlatform}-${nodeArch}.tar.gz`;
run("curl", ["--fail", "--location", "--silent", "--show-error", "-o", archive, url]);
mkdir(nodeDir);
run("tar", ["-xzf", archive, "--strip-components=1", "-C", nodeDir]);
rmSync(archive, { force: true });
const npm = process.platform === "win32" ? join(nodeDir, "npm.cmd") : join(nodeDir, "bin/npm");
for (const [name, spec] of [["pi", `${manifest.runtimes.pi.package}@${manifest.runtimes.pi.version}`], ["dsh", `${manifest.runtimes.dsh.package}@${manifest.runtimes.dsh.version}`], ["pnpm", `pnpm@${manifest.pnpm.version}`]]) {
  mkdir(join(out, name));
  run(npm, ["install", "--prefix", join(out, name), "--omit=dev", "--no-audit", "--no-fund", spec]);
}
if (process.platform !== "win32") {
  for (const name of ["pi", "dsh", "pnpm"]) mkdir(join(out, name, "bin"));
  const node = `$(CDPATH= cd -- "$(dirname -- "$0")/../../node/bin" && pwd)/node`;
  writeFileSync(join(out, "pi/bin/pi"), `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/${manifest.runtimes.pi.package}/${manifest.runtimes.pi.entrypoint}" "$@"\n`);
  writeFileSync(join(out, "dsh/bin/dsh"), `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/${manifest.runtimes.dsh.package}/${manifest.runtimes.dsh.entrypoint}" "$@"\n`);
  writeFileSync(join(out, "pnpm/bin/pnpm"), `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/pnpm/bin/pnpm.cjs" "$@"\n`);
  run("chmod", ["+x", join(out, "pi/bin/pi"), join(out, "dsh/bin/dsh"), join(out, "pnpm/bin/pnpm")]);
} else {
  for (const [name, pkg, entry] of [["pi", manifest.runtimes.pi.package, manifest.runtimes.pi.entrypoint], ["dsh", manifest.runtimes.dsh.package, manifest.runtimes.dsh.entrypoint]]) {
    mkdir(join(out, name, "bin"));
    writeFileSync(join(out, `${name}/bin/${name}.cmd`), `@echo off\r\n"%~dp0\\..\\..\\node\\node.exe" "%~dp0\\..\\node_modules\\${pkg}\\${entry}" %*\r\n`);
  }
  mkdir(join(out, "pnpm/bin"));
  writeFileSync(join(out, "pnpm/bin/pnpm.cmd"), `@echo off\r\n"%~dp0\\..\\..\\node\\node.exe" "%~dp0\\..\\node_modules\\pnpm\\bin\\pnpm.cjs" %*\r\n`);
}
writeFileSync(join(out, "runtime-manifest.json"), JSON.stringify({ ...manifest, platform: process.platform, arch: process.arch }, null, 2) + "\n");
console.log(`runtime closure prepared: ${out}`);
