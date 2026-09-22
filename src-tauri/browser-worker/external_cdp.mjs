const LOCAL_CDP_HOSTS = new Set(["localhost", "127.0.0.1", "::1"]);

function staleTarget(message) {
  return new Error(`stale_browser_target: ${message}`);
}
export function assertLocalCdpEndpoint(rawEndpoint) {
  let endpoint;
  try {
    endpoint = new URL(String(rawEndpoint || ""));
  } catch {
    throw new Error("Invalid external CDP endpoint.");
  }
  if (endpoint.protocol !== "ws:" && endpoint.protocol !== "wss:") {
    throw new Error("External CDP endpoint must use ws:// or wss://.");
  }
  const hostname = endpoint.hostname.toLowerCase().replace(/^\[|\]$/g, "");
  if (!LOCAL_CDP_HOSTS.has(hostname)) {
    throw new Error(`External CDP endpoint host '${hostname}' is not local.`);
  }
  return endpoint.toString();
}

async function pagesForBrowser(browser) {
  const contexts = typeof browser?.contexts === "function" ? browser.contexts() : [];
  const pages = [];
  for (const context of contexts || []) {
    const contextPages = typeof context?.pages === "function" ? context.pages() : [];
    pages.push(...(contextPages || []).map((page) => ({ page, context })));
  }
  return pages;
}

async function targetIdForPage(page, context) {
  try {
    const cdpSession = await context.newCDPSession(page);
    const targetInfo = await cdpSession.send("Target.getTargetInfo");
    return String(targetInfo?.targetInfo?.targetId || "");
  } catch {
    return "";
  }
}

/**
 * Resolve an external Electron page without ever treating a stale target id
 * as a request for the first available page. This is deliberately independent
 * of the browser worker so target resolution remains isolated to this module.
 */
export async function resolveExternalPage(browser, params = {}) {
  const candidates = await pagesForBrowser(browser);
  const targetId = String(params.cdpTargetId || params.targetId || "").trim();

  if (targetId) {
    for (const candidate of candidates) {
      if (await targetIdForPage(candidate.page, candidate.context) === targetId) {
        return candidate.page;
      }
    }
    throw staleTarget(`Electron target '${targetId}' was not found. Call find_roots again.`);
  }

  if (candidates.length === 0) {
    throw staleTarget("External Electron browser has no page. Call find_roots again.");
  }

  const expectedUrls = [params.cdpPageUrl, params.url]
    .map((value) => String(value || "").trim())
    .filter(Boolean);
  for (const expectedUrl of expectedUrls) {
    const matches = candidates.filter(({ page }) => {
      try {
        return page.url() === expectedUrl;
      } catch {
        return false;
      }
    });
    if (matches.length === 1) return matches[0].page;
  }

  const expectedTitle = String(params.title || "").trim();
  if (expectedTitle) {
    const matches = [];
    for (const candidate of candidates) {
      try {
        if (await candidate.page.title() === expectedTitle) matches.push(candidate);
      } catch {
        // Ignore a page that closed while resolving the non-identity fallback.
      }
    }
    if (matches.length === 1) return matches[0].page;
  }

  if (candidates.length === 1) return candidates[0].page;
  throw staleTarget(
    "External Electron target is ambiguous; its CDP target id is unavailable. Call find_roots again.",
  );
}
