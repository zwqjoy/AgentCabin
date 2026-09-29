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
    }`,
  );
}

const runtimeRoot = findRuntimeRoot();
console.log(`\n=== 1. Validating Runtime Closure at: ${runtimeRoot} ===`);

const manifest = JSON.parse(readFileSync(join(runtimeRoot, "runtime-manifest.json"), "utf8"));
if (manifest.runtimes?.pi?.version !== "0.87.1") {
  throw new Error(
    `Pi runtime version mismatch in manifest: expected 0.87.1, got ${manifest.runtimes?.pi?.version}`,
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

const agentBrowserBin = resolve(
  runtimeRoot,
  isWin ? "agent-browser/bin/agent-browser.cmd" : "agent-browser/bin/agent-browser",
);

const requiredBinaries = [
  ["node", nodeBin],
  ["npm-cli", npmCli],
  ["pnpm", pnpmBin],
  ["pi", piBin],
];

if (manifest.runtimes?.agentBrowser) {
  requiredBinaries.push(["agent-browser", agentBrowserBin]);
}

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
  resolve(runtimeRoot, isWin ? "agent-browser/bin" : "agent-browser/bin"),
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

async function testAgentBrowserSmoke() {
  console.log("\n=== 4. Testing agent-browser Presence & Version Validation ===");
  if (!existsSync(agentBrowserBin)) {
    throw new Error(`agent-browser binary not found at ${agentBrowserBin}`);
  }

  const expectedVersion = manifest.runtimes?.agentBrowser?.version || "0.37.0";

  // 1. Run agent-browser binary directly
  console.log("Checking agent-browser binary directly...");
  const res = runSync(agentBrowserBin, ["--version"]);
  const versionOutput = res.stdout.trim();
  console.log(`agent-browser output (direct): ${versionOutput}`);
  if (!versionOutput.includes(expectedVersion)) {
    throw new Error(
      `agent-browser version mismatch: expected ${expectedVersion}, got ${versionOutput}`,
    );
  }
  console.log(`✓ agent-browser direct binary verified (${expectedVersion})`);

  // 2. Run agent-browser via augmented PATH discovery
  console.log("Checking agent-browser discovery via augmented PATH...");
  const resPath = runSync("agent-browser", ["--version"]);
  const pathVersionOutput = resPath.stdout.trim();
  console.log(`agent-browser output (via PATH): ${pathVersionOutput}`);
  if (!pathVersionOutput.includes(expectedVersion)) {
    throw new Error(
      `agent-browser via PATH lookup failed: expected ${expectedVersion}, got ${pathVersionOutput}`,
    );
  }
  console.log(`✓ agent-browser verified via PATH lookup (${expectedVersion})`);

  // 3. Validate packaged pi-agent-browser-native extension pinning (0.8.2)
  const isPackagedClosure = !runtimeRoot.endsWith("runtime-build");
  const packagedExtPkgJson = resolve(
    runtimeRoot,
    "../runtime/extensions/node_modules/pi-agent-browser-native/package.json",
  );
  const devExtPkgJson = resolve(
    root,
    "src-tauri/runtime/extensions/node_modules/pi-agent-browser-native/package.json",
  );

  const targetExtPkgJson = isPackagedClosure ? packagedExtPkgJson : devExtPkgJson;

  if (!existsSync(targetExtPkgJson)) {
    throw new Error(
      `pi-agent-browser-native package.json not found in ${isPackagedClosure ? "packaged resources" : "dev extensions"} (checked ${targetExtPkgJson})`,
    );
  }

  const extPkg = JSON.parse(readFileSync(targetExtPkgJson, "utf8"));
  if (extPkg.version !== "0.8.2") {
    throw new Error(
      `pi-agent-browser-native version in ${targetExtPkgJson} mismatch: expected exact 0.8.2, got ${extPkg.version}`,
    );
  }
  console.log(
    `✓ pi-agent-browser-native installed package verified exact ${extPkg.version} (${targetExtPkgJson})`,
  );
  return targetExtPkgJson;
}

async function testBrowserUseActiveToolsSmoke(targetExtPkgJson) {
  console.log("\n=== 5. Testing Browser Use Active Tools (Code & Work Modes) ===");
  const browserExtDir = resolve(targetExtPkgJson, "..");
  const coreExtPath = resolve(root, "src-tauri/src/work/pi_core_extension.mjs");
  const piSdkPath = resolve(
    runtimeRoot,
    "pi/node_modules/@earendil-works/pi-coding-agent/dist/index.js",
  );

  const { createAgentSession, DefaultResourceLoader } = await import(piSdkPath);

  // 1. Code Mode + Browser Use ON
  console.log("Checking Code Mode + Browser Use ON active tools...");
  const tempAgentDirCode = createTempDir("smoke-code-bu");
  const resourceLoaderCode = new DefaultResourceLoader({
    cwd: process.cwd(),
    agentDir: tempAgentDirCode,
    additionalExtensionPaths: [browserExtDir],
  });
  await resourceLoaderCode.reload();
  const { session: codeSession } = await createAgentSession({
    agentDir: tempAgentDirCode,
    resourceLoader: resourceLoaderCode,
  });
  const codeTools = codeSession.getActiveToolNames();
  if (!codeTools.includes("agent_browser")) {
    throw new Error(`Code mode active tools missing 'agent_browser'. Got: ${codeTools.join(", ")}`);
  }
  if (!codeTools.includes("agent_browser_code")) {
    throw new Error(
      `Code mode active tools missing 'agent_browser_code'. Got: ${codeTools.join(", ")}`,
    );
  }
  if (!codeTools.includes("agent_browser_tools")) {
    throw new Error(
      `Code mode active tools missing 'agent_browser_tools'. Got: ${codeTools.join(", ")}`,
    );
  }
  console.log(
    "✓ Code Mode + Browser Use ON: active tools contain agent_browser, agent_browser_code, agent_browser_tools",
  );

  // 2. Work Mode + Browser Use ON
  console.log("Checking Work Mode + Browser Use ON active tools after session_start...");
  const prevEnv = process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED;
  process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED = "1";
  try {
    const tempAgentDirWork = createTempDir("smoke-work-bu");
    const resourceLoaderWork = new DefaultResourceLoader({
      cwd: process.cwd(),
      agentDir: tempAgentDirWork,
      additionalExtensionPaths: [coreExtPath, browserExtDir],
    });
    await resourceLoaderWork.reload();
    const { session: workSession } = await createAgentSession({
      agentDir: tempAgentDirWork,
      resourceLoader: resourceLoaderWork,
    });
    await workSession.extensionRunner.emit({ type: "session_start" });
    const workTools = workSession.getActiveToolNames();

    // Must include agent_browser*
    if (!workTools.includes("agent_browser")) {
      throw new Error(
        `Work mode active tools missing 'agent_browser'. Got: ${workTools.join(", ")}`,
      );
    }
    if (!workTools.includes("agent_browser_code")) {
      throw new Error(
        `Work mode active tools missing 'agent_browser_code'. Got: ${workTools.join(", ")}`,
      );
    }
    if (!workTools.includes("agent_browser_tools")) {
      throw new Error(
        `Work mode active tools missing 'agent_browser_tools'. Got: ${workTools.join(", ")}`,
      );
    }

    // Must NOT include legacy browser_*
    const forbiddenTools = [
      "browser_navigate",
      "browser_click",
      "browser_type",
      "browser_snapshot",
    ];
    for (const forbidden of forbiddenTools) {
      if (workTools.includes(forbidden)) {
        throw new Error(
          `Work mode active tools must not include legacy '${forbidden}'. Got: ${workTools.join(", ")}`,
        );
      }
    }
    console.log(
      "✓ Work Mode + Browser Use ON: active tools contain agent_browser* and no legacy browser_*",
    );
  } finally {
    if (prevEnv === undefined) {
      delete process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED;
    } else {
      process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED = prevEnv;
    }
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
    const targetExtPkgJson = await testAgentBrowserSmoke();
    await testBrowserUseActiveToolsSmoke(targetExtPkgJson);
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
