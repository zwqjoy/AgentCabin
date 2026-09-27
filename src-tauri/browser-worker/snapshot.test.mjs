import test from "node:test";
import assert from "node:assert/strict";
import { generatePageSnapshot } from "./snapshot.mjs";

function pageState(documentKey, refs, tree = "tree") {
  return {
    documentKey,
    refs,
    tree,
    title: "Example",
    url: "https://example.test/",
  };
}

function makePage(states) {
  let index = 0;
  return {
    title: async () => states[Math.min(index, states.length - 1)].title,
    url: () => states[Math.min(index, states.length - 1)].url,
    evaluate: async () => states[Math.min(index++, states.length - 1)],
  };
}

test("returns full then unchanged snapshots with increasing revisions", async () => {
  const state = pageState("1", { e1: { ref: "e1", role: "button", name: "Save" } });
  const page = makePage([state, state]);
  const first = await generatePageSnapshot(page);
  const second = await generatePageSnapshot(page, { sinceRevision: first.revision });
  assert.equal(first.snapshotType, "full");
  assert.equal(first.revision, 1);
  assert.equal(first.refs.e1.ref, "e1");
  assert.equal(second.snapshotType, "unchanged");
  assert.equal(second.revision, 2);
  assert.equal(second.baseRevision, 1);
});

test("reports added, changed, and removed semantic refs", async () => {
  const firstState = pageState("2", {
    e1: { ref: "e1", role: "textbox", name: "Email", value: "" },
    e2: { ref: "e2", role: "button", name: "Remove" },
  }, "semantic page heading ".repeat(60));
  const nextState = pageState("2", {
    e1: { ref: "e1", role: "textbox", name: "Email", value: "hello" },
    e3: { ref: "e3", role: "dialog", name: "Confirm" },
  }, "semantic page heading ".repeat(60));
  const page = makePage([firstState, nextState]);
  const first = await generatePageSnapshot(page);
  const delta = await generatePageSnapshot(page, { sinceRevision: first.revision });
  assert.equal(delta.snapshotType, "delta");
  assert.deepEqual(delta.added, [{ ref: "e3", role: "dialog", name: "Confirm" }]);
  assert.deepEqual(delta.changed, [{ ref: "e1", before: { value: "" }, after: { value: "hello" } }]);
  assert.deepEqual(delta.removed, ["e2"]);
});

test("document replacement and missing revisions fall back to full", async () => {
  const page = makePage([
    pageState("old-document", { e1: { ref: "e1", role: "button", name: "Old" } }),
    pageState("new-document", { e1: { ref: "e1", role: "button", name: "New" } }),
  ]);
  const first = await generatePageSnapshot(page);
  const next = await generatePageSnapshot(page, { sinceRevision: first.revision });
  assert.equal(next.snapshotType, "full");
  assert.notEqual(next.documentId, first.documentId);
  assert.equal(next.revision, 1);
});

test("interleaved tabs keep their own revisions and baselines", async () => {
  const tabA = makePage([
    pageState("doc-a", { e1: { ref: "e1", role: "button", name: "A", value: "" } }, "page A ".repeat(80)),
    pageState("doc-a", { e1: { ref: "e1", role: "button", name: "A", value: "A2" } }, "page A ".repeat(80)),
  ]);
  const tabB = makePage([
    pageState("doc-b", { e1: { ref: "e1", role: "button", name: "B", value: "" } }, "page B ".repeat(80)),
    pageState("doc-b", { e1: { ref: "e1", role: "button", name: "B", value: "B2" } }, "page B ".repeat(80)),
  ]);
  tabA.workerInstanceId = "bw-test";
  tabA.targetId = "tab-a";
  tabB.workerInstanceId = "bw-test";
  tabB.targetId = "tab-b";
  const a1 = await generatePageSnapshot(tabA);
  const b1 = await generatePageSnapshot(tabB);
  const b2 = await generatePageSnapshot(tabB, { sinceRevision: b1.revision });
  const a2 = await generatePageSnapshot(tabA, { sinceRevision: a1.revision });
  assert.equal(a1.revision, 1);
  assert.equal(a1.workerInstanceId, "bw-test");
  assert.equal(a1.targetId, "tab-a");
  assert.equal(a2.revision, 2);
  assert.equal(b1.revision, 1);
  assert.equal(b1.workerInstanceId, "bw-test");
  assert.equal(b1.targetId, "tab-b");
  assert.equal(b2.revision, 2);
  assert.equal(a2.changed[0].after.value, "A2");
  assert.equal(b2.changed[0].after.value, "B2");
  assert.notEqual(a1.documentId, b1.documentId);
});
