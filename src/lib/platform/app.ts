/**
 * App metadata (version / platform).
 */
import { getTransport } from "$lib/transport";

export async function getVersion(): Promise<string> {
  try {
    const t = getTransport();
    if (t.isDesktop()) {
      const b = window.agentcabinDesktop;
      if (b) return b.app.version();
    }
  } catch {
    /* fall through to static import */
  }
  const { version } = await import("../../../package.json");
  return version;
}

export function platform(): string {
  if (typeof navigator !== "undefined" && navigator.platform) {
    return navigator.platform;
  }
  return "unknown";
}
