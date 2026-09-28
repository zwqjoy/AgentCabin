export function createRefIdentityHelpers() {
  const safePart = (value) => String(value || "legacy").replace(/[^a-zA-Z0-9_-]/g, "_");
  return {
    namespace(workerInstanceId, documentNonce) {
      return `e${safePart(workerInstanceId)}_${safePart(documentNonce)}_`;
    },
    isInNamespace(ref, namespace) {
      return typeof ref === "string" && ref.startsWith(namespace) && /^\d+$/.test(ref.slice(namespace.length));
    },
    makeRef(namespace, counter) {
      return `${namespace}${counter}`;
    },
  };
}
