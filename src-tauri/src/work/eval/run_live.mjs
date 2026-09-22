#!/usr/bin/env node

/**
 * AgentCabin Computer Use V3 Real Live Evaluation Runner.
 *
 * Executes real end-to-end evaluation pipeline against native macOS applications
 * and local browser test fixtures without synthetic state injection or mock session factories.
 *
 * Usage:
 *   npm run eval:computer-use:live
 *   or: node src-tauri/src/work/eval/run_live.mjs
 */

import { spawn, execSync } from "node:child_process";
import { createServer } from "node:http";
import readline from "node:readline";
import { resolve, dirname, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";

import { ComputerUseV2Session } from "../computer_use_v2_runtime.mjs";
import { DesktopComputerUseBackend } from "../desktop_computer_use_backend.mjs";
import { CdpComputerUseBackend } from "../cdp_computer_use_backend.mjs";
import { VisualGroundingBackend } from "../visual_grounding_backend.mjs";
import { ComputerUseEvalRunner, RealComputerUseEvalSession } from "./eval_runner.mjs";
import { FAILURE_CATEGORIES } from "./eval_types.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, "../../../..");
const EVAL_FIXTURE_ROOT = resolve(REPO_ROOT, "fixtures/computer-use");

const FIXTURE_CONTENT_TYPES = Object.freeze({
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
});

function fixturePathFromRequest(requestUrl) {
  let pathname;
  try {
    pathname = decodeURIComponent(new URL(requestUrl, "http://127.0.0.1").pathname);
  } catch {
    return undefined;
  }
  const prefix = "/fixtures/computer-use/";
  if (!pathname.startsWith(prefix)) return undefined;
  const relative = pathname.slice(1);
  const filePath = resolve(REPO_ROOT, relative);
  if (!filePath.startsWith(`${EVAL_FIXTURE_ROOT}/`) || relative.includes("..")) return undefined;
  return filePath;
}

/**
 * Serve only the checked-in Computer Use fixtures from an ephemeral loopback
 * origin. Browser Runtime's normal SSRF policy stays closed; the live eval
 * passes this exact origin through its explicit eval-only allowlist.
 */
export async function createEvalFixtureServer() {
  const server = createServer(async (request, response) => {
    const filePath = fixturePathFromRequest(request.url || "");
    if (request.method !== "GET" || !filePath) {
      response.writeHead(404);
      response.end("Not found");
      return;
    }
    try {
      const body = await readFile(filePath);
      response.writeHead(200, {
        "content-type": FIXTURE_CONTENT_TYPES[extname(filePath)] || "application/octet-stream",
        "cache-control": "no-store",
      });
      response.end(body);
    } catch {
      response.writeHead(404);
      response.end("Not found");
    }
  });
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  const origin = `http://127.0.0.1:${address.port}`;
  return {
    origin,
    urlFor(fixture) {
      const value = String(fixture || "").trim();
      if (!value.startsWith("fixtures/computer-use/") || value.includes("..")) {
        throw new Error(`Unsupported live eval fixture path: ${value}`);
      }
      return `${origin}/${value}`;
    },
    async close() {
      if (!server.listening) return;
      await new Promise((resolveClose) => server.close(() => resolveClose()));
    },
  };
}

export const LIVE_EVAL_CASES = Object.freeze([
  // Calculator 1
  {
    id: "live-calc-add",
    title: "Calculator: 12 + 30 = 42",
    category: "desktop_calculation",
    targetApp: "Calculator",
    prompt: "Open Calculator, calculate 12 + 30, and verify display equals 42.",
    actions: [
      { action: "click", targetTitle: "1" },
      { action: "click", targetTitle: "2" },
      { action: "click", targetTitle: "+" },
      { action: "click", targetTitle: "3" },
      { action: "click", targetTitle: "0" },
      { action: "click", targetTitle: "=" },
    ],
    groundTruthOutcome: {
      expect: { text: "42" },
    },
  },
  // Calculator 2
  {
    id: "live-calc-mult",
    title: "Calculator: 7 × 8 = 56",
    category: "desktop_calculation",
    targetApp: "Calculator",
    prompt: "Calculate 7 multiplied by 8, and verify result is 56.",
    actions: [
      { action: "click", targetTitle: "7" },
      { action: "click", targetTitle: "×" },
      { action: "click", targetTitle: "8" },
      { action: "click", targetTitle: "=" },
    ],
    groundTruthOutcome: {
      expect: { text: "56" },
    },
  },
  // Calculator 3
  {
    id: "live-calc-clear",
    title: "Calculator: Clear resets value",
    category: "desktop_calculation",
    targetApp: "Calculator",
    prompt: "Press Clear to reset the calculator display to 0.",
    actions: [
      { action: "click", targetTitle: "5" },
      { action: "click", targetTitle: "All Clear" },
    ],
    groundTruthOutcome: {
      expect: { text: "0" },
    },
  },
  // TextEdit 1
  {
    id: "live-textedit-open",
    title: "TextEdit: Open blank document",
    category: "desktop_editor",
    targetApp: "TextEdit",
    prompt: "Launch TextEdit and observe the blank editor document window.",
    actions: [],
    groundTruthOutcome: {
      expect: { role: "root" },
    },
  },
  // TextEdit 2
  {
    id: "live-textedit-type",
    title: "TextEdit: Type known text",
    category: "desktop_editor",
    targetApp: "TextEdit",
    prompt: "Type 'AgentCabin Live Eval' into the active TextEdit document.",
    actions: [
      { action: "typeText", text: "AgentCabin Live Eval" },
    ],
    groundTruthOutcome: {
      expect: { text: "AgentCabin Live Eval" },
    },
  },
  // TextEdit 3
  {
    id: "live-textedit-verify",
    title: "TextEdit: Verify text state without file persistence",
    category: "desktop_editor",
    targetApp: "TextEdit",
    prompt: "Read active document text and verify it retains 'AgentCabin Live Eval'.",
    actions: [],
    groundTruthOutcome: {
      expect: { text: "AgentCabin Live Eval" },
    },
  },
  // Chrome 1 (Form Submit)
  {
    id: "live-chrome-form",
    title: "Chrome: Form type + submit",
    category: "browser_form",
    targetApp: "browser",
    fixture: "fixtures/computer-use/form.html",
    prompt: "Navigate to form.html, type 'qa_tester' into username, submit, and verify status.",
    actions: [
      { action: "typeText", targetTitle: "Username", text: "qa_tester" },
      { action: "click", targetTitle: "Submit Form" },
    ],
    groundTruthOutcome: {
      expect: { text: "Submitted: qa_tester" },
    },
  },
  // Chrome 2 (Delayed Postcondition)
  {
    id: "live-chrome-delayed",
    title: "Chrome: Delayed async postcondition",
    category: "browser_async",
    targetApp: "browser",
    fixture: "fixtures/computer-use/dynamic.html",
    prompt: "Click 'Load Message' button and wait for delayed message to resolve.",
    actions: [
      { action: "click", targetTitle: "Load Message" },
    ],
    groundTruthOutcome: {
      expect: { text: "Delayed message loaded successfully", timeoutMs: 2000 },
    },
  },
  // Chrome 3 (Visual / Canvas Fallback)
  {
    id: "live-chrome-canvas",
    title: "Chrome: Self-drawn canvas button click",
    category: "visual_grounding",
    targetApp: "browser",
    fixture: "fixtures/computer-use/canvas.html",
    prompt: "Identify and click the self-drawn Confirm Action button inside HTML5 canvas.",
    actions: [
      { action: "click", targetTitle: "Confirm Action" },
    ],
    groundTruthOutcome: {
      expect: { text: "Canvas button clicked" },
    },
  },
  // Cross-App 1 (Calculator -> TextEdit)
  {
    id: "live-cross-app-calc-to-textedit",
    title: "Cross-App: Calculator compute -> read display -> TextEdit type -> verify",
    category: "cross_app_orchestration",
    targetApp: "Calculator",
    secondApp: "TextEdit",
    prompt: "Compute 9 × 9 = 81 in Calculator, transfer the result into TextEdit, and verify.",
    actions: [
      { action: "click", targetTitle: "9" },
      { action: "click", targetTitle: "×" },
      { action: "click", targetTitle: "9" },
      { action: "click", targetTitle: "=" },
    ],
    groundTruthOutcome: {
      expect: { text: "81" },
    },
    crossApp: {
      targetApp: "TextEdit",
      sourceExpect: { text: "81" },
      actions: [{ action: "typeText", text: "{{transferText}}" }],
      expect: { text: "81" },
    },
  },
]);

/**
 * Creates live desktop invoker connected to native macOS bridge.
 */
export function createLiveDesktopInvoker(options = {}) {
  const bridgeBin = resolve(REPO_ROOT, "src-tauri/resources/agentcabin-computer-use/macos/bridge");

  return async (toolCallId, toolName, action, params = {}, signal) => {
    // 1. If running under AgentCabin Host Desktop Bridge
    const bridgePort = Number(process.env.AGENTCABIN_DESKTOP_BRIDGE_PORT || 0);
    const bridgeToken = String(process.env.AGENTCABIN_DESKTOP_BRIDGE_TOKEN || "").trim();
    if (options.hostOnly && (!bridgePort || !bridgeToken)) {
      return { success: false, stderr: "Host-only live eval requires AGENTCABIN_DESKTOP_BRIDGE_PORT and AGENTCABIN_DESKTOP_BRIDGE_TOKEN." };
    }
    if (bridgePort > 0 && bridgeToken) {
      try {
        const res = await fetch(`http://127.0.0.1:${bridgePort}/internal/desktop/call`, {
          method: "POST",
          headers: {
            authorization: `Bearer ${bridgeToken}`,
            "content-type": "application/json",
          },
          body: JSON.stringify({ toolCallId, method: toolName, params }),
          signal,
        });
        const data = await res.json();
        return res.ok ? data : { success: false, stderr: data?.error || `Bridge returned ${res.status}` };
      } catch (err) {
        return { success: false, stderr: err instanceof Error ? err.message : String(err) };
      }
    }

    // 2. Direct native bridge invocation via stdio JSON-RPC
    if (!existsSync(bridgeBin)) {
      return { success: false, stderr: `Desktop bridge binary not found at ${bridgeBin}` };
    }

    const invokeBridgeRpc = (cmd, payload = {}) => {
      return new Promise((resolveResult) => {
        const proc = spawn(bridgeBin, [], { stdio: ["pipe", "pipe", "pipe"] });
        let stdout = "";
        let stderr = "";
        proc.stdout.on("data", (d) => { stdout += d.toString(); });
        proc.stderr.on("data", (d) => { stderr += d.toString(); });

        proc.on("error", (err) => {
          resolveResult({ success: false, stderr: `Bridge execution failed: ${err.message}` });
        });

        proc.on("close", (code) => {
          if (code !== 0 && !stdout) {
            resolveResult({ success: false, stderr: stderr || `Bridge exited with code ${code}` });
            return;
          }
          try {
            const parsed = JSON.parse(stdout.trim());
            if (parsed.ok) {
              resolveResult({ success: true, structuredContent: parsed.result });
            } else {
              resolveResult({ success: false, stderr: parsed.error?.message || "Bridge returned error" });
            }
          } catch {
            resolveResult({ success: false, stderr: `Invalid JSON from bridge: ${stdout.slice(0, 200)}` });
          }
        });

        const req = JSON.stringify({ id: `rpc-${Date.now()}`, cmd, ...payload }) + "\n";
        proc.stdin.write(req);
        proc.stdin.end();
      });
    };

    try {
      if (toolName === "desktop_list_apps") {
        return await invokeBridgeRpc("listRoots", params);
      }
      if (toolName === "desktop_observe" || toolName === "desktop_screenshot") {
        return await invokeBridgeRpc("look", {
          rootRef: params.root_ref || params.rootRef,
          depth: 3,
          includeScreenshot: true,
          ...params,
        });
      }
      if (toolName === "desktop_act_batch") {
        return await invokeBridgeRpc("actBatch", {
          rootRef: params.root_ref || params.rootRef,
          actions: params.actions || [],
          ...params,
        });
      }
      if (toolName === "desktop_open_app") {
        const appName = params.name || params.bundleId || "";
        // Check if already in listRoots
        const listRes = await invokeBridgeRpc("listRoots");
        const existingRoots = Array.isArray(listRes?.structuredContent?.windows)
          ? listRes.structuredContent.windows
          : (Array.isArray(listRes?.structuredContent) ? listRes.structuredContent : []);
        const existing = existingRoots.find((w) => {
          const name = String(w.appName || w.app_name || w.owner || "").toLowerCase();
          return name === appName.toLowerCase() || name.includes(appName.toLowerCase());
        });
        if (existing) {
          return {
            success: true,
            structuredContent: {
              pid: existing.pid,
              window_id: existing.windowId || existing.window_id,
              root_ref: existing.rootRef || existing.root_ref,
              app_name: existing.appName || existing.app_name || appName,
              title: existing.title || "",
            },
          };
        }

        // Try launching via open
        try {
          const { execFileSync } = await import("node:child_process");
          execFileSync("/usr/bin/open", ["-a", appName], { stdio: "pipe", timeout: 3000 });
        } catch (launchErr) {
          const msg = launchErr.stderr ? String(launchErr.stderr) : launchErr.message;
          return { success: false, stderr: `Failed to open application '${appName}': ${msg}` };
        }

        // Poll listRoots for window
        for (let attempt = 0; attempt < 15; attempt++) {
          await new Promise((r) => setTimeout(r, 200));
          const pollRes = await invokeBridgeRpc("listRoots");
          const roots = Array.isArray(pollRes?.structuredContent?.windows)
            ? pollRes.structuredContent.windows
            : (Array.isArray(pollRes?.structuredContent) ? pollRes.structuredContent : []);
          const match = roots.find((w) => {
            const name = String(w.appName || w.app_name || w.owner || "").toLowerCase();
            return name === appName.toLowerCase() || name.includes(appName.toLowerCase());
          });
          if (match) {
            return {
              success: true,
              structuredContent: {
                pid: match.pid,
                window_id: match.windowId || match.window_id,
                root_ref: match.rootRef || match.root_ref,
                app_name: match.appName || match.app_name || appName,
                title: match.title || "",
              },
            };
          }
        }
        return { success: false, stderr: `Application '${appName}' launched but no window found in listRoots.` };
      }

      return await invokeBridgeRpc("diagnostics", params);
    } catch (err) {
      return { success: false, stderr: err instanceof Error ? err.message : String(err) };
    }
  };
}

/**
 * Creates live browser worker invoker connected to browser-worker/server.mjs.
 */
export function parseBrowserBridgeResponse(payload, responseOk = true, status = 200) {
  if (!responseOk) {
    return {
      ok: false,
      error: payload?.error || payload?.stderr || `Browser bridge returned ${status}`,
    };
  }
  if (typeof payload?.stdout === "string" && payload.stdout.trim()) {
    try {
      return JSON.parse(payload.stdout);
    } catch {
      // Preserve a human-readable successful wrapper for compatibility.
    }
  }
  return payload;
}

function prepareEvalBrowserParams(toolName, params, evalFixtureOrigin) {
  if (!evalFixtureOrigin || !params || typeof params !== "object") return params;
  const url = typeof params.url === "string" ? params.url : "";
  const isNavigating = toolName === "browser_navigate"
    || (toolName === "browser_tabs" && params.action === "new");
  if (!isNavigating || !url.startsWith(`${evalFixtureOrigin}/`)) return params;
  return { ...params, __evalFixtureOrigin: evalFixtureOrigin };
}

export function createLiveBrowserInvoker(options = {}) {
  const serverScript = resolve(REPO_ROOT, "src-tauri/browser-worker/server.mjs");
  const evalFixtureOrigin = String(options.evalFixtureOrigin || "").trim().replace(/\/$/, "");
  const fetchImpl = options.fetchImpl || globalThis.fetch;

  if (options.hostOnly) {
    return async (_toolCallId, toolName, params = {}, signal) => {
      const bridgePort = Number(options.bridgePort ?? process.env.AGENTCABIN_BROWSER_BRIDGE_PORT ?? 0);
      const bridgeToken = String(options.bridgeToken ?? process.env.AGENTCABIN_BROWSER_BRIDGE_TOKEN ?? "").trim();
      if (!bridgePort || !bridgeToken) {
        return { ok: false, error: "Host-only live eval requires AGENTCABIN_BROWSER_BRIDGE_PORT and AGENTCABIN_BROWSER_BRIDGE_TOKEN." };
      }
      const prepared = prepareEvalBrowserParams(toolName, params, evalFixtureOrigin);
      const evalOrigin = prepared?.__evalFixtureOrigin;
      const bridgeParams = prepared && typeof prepared === "object"
        ? Object.fromEntries(Object.entries(prepared).filter(([key]) => key !== "__evalFixtureOrigin"))
        : prepared;
      try {
        const response = await fetchImpl(`http://127.0.0.1:${bridgePort}/internal/browser/call`, {
          method: "POST",
          headers: { authorization: `Bearer ${bridgeToken}`, "content-type": "application/json" },
          body: JSON.stringify({
            toolCallId: _toolCallId,
            method: toolName,
            params: bridgeParams,
            ...(evalOrigin ? { evalFixtureOrigin: evalOrigin } : {}),
          }),
          signal,
        });
        const payload = await response.json();
        return parseBrowserBridgeResponse(payload, response.ok, response.status);
      } catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
      }
    };
  }

  let workerProc = null;
  let reqId = 1;
  const pendingRequests = new Map();

  function ensureWorker() {
    if (workerProc && !workerProc.killed) return workerProc;

    workerProc = spawn("node", [serverScript], {
      stdio: ["pipe", "pipe", "pipe"],
      env: { ...process.env, AGENTCABIN_BROWSER_HEADLESS: "1" },
    });

    const rl = readline.createInterface({ input: workerProc.stdout });
    rl.on("line", (line) => {
      try {
        const parsed = JSON.parse(line.trim());
        const cb = pendingRequests.get(parsed.id);
        if (cb) {
          pendingRequests.delete(parsed.id);
          cb(parsed);
        }
      } catch {}
    });

    workerProc.on("exit", () => {
      for (const cb of pendingRequests.values()) {
        cb({ ok: false, error: "Browser worker exited unexpectedly." });
      }
      pendingRequests.clear();
      workerProc = null;
    });

    return workerProc;
  }

  return async (toolCallId, toolName, params = {}, signal) => {
    const worker = ensureWorker();
    const id = reqId++;
    const prepared = prepareEvalBrowserParams(toolName, params, evalFixtureOrigin);
    const evalOrigin = prepared?.__evalFixtureOrigin;
    const workerParams = prepared && typeof prepared === "object"
      ? {
        ...prepared,
        ...(evalOrigin ? { allowOrigin: evalOrigin } : {}),
      }
      : prepared;
    if (workerParams && typeof workerParams === "object") delete workerParams.__evalFixtureOrigin;

    return await new Promise((resolvePromise) => {
      pendingRequests.set(id, (resp) => {
        if (resp.ok) {
          resolvePromise(resp.result ?? { ok: true });
        } else {
          resolvePromise({ error: resp.error || "Browser command failed", ok: false });
        }
      });

      const req = JSON.stringify({
        id,
        runId: "live-eval-run",
        method: toolName,
        params: workerParams,
      }) + "\n";

      try {
        worker.stdin.write(req);
      } catch (err) {
        resolvePromise({ error: err.message, ok: false });
      }
    });
  };
}

/**
 * Performs cleanup for evaluation isolation.
 */
export async function cleanupEvalEnvironment() {
  try {
    if (process.platform === "darwin") {
      // Close temporary TextEdit windows without saving
      try {
        execSync("osascript -e 'tell application \"TextEdit\" to close (every window whose saved is false) saving no' 2>/dev/null || true");
      } catch {}
      // Quit Calculator
      try {
        execSync("killall Calculator 2>/dev/null || true");
      } catch {}
    }
  } catch {}
}

/**
 * Runs the live evaluation suite and returns structured report.
 */
export async function runLiveEval(options = {}) {
  const fixtureServer = options.fixtureServer || await createEvalFixtureServer();
  try {
    return await runLiveEvalWithFixtureServer(options, fixtureServer);
  } finally {
    if (!options.fixtureServer) await fixtureServer.close();
  }
}

async function runLiveEvalWithFixtureServer(options, fixtureServer) {
  const cases = options.cases || LIVE_EVAL_CASES;
  console.log("===============================================================================");
  console.log("               AgentCabin Computer Use V3 — Live Evaluation Runner             ");
  console.log("===============================================================================");
  const requireHostBridge = Boolean(options.requireHostBridge);
  console.log(`Starting execution of ${cases.length} deterministic live cases...`);
  console.log(`Execution mode: ${requireHostBridge ? "host bridge" : "runtime direct"}`);
  console.log(`Platform: ${process.platform} (${process.arch})`);
  console.log("-------------------------------------------------------------------------------");

  const desktopInvoker = options.desktopInvoker || createLiveDesktopInvoker({ hostOnly: requireHostBridge });
  const browserInvoker = options.browserInvoker || createLiveBrowserInvoker({
    hostOnly: requireHostBridge,
    evalFixtureOrigin: fixtureServer.origin,
  });

  const session = new ComputerUseV2Session(desktopInvoker, {
    invokeBrowser: browserInvoker,
    visualGrounding: options.visualGrounding || {},
  });

  const runner = new ComputerUseEvalRunner({
    cases,
    sessionFactory: (caseDef) => new RealComputerUseEvalSession({
      session,
      caseDef,
      fixtureBaseUrl: fixtureServer.origin,
    }),
  });

  const results = [];
  const failureDistribution = {};
  let totalToolCalls = 0;
  let totalLatencyMs = 0;
  let totalWrongClicks = 0;
  let firstAttemptSuccessCount = 0;
  let visualFallbackCount = 0;
  let visualFallbackSuccessCount = 0;
  let staleStateCount = 0;

  for (let i = 0; i < cases.length; i++) {
    const caseDef = cases[i];
    console.log(`[${i + 1}/${cases.length}] Running case: ${caseDef.id} — "${caseDef.title}"`);

    // Teardown before case
    await cleanupEvalEnvironment();

    const caseRes = await runner.runCase(caseDef);
    results.push(caseRes);

    totalToolCalls += caseRes.toolCalls;
    totalLatencyMs += caseRes.latencyMs;
    totalWrongClicks += caseRes.wrongClicks;

    if (caseRes.passed) {
      firstAttemptSuccessCount++;
      if (caseDef.category === "visual_grounding") {
        visualFallbackCount++;
        visualFallbackSuccessCount++;
      }
      console.log(`  -> PASS (${caseRes.latencyMs}ms, ${caseRes.toolCalls} tool calls)`);
    } else {
      const cat = caseRes.failureCategory || FAILURE_CATEGORIES.ACTION_REJECTED;
      failureDistribution[cat] = (failureDistribution[cat] || 0) + 1;
      if (cat === FAILURE_CATEGORIES.STALE_STATE) staleStateCount++;
      if (caseDef.category === "visual_grounding") visualFallbackCount++;

      console.log(`  -> FAIL [${cat}] (${caseRes.latencyMs}ms): ${caseRes.error}`);
    }

    // Teardown after case
    await cleanupEvalEnvironment();
  }

  const totalCases = results.length;
  const passed = results.filter((r) => r.passed).length;
  const failed = totalCases - passed;
  const successRate = totalCases > 0 ? Number(((passed / totalCases) * 100).toFixed(2)) : 0;
  const wrongClickRate = totalToolCalls > 0 ? Number(((totalWrongClicks / totalToolCalls) * 100).toFixed(2)) : 0;
  const avgToolCalls = totalCases > 0 ? Number((totalToolCalls / totalCases).toFixed(2)) : 0;
  const avgLatencyMs = totalCases > 0 ? Math.round(totalLatencyMs / totalCases) : 0;
  const firstAttemptSuccessRate = totalCases > 0 ? Number(((firstAttemptSuccessCount / totalCases) * 100).toFixed(2)) : 0;
  const visualFallbackSuccessRate = visualFallbackCount > 0 ? Number(((visualFallbackSuccessCount / visualFallbackCount) * 100).toFixed(2)) : 0;

  const summary = {
    timestamp: new Date().toISOString(),
    totalCases,
    passed,
    failed,
    successRate,
    wrongClickRate,
    avgToolCalls,
    avgLatencyMs,
    firstAttemptSuccessRate,
    recoveryCount: 0,
    visualFallbackCount,
    visualFallbackSuccessRate,
    staleStateCount,
    executionMode: requireHostBridge ? "host" : "runtime",
    failureDistribution,
    results,
  };

  console.log("-------------------------------------------------------------------------------");
  console.log("                               Live Eval Report Summary                        ");
  console.log("-------------------------------------------------------------------------------");
  console.log(`Total Cases:                   ${totalCases}`);
  console.log(`Passed:                        ${passed}`);
  console.log(`Failed:                        ${failed}`);
  console.log(`Success Rate:                  ${successRate}%`);
  console.log(`Wrong Click Rate:              ${wrongClickRate}%`);
  console.log(`Avg Tool Calls / Case:         ${avgToolCalls}`);
  console.log(`Avg Latency / Case:            ${avgLatencyMs}ms`);
  console.log(`First Attempt Success Rate:    ${firstAttemptSuccessRate}%`);
  console.log(`Visual Fallback Count:         ${visualFallbackCount}`);
  console.log(`Visual Fallback Success Rate:  ${visualFallbackSuccessRate}%`);
  console.log(`Failure Distribution:          ${JSON.stringify(failureDistribution, null, 2)}`);
  console.log("===============================================================================");

  return summary;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  runLiveEval({ requireHostBridge: process.argv.includes("--host") }).catch((err) => {
    console.error("Live evaluation runner failed:", err);
    process.exit(1);
  });
}
