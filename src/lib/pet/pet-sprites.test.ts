import { describe, expect, it } from "vitest";
import { fitSpriteToCanvas, MODE_ROW } from "./pet-sprites";

describe("independent custom pet animations", () => {
  it("keeps the built-in status rows unchanged", () => {
    expect(MODE_ROW).toEqual({
      idle: 0,
      running: 7,
      failed: 5,
      waiting: 6,
      review: 8,
    });
  });

  it("fits built-in animation cells inside thumbnail canvases", () => {
    const destination = fitSpriteToCanvas(64, 64, 192, 208);
    expect(destination.width).toBeLessThanOrEqual(64);
    expect(destination.height).toBeLessThanOrEqual(64);
    expect(destination.x).toBeGreaterThan(0);
    expect(destination.y).toBe(0);
  });
});
