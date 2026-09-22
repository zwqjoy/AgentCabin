/**
 * Native file dialogs.
 *
 * - Tauri / Electron desktop hosts: native OS dialogs.
 * - Plain browser: no native dialog available; callers already guard with
 *   `transport.isDesktop()` where needed.
 *
 * The `AgentCabinDesktopBridge` type (and the `Window.agentcabinDesktop`
 * ambient) is provided by `./bridge`.
 */
import { getTransport } from "$lib/transport";
import type { ConfirmOptions, OpenDialogOptions, SaveDialogOptions } from "./dialog-types";

function electronBridge() {
  const b = window.agentcabinDesktop;
  if (!b) throw new Error("agentcabinDesktop bridge is not available");
  return b;
}

export async function open(options: OpenDialogOptions = {}): Promise<string | string[] | null> {
  const t = getTransport();
  if (!t.isDesktop()) return null;
  return electronBridge().dialog.open(options);
}

export async function save(options: SaveDialogOptions = {}): Promise<string | null> {
  const t = getTransport();
  if (!t.isDesktop()) return null;
  return electronBridge().dialog.save(options);
}

export async function confirm(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  const t = getTransport();
  if (!t.isDesktop()) return window.confirm(message);
  return electronBridge().dialog.confirm(message, options);
}
