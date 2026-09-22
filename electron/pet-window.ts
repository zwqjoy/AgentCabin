/**
 * Desktop Pet (Pet) Window Manager for AgentCabin Electron shell.
 *
 * Responsibilities:
 * - Owns the transparent, frameless, floating secondary BrowserWindow for /pet
 * - Manages window sizing (160x176 * scale), alwaysOnTop, skipTaskbar, macOS panel level
 * - Persists and restores pet position from ~/.agentcabin/pet_position.json
 * - Handles edge snapping (snap to monitor work-area boundaries when within 24px)
 * - Provides smooth relative movement (moveBy) for dragging
 * - Bridges settings (pet://settings) and run state (pet://state) to the pet window
 */
import { BrowserWindow, screen, shell } from "electron";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { broadcastCoreEvent } from "./ipc/core";

export const BASE_WIDTH = 160;
export const BASE_HEIGHT = 176;
export const MIN_SCALE = 0.3;
export const MAX_SCALE = 2.0;
export const EDGE_SNAP_THRESHOLD = 24;
export const DEFAULT_MARGIN = 20;

export const PATROL_TICK_MS = 50;
export const SPEED_PX_PER_SECOND = 28.0;
export const MIN_WALK_SECS = 20;
export const MAX_WALK_SECS = 55;
export const MIN_SHORT_REST_SECS = 3;
export const MAX_SHORT_REST_SECS = 9;
export const LONG_REST_CHANCE = 0.18;
export const MIN_LONG_REST_SECS = 20;
export const MAX_LONG_REST_SECS = 180;
export const MAX_TURN_RAD_PER_SEC = 0.70;
export const EDGE_REPEL_PX = 120.0;
export const EDGE_REPEL_STRENGTH = 1.8;
export const EDGE_MARGIN_PX = 8;

function randomRestDuration(pauseMin: number): number {
  if (Math.random() < LONG_REST_CHANCE) {
    const configuredMax = Math.max(MIN_LONG_REST_SECS, Math.min(MAX_LONG_REST_SECS, pauseMin * 60));
    const range = configuredMax - MIN_LONG_REST_SECS;
    return (MIN_LONG_REST_SECS + Math.random() * Math.max(0, range)) * 1000;
  } else {
    return (MIN_SHORT_REST_SECS + Math.random() * (MAX_SHORT_REST_SECS - MIN_SHORT_REST_SECS)) * 1000;
  }
}

function randomWalkDuration(): number {
  return (MIN_WALK_SECS + Math.random() * (MAX_WALK_SECS - MIN_WALK_SECS)) * 1000;
}

function angleDiff(to: number, from: number): number {
  let diff = (to - from) % (Math.PI * 2);
  if (diff < 0) diff += Math.PI * 2;
  return diff > Math.PI ? diff - Math.PI * 2 : diff;
}

export interface PetPosition {
  x: number;
  y: number;
}

export interface PetSettingsPayload {
  petEnabled: boolean;
  petScale: number;
  petAlwaysOnTop: boolean;
  petId: string;
  petPatrolEnabled: boolean;
  petPatrolPauseMin: number;
  petSnapToEdge: boolean;
  petClickInteractionEnabled: boolean;
  petGrokbotColor: string;
  petGrokbotShape: string;
  petGrokbotParts: string[];
  petGrokbotAccessories: string[];
}

export type PetMode = "idle" | "waiting" | "running" | "review" | "failed";

export interface PetAggregateState {
  mode: PetMode;
  runningCount: number;
  errorCount: number;
  activeRunId: string | null;
  timestamp: number;
}

export function normalizeScale(scale?: unknown): number {
  if (typeof scale !== "number" || !Number.isFinite(scale)) {
    return 1.0;
  }
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

export function petLogicalSize(scale: number): { width: number; height: number } {
  const s = normalizeScale(scale);
  return {
    width: Math.round(BASE_WIDTH * s),
    height: Math.round(BASE_HEIGHT * s),
  };
}

function getPositionFilePath(): string {
  const dataDir = process.env.AGENTCABIN_DATA_DIR || path.join(os.homedir(), ".agentcabin");
  return path.join(dataDir, "pet_position.json");
}

export function loadPetPosition(): PetPosition | null {
  try {
    const file = getPositionFilePath();
    if (fs.existsSync(file)) {
      const content = fs.readFileSync(file, "utf8");
      const parsed = JSON.parse(content) as Record<string, unknown>;
      if (typeof parsed.x === "number" && typeof parsed.y === "number") {
        return { x: Math.round(parsed.x), y: Math.round(parsed.y) };
      }
    }
  } catch (err) {
    console.warn("[pet-window] Failed to load pet position:", err);
  }
  return null;
}

export function savePetPosition(pos: PetPosition): void {
  try {
    const file = getPositionFilePath();
    const dir = path.dirname(file);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(file, JSON.stringify({ x: Math.round(pos.x), y: Math.round(pos.y) }, null, 2), "utf8");
  } catch (err) {
    console.warn("[pet-window] Failed to save pet position:", err);
  }
}

export function snapPositionToEdges(
  x: number,
  y: number,
  winW: number,
  winH: number,
  monX: number,
  monY: number,
  monW: number,
  monH: number,
  threshold = EDGE_SNAP_THRESHOLD,
): { x: number; y: number } {
  const maxX = monW > winW ? monX + monW - winW : monX;
  const maxY = monH > winH ? monY + monH - winH : monY;

  const snapAxis = (value: number, min: number, max: number) => {
    const clamped = Math.max(min, Math.min(max, value));
    const distToMin = Math.abs(clamped - min);
    const distToMax = Math.abs(max - clamped);
    if (distToMin <= threshold) return min;
    if (distToMax <= threshold) return max;
    return clamped;
  };

  return {
    x: snapAxis(x, monX, maxX),
    y: snapAxis(y, monY, maxY),
  };
}

export function clampPositionToDisplay(
  x: number,
  y: number,
  winW: number,
  winH: number,
  workArea: { x: number; y: number; width: number; height: number },
): { x: number; y: number } {
  const minX = workArea.x;
  const maxX = Math.max(workArea.x, workArea.x + workArea.width - winW);
  const minY = workArea.y;
  const maxY = Math.max(workArea.y, workArea.y + workArea.height - winH);

  return {
    x: Math.max(minX, Math.min(maxX, x)),
    y: Math.max(minY, Math.min(maxY, y)),
  };
}

export class PetWindowManager {
  private petWindow: BrowserWindow | null = null;
  private preloadPath = path.join(__dirname, "preload.cjs");
  private baseUrl = process.env.AGENTCABIN_DEV_SERVER_URL || "http://localhost:1420";
  private currentSettings: PetSettingsPayload = {
    petEnabled: false,
    petScale: 1.0,
    petAlwaysOnTop: true,
    petId: "grokbot",
    petPatrolEnabled: true,
    petPatrolPauseMin: 5,
    petSnapToEdge: false,
    petClickInteractionEnabled: true,
    petGrokbotColor: "blue",
    petGrokbotShape: "blob",
    petGrokbotParts: [],
    petGrokbotAccessories: [],
  };

  private currentState: PetAggregateState = {
    mode: "idle",
    runningCount: 0,
    errorCount: 0,
    activeRunId: null,
    timestamp: Date.now(),
  };

  private runModes = new Map<string, { mode: PetMode; expiresAt?: number }>();
  private holdTimer: NodeJS.Timeout | null = null;
  private isDragging = false;
  private dragTimer: NodeJS.Timeout | null = null;
  private patrolTimer: NodeJS.Timeout | null = null;
  private patrolPos: { x: number; y: number } | null = null;
  private patrolHeading: number = Math.random() * Math.PI * 2;
  private patrolPhase: { kind: "walking" | "resting"; until: number } = {
    kind: "walking",
    until: Date.now() + 20000,
  };
  private lastPatrolSaveTime = 0;

  init(preloadPath: string, baseUrl: string): void {
    if (preloadPath) this.preloadPath = preloadPath;
    if (baseUrl) this.baseUrl = baseUrl;
  }

  setBaseUrl(url: string): void {
    if (!url) return;
    this.baseUrl = url;
    if (this.petWindow && !this.petWindow.isDestroyed()) {
      this.petWindow.agentcabinAllowedOrigins = [new URL(url).origin];
      void this.petWindow.loadURL(`${url}/pet`);
    }
  }

  getPetWindow(): BrowserWindow | null {
    if (this.petWindow?.isDestroyed()) {
      this.petWindow = null;
    }
    return this.petWindow;
  }

  getSettings(): PetSettingsPayload {
    return { ...this.currentSettings };
  }

  getCurrentState(): PetAggregateState {
    return { ...this.currentState };
  }

  /** Initialize settings from a user settings object (from core or storage). */
  initSettings(raw: Record<string, unknown>): void {
    this.currentSettings = this.extractPetSettings(raw, this.currentSettings);
  }

  /** Convert a raw settings patch or UserSettings object into PetSettingsPayload. */
  extractPetSettings(
    source: Record<string, unknown>,
    fallback: PetSettingsPayload,
  ): PetSettingsPayload {
    const b = (k1: string, k2: string, def: boolean) => {
      if (typeof source[k1] === "boolean") return source[k1] as boolean;
      if (typeof source[k2] === "boolean") return source[k2] as boolean;
      return def;
    };
    const n = (k1: string, k2: string, def: number) => {
      if (typeof source[k1] === "number" && Number.isFinite(source[k1])) return source[k1] as number;
      if (typeof source[k2] === "number" && Number.isFinite(source[k2])) return source[k2] as number;
      return def;
    };
    const s = (k1: string, k2: string, def: string) => {
      if (typeof source[k1] === "string") return source[k1] as string;
      if (typeof source[k2] === "string") return source[k2] as string;
      return def;
    };
    const arr = (k1: string, k2: string, def: string[]) => {
      if (Array.isArray(source[k1])) return source[k1] as string[];
      if (Array.isArray(source[k2])) return source[k2] as string[];
      return def;
    };

    return {
      petEnabled: b("pet_enabled", "petEnabled", fallback.petEnabled),
      petScale: normalizeScale(n("pet_scale", "petScale", fallback.petScale)),
      petAlwaysOnTop: b("pet_always_on_top", "petAlwaysOnTop", fallback.petAlwaysOnTop),
      petId: s("pet_id", "petId", fallback.petId),
      petPatrolEnabled: b("pet_patrol_enabled", "petPatrolEnabled", fallback.petPatrolEnabled),
      petPatrolPauseMin: n("pet_patrol_pause_min", "petPatrolPauseMin", fallback.petPatrolPauseMin),
      petSnapToEdge: b("pet_snap_to_edge", "petSnapToEdge", fallback.petSnapToEdge),
      petClickInteractionEnabled: b(
        "pet_click_interaction_enabled",
        "petClickInteractionEnabled",
        fallback.petClickInteractionEnabled,
      ),
      petGrokbotColor: s("pet_grokbot_color", "petGrokbotColor", fallback.petGrokbotColor),
      petGrokbotShape: s("pet_grokbot_shape", "petGrokbotShape", fallback.petGrokbotShape),
      petGrokbotParts: arr("pet_grokbot_parts", "petGrokbotParts", fallback.petGrokbotParts),
      petGrokbotAccessories: arr(
        "pet_grokbot_accessories",
        "petGrokbotAccessories",
        fallback.petGrokbotAccessories,
      ),
    };
  }

  /** Apply incoming settings patch from renderer. */
  applySettingsPatch(patch: Record<string, unknown>, fullSettings?: Record<string, unknown>): void {
    const prevEnabled = this.currentSettings.petEnabled;
    const prevScale = this.currentSettings.petScale;
    const prevAlwaysOnTop = this.currentSettings.petAlwaysOnTop;

    if (fullSettings) {
      this.currentSettings = this.extractPetSettings(fullSettings, this.currentSettings);
    } else {
      this.currentSettings = this.extractPetSettings(patch, this.currentSettings);
    }

    const hasPetChange = Object.keys(patch).some((k) =>
      k.startsWith("pet_") || k.startsWith("pet")
    );

    if (hasPetChange || fullSettings) {
      // 1. Toggle visibility
      if (this.currentSettings.petEnabled) {
        if (!prevEnabled || !this.petWindow || this.petWindow.isDestroyed()) {
          void this.createOrShow();
        } else {
          this.petWindow.showInactive();
        }
      } else {
        this.hide();
      }

      // 2. Scale change
      if (this.petWindow && !this.petWindow.isDestroyed() && this.currentSettings.petScale !== prevScale) {
        const { width, height } = petLogicalSize(this.currentSettings.petScale);
        const bounds = this.petWindow.getBounds();
        this.petWindow.setBounds({
          x: bounds.x,
          y: bounds.y,
          width,
          height,
        });
      }

      // 3. Always on top change
      if (this.petWindow && !this.petWindow.isDestroyed() && this.currentSettings.petAlwaysOnTop !== prevAlwaysOnTop) {
        this.petWindow.setAlwaysOnTop(this.currentSettings.petAlwaysOnTop, "floating");
      }

      // 5. Patrol setting or general pet change
      this.syncPatrol();

      // Broadcast pet://settings to all windows so pet UI updates immediately
      broadcastCoreEvent("pet://settings", this.currentSettings);
    }
  }

  /** Create or show the Desktop Pet window. */
  async createOrShow(): Promise<BrowserWindow> {
    const url = this.baseUrl || process.env.AGENTCABIN_DEV_SERVER_URL || "http://localhost:1420";
    const preload = this.preloadPath || path.join(__dirname, "preload.cjs");

    if (this.petWindow && !this.petWindow.isDestroyed()) {
      const { width, height } = petLogicalSize(this.currentSettings.petScale);
      this.petWindow.setSize(width, height);
      this.petWindow.setAlwaysOnTop(this.currentSettings.petAlwaysOnTop, "floating");
      const currentUrl = this.petWindow.webContents.getURL();
      if (!currentUrl || currentUrl === "about:blank") {
        this.petWindow.agentcabinAllowedOrigins = [new URL(url).origin];
        await this.petWindow.loadURL(`${url}/pet`);
      }
      this.petWindow.showInactive();
      return this.petWindow;
    }

    const size = petLogicalSize(this.currentSettings.petScale);

    const win = new BrowserWindow({
      title: "Desktop Pet",
      width: size.width,
      height: size.height,
      show: false,
      frame: false,
      transparent: true,
      backgroundColor: "#00000000",
      hasShadow: false,
      resizable: false,
      // On macOS, skipTaskbar: true calls app.dock.hide() which hides the ENTIRE application
      // from the Dock. macOS only has per-app Dock icons anyway, so skipTaskbar is only for Windows/Linux.
      skipTaskbar: process.platform !== "darwin",
      alwaysOnTop: this.currentSettings.petAlwaysOnTop,
      webPreferences: {
        preload,
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webviewTag: false,
        spellcheck: false,
      },
    });

    this.petWindow = win;

    if (process.platform === "darwin") {
      // NOTE: Do NOT pass `{ visibleOnFullScreen: true }`!
      // In Electron on macOS, `visibleOnFullScreen: true` switches NSApplicationActivationPolicy
      // to NSApplicationActivationPolicyAccessory, which removes the entire application
      // from the macOS Dock and Cmd+Tab switcher.
      win.setVisibleOnAllWorkspaces(true);
      win.setAlwaysOnTop(this.currentSettings.petAlwaysOnTop, "floating");
    }

    // Determine initial position
    const saved = loadPetPosition();
    let display = screen.getPrimaryDisplay();
    let posX = display.workArea.x + display.workArea.width - size.width - DEFAULT_MARGIN;
    let posY = display.workArea.y + display.workArea.height - size.height - DEFAULT_MARGIN;

    if (saved) {
      const matched = screen.getDisplayMatching({
        x: saved.x,
        y: saved.y,
        width: size.width,
        height: size.height,
      });
      if (matched) display = matched;
      posX = saved.x;
      posY = saved.y;
    }

    if (this.currentSettings.petSnapToEdge) {
      const snapped = snapPositionToEdges(
        posX,
        posY,
        size.width,
        size.height,
        display.workArea.x,
        display.workArea.y,
        display.workArea.width,
        display.workArea.height,
      );
      posX = snapped.x;
      posY = snapped.y;
    }

    const clamped = clampPositionToDisplay(
      posX,
      posY,
      size.width,
      size.height,
      display.workArea,
    );

    win.setPosition(clamped.x, clamped.y);

    // Forward move events to the renderer so attachWindowDrag knows about movements
    let moveScheduled = false;
    win.on("move", () => {
      if (moveScheduled || win.isDestroyed()) return;
      moveScheduled = true;
      setImmediate(() => {
        moveScheduled = false;
        if (!win.isDestroyed()) {
          const { x, y } = win.getContentBounds();
          win.webContents.send("window:moved", { x, y });
        }
      });
    });

    win.webContents.setWindowOpenHandler(({ url: openUrl }) => {
      if (openUrl.startsWith("https://") || openUrl.startsWith("http://")) {
        void shell.openExternal(openUrl);
      }
      return { action: "deny" };
    });

    win.webContents.on("will-navigate", (event, navigateUrl) => {
      const allowed = win.agentcabinAllowedOrigins ?? [];
      let origin: string;
      try {
        origin = new URL(navigateUrl).origin;
      } catch {
        event.preventDefault();
        return;
      }
      if (!allowed.includes(origin)) {
        event.preventDefault();
        if (navigateUrl.startsWith("https://") || navigateUrl.startsWith("http://")) {
          void shell.openExternal(navigateUrl);
        }
      }
    });

    win.once("ready-to-show", () => {
      if (this.currentSettings.petEnabled && !win.isDestroyed()) {
        win.showInactive();
      }
    });

    win.on("closed", () => {
      this.stopDrag();
      this.stopPatrol();
      if (this.petWindow === win) {
        this.petWindow = null;
      }
    });

    win.agentcabinAllowedOrigins = [new URL(url).origin];
    await win.loadURL(`${url}/pet`);

    if (this.currentSettings.petEnabled && !win.isDestroyed()) {
      win.showInactive();
      this.syncPatrol();
    }

    return win;
  }

  hide(): void {
    this.stopDrag();
    this.stopPatrol();
    if (this.petWindow && !this.petWindow.isDestroyed()) {
      this.petWindow.hide();
    }
  }

  /** Called when the pet renderer sends pet://ready. */
  handlePetReady(): void {
    if (this.currentSettings.petEnabled && this.petWindow && !this.petWindow.isDestroyed()) {
      this.petWindow.showInactive();
    }
    // Push current snapshot
    broadcastCoreEvent("pet://settings", this.currentSettings);
    broadcastCoreEvent("pet://state", this.currentState);
    this.syncPatrol();
  }

  /** Start smooth, native 60fps cursor-following drag. */
  startDrag(): void {
    if (!this.petWindow || this.petWindow.isDestroyed()) return;
    this.isDragging = true;
    this.stopPatrol();

    if (this.dragTimer) {
      clearInterval(this.dragTimer);
      this.dragTimer = null;
    }

    const initialCursor = screen.getCursorScreenPoint();
    const initialBounds = this.petWindow.getBounds();
    const offsetX = initialCursor.x - initialBounds.x;
    const offsetY = initialCursor.y - initialBounds.y;
    const startTime = Date.now();

    this.dragTimer = setInterval(() => {
      if (
        !this.isDragging ||
        !this.petWindow ||
        this.petWindow.isDestroyed() ||
        Date.now() - startTime > 60000
      ) {
        this.stopDrag();
        return;
      }

      const cur = screen.getCursorScreenPoint();
      const targetX = cur.x - offsetX;
      const targetY = cur.y - offsetY;

      const display =
        screen.getDisplayMatching({
          x: targetX,
          y: targetY,
          width: initialBounds.width,
          height: initialBounds.height,
        }) ?? screen.getPrimaryDisplay();

      const clamped = clampPositionToDisplay(
        targetX,
        targetY,
        initialBounds.width,
        initialBounds.height,
        display.workArea,
      );

      this.petWindow.setPosition(clamped.x, clamped.y);
    }, 16);
  }

  /** Stop drag tracking, snap if configured, save position and resume patrol. */
  stopDrag(): void {
    this.isDragging = false;
    if (this.dragTimer) {
      clearInterval(this.dragTimer);
      this.dragTimer = null;
    }
    if (this.currentSettings.petSnapToEdge) {
      this.snapToEdge();
    } else if (this.petWindow && !this.petWindow.isDestroyed()) {
      const bounds = this.petWindow.getBounds();
      savePetPosition({ x: bounds.x, y: bounds.y });
    }
    this.syncPatrol();
  }

  setDragging(dragging: boolean): void {
    if (dragging) {
      this.startDrag();
    } else {
      this.stopDrag();
    }
  }

  /** Move window by delta pixels (called during drag fallback). */
  moveBy(dx: number, dy: number): void {
    if (!this.petWindow || this.petWindow.isDestroyed()) return;
    const bounds = this.petWindow.getBounds();
    const targetX = bounds.x + dx;
    const targetY = bounds.y + dy;

    const display = screen.getDisplayMatching({
      x: targetX,
      y: targetY,
      width: bounds.width,
      height: bounds.height,
    }) ?? screen.getPrimaryDisplay();

    const clamped = clampPositionToDisplay(
      targetX,
      targetY,
      bounds.width,
      bounds.height,
      display.workArea,
    );

    this.petWindow.setPosition(clamped.x, clamped.y);
  }

  /** Snap window to edge if within threshold. */
  snapToEdge(): void {
    if (!this.petWindow || this.petWindow.isDestroyed()) return;
    const bounds = this.petWindow.getBounds();
    const display = screen.getDisplayMatching(bounds) ?? screen.getPrimaryDisplay();

    const snapped = snapPositionToEdges(
      bounds.x,
      bounds.y,
      bounds.width,
      bounds.height,
      display.workArea.x,
      display.workArea.y,
      display.workArea.width,
      display.workArea.height,
    );

    if (snapped.x !== bounds.x || snapped.y !== bounds.y) {
      this.petWindow.setPosition(snapped.x, snapped.y);
    }
    savePetPosition(snapped);
  }

  /**
   * Synchronize patrol state based on settings, active window and aggregate mode.
   * Mirrors the edge-repulsion random wander algorithm from src-tauri/src/pet/patrol.rs.
   */
  syncPatrol(): void {
    const shouldPatrol =
      this.currentSettings.petEnabled &&
      this.currentSettings.petPatrolEnabled &&
      !this.isDragging &&
      this.currentState.mode === "idle" &&
      !!this.petWindow &&
      !this.petWindow.isDestroyed();

    if (!shouldPatrol || !this.petWindow) {
      this.stopPatrol();
      return;
    }

    if (this.patrolTimer) return;

    const bounds = this.petWindow.getBounds();
    this.patrolPos = { x: bounds.x, y: bounds.y };
    this.patrolHeading = Math.random() * Math.PI * 2;
    this.patrolPhase = { kind: "walking", until: Date.now() + randomWalkDuration() };

    const dt = PATROL_TICK_MS / 1000.0;
    const step = SPEED_PX_PER_SECOND * dt;
    const maxTurn = MAX_TURN_RAD_PER_SEC * dt;

    this.patrolTimer = setInterval(() => {
      if (
        !this.petWindow ||
        this.petWindow.isDestroyed() ||
        !this.currentSettings.petEnabled ||
        !this.currentSettings.petPatrolEnabled
      ) {
        this.stopPatrol();
        return;
      }

      if (this.isDragging || this.currentState.mode !== "idle") {
        const b = this.petWindow.getBounds();
        this.patrolPos = { x: b.x, y: b.y };
        this.patrolHeading = Math.random() * Math.PI * 2;
        this.patrolPhase = { kind: "walking", until: Date.now() + randomWalkDuration() };
        return;
      }

      const now = Date.now();
      if (now >= this.patrolPhase.until) {
        if (this.patrolPhase.kind === "walking") {
          this.patrolPhase = {
            kind: "resting",
            until: now + randomRestDuration(this.currentSettings.petPatrolPauseMin),
          };
        } else {
          this.patrolPhase = {
            kind: "walking",
            until: now + randomWalkDuration(),
          };
          this.patrolHeading = Math.random() * Math.PI * 2;
        }
      }

      if (this.patrolPhase.kind === "resting") {
        return;
      }

      const bounds = this.petWindow.getBounds();
      const display = screen.getDisplayMatching(bounds) ?? screen.getPrimaryDisplay();
      const mon = display.workArea;

      const edgeMargin = this.currentSettings.petSnapToEdge ? 0 : EDGE_MARGIN_PX;
      const left = mon.x + edgeMargin;
      const right = mon.x + mon.width - bounds.width - edgeMargin;
      const top = mon.y + edgeMargin;
      const bottom = mon.y + mon.height - bounds.height - edgeMargin;

      if (right <= left || bottom <= top) return;

      const cx = (left + right) / 2.0;
      const cy = (top + bottom) / 2.0;
      let x = this.patrolPos ? this.patrolPos.x : bounds.x;
      let y = this.patrolPos ? this.patrolPos.y : bounds.y;

      const towardCenter = Math.atan2(cy - y, cx - x);

      const distL = Math.max(0, Math.min(1, (x - left) / EDGE_REPEL_PX));
      const distR = Math.max(0, Math.min(1, (right - x) / EDGE_REPEL_PX));
      const distT = Math.max(0, Math.min(1, (y - top) / EDGE_REPEL_PX));
      const distB = Math.max(0, Math.min(1, (bottom - y) / EDGE_REPEL_PX));

      const proximity = Math.max(1.0 - distL, 1.0 - distR, 1.0 - distT, 1.0 - distB);

      if (proximity > 0.0) {
        const diff = angleDiff(towardCenter, this.patrolHeading);
        const correction = Math.min(Math.abs(diff), EDGE_REPEL_STRENGTH * dt * proximity) * Math.sign(diff);
        this.patrolHeading += correction;
      }

      const turn = (Math.random() * 2 - 1) * maxTurn;
      this.patrolHeading = (this.patrolHeading + turn) % (Math.PI * 2);

      x = Math.max(left, Math.min(right, x + Math.cos(this.patrolHeading) * step));
      y = Math.max(top, Math.min(bottom, y + Math.sin(this.patrolHeading) * step));
      this.patrolPos = { x, y };

      const px = Math.round(x);
      const py = Math.round(y);
      if (px !== bounds.x || py !== bounds.y) {
        this.petWindow.setPosition(px, py);
      }

      if (now - this.lastPatrolSaveTime > 10000) {
        this.lastPatrolSaveTime = now;
        savePetPosition({ x: px, y: py });
      }
    }, PATROL_TICK_MS);
  }

  stopPatrol(): void {
    if (this.patrolTimer) {
      clearInterval(this.patrolTimer);
      this.patrolTimer = null;
    }
  }

  /** Bridge run bus-events into pet://state. */
  handleBusEvent(rawPayload: unknown): void {
    if (!rawPayload || typeof rawPayload !== "object") return;
    const payload = rawPayload as { type?: string; run_id?: string; state?: string };

    if (payload.type !== "run_state" || typeof payload.run_id !== "string") {
      return;
    }

    const runId = payload.run_id;
    const rawState = (payload.state ?? "").toLowerCase();

    let mode: PetMode = "idle";
    switch (rawState) {
      case "spawning":
      case "loading":
        mode = "waiting";
        break;
      case "running":
        mode = "running";
        break;
      case "completed":
        mode = "review";
        break;
      case "failed":
      case "cancelled":
        mode = "failed";
        break;
      case "idle":
      case "stopped":
      case "ready":
      case "empty":
        mode = "idle";
        break;
      default:
        mode = "idle";
    }

    if (mode === "idle") {
      const prev = this.runModes.get(runId);
      if (prev?.mode === "running") {
        mode = "review";
      }
    }

    if (mode === "review" || mode === "failed") {
      this.runModes.set(runId, {
        mode,
        expiresAt: Date.now() + 4000,
      });
      if (this.holdTimer) clearTimeout(this.holdTimer);
      this.holdTimer = setTimeout(() => {
        this.cleanExpiredRuns();
      }, 4100);
    } else if (mode === "idle") {
      this.runModes.delete(runId);
    } else {
      this.runModes.set(runId, { mode });
    }

    this.recomputeAggregateState();
  }

  private cleanExpiredRuns(): void {
    const now = Date.now();
    for (const [runId, info] of this.runModes.entries()) {
      if (info.expiresAt && info.expiresAt <= now) {
        this.runModes.delete(runId);
      }
    }
    this.recomputeAggregateState();
  }

  private recomputeAggregateState(): void {
    let runningCount = 0;
    let errorCount = 0;
    let hasWaiting = false;
    let activeRunId: string | null = null;
    let topPriority = 0;

    const priorityOf = (m: PetMode) => {
      switch (m) {
        case "failed": return 5;
        case "running": return 4;
        case "waiting": return 3;
        case "review": return 2;
        default: return 1;
      }
    };

    for (const [runId, info] of this.runModes.entries()) {
      if (info.mode === "running") runningCount++;
      else if (info.mode === "failed") errorCount++;
      else if (info.mode === "waiting") hasWaiting = true;

      const p = priorityOf(info.mode);
      if (p > topPriority) {
        topPriority = p;
        activeRunId = runId;
      }
    }

    let mode: PetMode = "idle";
    if (errorCount > 0) {
      mode = "failed";
    } else if (runningCount > 0) {
      mode = "running";
    } else if (hasWaiting) {
      mode = "waiting";
    } else if (topPriority === 2) {
      mode = "review";
    }

    const nextState: PetAggregateState = {
      mode,
      runningCount,
      errorCount,
      activeRunId,
      timestamp: Date.now(),
    };

    this.currentState = nextState;
    broadcastCoreEvent("pet://state", nextState);
    this.syncPatrol();
  }
}

export const petWindowManager = new PetWindowManager();
