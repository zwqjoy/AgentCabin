/** Geometry for a renderer host element that reserves native browser space. */
export interface SurfaceRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function computeSurfaceRect(hostRect: SurfaceRect, viewport: SurfaceRect): SurfaceRect {
  const x = Math.max(hostRect.x, viewport.x);
  const y = Math.max(hostRect.y, viewport.y);
  const right = Math.min(hostRect.x + hostRect.width, viewport.x + viewport.width);
  const bottom = Math.min(hostRect.y + hostRect.height, viewport.y + viewport.height);
  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.max(0, Math.round(right - x)),
    height: Math.max(0, Math.round(bottom - y)),
  };
}
