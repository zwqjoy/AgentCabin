import readline from "node:readline";

import { generatePageSnapshot } from "./snapshot.mjs";
import { createEmbeddedCdpClient } from "./embedded_cdp.mjs";
import { createEmbeddedPage } from "./embedded_page.mjs";

/**
 * Browser Worker for the built-in browser.
 *
 * Electron owns Chromium and exposes a page-level CDP session through the
 * authenticated loopback relay. This worker is only a stdio JSON-RPC to raw
 * CDP adapter; it never launches or imports Playwright.
 */

const embeddedSessions = new Map();

async function getEmbeddedSession(runId, spec) {
  if (!spec?.cdpEndpoint || !spec?.cdpToken) {
    throw new Error("Embedded browser requires cdpEndpoint and cdpToken");
  }

  const existing = embeddedSessions.get(runId);
  if (existing?.client.isReady && existing.endpoint === spec.cdpEndpoint) return existing;

  existing?.client.close();
  const client = createEmbeddedCdpClient({
    endpoint: spec.cdpEndpoint,
    token: spec.cdpToken,
  });
  await client.connect();
  // Explicitly initialize the CDP domains used by the page adapter for each
  // fresh Electron debugger session.
  await client.send("Runtime.enable");
  await client.send("Page.enable");
  const page = createEmbeddedPage({ client });
  await page.refreshLocation();

  const session = {
    client,
    page,
    endpoint: spec.cdpEndpoint,
    targetId: spec.cdpTargetId || "",
  };
  embeddedSessions.set(runId, session);
  return session;
}

function imageBlock(dataUrl) {
  if (typeof dataUrl !== "string" || !dataUrl.startsWith("data:")) return [];
  const comma = dataUrl.indexOf(",");
  if (comma < 0) return [];
  const header = dataUrl.slice(5, comma);
  const [mimeType] = header.split(";");
  return [{ type: "image", data: dataUrl.slice(comma + 1), mimeType: mimeType || "image/jpeg" }];
}

async function relayObservation(page, root = {}) {
  const snapshot = await generatePageSnapshot(page);
  const elements = Object.entries(snapshot.refs || {}).map(([ref, item]) => {
    const role = String(item.role || item.tag || "element").toLowerCase();
    const label = String(item.name || item.title || item.label || "").trim();
    return {
      ref,
      nativeRef: ref,
      legacyRef: ref,
      backendRef: ref,
      role,
      title: label,
      name: label,
      label,
      value: item.value === undefined ? undefined : String(item.value),
      tag: item.tag,
      rect: item.rect,
    };
  });
  return {
    ok: true,
    backend: "cdp",
    title: snapshot.title || root.title || "",
    url: snapshot.url || root.url || "",
    elements,
    images: imageBlock(await page.screenshot()),
    platformState: { browserTargetId: root.browserTargetId || root.cdpTargetId, url: snapshot.url },
  };
}

async function relayAct(page, params = {}) {
  const steps = [];
  let stoppedAt;
  for (let index = 0; index < (params.actions || []).length; index++) {
    const action = params.actions[index] || {};
    try {
      if (action.action === "click" || action.action === "press") {
        if (action.x !== undefined && action.y !== undefined) await page.clickAt(action.x, action.y);
        else if (action.ref) await page.clickRef(String(action.ref).replace(/^@/, ""));
        else throw new Error("CDP click requires a ref or coordinates.");
      } else if (action.action === "setText" || action.action === "typeText") {
        if (action.ref) await page.clickRef(String(action.ref).replace(/^@/, ""));
        await page.typeText(String(action.text || ""));
      } else if (action.action === "keypress") {
        const key = Array.isArray(action.keys) ? action.keys.at(-1) : action.key;
        if (!key) throw new Error("keypress requires a key.");
        await page.pressKey(key);
      } else if (action.action === "scroll") {
        await page.scroll(Number(action.scrollY || 0), Number(action.scrollX || 0));
      } else {
        throw new Error(`CDP action '${action.action}' is not supported.`);
      }
      steps.push({ outcome: "worked" });
    } catch (error) {
      steps.push({ outcome: "didnt", error: error instanceof Error ? error.message : String(error) });
      stoppedAt = index;
      break;
    }
  }
  const outcome = steps.some((step) => step.outcome === "didnt") ? "didnt" : "worked";
  return { ok: true, outcome, execution: { outcome, steps, ...(stoppedAt === undefined ? {} : { stoppedAt }) }, observation: await relayObservation(page, params) };
}

async function handleEmbedded(method, runId, params = {}) {
  const session = await getEmbeddedSession(runId, params);
  const page = session.page;

  switch (method) {
    case "browser_attach_embedded":
      return { ok: true, endpoint: session.endpoint, targetId: session.targetId };
    case "browser_navigate": {
      await page.navigate(params.url);
      const snapshot = await generatePageSnapshot(page);
      return { ok: true, url: snapshot.url, title: snapshot.title, screenshot: await page.screenshot() };
    }
    case "browser_snapshot": {
      const snapshot = await generatePageSnapshot(page);
      return {
        ok: true,
        title: snapshot.title,
        url: snapshot.url,
        tree: snapshot.tree,
        refsCount: Object.keys(snapshot.refs).length,
        screenshot: await page.screenshot(),
      };
    }
    case "browser_take_screenshot":
      return { ok: true, screenshot: await page.screenshot() };
    case "browser_cdp_observe":
      return relayObservation(page, params);
    case "browser_cdp_act":
      return relayAct(page, params);
    case "browser_click":
      if (params.x !== undefined && params.y !== undefined) await page.clickAt(params.x, params.y);
      else if (params.ref) await page.clickRef(params.ref);
      else if (params.selector) await page.clickSelector(params.selector);
      else throw new Error("Embedded click requires a ref, selector, or coordinates");
      return { ok: true, ...(await generatePageSnapshot(page)) };
    case "browser_type":
      if (params.ref) await page.clickRef(params.ref);
      else if (params.selector) await page.clickSelector(params.selector);
      await page.typeText(params.text ?? "");
      if (params.pressEnter) await page.pressKey("Enter");
      return { ok: true };
    case "browser_scroll": {
      const delta = params.deltaY ?? (
        params.direction === "up" ? -Math.abs(params.amount ?? 500) : Math.abs(params.amount ?? 500)
      );
      await page.scroll(delta, params.deltaX ?? 0);
      return { ok: true, url: await page.refreshLocation(), title: await page.title() };
    }
    case "browser_interact": {
      if (params.action === "navigate") await page.navigate(params.url);
      else if (params.action === "go_back") await page.goBack();
      else if (params.action === "go_forward") await page.goForward();
      else if (params.action === "reload") await page.reload();
      else if (params.action === "click") await page.clickAt(params.x, params.y);
      else if (params.action === "scroll") await page.scroll(params.deltaY ?? 300, params.deltaX ?? 0);
      else throw new Error(`unsupported_action: ${params.action} is not available on the embedded browser surface`);
      const snapshot = await generatePageSnapshot(page);
      return {
        ok: true,
        action: params.action,
        url: snapshot.url,
        title: snapshot.title,
        screenshot: await page.screenshot(),
      };
    }
    case "browser_tabs":
      return {
        ok: true,
        action: "list",
        tabs: [{
          id: session.targetId || "embedded",
          targetId: session.targetId || "embedded",
          index: 0,
          url: await page.refreshLocation(),
          title: await page.title(),
          active: true,
        }],
      };
    case "browser_focus":
      return { ok: true, url: await page.refreshLocation(), title: await page.title() };
    case "browser_wait_for":
      if (params.ms) await new Promise((resolve) => setTimeout(resolve, Math.min(Number(params.ms), 30000)));
      return { ok: true, url: await page.refreshLocation(), title: await page.title() };
    case "browser_close":
      page.close();
      embeddedSessions.delete(runId);
      return { ok: true, runId, closed: true };
    case "browser_select_option":
      throw new Error("unsupported_action: browser_select_option is not available on the embedded browser surface");
    default:
      throw new Error(`unsupported_action: ${method} is not available on the embedded browser surface`);
  }
}

async function dispatch(method, runId, params = {}) {
  if (method === "ping") return { ok: true, pong: Date.now() };
  if (!params?.cdpEndpoint) {
    throw new Error("The built-in browser requires an Electron CDP relay; standalone browser sessions are no longer supported.");
  }
  return handleEmbedded(method, runId, params);
}

async function closeAll() {
  for (const session of embeddedSessions.values()) session.client.close();
  embeddedSessions.clear();
}

const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });

rl.on("line", async (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msgId = null;
  try {
    const parsed = JSON.parse(trimmed);
    msgId = parsed.id;
    const { method, runId, params } = parsed;
    if (!runId && method !== "ping") throw new Error("Missing runId in request");
    const result = await dispatch(method, runId, params || {});
    process.stdout.write(`${JSON.stringify({ id: msgId, ok: true, result })}\n`);
  } catch (error) {
    process.stdout.write(`${JSON.stringify({
      id: msgId,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    })}\n`);
  }
});

async function shutdown() {
  await closeAll();
  process.exit(0);
}

for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, shutdown);
rl.on("close", shutdown);
