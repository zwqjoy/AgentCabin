import { describe, expect, it } from "vitest";
import { computeSurfaceRect } from "./browser-surface-bounds";

const host = (x: number, y: number, width: number, height: number) => ({ x, y, width, height });

describe("computeSurfaceRect", () => {
  it("returns the host rect when nothing clips it", () => {
    expect(computeSurfaceRect(host(20, 40, 600, 400), host(0, 0, 1200, 800))).toEqual({
      x: 20,
      y: 40,
      width: 600,
      height: 400,
    });
  });

  it("clips against the viewport on the right and bottom", () => {
    expect(computeSurfaceRect(host(1000, 700, 400, 400), host(0, 0, 1200, 800))).toEqual({
      x: 1000,
      y: 700,
      width: 200,
      height: 100,
    });
  });

  it("returns a zero rect when the host is scrolled fully out of view", () => {
    expect(computeSurfaceRect(host(0, -500, 600, 400), host(0, 0, 1200, 800))).toEqual({
      x: 0,
      y: 0,
      width: 600,
      height: 0,
    });
  });

  it("returns a zero rect for a zero-size host", () => {
    expect(computeSurfaceRect(host(0, 0, 0, 0), host(0, 0, 1200, 800))).toEqual({
      x: 0,
      y: 0,
      width: 0,
      height: 0,
    });
  });
});
