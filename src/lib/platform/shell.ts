/**
 * Shell actions: opening external URLs and local files in the system
 * browser / file manager.
 *
 * The `AgentCabinDesktopBridge` ambient is provided by `./bridge`.
 */
import { getTransport } from "$lib/transport";

function electronBridge() {
  const b = window.agentcabinDesktop;
  if (!b) throw new Error("agentcabinDesktop bridge is not available");
  return b;
}

export async function openExternal(url: string): Promise<void> {
  const t = getTransport();
  if (!t.isDesktop()) {
    window.open(url, "_blank");
    return;
  }
  await electronBridge().shell.openExternal(url);
}

export async function openPath(path: string): Promise<void> {
  const t = getTransport();
  if (t.isDesktop() && window.agentcabinDesktop?.shell?.openPath) {
    await window.agentcabinDesktop.shell.openPath(path);
    return;
  }
  await t.invoke<void>("open_path", { path });
}
