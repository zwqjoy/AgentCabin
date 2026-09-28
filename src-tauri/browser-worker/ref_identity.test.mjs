import test from "node:test";
import assert from "node:assert/strict";
import { createRefIdentityHelpers } from "./ref_identity.mjs";

test("Browser refs are scoped to both the Worker lifetime and document", () => {
  const refs = createRefIdentityHelpers();
  const worker1DocumentA = refs.namespace("bw-1", "doc-a");
  const worker1DocumentB = refs.namespace("bw-1", "doc-b");
  const worker2DocumentA = refs.namespace("bw-2", "doc-a");
  const oldRef = refs.makeRef(worker1DocumentA, 1);

  assert.notEqual(oldRef, refs.makeRef(worker1DocumentB, 1));
  assert.notEqual(oldRef, refs.makeRef(worker2DocumentA, 1));
  assert.equal(refs.isInNamespace(oldRef, worker1DocumentA), true);
  assert.equal(refs.isInNamespace(oldRef, worker1DocumentB), false);
  assert.equal(refs.isInNamespace(oldRef, worker2DocumentA), false);
  assert.equal(refs.makeRef(worker1DocumentA, 2), `${worker1DocumentA}2`);
});
