import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { createEmbeddedPage } from "./embedded_page.mjs";
import { createRefIdentityHelpers } from "./ref_identity.mjs";
import { generatePageSnapshot } from "./snapshot.mjs";

function makeClient(responder) {
  const calls = [];
  return {
    calls,
    isReady: true,
    send: async (method, params) => {
      calls.push({ method, params });
      return responder(method, params);
    },
    onEvent: () => () => {},
    close: () => {},
  };
}

test("the CDP page shim satisfies generatePageSnapshot", async () => {
  const client = makeClient((method, params) => {
    if (method === "Runtime.evaluate") {
      const expression = String(params.expression);
      if (expression.includes("document.title")) return { result: { value: "Example" } };
      if (expression.includes("location.href")) return { result: { value: "https://example.com/" } };
      return { result: { value: { tree: '[ref=e1] link "More"', refs: { e1: { ref: "e1", name: "More" } } } } };
    }
    throw new Error(`unexpected ${method}`);
  });

  const page = createEmbeddedPage({ client });
  await page.refreshLocation();
  const snapshot = await generatePageSnapshot(page);
  assert.equal(snapshot.title, "Example");
  assert.equal(snapshot.url, "https://example.com/");
  assert.equal(snapshot.refs.e1.name, "More");
});

test("clickAt dispatches a mouse press and release at the given coordinates", async () => {
  const client = makeClient(() => ({}));
  const page = createEmbeddedPage({ client });
  await page.clickAt(120, 240);
  const types = client.calls.map((call) => call.params.type);
  assert.deepEqual(types, ["mousePressed", "mouseReleased"]);
  assert.equal(client.calls[0].params.x, 120);
  assert.equal(client.calls[0].params.y, 240);
  assert.equal(client.calls[0].params.button, "left");
});

test("clickRef resolves the element rect before clicking its centre", async () => {
  const client = makeClient((method, params) => {
    if (method === "Runtime.evaluate") {
      assert.match(String(params.expression), /data-work-ref/);
      return { result: { value: { element: { ref: "e2", role: "button", name: "Save", rect: { x: 10, y: 20, width: 100, height: 40 } }, source: "ref" } } };
    }
    return {};
  });
  const page = createEmbeddedPage({ client });
  await page.clickRef("e2");
  const click = client.calls.find((call) => call.method === "Input.dispatchMouseEvent");
  assert.equal(click.params.x, 60);
  assert.equal(click.params.y, 40);
});

test("HTTP page click resolves a document ref when randomUUID is unavailable", async () => {
  const refIdentity = createRefIdentityHelpers();
  const bytesToNonce = { getRandomValues(bytes) { for (let i = 0; i < bytes.length; i++) bytes[i] = i; return bytes; } };
  const nonce = refIdentity.createDocumentNonce(bytesToNonce);
  const ref = refIdentity.makeRef(refIdentity.namespace("legacy", nonce), 1);
  const crypto = { getRandomValues(bytes) { for (let i = 0; i < bytes.length; i++) bytes[i] = i; return bytes; } };
  const button = {
    isConnected: true,
    tagName: "BUTTON",
    type: "button",
    disabled: false,
    readOnly: false,
    labels: [],
    innerText: "Save",
    textContent: "Save",
    getAttribute: (name) => name === "data-work-ref" ? ref : null,
    getBoundingClientRect: () => ({ x: 12, y: 24, width: 80, height: 30 }),
  };
  const refNodes = new WeakMap([[button, ref]]);
  const document = {
    querySelector: (selector) => selector.includes(ref) ? button : null,
    querySelectorAll: () => [button],
    getElementById: () => null,
  };
  const window = {
    __agentCabinWorkRefNodes: refNodes,
    getComputedStyle: () => ({ display: "block", visibility: "visible", opacity: "1" }),
  };
  const client = makeClient(async (method, params) => {
    if (method === "Runtime.evaluate") {
      const expression = String(params.expression);
      if (expression.includes("const ref =")) {
        const result = await vm.runInNewContext(expression, { crypto, window, document, getComputedStyle: window.getComputedStyle });
        return { result: { value: result } };
      }
      return { result: { value: false } };
    }
    return {};
  });
  const page = createEmbeddedPage({ client });

  const result = await page.clickRef(ref);
  assert.equal(window.__agentCabinWorkDocumentNonce, nonce);
  assert.equal(result.resolvedBy, "ref");
  assert.equal(result.target.ref, ref);
  const click = client.calls.filter((call) => call.method === "Input.dispatchMouseEvent");
  assert.deepEqual(click.map((call) => call.params.type), ["mousePressed", "mouseReleased"]);
  assert.equal(click[0].params.x, 52);
  assert.equal(click[0].params.y, 39);
});

test("clickRef reports an actionable error for an unknown ref", async () => {
  const client = makeClient(() => ({ result: { value: { error: { code: "stale_ref", message: "stale ref" } } } }));
  const page = createEmbeddedPage({ client });
  await assert.rejects(() => page.clickRef("e9"), (error) => error.code === "stale_ref");
});

test("a stale ref resolves through one matching semantic locator", async () => {
  const client = makeClient((method, params) => {
    if (method === "Runtime.evaluate") {
      const expression = String(params.expression);
      if (expression.includes("const locator")) return { result: { value: { element: { ref: "e88", role: "button", name: "Save", rect: { x: 0, y: 0, width: 20, height: 20 } }, source: "locator" } } };
      return { result: { value: { element: { ref: "e88", role: "button", name: "Save", rect: { x: 0, y: 0, width: 20, height: 20 } }, source: "ref" } } };
    }
    return {};
  });
  const page = createEmbeddedPage({ client });
  const result = await page.clickRef("e12", { role: "button", name: "Save" });
  assert.equal(result.target.ref, "e88");
  assert.equal(result.resolvedBy, "locator");
});

test("ambiguous semantic targets fail closed", async () => {
  const client = makeClient(() => ({ result: { value: { error: { code: "ambiguous_target", message: "multiple matches" } } } }));
  const page = createEmbeddedPage({ client });
  await assert.rejects(() => page.clickRef("e12", { role: "button", name: "Save" }), (error) => error.code === "ambiguous_target");
});
