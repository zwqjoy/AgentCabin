/**
 * ElectronTransport: desktop IPC over the `window.agentcabinDesktop` bridge
 * installed by the sandboxed preload script.
 *
 * - invoke/listen are forwarded to the Rust core passthrough channels; until
 *   the Electron core process lands, `core:invoke` rejects with a clear error
 *   (matching the "not connected" state, not a silent failure).
 * - Window dragging needs no IPC: the main window is `hiddenInset` on macOS
 *   and the existing `-webkit-app-region` CSS handles it natively.
 * - Event/payload shapes mirror TauriTransport (raw payload, no envelope) so
 *   callers stay transport-agnostic.
 */
import type { AgentCabinDesktopBridge } from "$lib/platform/bridge";
import { dbg, dbgWarn } from "$lib/utils/debug";
import type { Transport } from "./index";

function bridge(): AgentCabinDesktopBridge {
  const b = window.agentcabinDesktop;
  if (!b) {
    throw new Error("agentcabinDesktop bridge is not available");
  }
  return b;
}

export class ElectronTransport implements Transport {
  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    dbg("transport", "electron.invoke", { cmd });
    // Electron IPC uses structured clone, which rejects Svelte $state proxies
    // ("An object could not be cloned"). The core only consumes JSON anyway,
    // so round-trip the args through JSON before crossing the bridge.
    const plain =
      args === undefined
        ? undefined
        : (JSON.parse(JSON.stringify(args)) as Record<string, unknown>);
    return (await bridge().core.invoke(cmd, plain)) as T;
  }

  async listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
    dbg("transport", "electron.listen", { event });
    const off = bridge().core.listen(event, handler as (payload: unknown) => void);
    return off;
  }

  async emit(event: string, payload?: unknown): Promise<void> {
    const plain = payload === undefined ? undefined : JSON.parse(JSON.stringify(payload));
    dbg("transport", "electron.emit", { event });
    await bridge().core.emit(event, plain);
  }

  async startDragging(): Promise<void> {
    // Handled natively by -webkit-app-region CSS on the drag regions.
  }

  async outerPosition(): Promise<{ x: number; y: number }> {
    return bridge().window.outerPosition();
  }

  async listenWindowMoved(
    handler: (position: { x: number; y: number }) => void,
  ): Promise<() => void> {
    const off = await bridge().listen<{ x: number; y: number }>("window:moved", handler);
    return off;
  }

  isDesktop(): boolean {
    return true;
  }

  subscribeRun(_runId: string, _lastSeq?: number): void {
    // No-op: all core events are pushed to every window, same as Tauri.
  }

  unsubscribeRun(_runId: string): void {
    // No-op
  }
}

export function hasElectronBridge(): boolean {
  try {
    return typeof window !== "undefined" && !!window.agentcabinDesktop;
  } catch (e) {
    dbgWarn("transport", "electron.detectFailed", e);
    return false;
  }
}
