/**
 * Renderer-facing view of the preload-installed desktop bridge.
 *
 * electron/types.ts is the source of truth on the main-process side; this
 * module mirrors the renderer-consumed subset and installs the
 * `Window.agentcabinDesktop` ambient type for the renderer TS program.
 *
 * Pages must import from `$lib/platform`, not from this file directly.
 */
import type { ConfirmOptions, OpenDialogOptions, SaveDialogOptions } from "./dialog-types";

export interface AgentCabinDesktopBridge {
  invoke<T = unknown>(channel: string, payload?: unknown): Promise<T>;
  listen<T = unknown>(channel: string, handler: (payload: T) => void): Promise<() => void>;
  window: {
    minimize(): Promise<void>;
    toggleMaximize(): Promise<void>;
    close(): Promise<void>;
    focus(): Promise<void>;
    setZoom(factor: number): Promise<void>;
    outerPosition(): Promise<{ x: number; y: number }>;
  };
  app: {
    version(): Promise<string>;
    platform(): Promise<string>;
  };
  core: {
    invoke(method: string, params?: unknown): Promise<unknown>;
    listen(event: string, handler: (payload: unknown) => void): () => void;
    emit(event: string, payload?: unknown): Promise<void>;
  };
  dialog: {
    open(options: OpenDialogOptions): Promise<string | string[] | null>;
    save(options: SaveDialogOptions): Promise<string | null>;
    confirm(message: string, options: ConfirmOptions): Promise<boolean>;
  };
  shell: {
    openExternal(url: string): Promise<void>;
    openPath(path: string): Promise<void>;
  };
  path: {
    homeDir(): Promise<string>;
    join(...segments: string[]): Promise<string>;
  };
  browser: {
    attach(payload: {
      viewId: string;
      runId: string;
      url?: string;
      rect?: { x: number; y: number; width: number; height: number };
    }): Promise<{
      viewId: string;
      targetId: string;
      endpoint: { host: string; port: number; token: string };
      tabs: Array<{
        id: string;
        targetId: string;
        index: number;
        url: string;
        title: string;
        active: boolean;
      }>;
      activeTargetId: string;
    }>;
    setBounds(payload: {
      viewId: string;
      rect: { x: number; y: number; width: number; height: number };
    }): Promise<void>;
    setVisible(payload: { viewId: string; visible: boolean }): Promise<void>;
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
    detach(payload: { viewId: string }): Promise<void>;
  };
}

declare global {
  interface Window {
    agentcabinDesktop?: AgentCabinDesktopBridge;
  }
}
