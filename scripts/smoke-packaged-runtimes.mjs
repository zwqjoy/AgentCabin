#!/usr/bin/env node
import { smokeWorkNativeMcp } from './smoke-work-native-mcp.mjs';
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

import { assertRuntimeCapabilities } from "./assert-runtime-capabilities.mjs";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

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
    }`,
  );
}

const runtimeRoot = findRuntimeRoot();
console.log(`\n=== 1. Validating Runtime Closure at: ${runtimeRoot} ===`);

const manifest = JSON.parse(readFileSync(join(runtimeRoot, "runtime-manifest.json"), "utf8"));
if (manifest.runtimes?.pi?.version !== "1.0.2") {
  throw new Error(
    `Pi runtime version mismatch in manifest: expected 1.0.2, got ${manifest.runtimes?.pi?.version}`,
  );
}
if (manifest.node?.version !== "24.21.0") {
  throw new Error(
    `Node runtime version mismatch in manifest: expected 24.21.0, got ${manifest.node?.version}`,
  );
}
console.log(
  `Manifest: Pi ${manifest.runtimes.pi.version}, Node ${manifest.node.version}, pnpm ${manifest.pnpm.version}`,
);

const packagedExtensions = resolve(runtimeRoot, "../runtime/extensions");
assertRuntimeCapabilities(runtimeRoot, existsSync(join(packagedExtensions, "package.json")) ? packagedExtensions : join(root, "src-tauri/runtime/extensions"));

const isWin = process.platform === "win32";
const nodeBin = resolve(runtimeRoot, isWin ? "node/node.exe" : "node/bin/node");
const pnpmBin = resolve(runtimeRoot, isWin ? "pnpm/bin/pnpm.cmd" : "pnpm/bin/pnpm");
const piBin = resolve(runtimeRoot, isWin ? "pi/bin/pi.cmd" : "pi/bin/pi");

const npmCli = resolve(
  runtimeRoot,
  existsSync(join(runtimeRoot, "node/lib/node_modules/npm/bin/npm-cli.js"))
    ? "node/lib/node_modules/npm/bin/npm-cli.js"
    : "node/node_modules/npm/bin/npm-cli.js",
);

const requiredBinaries = [
  ["node", nodeBin],
  ["npm-cli", npmCli],
  ["pnpm", pnpmBin],
  ["pi", piBin],
];



for (const [name, path] of requiredBinaries) {
  if (!existsSync(path)) {
    throw new Error(`Missing required binary/script: ${name} at ${path}`);
  }
}
console.log("✓ All bundled binaries and scripts present");

// Assert no DSH runtime was accidentally packaged
if (existsSync(join(runtimeRoot, "dsh"))) {
  throw new Error(
    "RuntimeClosureInvalid: DSH runtime directory found in packaged closure — DSH must not be bundled",
  );
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
      `Command failed (${cmd} ${args.join(" ")}): exit ${res.status}\nStdout: ${res.stdout}\nStderr: ${res.stderr}`,
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
        rej(
          new Error(`Pi RPC (${label}) exited without valid get_state response. Output: ${output}`),
        );
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

  const extensionFixture = join(root, "scripts/fixtures/pi-extension-local");
  const extensionSource = `./${extensionFixture.slice(root.length + 1).replaceAll("\\", "/")}`;
  console.log("Installing the local acceptance extension via bundled Pi/npm...");
  runSync(piBin, ["install", extensionSource], {
    env: {
      PI_CODING_AGENT_DIR: tempDir,
    },
  });

  const settingsPath = join(tempDir, "settings.json");
  const installedSettings = JSON.parse(readFileSync(settingsPath, "utf8"));
  if (!installedSettings.packages?.some((entry) => entry.includes("scripts/fixtures/pi-extension-local"))) {
    throw new Error(`Pi extension source was not registered in ${settingsPath}`);
  }
  console.log("✓ Pi registered the local extension package");

  console.log("Removing the local acceptance extension via Pi CLI...");
  runSync(piBin, ["remove", extensionSource], {
    env: {
      PI_CODING_AGENT_DIR: tempDir,
    },
  });
  const removedSettings = JSON.parse(readFileSync(settingsPath, "utf8"));
  if (removedSettings.packages?.some((entry) => entry.includes("scripts/fixtures/pi-extension-local"))) {
    throw new Error("Pi did not remove the local extension package registration");
  }
  console.log("✓ Pi removed the local extension registration successfully");
}

async function testWorkActiveToolsSmoke() {
  const piSdkPath = resolve(runtimeRoot, "pi/node_modules/@earendil-works/pi-coding-agent/dist/index.js");
  const { createAgentSession, DefaultResourceLoader } = await import(pathToFileURL(piSdkPath).href);
  const agentDir = createTempDir("work-tools");
  const previous = process.env.AGENTCABIN_WORK_BROWSER_ENABLED;
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  let session;
  try {
    const resourceLoader = new DefaultResourceLoader({
      cwd: agentDir, agentDir,
      additionalExtensionPaths: [resolve(root, "src-tauri/src/work/pi_core_extension.mjs"), resolve(root, "src-tauri/src/work/pi_browser_adapter.mjs")],
    });
    await resourceLoader.reload();
    ({ session } = await createAgentSession({ agentDir, resourceLoader }));
    await session.extensionRunner.emit({ type: "session_start" });
    const tools = session.getActiveToolNames();
    for (const name of ["web_search", "web_open", "web_extract", "web_cite", "work_list_apps", "work_call_app", "work_run_connector_cli"]) {
      if (!tools.includes(name)) throw new Error(`Work active tools missing ${name}: ${tools}`);
    }
    for (const name of ["agent_browser", "agent_browser_code", "agent_browser_tools", "launch_app", "find_roots", "observe_ui", "search_ui", "expand_ui", "inspect_ui", "act_ui", "read_text", "wait_for"]) {
      if (tools.includes(name)) throw new Error(`Removed automation tool registered: ${name}`);
    }
    console.log("✓ Work active tools retain Web Research and Apps; Browser Automation removed");
  } finally {
    session?.dispose();
    if (previous === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED;
    else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous;
  }
}

async function verifyNoOrphans() {
  console.log("\n=== 6. Verifying Zero Orphan Processes ===");
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
    await testWorkActiveToolsSmoke();
    await smokeWorkNativeMcp(runtimeRoot);
    runSync(nodeBin, [join(root, "scripts/test-pi-rpc.mjs")], { env: { AGENTCABIN_RUNTIME_ROOT: runtimeRoot } });
    console.log("✓ Extended Pi RPC regression passed");
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
