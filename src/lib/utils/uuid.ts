/**
 * Secure-context-safe UUID v4 generator.
 *
 * `crypto.randomUUID()` is only available in secure contexts (HTTPS / localhost).
 * When accessed via LAN IP over HTTP (e.g. web server "all devices" mode),
 * the browser throws "crypto.randomUUID is not a function".
 *
 * This wrapper uses `crypto.randomUUID()` when available, falling back to
 * `crypto.getRandomValues()` which works in ALL contexts.
 */
export function uuid(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  // Fallback: crypto.getRandomValues() works in non-secure contexts
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  // Set version (4) and variant (RFC 4122)
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
