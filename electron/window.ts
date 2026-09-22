/**
 * Window management for the AgentCabin Electron shell.
 *
 * P2: on macOS the window uses `hiddenInset` so the overlaid traffic lights
 * match the Tauri `titleBarStyle: Overlay` design; dragging works natively
 * through the existing `-webkit-app-region` CSS on `[data-tauri-drag-region]`.
 * Move/focus events are forwarded to the renderer on the bridge channels.
 */
import { BrowserWindow, ipcMain, app, shell } from "electron";
import fs from "node:fs";
import path from "node:path";

const WINDOW_DEFAULTS = {
  width: 1280,
  height: 920,
  minWidth: 900,
  minHeight: 600,
} as const;

export function getAppIconPath(): string {
  const candidates = [
    path.join(__dirname, "..", "..", "src-tauri", "icons", "icon.png"),
    path.join(app.getAppPath(), "src-tauri", "icons", "icon.png"),
    path.join(__dirname, "..", "..", "static", "logo.png"),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return "";
}

export function createMainWindow(preloadPath: string): BrowserWindow {
  const icon = getAppIconPath();
  const win = new BrowserWindow({
    title: "AgentCabin",
    ...(icon ? { icon } : {}),
    ...WINDOW_DEFAULTS,
    center: true,
    show: false,
    backgroundColor: "#111111",
    // macOS: overlay-style title bar, same look as the Tauri build.
    ...(process.platform === "darwin" ? { titleBarStyle: "hiddenInset" as const } : {}),
    webPreferences: {
      preload: preloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webviewTag: false,
      spellcheck: false,
    },
  });

  win.once("ready-to-show", () => win.show());
  forwardWindowEvents(win);

  // No new windows from the renderer; hand external links to the OS browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://") || url.startsWith("http://")) {
      void shell.openExternal(url);
    }
    return { action: "deny" };
  });

  // Lock navigation to the app origin(s); set by main after loading.
  win.webContents.on("will-navigate", (event, url) => {
    const allowed = win.agentcabinAllowedOrigins ?? [];
    let origin: string;
    try {
      origin = new URL(url).origin;
    } catch {
      event.preventDefault();
      return;
    }
    if (!allowed.includes(origin)) {
      event.preventDefault();
      if (url.startsWith("https://") || url.startsWith("http://")) {
        void shell.openExternal(url);
      }
    }
  });

  return win;
}

declare module "electron" {
  interface BrowserWindow {
    /** Origins the renderer may navigate to; set right after creation. */
    agentcabinAllowedOrigins?: string[];
  }
}

/** Forward move/focus events to the renderer (pet interaction parity). */
function forwardWindowEvents(win: BrowserWindow): void {
  const sendMoved = () => {
    if (win.isDestroyed()) return;
    const { x, y } = win.getContentBounds();
    win.webContents.send("window:moved", { x, y });
  };
  // `move` fires continuously while dragging; coalesce to animation frames.
  let moveScheduled = false;
  win.on("move", () => {
    if (moveScheduled) return;
    moveScheduled = true;
    setImmediate(() => {
      moveScheduled = false;
      sendMoved();
    });
  });

  win.on("focus", () => win.webContents.send("window:focus", { focused: true }));
  win.on("blur", () => win.webContents.send("window:focus", { focused: false }));
}

export function preloadFile(): string {
  return path.join(__dirname, "preload.cjs");
}

export function registerWindowIpc(getWindow: () => BrowserWindow | null): void {
  ipcMain.handle("window:minimize", (event) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    win?.minimize();
  });

  ipcMain.handle("window:toggleMaximize", (event) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    if (!win) return;
    if (win.isMaximized()) {
      win.unmaximize();
    } else {
      win.maximize();
    }
  });

  ipcMain.handle("window:close", (event) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    win?.close();
  });

  ipcMain.handle("window:outer-position", (event) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? getWindow();
    if (!win || win.isDestroyed()) return { x: 0, y: 0 };
    const { x, y } = win.getContentBounds();
    return { x, y };
  });

  ipcMain.handle("app:version", () => app.getVersion());
  ipcMain.handle("app:platform", () => process.platform);
}
