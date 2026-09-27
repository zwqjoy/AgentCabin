import test from "node:test";
import assert from "node:assert/strict";
import { structuredAction } from "./action_result.mjs";

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
