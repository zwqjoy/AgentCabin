#!/usr/bin/env node

/**
 * End-to-end packaging pipeline for AgentCabin Electron.
 *
 * Steps:
 * 1. Build renderer (SvelteKit static build -> build/)
 * 2. Build Electron main & preload bundles (esbuild -> electron/dist/)
 * 3. Build Rust core in release mode (cargo build --release --bin AgentCabin)
 * 4. Run electron-builder to generate DMG / ZIP / app bundle
 *
 * Usage:
 *   node scripts/package-electron.mjs               # Full release build
 *   node scripts/package-electron.mjs --dir         # Fast unpacked app bundle
 *   node scripts/package-electron.mjs --skip-core   # Use existing target/release/AgentCabin
 *   node scripts/package-electron.mjs --debug-core  # Reuse target/debug/AgentCabin
 */

import { spawnSync } from "node:child_process";
import { existsSync, copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);

function runStep(name, cmd, cmdArgs, opts = {}) {
  console.log(`\n▶ [package-electron] Step: ${name}`);
  const res = spawnSync(cmd, cmdArgs, {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
    ...opts,
  });
  if (res.status !== 0) {
    console.error(`✖ [package-electron] ${name} failed with code ${res.status}`);
    process.exit(res.status ?? 1);
  }
}

const skipVite = args.includes("--skip-vite");
const skipCore = args.includes("--skip-core");
const debugCore = args.includes("--debug-core");
const builderArgs = args.filter((a) => !["--skip-vite", "--skip-core", "--debug-core"].includes(a));

// 1. Build Renderer
if (!skipVite) {
  runStep("Build frontend", "npm", ["run", "build"]);
} else {
  console.log("⏭ Skipping frontend build (--skip-vite)");
}

// 2. Build Electron scripts
runStep("Build Electron main & preload", "node", ["scripts/build-electron.mjs"]);

// 3. Build Rust core
const releaseCoreBin = path.join(root, "src-tauri", "target", "release", "AgentCabin");
const debugCoreBin = path.join(root, "src-tauri", "target", "debug", "AgentCabin");

if (debugCore) {
  console.log("\n▶ [package-electron] Using debug core binary (--debug-core)");
  if (!existsSync(debugCoreBin)) {
    runStep("Build debug core", "cargo", ["build", "--bin", "AgentCabin", "--manifest-path", "src-tauri/Cargo.toml"]);
  }
  mkdirSync(path.dirname(releaseCoreBin), { recursive: true });
  copyFileSync(debugCoreBin, releaseCoreBin);
  console.log(`Copied ${debugCoreBin} -> ${releaseCoreBin}`);
} else if (skipCore) {
  console.log("⏭ Skipping core build (--skip-core)");
  if (!existsSync(releaseCoreBin)) {
    console.error(`✖ Release core binary not found at ${releaseCoreBin}. Run without --skip-core first.`);
    process.exit(1);
  }
} else {
  runStep("Build release core", "cargo", [
    "build",
    "--release",
    "--bin",
    "AgentCabin",
    "--manifest-path",
    "src-tauri/Cargo.toml",
  ]);
}

// 4. Prepare and verify the immutable runtime closure before packaging.
runStep("Prepare bundled runtime closure", "npm", ["run", "prepare:runtimes"]);
runStep("Verify bundled runtime closure", "npm", ["run", "verify:runtimes"]);
// 5. Run electron-builder
const defaultBuilderFlags = builderArgs.length > 0 ? builderArgs : ["--mac"];
runStep("electron-builder", "npx", ["electron-builder", ...defaultBuilderFlags]);

// 6. Smoke test packaged runtimes
runStep("Smoke test packaged runtimes", "npm", ["run", "smoke:packaged-runtimes"], {
  env: {
    ...process.env,
    AGENTCABIN_REQUIRE_PACKAGED: "1",
  },
});

console.log("\n✔ [package-electron] Packaging complete! Artifacts in dist-electron/\n");
