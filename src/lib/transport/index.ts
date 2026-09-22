/**
 * Transport abstraction layer.
 *
 * Detects Electron vs Tauri desktop vs browser environment and returns the
 * appropriate transport implementation. The singleton is cached after first
 * call. Detection order matters: the Electron bridge (installed by preload)
 * is checked first because an Electron renderer never has __TAURI_INTERNALS__.
 */
import { dbg } from "$lib/utils/debug";
import { ElectronTransport, hasElectronBridge } from "./electron";
import { WsTransport } from "./websocket";

export interface Transport {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
  emit(event: string, payload?: unknown): Promise<void>;
  startDragging(): Promise<void>;
  outerPosition(): Promise<{ x: number; y: number }>;
  listenWindowMoved(handler: (position: { x: number; y: number }) => void): Promise<() => void>;
  isDesktop(): boolean;
  /** Subscribe to a run's real-time events (WS only, no-op on desktop) */
  subscribeRun(runId: string, lastSeq?: number): void;
  /** Unsubscribe from a run's events (WS only, no-op on desktop) */
  unsubscribeRun(runId: string): void;
}

let _instance: Transport | null = null;

export function getTransport(): Transport {
  if (!_instance) {
    const isElectron = hasElectronBridge();

    _instance = isElectron ? new ElectronTransport() : new WsTransport();

    dbg("transport", "initialized", {
      type: isElectron ? "electron" : "websocket",
    });
  }
  return _instance;
}
