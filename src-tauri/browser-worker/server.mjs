import readline from "node:readline";

import { generatePageSnapshot } from "./snapshot.mjs";
import { structuredAction } from "./action_result.mjs";
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

  let runSession = embeddedSessions.get(runId);
  if (!runSession) {
    runSession = { sessionsByTarget: new Map(), activeSession: null };
    embeddedSessions.set(runId, runSession);
  }
  const targetId = spec.cdpTargetId || spec.cdpTargetID || spec.cdpEndpoint;
  const existing = runSession.sessionsByTarget.get(targetId);
  if (existing?.client.isReady && existing.endpoint === spec.cdpEndpoint) {
    runSession.activeSession = existing;
    return existing;
  }

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
    runSession,
    endpoint: spec.cdpEndpoint,
    targetId: spec.cdpTargetId || spec.cdpTargetID || "",
  };
  runSession.sessionsByTarget.set(targetId, session);
  runSession.activeSession = session;
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

async function optionalScreenshot(page) {
  try {
    const screenshot = await page.screenshot();
    return screenshot ? { screenshot } : {};
  } catch (error) {
    return {
      screenshotError: error instanceof Error ? error.message : String(error),
    };
  }
}

async function relayObservation(page, root = {}) {
  const screenshot = await optionalScreenshot(page);
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
      checked: item.checked,
      selectedLabel: item.selectedLabel,
      disabled: item.disabled,
      readOnly: item.readOnly,
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
    images: imageBlock(screenshot.screenshot),
    ...(screenshot.screenshotError ? { screenshotError: screenshot.screenshotError } : {}),
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

function actionLocator(params, beforeSnapshot) {
  if (params.locator) return params.locator;
  const ref = String(params.ref || "").replace(/^@/, "");
  if (!ref) return undefined;
  const target = beforeSnapshot.refs?.[ref];
  const name = String(params.target_label || target?.name || "").trim();
  if (!name) return undefined;
  return { ...(target?.role ? { role: target.role } : {}), name };
}

async function waitForPageCondition(page, expect = {}, timeoutMs = 10000) {
  const keys = [
    "url_contains",
    "text_contains",
    "text_absent",
    "selector_visible",
    "selector_absent",
  ];
  const conditions = Object.fromEntries(
    keys.filter((key) => typeof expect[key] === "string" && expect[key].length > 0)
      .map((key) => [key, expect[key]]),
  );
  if (Object.keys(conditions).length === 0) return { verified: false, checks: [] };

  const deadline = Date.now() + timeoutMs;
  let last = { matched: false, failures: [] };
  while (true) {
    last = await page.evaluate((expected) => {
      const failures = [];
      const text = document.body?.innerText || "";
      if (expected.url_contains && !location.href.includes(expected.url_contains)) {
        failures.push(`URL does not contain: ${expected.url_contains}`);
      }
      if (expected.text_contains && !text.includes(expected.text_contains)) {
        failures.push(`Page text is missing: ${expected.text_contains}`);
      }
      if (expected.text_absent && text.includes(expected.text_absent)) {
        failures.push(`Page text is still present: ${expected.text_absent}`);
      }
      if (expected.selector_visible) {
        try {
          const element = document.querySelector(expected.selector_visible);
          const style = element ? getComputedStyle(element) : null;
          const rect = element?.getBoundingClientRect();
          if (!element || !style || style.display === "none" || style.visibility === "hidden" || !rect?.width || !rect?.height) {
            failures.push(`Visible element not found: ${expected.selector_visible}`);
          }
        } catch {
          failures.push(`Invalid CSS selector: ${expected.selector_visible}`);
        }
      }
      if (expected.selector_absent) {
        try {
          if (document.querySelector(expected.selector_absent)) {
            failures.push(`Element is still present: ${expected.selector_absent}`);
          }
        } catch {
          failures.push(`Invalid CSS selector: ${expected.selector_absent}`);
        }
      }
      return { matched: failures.length === 0, failures, url: location.href, title: document.title };
    }, conditions);
    if (last.matched) return { verified: true, checks: conditions, url: last.url, title: last.title };
    if (Date.now() >= deadline) {
      return { verified: false, timedOut: true, checks: conditions, failures: last.failures, url: last.url, title: last.title };
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
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
      return {
        ok: true,
        url: snapshot.url,
        title: snapshot.title,
        ...(await optionalScreenshot(page)),
      };
    }
    case "browser_snapshot": {
      const snapshot = await generatePageSnapshot(page, { sinceRevision: params.sinceRevision });
      return {
        ok: true,
        ...snapshot,
        title: snapshot.title,
        url: snapshot.url,
        ...(snapshot.refs ? { refsCount: Object.keys(snapshot.refs).length } : {}),
        ...(params.include_screenshot === false ? {} : await optionalScreenshot(page)),
      };
    }
    case "browser_take_screenshot":
      return { ok: true, screenshot: await page.screenshot() };
    case "browser_cdp_observe":
      return relayObservation(page, params);
    case "browser_cdp_act":
      return relayAct(page, params);
    case "browser_click":
      return structuredAction(page, "click", params, async (beforeSnapshot) => {
        const locator = actionLocator(params, beforeSnapshot);
        if (params.ref || locator || params.selector) return page.clickRef(params.ref, locator, params.selector);
        if (params.x !== undefined && params.y !== undefined) {
          await page.clickAt(params.x, params.y);
          return {};
        }
        throw Object.assign(new Error("Embedded click requires a ref, semantic locator, selector, or coordinates."), { code: "target_not_found" });
      });
    case "browser_type":
      return structuredAction(page, "type", params, async (beforeSnapshot) => {
        const locator = actionLocator(params, beforeSnapshot);
        if (params.ref || locator || params.selector) {
          const resolved = await page.resolveTarget(params.ref, locator, params.selector);
          await page.clickRef(resolved.element.ref);
          if (params.clear !== false) await page.clearRef(resolved.element.ref);
          await page.typeText(params.text ?? "");
          if (params.pressEnter) await page.pressKey("Enter");
          return { target: resolved.element, resolvedBy: resolved.source };
        }
        throw Object.assign(new Error("Embedded type requires a ref, semantic locator, or selector."), { code: "target_not_found" });
      });
    case "browser_press_key":
      return structuredAction(page, "press_key", params, async (beforeSnapshot) => {
        const locator = actionLocator(params, beforeSnapshot);
        let resolved;
        if (params.ref || locator || params.selector) {
          resolved = await page.resolveTarget(params.ref, locator, params.selector);
          await page.clickRef(resolved.element.ref);
        }
        await page.pressKey(params.key || "");
        return resolved ? { target: resolved.element, resolvedBy: resolved.source } : {};
      });
    case "browser_scroll": {
      const delta = params.deltaY ?? (
        params.direction === "up" ? -Math.abs(params.amount ?? 500) : Math.abs(params.amount ?? 500)
      );
      return structuredAction(page, "scroll", params, async () => page.scroll(delta, params.deltaX ?? 0));
    }
    case "browser_select_option": {
      return structuredAction(page, "select_option", params, async (beforeSnapshot) =>
        page.selectOption(params.ref, params.selector, params.value ?? "", actionLocator(params, beforeSnapshot)));
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
        ...(await optionalScreenshot(page)),
      };
    }
    case "browser_tabs": {
      const action = params.action || "list";
      const result = await session.client.send("AgentCabin.browserTabs", {
        action,
        index: params.index,
        targetId: params.targetId ?? params.target_id ?? params.id,
        url: params.url,
      });
      if (!result?.ok || !Array.isArray(result.tabs)) {
        throw new Error(result?.error || "The embedded browser could not update its tab list.");
      }
      const openTargetIds = new Set(result.tabs.map((tab) => String(tab.targetId || tab.id || "")));
      const runSession = embeddedSessions.get(runId);
      for (const [targetId, tabSession] of runSession?.sessionsByTarget || []) {
        if (!openTargetIds.has(targetId)) {
          tabSession.client.close();
          runSession.sessionsByTarget.delete(targetId);
        }
      }
      return result;
    }
    case "browser_focus":
      return { ok: true, url: await page.refreshLocation(), title: await page.title() };
    case "browser_wait_for": {
      return structuredAction(page, "wait_for", params, async () => {
        const expectation = await waitForPageCondition(
          page,
          params.expect || {},
          Math.max(0, Math.min(Number(params.timeout_ms ?? params.timeoutMs ?? 10000), 30000)),
        );
        if (!expectation.checks.length && params.ms) {
          await new Promise((resolve) => setTimeout(resolve, Math.min(Number(params.ms), 30000)));
        }
        return expectation;
      });
    }
    case "browser_close": {
      for (const tabSession of session.runSession.sessionsByTarget.values()) tabSession.client.close();
      embeddedSessions.delete(runId);
      return { ok: true, runId, closed: true };
    }
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
  for (const runSession of embeddedSessions.values()) {
    for (const session of runSession.sessionsByTarget.values()) session.client.close();
  }
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
