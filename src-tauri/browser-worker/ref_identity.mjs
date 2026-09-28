export function createRefIdentityHelpers() {
  const safePart = (value) => String(value || "legacy").replace(/[^a-zA-Z0-9_-]/g, "_");
  function createDocumentNonce(cryptoObject = globalThis.crypto) {
    if (typeof cryptoObject?.randomUUID === "function") return cryptoObject.randomUUID();
    if (typeof cryptoObject?.getRandomValues !== "function") {
      throw new Error("Secure random values are unavailable for Browser document identity.");
    }

    const bytes = cryptoObject.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }

  return {
    createDocumentNonce,
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
