#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import process from "node:process";
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(import.meta.url), "../..");
const manifest = JSON.parse(
  readFileSync(join(root, "src-tauri/runtime/runtime-manifest.json"), "utf8"),
);
const out = resolve(process.env.AGENTCABIN_RUNTIME_OUTPUT || join(root, "runtime-build"));
const nodePlatform =
  process.platform === "darwin" ? "darwin" : process.platform === "win32" ? "win" : "linux";
const nodeArch = process.arch === "arm64" ? "arm64" : "x64";
const nodeDir = join(out, "node");
const run = (cmd, args) =>
  execFileSync(cmd, args, {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
const mkdir = (dir) => mkdirSync(dir, { recursive: true });

function patchAgentBrowserForReadOnlySandbox() {
  const entrypoint = join(out, "agent-browser/node_modules/agent-browser/bin/agent-browser.js");
  const source = readFileSync(entrypoint, "utf8");
  const executableProbe = [
    "  if (platform() !== 'win32') {",
    "    try {",
    "      accessSync(binaryPath, constants.X_OK);",
    "    } catch {",
    "      // Binary exists but isn't executable - fix it",
    "      try {",
    "        chmodSync(binaryPath, 0o755);",
    "      } catch (chmodErr) {",
    "        console.error(`Error: Cannot make binary executable: ${chmodErr.message}`);",
    "        console.error('Try running: chmod +x ' + binaryPath);",
    "        process.exit(1);",
    "      }",
    "    }",
    "  }",
  ].join("\n");
  if (source.split(executableProbe).length !== 2) {
    throw new Error(
      `agent-browser ${manifest.runtimes.agentBrowser.version} executable probe changed; review its sandbox compatibility before packaging`,
    );
  }

  // The bundled platform binary is made executable during npm's postinstall.
  // Do not probe X_OK or chmod it at runtime: macOS Seatbelt can deny those
  // metadata operations even when process-exec on the binary is allowed.
  const patchedSource = source.replace(
    executableProbe,
    "  // AgentCabin prepares the bundled binary's executable bit at build time.",
  );
  writeFileSync(entrypoint, patchedSource);
}

rmSync(out, { recursive: true, force: true });
mkdir(out);
const nodeArchive = nodePlatform === "win" ? "zip" : "tar.gz";
const archive = join(
  out,
  `node-v${manifest.node.version}-${nodePlatform}-${nodeArch}.${nodeArchive}`,
);
const url = `https://nodejs.org/dist/v${manifest.node.version}/node-v${manifest.node.version}-${nodePlatform}-${nodeArch}.${nodeArchive}`;
run("curl", ["--fail", "--location", "--silent", "--show-error", "-o", archive, url]);
mkdir(nodeDir);
if (process.platform === "win32") {
  const extracted = join(out, "node-extracted");
  const powershellPath = archive.replaceAll("'", "''");
  const extractedPath = extracted.replaceAll("'", "''");
  run("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    `Expand-Archive -LiteralPath '${powershellPath}' -DestinationPath '${extractedPath}' -Force`,
  ]);
  const [nodeRoot] = readdirSync(extracted, { withFileTypes: true }).filter((entry) =>
    entry.isDirectory(),
  );
  if (!nodeRoot) throw new Error("NodeRuntimeArchiveInvalid: missing extracted root directory");
  cpSync(join(extracted, nodeRoot.name), nodeDir, { recursive: true });
  rmSync(extracted, { recursive: true, force: true });
} else {
  run("tar", ["-xzf", archive, "--strip-components=1", "-C", nodeDir]);
}
rmSync(archive, { force: true });
const npm = process.platform === "win32" ? join(nodeDir, "npm.cmd") : join(nodeDir, "bin/npm");
const installRuntime = (name, spec, overrides = undefined) => {
  const prefix = join(out, name);
  mkdir(prefix);
  if (overrides) {
    writeFileSync(
      join(prefix, "package.json"),
      `${JSON.stringify({ private: true, overrides }, null, 2)}\n`,
    );
  }
  run(npm, ["install", "--prefix", prefix, "--omit=dev", "--no-audit", "--no-fund", spec]);
};
installRuntime("pi", `${manifest.runtimes.pi.package}@${manifest.runtimes.pi.version}`);
if (manifest.runtimes.agentBrowser) {
  installRuntime(
    "agent-browser",
    `${manifest.runtimes.agentBrowser.package}@${manifest.runtimes.agentBrowser.version}`,
  );
  patchAgentBrowserForReadOnlySandbox();
}
installRuntime("pnpm", `pnpm@${manifest.pnpm.version}`);
if (process.platform !== "win32") {
  for (const name of ["pi", "pnpm", "agent-browser"]) mkdir(join(out, name, "bin"));
  const node = `$(CDPATH= cd -- "$(dirname -- "$0")/../../node/bin" && pwd)/node`;
  writeFileSync(
    join(out, "pi/bin/pi"),
    `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/${manifest.runtimes.pi.package}/${manifest.runtimes.pi.entrypoint}" "$@"\n`,
  );
  if (manifest.runtimes.agentBrowser) {
    writeFileSync(
      join(out, "agent-browser/bin/agent-browser"),
      `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/${manifest.runtimes.agentBrowser.package}/${manifest.runtimes.agentBrowser.entrypoint}" "$@"\n`,
    );
    run("chmod", ["+x", join(out, "agent-browser/bin/agent-browser")]);
  }
  writeFileSync(
    join(out, "pnpm/bin/pnpm"),
    `#!/bin/sh\nexec "${node}" "$(dirname -- "$0")/../node_modules/pnpm/bin/pnpm.cjs" "$@"\n`,
  );
  run("chmod", ["+x", join(out, "pi/bin/pi"), join(out, "pnpm/bin/pnpm")]);
} else {
  const runtimeBins = [["pi", manifest.runtimes.pi.package, manifest.runtimes.pi.entrypoint]];
  if (manifest.runtimes.agentBrowser) {
    runtimeBins.push([
      "agent-browser",
      manifest.runtimes.agentBrowser.package,
      manifest.runtimes.agentBrowser.entrypoint,
    ]);
  }
  for (const [name, pkg, entry] of runtimeBins) {
    mkdir(join(out, name, "bin"));
    writeFileSync(
      join(out, `${name}/bin/${name}.cmd`),
      `@echo off\r\n"%~dp0\\..\\..\\node\\node.exe" "%~dp0\\..\\node_modules\\${pkg}\\${entry}" %*\r\n`,
    );
  }
  mkdir(join(out, "pnpm/bin"));
  writeFileSync(
    join(out, "pnpm/bin/pnpm.cmd"),
    `@echo off\r\n"%~dp0\\..\\..\\node\\node.exe" "%~dp0\\..\\node_modules\\pnpm\\bin\\pnpm.cjs" %*\r\n`,
  );
}
writeFileSync(
  join(out, "runtime-manifest.json"),
  JSON.stringify({ ...manifest, platform: process.platform, arch: process.arch }, null, 2) + "\n",
);
console.log(`runtime closure prepared: ${out}`);
