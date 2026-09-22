/**
 * Pure geometry for the embedded browser surface.
 *
 * The renderer reports a rect in CSS pixels relative to the window viewport.
 * Electron's WebContentsView uses DIP relative to the window content area, so
 * the rect must be divided by the renderer zoom factor and clamped to the
 * window. Keeping this pure makes the zoom/clip cases unit-testable.
 */

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SurfaceViewport {
  width: number;
  height: number;
}

function finiteNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

export function normalizeSurfaceBounds(
  rect: unknown,
  viewport: SurfaceViewport,
  zoomFactor: number,
): Rect {
  const viewportWidth = Math.max(0, Math.floor(finiteNumber(viewport?.width)));
  const viewportHeight = Math.max(0, Math.floor(finiteNumber(viewport?.height)));
  const zoom = finiteNumber(zoomFactor) > 0 ? finiteNumber(zoomFactor) : 1;

  const raw = (rect ?? {}) as Partial<Rect>;
  const cssX = Math.floor(finiteNumber(raw.x) / zoom);
  const cssY = Math.floor(finiteNumber(raw.y) / zoom);
  const cssWidth = Math.floor(finiteNumber(raw.width) / zoom);
  const cssHeight = Math.floor(finiteNumber(raw.height) / zoom);

  // Clip the rect against the window the same way the renderer clips it against
  // the viewport, so a partially scrolled-out host element reports only the
  // visible extent instead of overflowing the window.
  const x = Math.min(Math.max(0, cssX), viewportWidth);
  const y = Math.min(Math.max(0, cssY), viewportHeight);
  const right = Math.min(Math.max(0, cssX + cssWidth), viewportWidth);
  const bottom = Math.min(Math.max(0, cssY + cssHeight), viewportHeight);

  return {
    x,
    y,
    width: Math.max(0, right - x),
    height: Math.max(0, bottom - y),
  };
}
