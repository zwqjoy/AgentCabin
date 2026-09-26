/**
 * AgentCabin Electron main process (P1 shell).
 *
 * Responsibilities, deliberately small:
 * - app lifecycle (single instance, macOS re-activate)
 * - one BrowserWindow with secure webPreferences
 * - dev:  load the Vite dev server (http://localhost:1420)
 * - prod: serve `build/` over a loopback-only static server and load it
 *
 * The Rust core, runtimes, dialogs and packaging all arrive in later phases
 * (P3-P6); this file must stay thin.
 */
import { app, BrowserWindow } from "electron";
import path from "node:path";
import { createMainWindow, getAppIconPath, preloadFile, registerWindowIpc } from "./window";
import { registerCoreIpc, broadcastCoreEvent } from "./ipc/core";
import { registerSystemIpc } from "./ipc/system";
import {
  destroyBrowserViews,
  destroyBrowserViewsForWindow,
  destroyBrowserGroup,
  registerBrowserIpc,
} from "./ipc/browser";
import { unregisterDesktopShortcuts } from "./ipc/desktop";
import { CoreProcessManager } from "./core-process";
import { startStaticServer } from "./static-server";

const DEV_SERVER_URL = process.env.AGENTCABIN_DEV_SERVER_URL ?? "http://localhost:1420";
const RENDERER_DIST = path.join(__dirname, "..", "..", "build");

// ── Crash-dialog guards ──
// When the app is launched from a terminal that later closes (or via `open`
// with dead fds), stdout/stderr become broken pipes. Any console.* write then
// throws `write EIO` synchronously, which pops the "A JavaScript error
// occurred in the main process" dialog. None of these failures are fatal to
// the shell, so swallow pipe errors and keep uncaught exceptions from
// surfacing as a dialog.
for (const stream of [process.stdout, process.stderr]) {
  stream?.on?.("error", (err: NodeJS.ErrnoException) => {
    if (err.code === "EIO" || err.code === "EPIPE") return;
    throw err;
  });
}

function safeLog(line: string): void {
  try {
    process.stderr?.write?.(`${line}\n`);
  } catch {
    // stderr is broken — nowhere left to log in-process.
  }
}

process.on("uncaughtException", (err) => {
  safeLog(`[main] uncaughtException: ${err?.stack ?? err}`);
});
process.on("unhandledRejection", (reason) => {
  safeLog(`[main] unhandledRejection: ${reason}`);
});

import { petWindowManager } from "./pet-window";

const coreProcess = new CoreProcessManager((event, payload) => {
  broadcastCoreEvent(event, payload);
  if (event === "browser-event" && payload && typeof payload === "object") {
    const browserEvent = payload as { eventType?: unknown; runId?: unknown };
    if (browserEvent.eventType === "session_closed" && typeof browserEvent.runId === "string") {
      void destroyBrowserGroup(browserEvent.runId).catch((error: unknown) => {
        safeLog(`[browser] failed to destroy closed Run group: ${String(error)}`);
      });
    }
  }
  if (event === "bus-event") {
    petWindowManager.handleBusEvent(payload);
  }
});

let mainWindow: BrowserWindow | null = null;

function isDev(): boolean {
  return !!process.env.AGENTCABIN_DEV_SERVER_URL && !app.isPackaged;
}

async function createWindow(): Promise<void> {
  const window = createMainWindow(preloadFile());
  mainWindow = window;
  window.on("closed", () => {
    if (mainWindow === window) mainWindow = null;
    void destroyBrowserViewsForWindow(window).catch((error: unknown) => {
      safeLog(`[browser] failed to clean up views for closed window: ${String(error)}`);
    });
  });

  let appUrl = DEV_SERVER_URL;
  if (isDev()) {
    await window.loadURL(DEV_SERVER_URL);
    window.agentcabinAllowedOrigins = [new URL(DEV_SERVER_URL).origin];
    window.webContents.openDevTools({ mode: "detach" });
  } else {
    const { url } = await startStaticServer(RENDERER_DIST);
    appUrl = url;
    mainWindow.agentcabinAllowedOrigins = [url];
    await window.loadURL(url);
  }

  petWindowManager.init(preloadFile(), appUrl);
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  console.warn(
    "[main] Another instance of AgentCabin is already running. Quitting duplicate instance.",
  );
  app.quit();
} else {
  app.on("second-instance", () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });

  app.whenReady().then(async () => {
    if (process.platform === "darwin" && app.dock) {
      const icon = getAppIconPath();
      if (icon) {
        try {
          app.dock.setIcon(icon);
        } catch (err) {
          console.warn("[main] Failed to set dock icon:", err);
        }
      }
    }

    registerWindowIpc(() => mainWindow);
    registerSystemIpc(() => mainWindow);
    registerCoreIpc(coreProcess, () => mainWindow);
    registerBrowserIpc(
      () => mainWindow,
      (method, params) => coreProcess.invokeWhenReady(method, params ?? {}),
    );

    try {
      await createWindow();
    } catch (err) {
      const { dialog } = await import("electron");
      dialog.showErrorBox("AgentCabin failed to start", String(err));
      app.quit();
      return;
    }

    // Start the headless Rust core in the background; renderer invokes fail
    // with a "starting" message until the READY line arrives.
    coreProcess
      .start()
      .then(async () => {
        try {
          const settings = (await coreProcess.invoke("get_user_settings", {})) as Record<
            string,
            unknown
          >;
          if (settings) {
            petWindowManager.initSettings(settings);
            if (petWindowManager.getSettings().petEnabled) {
              void petWindowManager.createOrShow();
            }
          }
        } catch (err) {
          console.warn("[pet] Failed to load startup pet settings:", err);
        }
      })
      .catch((err) => {
        console.error(`[core-process] initial start failed: ${err.message}`);
      });

    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) {
        void createWindow();
      }
    });
  });

  app.on("window-all-closed", () => {
    if (process.platform !== "darwin" || isDev()) {
      app.quit();
    } else {
      // macOS convention: keep the app alive without windows.
      mainWindow = null;
    }
  });

  let coreStopped = false;
  app.on("before-quit", (event) => {
    unregisterDesktopShortcuts();
    if (coreStopped) return;
    // Give the core a chance to shut down its actors gracefully, then exit.
    event.preventDefault();
    coreStopped = true;
    void destroyBrowserViews()
      .catch((error: unknown) => {
        safeLog(`[browser] failed to clean up views during shutdown: ${String(error)}`);
      })
      .then(() => coreProcess.stop())
      .then(() => app.exit(0));
  });
}
