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

test("document nonce falls back to getRandomValues when randomUUID is unavailable", () => {
  const refs = createRefIdentityHelpers();
  let generated = 0;
  const insecureContextCrypto = {
    getRandomValues(bytes) {
      generated++;
      for (let index = 0; index < bytes.length; index++) bytes[index] = index;
      return bytes;
    },
  };

  const nonce = refs.createDocumentNonce(insecureContextCrypto);
  assert.equal(generated, 1);
  assert.match(nonce, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});

test("document nonce prefers randomUUID when available", () => {
  const refs = createRefIdentityHelpers();
  const nonce = "worker-document-nonce";
  assert.equal(refs.createDocumentNonce({ randomUUID: () => nonce }), nonce);
});
