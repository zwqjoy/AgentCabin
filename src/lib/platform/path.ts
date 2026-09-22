/**
 * Path helpers. Tauri resolves these on the main process (correct per-OS
 * home dir / separator); Electron does the same over the bridge. Web has no
 * native path API — callers should treat these as desktop-only.
 */
import { getTransport } from "$lib/transport";

function electronBridge() {
  const b = window.agentcabinDesktop;
  if (!b) throw new Error("agentcabinDesktop bridge is not available");
  return b;
}

function native(): {
  homeDir(): Promise<string>;
  join(...s: string[]): Promise<string>;
} {
  return electronBridge().path;
}

export async function homeDir(): Promise<string> {
  const t = getTransport();
  if (!t.isDesktop()) throw new Error("homeDir is only available in the desktop app");
  return native().homeDir();
}

export async function join(...segments: string[]): Promise<string> {
  const t = getTransport();
  if (!t.isDesktop()) {
    // Browser fallback: naive join (used only for display/logging in web mode).
    return segments.filter(Boolean).join("/");
  }
  return native().join(...segments);
}
