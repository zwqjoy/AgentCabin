/**
 * Core RPC passthrough between the renderer and the Rust core.
 *
 * The renderer calls `core:invoke` / subscribes to core events; this module
 * forwards invokes to the CoreProcessManager (headless core over HTTP) and
 * fans core events out to every live window on the `core:event` channel.
 */
import { BrowserWindow, ipcMain } from "electron";
import type { CoreEventEnvelope } from "../types";
import type { CoreProcessManager } from "../core-process";
import { isDesktopIntercepted, handleDesktopIntercept } from "./desktop";

export function registerCoreIpc(
  core: CoreProcessManager,
  getWindow: () => BrowserWindow | null = () => null,
): void {
  ipcMain.handle("core:invoke", async (_event, payload: { method: string; params?: unknown }) => {
    const { method, params } = payload ?? {};
    try {
      if (isDesktopIntercepted(method)) {
        const result = await handleDesktopIntercept(method, params, core, getWindow);
        return { ok: true, result };
      }
      const result = await core.invoke(method, params);
      if (method === "update_user_settings") {
        try {
          const { petWindowManager } = await import("../pet-window");
          const patch =
            (params as { patch?: Record<string, unknown> })?.patch ??
            (params as Record<string, unknown>) ??
            {};
          petWindowManager.applySettingsPatch(patch, result as Record<string, unknown>);
        } catch (e) {
          console.warn("[ipc/core] Failed to sync pet settings on update_user_settings:", e);
        }
      }
      return { ok: true, result };
    } catch (err: unknown) {
      const error = err instanceof Error ? err.message : String(err);
      return { ok: false, error };
    }
  });

  // Renderer -> all windows broadcast. Mirrors the Tauri event bus semantics
  // used by the pet window today.
  ipcMain.handle("core:emit", async (event, payload: { event: string; payload?: unknown }) => {
    broadcastCoreEvent(payload.event, payload.payload);
    if (payload?.event === "pet://ready") {
      try {
        const { petWindowManager } = await import("../pet-window");
        petWindowManager.handlePetReady();
      } catch (e) {
        console.warn("[ipc/core] Failed to handle pet://ready:", e);
      }
    }
    void event;
  });
}

/** Push a core event to every live window (main-process side entry point). */
export function broadcastCoreEvent(event: string, payload?: unknown): void {
  const envelope: CoreEventEnvelope = { event, payload };
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) {
      win.webContents.send("core:event", envelope);
    }
  }
}
