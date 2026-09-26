/**
 * Shared types for the AgentCabin Electron host.
 *
 * The renderer only ever sees `window.agentcabinDesktop` (installed by the
 * preload script). Everything else — Electron, Node, ipcRenderer — stays on
 * the main-process side of the contextBridge.
 *
 * P2/P3 scope: window controls + app info + core passthrough + dialog /
 * shell / path. The core channels (`core:invoke` / `core:emit` /
 * `core:event`) are wired end-to-end now so P4 only swaps the main-side
 * implementation from "not connected" to a real CoreProcess client — the
 * renderer-facing contract does not change.
 */

/** Dialog option shapes mirrored from @tauri-apps/plugin-dialog v2. */
export interface DialogFilter {
  name: string;
  extensions: string[];
}

export interface OpenDialogOptions {
  title?: string;
  filters?: DialogFilter[];
  defaultPath?: string;
  multiple?: boolean;
  directory?: boolean;
}

export interface SaveDialogOptions {
  title?: string;
  filters?: DialogFilter[];
  defaultPath?: string;
}

export type ConfirmKind = "info" | "warning" | "error";

export interface ConfirmOptions {
  title?: string;
  kind?: ConfirmKind;
}

/** Channels the renderer may invoke. Keep this list tight. */
export const INVOKE_CHANNELS = [
  "app:version",
  "app:platform",
  "window:minimize",
  "window:toggleMaximize",
  "window:close",
  "window:focus",
  "window:set-zoom",
  "window:outer-position",
  "dialog:open",
  "dialog:save",
  "dialog:confirm",
  "shell:open-external",
  "shell:open-path",
  "path:home",
  "path:join",
  "core:invoke",
  "core:emit",
  "browser:attach",
  "browser:set-bounds",
  "browser:set-visible",
  "browser:command",
  "browser:unbind",
  "browser:destroy",
  "browser:get-endpoint",
] as const;

export type InvokeChannel = (typeof INVOKE_CHANNELS)[number];

/** Event channels the renderer may subscribe to (main -> renderer push). */
export const EVENT_CHANNELS = [
  "window:moved",
  "window:focus",
  "core:event",
  "browser:state-changed",
  "browser:tabs-changed",
] as const;

export type EventChannel = (typeof EVENT_CHANNELS)[number];

/** Envelope pushed on the `core:event` channel for every core event. */
export interface CoreEventEnvelope {
  event: string;
  payload?: unknown;
  seq?: number;
}

export interface BrowserTabInfo {
  id: string;
  targetId: string;
  index: number;
  url: string;
  title: string;
  active: boolean;
}

export interface BrowserTabsChanged {
  viewId: string;
  runId: string;
  activeTargetId: string;
  tabs: BrowserTabInfo[];
}

export interface AgentCabinDesktopBridge {
  /** JSON-RPC style request to the Electron main process. */
  invoke<T = unknown>(channel: InvokeChannel, payload?: unknown): Promise<T>;
  /** Subscribe to a main-process event; returns an unsubscribe function. */
  listen<T = unknown>(channel: EventChannel, handler: (payload: T) => void): Promise<() => void>;
  /** Present for Transport parity; renderer->main push goes through core.emit. */
  emit(channel: string, payload?: unknown): Promise<void>;
  window: {
    minimize(): Promise<void>;
    toggleMaximize(): Promise<void>;
    close(): Promise<void>;
    focus(): Promise<void>;
    setZoom(factor: number): Promise<void>;
    /** Outer window position in DIP screen coordinates. */
    outerPosition(): Promise<{ x: number; y: number }>;
  };
  app: {
    version(): Promise<string>;
    platform(): Promise<NodeJS.Platform>;
  };
  /** Rust Core RPC passthrough. Rejected until the CoreProcess lands (P4). */
  core: {
    invoke(method: string, params?: unknown): Promise<unknown>;
    /**
     * Subscribe to a core event. Resolves immediately; the handler fires for
     * every matching envelope pushed on `core:event`.
     */
    listen(event: string, handler: (payload: unknown) => void): () => void;
    /** Broadcast an event to all windows (pet/main comms parity with Tauri). */
    emit(event: string, payload?: unknown): Promise<void>;
  };
  /** Native OS dialogs (options mirror the Tauri plugin-dialog API). */
  dialog: {
    open(options: OpenDialogOptions): Promise<string | string[] | null>;
    save(options: SaveDialogOptions): Promise<string | null>;
    confirm(message: string, options: ConfirmOptions): Promise<boolean>;
  };
  /** System shell actions. */
  shell: {
    openExternal(url: string): Promise<void>;
    openPath(path: string): Promise<void>;
  };
  /** Path helpers resolved by the main process. */
  path: {
    homeDir(): Promise<string>;
    join(...segments: string[]): Promise<string>;
  };
  browser: {
    attach(payload: {
      viewId: string;
      runId: string;
      bindingId: string;
      url?: string;
      rect?: { x: number; y: number; width: number; height: number };
    }): Promise<{
      viewId: string;
      bindingId: string;
      targetId: string;
      endpoint: { host: string; port: number; token: string };
      state: unknown;
      tabs: BrowserTabInfo[];
      activeTargetId: string;
    }>;
    setBounds(payload: {
      viewId: string;
      bindingId: string;
      rect: { x: number; y: number; width: number; height: number };
    }): Promise<void>;
    setVisible(payload: { viewId: string; bindingId: string; visible: boolean }): Promise<void>;
    command(payload: {
      viewId: string;
      action: "back" | "forward" | "reload" | "stop";
    }): Promise<void>;
    getEndpoint(payload: { viewId: string }): Promise<{
      host: string;
      port: number;
      token: string;
      targetId: string;
    } | null>;
    unbind(payload: { viewId: string; bindingId: string }): Promise<void>;
    destroy(payload: { runId: string }): Promise<void>;
  };
}

declare global {
  interface Window {
    agentcabinDesktop?: AgentCabinDesktopBridge;
  }
}
