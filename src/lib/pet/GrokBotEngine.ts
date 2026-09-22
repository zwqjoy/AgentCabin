/**
 * GrokBotEngine – AgentCabin integration of the LaoA-GrokBot rendering engine.
 *
 * Derived from https://github.com/zhulin025/LaoA-GrokBot (MIT License)
 * Original author: 老A玩AI (zhulin025)
 *
 * This class manages:
 *  - Spring-interpolated morph animation between expression ring shapes
 *  - Eye gaze tracking (normalised -1..1 coordinates)
 *  - Blink timer
 *  - Expression pool auto-cycling (EXPR_CADENCE)
 *  - State changes (PetMode → GrokBot state key)
 *  - Quick jelly actions (CSS class triggers)
 */

import {
  EXPRESSIONS,
  POOLS,
  BLINK,
  EXPR_CADENCE,
  PET_MODE_TO_GROKBOT_STATE,
  PET_MODE_TO_QUICK_ACTION,
  type GrokBotStateKey,
  type GrokBotQuickAction,
} from "./grokbot-data";
import type { PetMode } from "./types";

export interface GrokBotFrame {
  /** Two rings of [x, y] points after morph interpolation */
  rings: number[][][];
  /** Current blink scale 0..1 (1 = fully open) */
  blinkScale: number;
  /** Horizontal gaze offset in SVG units */
  gazeX: number;
  /** Vertical gaze offset in SVG units */
  gazeY: number;
  /** Head rotation in degrees (for orbit/spinner states) */
  turn: number;
}

export type FrameCallback = (frame: GrokBotFrame) => void;

const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));

export class GrokBotEngine {
  // ── Morph spring state ──────────────────────────────────────────────────
  private current: number[][][] = EXPRESSIONS[0].map((r) => r.map((p) => [...p]));
  private target: number[][][] = EXPRESSIONS[0];
  private expression = 0;
  private morph = 1;
  private velocity = 0;

  // ── Timing ──────────────────────────────────────────────────────────────
  private last = performance.now();

  // ── Blink ────────────────────────────────────────────────────────────────
  private blinkStart = 0;
  private blinkTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Gaze ─────────────────────────────────────────────────────────────────
  private gazeX = 0;
  private gazeY = 0;
  private turn = 0;

  // ── State ─────────────────────────────────────────────────────────────────
  private activeState: GrokBotStateKey = "idle";
  private cadenceTimer: ReturnType<typeof setTimeout> | null = null;
  private quickTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Quick action host element (for CSS class toggling) ────────────────────
  private hostEl: HTMLElement | null = null;

  // ── rAF ──────────────────────────────────────────────────────────────────
  private rafId = 0;
  private stopped = false;
  private onFrame: FrameCallback;

  private requestFrame(cb: FrameRequestCallback): number {
    if (typeof requestAnimationFrame !== "undefined") {
      return requestAnimationFrame(cb);
    }
    return setTimeout(() => cb(performance.now()), 16) as unknown as number;
  }

  private cancelFrame(id: number) {
    if (typeof cancelAnimationFrame !== "undefined") {
      cancelAnimationFrame(id);
    } else {
      clearTimeout(id);
    }
  }

  constructor(onFrame: FrameCallback, hostEl?: HTMLElement) {
    this.onFrame = onFrame;
    this.hostEl = hostEl ?? null;
    this.rafId = this.requestFrame(this.tick);
    this.scheduleBlink();
  }

  // ── Public API ────────────────────────────────────────────────────────────

  /** Update from a PetMode (AgentCabin session state). */
  public setPetMode(mode: PetMode) {
    const stateKey = PET_MODE_TO_GROKBOT_STATE[mode] ?? "idle";
    const action = PET_MODE_TO_QUICK_ACTION[mode];
    this.setState(stateKey);
    if (action) this.triggerAction(action);
  }

  /** Directly set a GrokBot state key. */
  public setState(key: GrokBotStateKey) {
    if (key === this.activeState) return;
    this.activeState = key;
    this.clearCadence();
    this.clearBlink();

    const pool = POOLS[key];
    const next = pool.find((i) => i !== this.expression) ?? pool[0];
    this.chooseExpression(next);
    this.scheduleBlink();
    this.scheduleCadence();
  }

  /** Set normalised gaze direction. nx/ny in range [-1, 1]. */
  public setGaze(nx: number, ny: number) {
    this.gazeX = clamp(nx, -0.6, 0.6) * 22;
    this.gazeY = clamp(ny, -0.6, 0.6) * 14;
  }

  /** Clear gaze (reset to centre). */
  public clearGaze() {
    this.gazeX = 0;
    this.gazeY = 0;
  }

  /** Trigger a quick jelly CSS action on the host element. */
  public triggerAction(name: GrokBotQuickAction) {
    if (!this.hostEl) return;
    const cls = `grokbot-quick-${name}`;
    const ALL = ["bounce", "shake", "peek", "pinch", "squish", "wave"].map(
      (a) => `grokbot-quick-${a}`,
    );

    // Force reflow so re-triggering same class restarts animation
    ALL.forEach((c) => this.hostEl!.classList.remove(c));
    void this.hostEl.offsetWidth;
    this.hostEl.classList.add(cls);

    if (this.quickTimer) clearTimeout(this.quickTimer);
    const dur = name === "wave" ? 1350 : name === "pinch" ? 1200 : 950;
    this.quickTimer = setTimeout(() => {
      this.hostEl?.classList.remove(cls);
      this.quickTimer = null;
    }, dur);
  }

  private lastQuickAction: GrokBotQuickAction | null = null;

  /** Trigger a random jelly quick action + a fresh expression from the active state. */
  public triggerRandomAction(): GrokBotQuickAction {
    const ALL: GrokBotQuickAction[] = ["bounce", "wave", "peek", "pinch", "squish", "shake"];
    const candidates = this.lastQuickAction
      ? ALL.filter((action) => action !== this.lastQuickAction)
      : ALL;
    const action = candidates[Math.floor(Math.random() * candidates.length)];
    this.lastQuickAction = action;
    this.triggerAction(action);

    const pool = POOLS[this.activeState];
    if (pool && pool.length > 0) {
      const next = pool[Math.floor(Math.random() * pool.length)];
      this.chooseExpression(next);
    }
    return action;
  }

  /** Register (or replace) the host element used for CSS quick actions. */
  public setHostElement(el: HTMLElement | null) {
    this.hostEl = el;
  }

  public destroy() {
    this.stopped = true;
    if (this.rafId) this.cancelFrame(this.rafId);
    this.clearCadence();
    this.clearBlink();
    if (this.quickTimer) clearTimeout(this.quickTimer);
  }

  // ── Private helpers ───────────────────────────────────────────────────────

  private chooseExpression(index: number) {
    this.current = this.interpolatedRings();
    this.target = EXPRESSIONS[index];
    this.expression = index;
    this.morph = 0;
    this.velocity = 0;
  }

  private interpolatedRings(): number[][][] {
    return this.current.map((ring, e) =>
      ring.map((p, i) => [
        p[0] + (this.target[e][i][0] - p[0]) * clamp(this.morph, 0, 1),
        p[1] + (this.target[e][i][1] - p[1]) * clamp(this.morph, 0, 1),
      ]),
    );
  }

  private centroid(ring: number[][]): [number, number] {
    return ring.reduce(
      (a, p) => [a[0] + p[0] / ring.length, a[1] + p[1] / ring.length],
      [0, 0],
    ) as [number, number];
  }

  // ── Blink ─────────────────────────────────────────────────────────────────

  private scheduleBlink() {
    const range = BLINK[this.activeState];
    if (!range) return;
    const delay = range[0] + Math.random() * (range[1] - range[0]);
    this.blinkTimer = setTimeout(() => {
      this.blink();
      this.scheduleBlink();
    }, delay);
  }

  private blink() {
    this.blinkStart = performance.now();
  }

  private blinkScale(now: number): number {
    if (!this.blinkStart) return 1;
    const t = (now - this.blinkStart) / 320;
    if (t >= 1) {
      this.blinkStart = 0;
      return 1;
    }
    return Math.max(t < 0.42 ? 1 - t / 0.42 : (t - 0.42) / 0.58, 0.04);
  }

  private clearBlink() {
    if (this.blinkTimer) {
      clearTimeout(this.blinkTimer);
      this.blinkTimer = null;
    }
    this.blinkStart = 0;
  }

  // ── Cadence (auto expression pool cycling) ────────────────────────────────

  private scheduleCadence() {
    const range = EXPR_CADENCE[this.activeState];
    if (!range) return;
    const delay = range[0] + Math.random() * (range[1] - range[0]);
    this.cadenceTimer = setTimeout(() => {
      const pool = POOLS[this.activeState];
      const next = pool[Math.floor(Math.random() * pool.length)];
      this.chooseExpression(next);
      this.scheduleCadence();
    }, delay);
  }

  private clearCadence() {
    if (this.cadenceTimer) {
      clearTimeout(this.cadenceTimer);
      this.cadenceTimer = null;
    }
  }

  // ── rAF tick ──────────────────────────────────────────────────────────────

  private tick = (now: number) => {
    if (this.stopped) return;

    const dt = Math.min((now - this.last) / 1000, 0.1);
    this.last = now;

    // Spring integration (spring constant 49, damping 14 – same as original)
    this.velocity += (-14 * this.velocity - 49 * (this.morph - 1)) * dt;
    this.morph += this.velocity * dt;
    if (!Number.isFinite(this.morph)) {
      this.morph = 1;
      this.velocity = 0;
    }

    const shown = this.interpolatedRings();
    const bs = this.blinkScale(now);

    // Build frame rings with gaze + perspective transform baked in
    const rings: number[][][] = shown.map((ring) => {
      const c = this.centroid(ring);
      const base = Math.asin(clamp((c[0] - 114.2705) / 105, -1, 1));
      const longitude = base + (this.turn * Math.PI) / 180;
      const depth = Math.cos(longitude);
      const perspective = Math.max(depth, 0.02) / Math.max(Math.cos(base), 0.02);
      const tx = 114.2705 + 105 * Math.sin(longitude) + this.gazeX - c[0];
      const ty = c[1] + this.gazeY - c[1];

      return ring.map((p) => [
        // apply translation + perspective scale around centroid
        c[0] + (p[0] - c[0]) * clamp(perspective, 0.02, 2.4) + tx,
        c[1] + (p[1] - c[1]) * bs + ty,
      ]);
    });

    this.onFrame({
      rings,
      blinkScale: bs,
      gazeX: this.gazeX,
      gazeY: this.gazeY,
      turn: this.turn,
    });

    this.rafId = this.requestFrame(this.tick);
  };
}
