#!/usr/bin/env node
/**
 * Automated Packaged Runtimes Behavioral Smoke Test
 *
 * Verifies end-to-end:
 * 1. Runtime Closure integrity (manifest, bundled Node, npm, pnpm, Pi).
 * 2. Pi Code RPC startup and response.
 * 3. Pi Work isolated RPC startup.
 * 4. Pi Extension installation & uninstallation using bundled Node/npm.
 * 5. No DSH runtime present in packaged closure.
 * 6. Zero orphan processes lingering after completion.
 */

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(import.meta.url), "../..");

function findRuntimeRoot() {
  const requirePackaged =
    process.env.AGENTCABIN_REQUIRE_PACKAGED === "1" ||
    process.env.AGENTCABIN_REQUIRE_PACKAGED === "true";

  const candidates = [
    process.env.AGENTCABIN_RUNTIME_ROOT,
    join(root, "dist-electron/mac-arm64/AgentCabin.app/Contents/Resources/runtimes"),
    join(root, "dist-electron/mac/AgentCabin.app/Contents/Resources/runtimes"),
    join(root, "dist-electron/mac-universal/AgentCabin.app/Contents/Resources/runtimes"),
    join(root, "dist-electron/win-unpacked/resources/runtimes"),
    join(root, "dist-electron/linux-unpacked/resources/runtimes"),
    join(root, "dist/mac-arm64/AgentCabin.app/Contents/Resources/runtimes"),
    join(root, "dist/mac/AgentCabin.app/Contents/Resources/runtimes"),
    join(root, "dist/mac-universal/AgentCabin.app/Contents/Resources/runtimes"),
    requirePackaged ? null : join(root, "runtime-build"),
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (existsSync(join(candidate, "runtime-manifest.json"))) {
      if (candidate.endsWith("runtime-build")) {
        console.warn("\n⚠️  [NOTICE] Running smoke against unpackaged 'runtime-build'.");
      } else {
        console.log(`\n📦 Found packaged application runtime closure:\n   ${candidate}`);
      }
      return candidate;
    }
  }
  throw new Error(
    `No runtime closure found (requirePackaged=${requirePackaged}). Checked:\n${candidates
      .map((c) => `  - ${c}`)
      .join("\n")}\n${
      requirePackaged
        ? "Build the packaged Electron app first ('npm run package' or 'npm run electron:package:dir')."
        : "Run 'npm run prepare:runtimes' first."
    }`
  );
}

const runtimeRoot = findRuntimeRoot();
console.log(`\n=== 1. Validating Runtime Closure at: ${runtimeRoot} ===`);

const manifest = JSON.parse(readFileSync(join(runtimeRoot, "runtime-manifest.json"), "utf8"));
console.log(`Manifest: Pi ${manifest.runtimes.pi.version}, Node ${manifest.node.version}, pnpm ${manifest.pnpm.version}`);

const isWin = process.platform === "win32";
const nodeBin = resolve(runtimeRoot, isWin ? "node/node.exe" : "node/bin/node");
const pnpmBin = resolve(runtimeRoot, isWin ? "pnpm/bin/pnpm.cmd" : "pnpm/bin/pnpm");
const piBin = resolve(runtimeRoot, isWin ? "pi/bin/pi.cmd" : "pi/bin/pi");

const npmCli = resolve(
  runtimeRoot,
  existsSync(join(runtimeRoot, "node/lib/node_modules/npm/bin/npm-cli.js"))
    ? "node/lib/node_modules/npm/bin/npm-cli.js"
    : "node/node_modules/npm/bin/npm-cli.js"
);

for (const [name, path] of [
  ["node", nodeBin],
  ["npm-cli", npmCli],
  ["pnpm", pnpmBin],
  ["pi", piBin],
]) {
  if (!existsSync(path)) {
    throw new Error(`Missing required binary/script: ${name} at ${path}`);
  }
}
console.log("✓ All bundled binaries and scripts present");

// Assert no DSH runtime was accidentally packaged
if (existsSync(join(runtimeRoot, "dsh"))) {
  throw new Error("RuntimeClosureInvalid: DSH runtime directory found in packaged closure — DSH must not be bundled");
}
console.log("✓ No DSH runtime present in packaged closure (expected)");

// Construct augmented PATH putting bundled tools first
const bundledBinDirs = [
  resolve(runtimeRoot, isWin ? "node" : "node/bin"),
  resolve(runtimeRoot, isWin ? "pnpm/bin" : "pnpm/bin"),
  resolve(runtimeRoot, isWin ? "pi/bin" : "pi/bin"),
];
const augmentedPath = `${bundledBinDirs.join(isWin ? ";" : ":")}${isWin ? ";" : ":"}${process.env.PATH || ""}`;

const trackedPids = new Set();
const tempDirsToClean = [];

function createTempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), `agentcabin-smoke-${prefix}-`));
  tempDirsToClean.push(dir);
  return dir;
}

function runSync(cmd, args, options = {}) {
  const res = spawnSync(cmd, args, {
    ...options,
    encoding: "utf8",
    shell: isWin,
    env: {
      ...process.env,
      PATH: augmentedPath,
      ...(options.env || {}),
    },
  });
  if (res.error) throw res.error;
  if (res.status !== 0) {
    throw new Error(
      `Command failed (${cmd} ${args.join(" ")}): exit ${res.status}\nStdout: ${res.stdout}\nStderr: ${res.stderr}`
    );
  }
  return res;
}

async function runPiRpcSmoke(label, extraArgs = [], extraEnv = {}) {
  console.log(`\n=== 2. Testing Pi RPC: ${label} ===`);
  const tempHome = createTempDir("pi-rpc");
  const child = spawn(piBin, ["--mode", "rpc", ...extraArgs], {
    shell: isWin,
    env: {
      ...process.env,
      PATH: augmentedPath,
      PI_CODING_AGENT_DIR: tempHome,
      ...extraEnv,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });

  trackedPids.add(child.pid);

  let output = "";
  child.stdout.on("data", (chunk) => {
    output += chunk.toString("utf8");
  });

  // Query Pi internal state via JSON RPC to verify true protocol loop
  const requestId = `smoke-${label.replace(/[^a-z0-9]/gi, "-").toLowerCase()}`;
  child.stdin.write(JSON.stringify({ id: requestId, type: "get_state" }) + "\n");

  await new Promise((res, rej) => {
    let closed = false;
    const timeout = setTimeout(() => {
      try {
        child.kill("SIGKILL");
      } catch {}
      rej(new Error(`Pi RPC (${label}) timed out waiting for response. Output so far: ${output}`));
    }, 15000);

    const check = setInterval(() => {
      if (closed) return;
      for (const line of output.split("\n")) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        try {
          const msg = JSON.parse(trimmed);
          if (
            msg.id === requestId &&
            msg.type === "response" &&
            msg.command === "get_state" &&
            msg.success === true
          ) {
            closed = true;
            clearInterval(check);
            clearTimeout(timeout);
            try {
              child.stdin.end();
            } catch {}
            try {
              child.kill("SIGTERM");
            } catch {}
            break;
          }
        } catch {}
      }
    }, 100);

    child.on("exit", () => {
      clearInterval(check);
      clearTimeout(timeout);
      if (closed) {
        res();
      } else {
        rej(new Error(`Pi RPC (${label}) exited without valid get_state response. Output: ${output}`));
      }
    });
  });

  console.log(`✓ Pi RPC (${label}) state protocol responded cleanly with success=true`);
}

async function testPiExtensionInstall() {
  console.log("\n=== 3. Testing Pi Extension Installation with Bundled npm ===");
  const tempDir = createTempDir("pi-ext");
  const cacheDir = join(tempDir, "npm-cache");
  mkdirSync(cacheDir, { recursive: true });

  // Configure settings.json with bundled npmCommand
  const settings = {
    npmCommand: [nodeBin, npmCli, "--cache", cacheDir],
  };
  writeFileSync(join(tempDir, "settings.json"), JSON.stringify(settings, null, 2) + "\n");

  console.log("Installing 'npm:is-sorted' via Pi CLI using bundled npmCommand...");
  runSync(piBin, ["install", "npm:is-sorted"], {
    env: {
      PI_CODING_AGENT_DIR: tempDir,
    },
  });

  const installedPkg = join(tempDir, "npm/node_modules/is-sorted/package.json");
  if (!existsSync(installedPkg)) {
    throw new Error(`Pi extension failed to install to ${installedPkg}`);
  }
  console.log("✓ Pi extension installed package bytes successfully");

  console.log("Removing 'npm:is-sorted' via Pi CLI...");
  runSync(piBin, ["remove", "npm:is-sorted"], {
    env: {
      PI_CODING_AGENT_DIR: tempDir,
    },
  });
  console.log("✓ Pi extension removed successfully");
}

async function verifyNoOrphans() {
  console.log("\n=== 4. Verifying Zero Orphan Processes ===");
  for (let i = 0; i < 20; i++) {
    let alive = 0;
    for (const pid of trackedPids) {
      try {
        process.kill(pid, 0);
        alive++;
      } catch (err) {
        if (err.code === "ESRCH") {
          // Process exited
        }
      }
    }
    if (alive === 0) break;
    await new Promise((r) => setTimeout(r, 100));
  }

  for (const pid of trackedPids) {
    try {
      process.kill(pid, 0);
      throw new Error(`Orphan process found! PID ${pid} is still alive.`);
    } catch (err) {
      if (err.code !== "ESRCH") throw err;
    }
  }
  console.log(`✓ All ${trackedPids.size} spawned process(es) exited cleanly without orphans.`);
}

async function main() {
  try {
    await runPiRpcSmoke("Code Mode", []);
    await runPiRpcSmoke("Work Mode (isolated)", [
      "--no-extensions",
      "--no-skills",
      "--no-prompt-templates",
      "--no-themes",
      "--no-context-files",
    ]);
    await testPiExtensionInstall();
    await verifyNoOrphans();

    console.log("\n🎉 ALL PACKAGED RUNTIME CLOSURE SMOKE CHECKS PASSED SUCCESSFULLY!\n");
  } finally {
    for (const dir of tempDirsToClean) {
      rmSync(dir, { recursive: true, force: true });
    }
  }
}

main().catch((err) => {
  console.error("\n❌ Packaged Runtime Smoke Test FAILED:", err);
  process.exit(1);
});
