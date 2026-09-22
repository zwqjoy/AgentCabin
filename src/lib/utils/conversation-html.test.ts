import { describe, expect, it } from "vitest";
import { conversationParts } from "./conversation-html";

describe("conversation HTML previews", () => {
  it("keeps two previews between their original prose paragraphs", () => {
    const parts = conversationParts(
      "Before\n\n```html\n<canvas></canvas>\n```\n\nBetween\n\n~~~html-preview\n<b>Second</b>\n~~~\n\nAfter",
    );
    expect(parts.map((part) => part.kind)).toEqual([
      "markdown",
      "html",
      "markdown",
      "html",
      "markdown",
    ]);
    expect(parts[1].content).toBe("<canvas></canvas>");
    expect(parts[4].content).toContain("After");
  });
  it("waits for a complete fence during streaming but renders it before the answer ends", () => {
    expect(conversationParts("```html\n<script>draw()", true)[0].pending).toBe(true);
    expect(conversationParts("```html\n<script>draw()</script>\n```\nMore", true)[0].pending).toBe(
      false,
    );
  });
  it("does not execute quoted examples, raw HTML, or HTML inside another fence", () => {
    for (const text of [
      "> ```html\n> <b>x</b>\n> ```",
      "<script>alert(1)</script>",
      "````md\n```html\n<b>x</b>\n```\n````",
    ]) {
      expect(conversationParts(text).every((part) => part.kind === "markdown")).toBe(true);
    }
  });
  it("retains reference definitions on either side of a preview", () => {
    const parts = conversationParts(
      "[before][ref]\n\n```html\n<b>x</b>\n```\n\n[after][ref]\n\n[ref]: https://example.com",
    );
    expect(parts[0].content).toContain("[ref]: <https://example.com>");
    expect(parts[2].content).toContain("[ref]: <https://example.com>");
  });
});
