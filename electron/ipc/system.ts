/**
 * Main-process handlers for the system capabilities the renderer reaches
 * through the platform layer: native dialogs, shell (open external / open
 * path), path helpers, and window focus/zoom.
 *
 * Dialog option shapes arrive in the Tauri plugin-dialog format and are
 * translated to Electron's dialog API here, keeping the renderer
 * transport-agnostic.
 */
import { BrowserWindow, app, dialog, ipcMain, shell } from "electron";
import os from "node:os";
import path from "node:path";
import type { ConfirmOptions, DialogFilter, OpenDialogOptions, SaveDialogOptions } from "../types";

function toElectronFilters(filters?: DialogFilter[]) {
  if (!filters || filters.length === 0) return undefined;
  return filters.map((f) => ({
    name: f.name,
    extensions: f.extensions.map((e) => e.replace(/^\./, "")),
  }));
}

function toOpenProperties(options: OpenDialogOptions) {
  const props: Electron.OpenDialogOptions["properties"] = [];
  if (options.directory) props.push("openDirectory");
  if (options.multiple) props.push("multiSelections");
  if (props.length === 0) props.push("openFile");
  return props;
}

export function registerSystemIpc(getWindow: () => BrowserWindow | null): void {
  // ── window ──
  ipcMain.handle("window:focus", () => {
    const win = getWindow();
    if (win && !win.isFocused()) win.focus();
  });

  ipcMain.handle("window:set-zoom", (_event, payload: { factor: number }) => {
    getWindow()?.webContents.setZoomFactor(payload?.factor ?? 1);
  });

  // ── dialogs ──
  ipcMain.handle("dialog:open", async (event, options: OpenDialogOptions = {}) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    const dialogOptions = {
      title: options.title,
      defaultPath: options.defaultPath,
      filters: toElectronFilters(options.filters),
      properties: toOpenProperties(options),
    };
    const result = win
      ? await dialog.showOpenDialog(win, dialogOptions)
      : await dialog.showOpenDialog(dialogOptions);
    if (result.canceled) return null;
    return options.multiple ? result.filePaths : (result.filePaths[0] ?? null);
  });

  ipcMain.handle("dialog:save", async (event, options: SaveDialogOptions = {}) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    const dialogOptions = {
      title: options.title,
      defaultPath: options.defaultPath,
      filters: toElectronFilters(options.filters),
    };
    const result = win
      ? await dialog.showSaveDialog(win, dialogOptions)
      : await dialog.showSaveDialog(dialogOptions);
    if (result.canceled || !result.filePath) return null;
    return result.filePath;
  });

  ipcMain.handle(
    "dialog:confirm",
    async (event, payload: { message: string; options?: ConfirmOptions }) => {
      const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
      const kind = payload?.options?.kind;
      const detail = payload?.message ?? "";
      const title = payload?.options?.title ?? "";
      const boxOptions: Electron.MessageBoxOptions = {
        type: kind === "error" ? "error" : kind === "warning" ? "warning" : "info",
        title,
        message: title,
        detail,
        buttons: ["OK", "Cancel"],
        defaultId: 0,
        cancelId: 1,
      };
      const result = win
        ? await dialog.showMessageBox(win, boxOptions)
        : await dialog.showMessageBox(boxOptions);
      return result.response === 0;
    },
  );

  // ── shell ──
  ipcMain.handle("shell:open-external", (_event, url: string) => shell.openExternal(String(url)));
  ipcMain.handle("shell:open-path", (_event, p: string) => shell.openPath(String(p)));

  // ── path ──
  ipcMain.handle("path:home", () => os.homedir());
  ipcMain.handle("path:join", (_event, segments: string[]) =>
    path.join(...(Array.isArray(segments) ? segments : [])),
  );
}
