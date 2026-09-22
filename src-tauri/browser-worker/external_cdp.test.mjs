import assert from "node:assert/strict";
import test from "node:test";

import {
  assertLocalCdpEndpoint,
  resolveExternalPage,
} from "./external_cdp.mjs";

function page(targetId, url) {
  return {
    targetId,
    url: () => url,
  };
}

function browserWithPages(pages) {
  return {
    contexts: () => [{
      pages: () => pages,
      newCDPSession: async (candidate) => ({
        send: async () => ({ targetInfo: { targetId: candidate.targetId } }),
      }),
    }],
  };
}

test("external CDP only accepts local websocket endpoints", () => {
  assert.equal(assertLocalCdpEndpoint("ws://127.0.0.1:9222/devtools/browser/x"), "ws://127.0.0.1:9222/devtools/browser/x");
  assert.throws(
    () => assertLocalCdpEndpoint("ws://192.168.1.10:9222/devtools/browser/x"),
    /not local/,
  );
  assert.throws(
    () => assertLocalCdpEndpoint("http://127.0.0.1:9222/json/version"),
    /must use ws/,
  );
});

test("external CDP resolves the exact target id and fails closed when it disappears", async () => {
  const first = page("target-a", "file:///a");
  const second = page("target-b", "file:///b");
  const browser = browserWithPages([first, second]);

  assert.equal(
    await resolveExternalPage(browser, { cdpTargetId: "target-b", url: "file:///a" }),
    second,
    "target identity must win over URL ordering",
  );
  await assert.rejects(
    () => resolveExternalPage(browser, { cdpTargetId: "closed-target", url: "file:///a" }),
    /stale_browser_target: Electron target 'closed-target' was not found/,
  );
});

test("external CDP uses URL only when no target id is available and rejects ambiguity", async () => {
  const first = page("target-a", "file:///same");
  const second = page("target-b", "file:///same");
  const browser = browserWithPages([first, second]);

  await assert.rejects(
    () => resolveExternalPage(browser, { url: "file:///same" }),
    /stale_browser_target: External Electron target is ambiguous/,
  );
  assert.equal(
    await resolveExternalPage(browserWithPages([first]), { url: "file:///same" }),
    first,
  );
});
