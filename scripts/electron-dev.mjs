/**
 * Dev runner: starts the Vite dev server, waits for it to answer, then
 * launches Electron pointed at it. Ctrl+C tears down both.
 *
 * Usage: npm run electron:dev
 */
import { spawn, execSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import electronPath from "electron";

const root = path.dirname(fileURLToPath(import.meta.url)) + "/..";
const DEV_URL = "http://localhost:1420";

// Clean up any stale Electron main processes from prior dev runs
if (process.platform !== "win32") {
  try {
    const raw = execSync('pgrep -f "Electron.app/Contents/MacOS/Electron ." || true', { encoding: "utf8" }).trim();
    if (raw) {
      for (const pidStr of raw.split("\n")) {
        const p = parseInt(pidStr.trim(), 10);
        if (p && p !== process.pid) {
          try { process.kill(p, "SIGKILL"); } catch {}
        }
      }
    }
  } catch {}
}

// Ensure Electron main/preload build exists and is up to date
console.log("[dev] building electron main/preload bundle...");
execSync("node scripts/build-electron.mjs", { cwd: root, stdio: "inherit" });

// Always rebuild the debug core for development. A stale release binary may
// otherwise win resolution and make Rust/runtime changes appear ineffective.
const exeName = process.platform === "win32" ? "AgentCabin.exe" : "AgentCabin";
console.log("[dev] building latest Rust debug core...");
execSync("cargo build --bin AgentCabin --manifest-path src-tauri/Cargo.toml", {
  cwd: root,
  stdio: "inherit",
});

function run(name, command, args, { detached = false } = {}) {
  const child = spawn(command, args, {
    cwd: root,
    stdio: "inherit",
    detached,
    env: process.env,
    shell: process.platform === "win32",
  });
  child.on("exit", (code) => {
    if (code !== 0 && code !== null) {
      console.error(`[${name}] exited with code ${code}`);
      shutdown(code);
    }
  });
  return child;
}

const vite = run("vite", "node", ["node_modules/vite/bin/vite.js", "dev"], {
  detached: process.platform !== "win32",
});

async function waitForServer(url, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`Vite dev server did not come up at ${url}`);
}

let electron = null;
let shuttingDown = false;

function shutdown(code = 0) {
  if (shuttingDown) return;
  if (electron && !electron.killed) {
    try {
      if (process.platform !== "win32" && electron.pid) {
        process.kill(-electron.pid, "SIGTERM");
      } else {
        electron.kill("SIGTERM");
      }
    } catch {
      electron.kill("SIGTERM");
    }
  }
  if (vite && !vite.killed) {
    try {
      // Kill the whole process group (vite spawn esbuild children).
      process.kill(-vite.pid, "SIGTERM");
    } catch {
      vite.kill("SIGTERM");
    }
  }
  process.exit(code);
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

await waitForServer(DEV_URL);
console.log(`[electron] dev server ready at ${DEV_URL}`);

electron = spawn(
  electronPath,
  ["."],
  {
    cwd: root,
    stdio: "inherit",
    detached: process.platform !== "win32",
    shell: process.platform === "win32",
    env: { ...process.env, AGENTCABIN_DEV_SERVER_URL: DEV_URL },
  },
);
electron.on("exit", (code) => {
  console.log(`[electron] exited (${code})`);
  shutdown(0);
});
