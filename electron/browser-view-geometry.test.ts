import { describe, expect, it } from "vitest";
import { normalizeSurfaceBounds } from "./browser-view-geometry";

describe("normalizeSurfaceBounds", () => {
  const viewport = { width: 1200, height: 800 };

  it("passes a fitting rect through", () => {
    expect(normalizeSurfaceBounds({ x: 100, y: 60, width: 400, height: 500 }, viewport, 1)).toEqual(
      {
        x: 100,
        y: 60,
        width: 400,
        height: 500,
      },
    );
  });

  it("clamps negative origins to zero", () => {
    expect(
      normalizeSurfaceBounds({ x: -40, y: -10, width: 200, height: 200 }, viewport, 1),
    ).toEqual({ x: 0, y: 0, width: 160, height: 190 });
  });

  it("clips overflow on the right and bottom edges", () => {
    expect(
      normalizeSurfaceBounds({ x: 1100, y: 700, width: 400, height: 400 }, viewport, 1),
    ).toEqual({ x: 1100, y: 700, width: 100, height: 100 });
  });

  it("returns a zero-size rect when the origin is outside the viewport", () => {
    expect(
      normalizeSurfaceBounds({ x: 1300, y: 900, width: 100, height: 100 }, viewport, 1),
    ).toEqual({ x: 1200, y: 800, width: 0, height: 0 });
  });

  it("divides CSS pixels by the zoom factor", () => {
    expect(
      normalizeSurfaceBounds({ x: 200, y: 100, width: 400, height: 300 }, viewport, 2),
    ).toEqual({
      x: 100,
      y: 50,
      width: 200,
      height: 150,
    });
  });

  it("treats non-finite input as zero", () => {
    expect(
      normalizeSurfaceBounds(
        { x: Number.NaN, y: "60", width: Number.POSITIVE_INFINITY, height: null },
        viewport,
        1,
      ),
    ).toEqual({ x: 0, y: 0, width: 0, height: 0 });
  });

  it("floors fractional values", () => {
    expect(
      normalizeSurfaceBounds({ x: 10.7, y: 20.2, width: 100.9, height: 50.5 }, viewport, 1),
    ).toEqual({ x: 10, y: 20, width: 100, height: 50 });
  });
});
