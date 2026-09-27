import test from "node:test";
import assert from "node:assert/strict";
import { structuredAction } from "./action_result.mjs";
import { generatePageSnapshot } from "./snapshot.mjs";

function page(states) {
  let index = 0;
  return {
    title: async () => "Example",
    url: () => "https://example.test/",
    evaluate: async () => states[Math.min(index++, states.length - 1)],
  };
}

test("successful actions return a compact structured result and delta observation", async () => {
  const target = { ref: "e1", role: "button", name: "Save" };
  const tab = page([
    { documentKey: "doc", refs: { e1: target }, tree: "before" },
    { documentKey: "doc", refs: { e1: target, e2: { ref: "e2", role: "status", name: "Saved" } }, tree: "after" },
  ]);
  const result = await structuredAction(tab, "click", { ref: "e1" }, async () => ({ target, resolvedBy: "ref" }));
  assert.equal(result.ok, true);
  assert.equal(result.action, "click");
  assert.deepEqual(result.target, target);
  assert.equal(result.execution.performed, true);
  assert.equal(result.execution.resolvedBy, "ref");
  assert.equal(result.before.revision, 1);
  assert.equal(result.after.revision, 2);
  assert.equal(result.change.type, "delta");
  assert.equal(result.observation.added[0].name, "Saved");
});

test("failed actions expose a machine-readable code and recovery", async () => {
  const tab = page([{ documentKey: "doc", refs: { e1: { ref: "e1", role: "button", name: "Save" } }, tree: "before" }]);
  const result = await structuredAction(tab, "click", { ref: "e1" }, async () => {
    throw Object.assign(new Error("multiple matches"), { code: "ambiguous_target" });
  });
  assert.equal(result.ok, false);
  assert.equal(result.error.code, "ambiguous_target");
  assert.equal(result.execution.performed, false);
  assert.deepEqual(result.recovery, { recommended: "snapshot" });
});

test("normalizes common action failure codes", async () => {
  for (const code of ["stale_ref", "target_not_found", "ambiguous_target", "timeout", "unsupported_action"]) {
    const tab = page([{ documentKey: code, refs: {}, tree: "before" }]);
    const result = await structuredAction(tab, "click", {}, async () => {
      throw Object.assign(new Error(code), { code });
    });
    assert.equal(result.ok, false);
    assert.equal(result.error.code, code);
    assert.equal(result.recovery.recommended, "snapshot");
  }
});

test("rejects refs from a previous Browser Worker instance without performing an action", async () => {
  const tab = page([{ documentKey: "doc", refs: { e17: { ref: "e17", role: "button", name: "Continue" } }, tree: "page" }]);
  tab.workerInstanceId = "bw-current";
  tab.targetId = "tab-a";
  let performed = false;
  const result = await structuredAction(tab, "click", {
    ref: "e17",
    targetIdentity: { workerInstanceId: "bw-old", documentId: "doc", revision: 12, targetId: "tab-a" },
  }, async () => { performed = true; });
  assert.equal(performed, false);
  assert.equal(result.ok, false);
  assert.equal(result.execution.performed, false);
  assert.equal(result.error.code, "stale_browser_instance");
  assert.equal(result.recovery.recommended, "snapshot");
  assert.equal(result.before.workerInstanceId, "bw-current");
});

test("document changes discard the native ref but preserve a semantic recovery locator", async () => {
  const tab = page([{ documentKey: "new-doc", refs: {}, tree: "new page" }]);
  tab.workerInstanceId = "bw-current";
  tab.targetId = "tab-a";
  let actionParams;
  const result = await structuredAction(tab, "click", {
    ref: "e17", locator: { role: "button", name: "Continue" },
    targetIdentity: { workerInstanceId: "bw-current", documentId: "old-doc", revision: 10, targetId: "tab-a" },
  }, async (_snapshot, params) => { actionParams = params; return { resolvedBy: "locator" }; });
  assert.equal(result.ok, true);
  assert.equal(actionParams.ref, undefined);
  assert.deepEqual(actionParams.locator, { role: "button", name: "Continue" });
  assert.equal(result.execution.resolvedBy, "semantic_recovery");
});

test("revision changes alone do not invalidate a ref identity", async () => {
  const tab = page([{ documentKey: "same-doc", refs: { e17: { ref: "e17", role: "button", name: "Continue" } }, tree: "page" }]);
  tab.workerInstanceId = "bw-current";
  tab.targetId = "tab-a";
  const current = await generatePageSnapshot(tab);
  const result = await structuredAction(tab, "click", {
    ref: "e17", targetIdentity: { workerInstanceId: "bw-current", documentId: current.documentId, revision: 10, targetId: "tab-a" },
  }, async (_snapshot, params) => ({ resolvedBy: params.ref ? "ref" : "locator" }));
  assert.equal(result.ok, true);
  assert.equal(result.execution.resolvedBy, "ref");
});

test("an explicit target identity cannot cross tabs", async () => {
  const tab = page([{ documentKey: "doc-b", refs: { e17: { ref: "e17", role: "button", name: "Continue" } }, tree: "tab B" }]);
  tab.workerInstanceId = "bw-current";
  tab.targetId = "tab-b";
  let performed = false;
  const result = await structuredAction(tab, "click", {
    ref: "e17", locator: { role: "button", name: "Continue" },
    targetIdentity: { workerInstanceId: "bw-current", documentId: "doc-a", revision: 7, targetId: "tab-a" },
  }, async () => { performed = true; });
  assert.equal(performed, false);
  assert.equal(result.error.code, "stale_browser_instance");
});
