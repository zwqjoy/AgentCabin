import test from "node:test";
import assert from "node:assert/strict";
import { createEmbeddedPage } from "./embedded_page.mjs";
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
      return { result: { value: { x: 10, y: 20, width: 100, height: 40 } } };
    }
    return {};
  });
  const page = createEmbeddedPage({ client });
  await page.clickRef("e2");
  const click = client.calls.find((call) => call.method === "Input.dispatchMouseEvent");
  assert.equal(click.params.x, 60);
  assert.equal(click.params.y, 40);
});

test("clickRef reports an actionable error for an unknown ref", async () => {
  const client = makeClient(() => ({ result: { value: null } }));
  const page = createEmbeddedPage({ client });
  await assert.rejects(() => page.clickRef("e9"), /stale ref/);
});
