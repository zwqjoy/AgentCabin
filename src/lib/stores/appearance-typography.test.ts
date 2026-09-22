import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  CODE_FONT_SIZE_LIMITS,
  getAppearance,
  getUiTypographyCssVariables,
  setAppearance,
  UI_FONT_SIZE_LIMITS,
} from "./appearance.svelte";

describe("appearance UI typography", () => {
  it("derives shared UI text sizes from the configured base size", () => {
    expect(UI_FONT_SIZE_LIMITS).toEqual({ min: 12, max: 16 });
    expect(CODE_FONT_SIZE_LIMITS).toEqual({ min: 10, max: 20 });

    expect(getUiTypographyCssVariables(16)).toEqual({
      "--ui-font-size": "16px",
      "--ui-font-size-secondary": "13.71px",
      "--ui-font-size-caption": "11.43px",
      "--ui-font-size-micro": "12.57px",
      "--ui-font-size-tiny": "10.29px",
      "--ui-font-size-heading": "18.29px",
    });

    // Values from old settings or external callers are clamped to the new UI limit.
    expect(getUiTypographyCssVariables(18)).toEqual(getUiTypographyCssVariables(16));
  });

  it("clamps UI and code font updates before applying them", () => {
    const original = getAppearance();
    setAppearance({ uiFontSize: 99, codeFontSize: 99 });
    expect(getAppearance().uiFontSize).toBe(UI_FONT_SIZE_LIMITS.max);
    expect(getAppearance().codeFontSize).toBe(CODE_FONT_SIZE_LIMITS.max);

    setAppearance({ uiFontSize: 1, codeFontSize: 1 });
    expect(getAppearance().uiFontSize).toBe(UI_FONT_SIZE_LIMITS.min);
    expect(getAppearance().codeFontSize).toBe(CODE_FONT_SIZE_LIMITS.min);

    setAppearance({ uiFontSize: original.uiFontSize, codeFontSize: original.codeFontSize });
  });

  it("binds chat message text to UI typography while keeping code independent", () => {
    const css = readFileSync(resolve(process.cwd(), "src/app.css"), "utf8");
    const message = readFileSync(
      resolve(process.cwd(), "src/lib/components/work/WorkConversationMessage.svelte"),
      "utf8",
    );

    expect(message).toContain("chat-message-user-body");
    expect(message).toContain("chat-message-body");
    expect(css).toContain(".prose-chat > .prose");
    expect(css).toContain(".chat-message-body > .prose");
    expect(css).toContain(".prose-chat :not(pre) > code");
    expect(css).toContain("font-size: var(--code-font-size);");
  });
});
