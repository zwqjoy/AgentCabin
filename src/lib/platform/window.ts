/**
 * Window / webview controls.
 *
 * These go through the Transport (dragging, position) or the desktop bridge
 * (focus, zoom) so no page touches Tauri APIs directly.
 */
import { getTransport } from "$lib/transport";

export async function startDragging(): Promise<void> {
  await getTransport().startDragging();
}

export async function outerPosition(): Promise<{ x: number; y: number }> {
  return getTransport().outerPosition();
}

export async function focus(): Promise<void> {
  if (typeof window === "undefined" || !window.agentcabinDesktop) return;
  await window.agentcabinDesktop.window.focus();
}

export async function setZoom(factor: number): Promise<void> {
  if (typeof window === "undefined" || !window.agentcabinDesktop) return;
  await window.agentcabinDesktop.window.setZoom(factor);
}
