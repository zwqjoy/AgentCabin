import { describe, expect, it } from "vitest";
import {
  PREVIEW_HEIGHT_MESSAGE,
  injectPreviewBridge,
  parsePreviewHeightMessage,
} from "./html-preview-bridge";

describe("inline HTML preview height bridge", () => {
  it("injects the bridge before the last </body>", () => {
    const result = injectPreviewBridge("<html><head></head><body><p>hi</p></body></html>");
    expect(result.indexOf("data-agentcabin-preview-bridge")).toBeGreaterThan(-1);
    expect(result.indexOf("data-agentcabin-preview-bridge")).toBeLessThan(
      result.indexOf("</body>"),
    );
    expect(result).toContain("<p>hi</p>");
  });

  it("falls back to </html> and to plain appending for fragments", () => {
    expect(injectPreviewBridge("<html><body><b>x</b></html>")).toMatch(
      /data-agentcabin-preview-bridge[\s\S]*<\/html>/,
    );
    const fragment = injectPreviewBridge("<figure><svg></svg></figure>");
    expect(fragment.startsWith("<figure>")).toBe(true);
    expect(fragment).toContain("data-agentcabin-preview-bridge");
  });

  it("never injects twice and leaves empty input alone", () => {
    const once = injectPreviewBridge("<body><p>hi</p></body>");
    expect(injectPreviewBridge(once)).toBe(once);
    expect(injectPreviewBridge("")).toBe("");
  });

  it("accepts only well-formed height reports", () => {
    expect(parsePreviewHeightMessage({ type: PREVIEW_HEIGHT_MESSAGE, height: 264.4 })).toBe(264);
    expect(parsePreviewHeightMessage({ type: PREVIEW_HEIGHT_MESSAGE, height: 1 })).toBe(1);
    for (const payload of [
      null,
      undefined,
      "264",
      { type: "other", height: 200 },
      { type: PREVIEW_HEIGHT_MESSAGE },
      { type: PREVIEW_HEIGHT_MESSAGE, height: 0 },
      { type: PREVIEW_HEIGHT_MESSAGE, height: -12 },
      { type: PREVIEW_HEIGHT_MESSAGE, height: Number.NaN },
      { type: PREVIEW_HEIGHT_MESSAGE, height: Number.POSITIVE_INFINITY },
      { type: PREVIEW_HEIGHT_MESSAGE, height: 50000 },
      { type: PREVIEW_HEIGHT_MESSAGE, height: "300" },
    ]) {
      expect(parsePreviewHeightMessage(payload)).toBeNull();
    }
  });

  it("includes flow-root margin containment and oscillation guard in the bridge script", () => {
    const injected = injectPreviewBridge("<p>test</p>");
    expect(injected).toContain("display:flow-root !important");
    expect(injected).toContain("recentHeights");
  });
});
