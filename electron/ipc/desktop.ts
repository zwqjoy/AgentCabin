/**
 * Desktop capability parity layer for Electron (P6).
 *
 * Intercepts desktop-only commands that cannot or should not be fulfilled by
 * the headless Rust core over HTTP:
 * - capture_screenshot: native macOS screencapture -> base64 -> screenshot-taken event
 * - update_screenshot_hotkey: globalShortcut registration
 * - check_for_updates: update check & graceful fallback
 * - get_web_server_token: returns local core server token
 */
import { app, BrowserWindow, globalShortcut } from "electron";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { broadcastCoreEvent } from "./core";
import type { CoreProcessManager } from "../core-process";

const execFileAsync = promisify(execFile);

export interface UpdateInfo {
  hasUpdate: boolean;
  latestVersion: string;
  currentVersion: string;
  downloadUrl: string;
}

let activeScreenshotAccelerator: string | null = null;

/** Convert shortcut strings like "Cmd+Ctrl+S" or "Ctrl+Alt+S" to Electron Accelerator format. */
function toElectronAccelerator(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => {
      const trimmed = part.trim();
      const lower = trimmed.toLowerCase();
      if (lower === "cmd" || lower === "command" || lower === "super") return "Cmd";
      if (lower === "ctrl" || lower === "control") return "Ctrl";
      if (lower === "alt" || lower === "option") return "Alt";
      if (lower === "shift") return "Shift";
      return trimmed;
    })
    .join("+");
}

/** Capture interactive screenshot on macOS and broadcast screenshot-taken event. */
export async function captureScreenshot(getWindow: () => BrowserWindow | null): Promise<void> {
  if (process.platform !== "darwin") {
    console.warn("[desktop] capture_screenshot is only supported on macOS");
    throw new Error("Screenshot capture is only supported on macOS");
  }

  const tempPath = path.join(os.tmpdir(), `agentcabin-screenshot-${Date.now()}.png`);

  try {
    // /usr/sbin/screencapture -i launches interactive selection (crosshair)
    await execFileAsync("/usr/sbin/screencapture", ["-i", tempPath]);
  } catch (_err: unknown) {
    // screencapture returns non-zero when cancelled via ESC or interrupted
    console.debug?.("[desktop] screencapture finished with non-zero status (cancelled)");
  }

  if (!existsSync(tempPath)) {
    // User cancelled selection with ESC — no screenshot taken
    return;
  }

  try {
    const data = await fs.readFile(tempPath);
    const contentBase64 = data.toString("base64");

    // Focus and restore main window if needed
    const win = getWindow();
    if (win && !win.isDestroyed()) {
      if (win.isMinimized()) win.restore();
      win.show();
      win.focus();
    }

    const now = new Date();
    const pad = (n: number) => n.toString().padStart(2, "0");
    const filename = `screenshot-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}.png`;

    broadcastCoreEvent("screenshot-taken", {
      contentBase64,
      mediaType: "image/png",
      filename,
    });
  } catch (e) {
    console.warn("[desktop] failed to read or broadcast screenshot:", e);
  } finally {
    try {
      await fs.unlink(tempPath);
    } catch {
      // Ignore cleanup error
    }
  }
}

/** Register or update the global screenshot hotkey. */
export function updateScreenshotHotkey(
  getWindow: () => BrowserWindow | null,
  hotkey?: string | null,
): void {
  if (activeScreenshotAccelerator) {
    try {
      globalShortcut.unregister(activeScreenshotAccelerator);
    } catch (err) {
      console.warn(`[desktop] failed to unregister shortcut ${activeScreenshotAccelerator}:`, err);
    }
    activeScreenshotAccelerator = null;
  }

  if (!hotkey || hotkey === "disabled" || hotkey.trim() === "") {
    return;
  }

  const accelerator = toElectronAccelerator(hotkey);
  try {
    const success = globalShortcut.register(accelerator, () => {
      void captureScreenshot(getWindow);
    });
    if (success) {
      activeScreenshotAccelerator = accelerator;
      console.log(`[desktop] registered screenshot shortcut: ${accelerator}`);
    } else {
      console.warn(`[desktop] failed to register shortcut: ${accelerator}`);
    }
  } catch (err) {
    console.warn(`[desktop] error registering shortcut ${accelerator}:`, err);
  }
}

/** Unregister all desktop global shortcuts. */
export function unregisterDesktopShortcuts(): void {
  if (activeScreenshotAccelerator) {
    try {
      globalShortcut.unregister(activeScreenshotAccelerator);
    } catch {
      // Ignore
    }
    activeScreenshotAccelerator = null;
  }
  globalShortcut.unregisterAll();
}

/** Compare simple semver version strings (e.g. "4.2.0" > "4.1.0"). */
function isNewerVersion(latest: string, current: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/, "")
      .split("-")[0]
      .split(".")
      .map((num) => parseInt(num, 10) || 0);

  const [lMaj = 0, lMin = 0, lPatch = 0] = parse(latest);
  const [cMaj = 0, cMin = 0, cPatch = 0] = parse(current);

  if (lMaj !== cMaj) return lMaj > cMaj;
  if (lMin !== cMin) return lMin > cMin;
  return lPatch > cPatch;
}

/** Check for app updates against configured update endpoint or fallback safely. */
export async function checkForUpdates(): Promise<UpdateInfo> {
  const currentVersion = app.getVersion();
  const updateApiUrl = process.env.AGENTCABIN_UPDATE_API_URL;

  const fallback: UpdateInfo = {
    hasUpdate: false,
    latestVersion: "",
    currentVersion,
    downloadUrl: "",
  };

  if (!updateApiUrl) {
    return fallback;
  }

  try {
    const res = await fetch(updateApiUrl, {
      headers: { Accept: "application/vnd.github+json" },
      signal: AbortSignal.timeout(10_000),
    });

    if (!res.ok) {
      return fallback;
    }

    const data = (await res.json()) as {
      tag_name?: string;
      assets?: Array<{ browser_download_url?: string; name?: string }>;
    };

    const tagName = data.tag_name ?? "";
    if (!tagName) return fallback;

    let downloadUrl = "";
    if (Array.isArray(data.assets)) {
      const ext = process.platform === "darwin" ? ".dmg" : process.platform === "win32" ? ".exe" : ".AppImage";
      const matched = data.assets.find((a) => a.name?.endsWith(ext));
      if (matched?.browser_download_url) {
        downloadUrl = matched.browser_download_url;
      }
    }

    const hasUpdate = isNewerVersion(tagName, currentVersion);
    return {
      hasUpdate,
      latestVersion: tagName.replace(/^v/, ""),
      currentVersion,
      downloadUrl,
    };
  } catch (err) {
    console.warn("[desktop] update check failed:", err);
    return fallback;
  }
}

const INTERCEPTED_METHODS = new Set([
  "capture_screenshot",
  "update_screenshot_hotkey",
  "check_for_updates",
  "get_web_server_token",
  "save_pet_position",
  "load_pet_position",
  "snap_pet_to_edge",
  "set_pet_dragging",
  "move_pet_window",
  "toggle_pet_window",
]);

export function isDesktopIntercepted(method: string): boolean {
  return INTERCEPTED_METHODS.has(method);
}

export async function handleDesktopIntercept(
  method: string,
  params: unknown,
  core: CoreProcessManager,
  getWindow: () => BrowserWindow | null,
): Promise<unknown> {
  switch (method) {
    case "capture_screenshot":
      await captureScreenshot(getWindow);
      return { ok: true };

    case "update_screenshot_hotkey": {
      const payload = params as { hotkey?: string | null } | undefined;
      updateScreenshotHotkey(getWindow, payload?.hotkey ?? null);
      return { ok: true };
    }

    case "check_for_updates":
      return await checkForUpdates();

    case "get_web_server_token":
      return core.getToken();

    case "save_pet_position": {
      const { savePetPosition } = await import("../pet-window");
      const payload = params as { x?: number; y?: number } | undefined;
      if (typeof payload?.x === "number" && typeof payload?.y === "number") {
        savePetPosition({ x: payload.x, y: payload.y });
      }
      return { ok: true };
    }

    case "load_pet_position": {
      const { loadPetPosition } = await import("../pet-window");
      return loadPetPosition();
    }

    case "snap_pet_to_edge": {
      const { petWindowManager } = await import("../pet-window");
      petWindowManager.snapToEdge();
      return { ok: true };
    }

    case "set_pet_dragging": {
      const { petWindowManager } = await import("../pet-window");
      const payload = params as { dragging?: boolean } | undefined;
      petWindowManager.setDragging(!!payload?.dragging);
      return { ok: true };
    }

    case "move_pet_window": {
      const { petWindowManager } = await import("../pet-window");
      const payload = params as { dx?: number; dy?: number } | undefined;
      if (typeof payload?.dx === "number" && typeof payload?.dy === "number") {
        petWindowManager.moveBy(payload.dx, payload.dy);
      }
      return { ok: true };
    }

    case "toggle_pet_window": {
      const { petWindowManager } = await import("../pet-window");
      const payload = params as { enabled?: boolean } | undefined;
      if (payload?.enabled) {
        void petWindowManager.createOrShow();
      } else {
        petWindowManager.hide();
      }
      return { ok: true };
    }

    default:
      throw new Error(`Unhandled desktop intercepted method: ${method}`);
  }
}
