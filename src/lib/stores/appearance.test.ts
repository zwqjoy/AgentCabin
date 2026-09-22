import { beforeEach, describe, expect, it } from "vitest";
import {
  getAppearance,
  setAppearance,
  importThemeJson,
  exportThemeJson,
  effectiveReduceMotion,
  BUILTIN_THEMES,
} from "./appearance.svelte";

const mockStorage: Record<string, string> = {};

describe("appearance store", () => {
  beforeEach(() => {
    for (const k of Object.keys(mockStorage)) delete mockStorage[k];
    (globalThis as any).localStorage = {
      getItem: (k: string) => mockStorage[k] ?? null,
      setItem: (k: string, v: string) => {
        mockStorage[k] = v;
      },
      removeItem: (k: string) => {
        delete mockStorage[k];
      },
      clear: () => {
        for (const k of Object.keys(mockStorage)) delete mockStorage[k];
      },
    };
    (globalThis as any).window = {
      dispatchEvent: () => true,
      matchMedia: () => ({ matches: false }),
    };
    (globalThis as any).document = {
      documentElement: {
        classList: {
          toggle: () => {},
          add: () => {},
          remove: () => {},
        },
        style: {
          setProperty: () => {},
          removeProperty: () => {},
        },
      },
    };

    // Reset to default
    setAppearance({
      themeMode: "light",
      colorScheme: "neutral",
      builtinTheme: "default",
      accentColor: "",
      backgroundColor: "",
      foregroundColor: "",
      contrast: 50,
      translucentSidebar: false,
      pointerCursor: false,
      reduceMotion: "system",
      uiFontSize: 14,
      codeFontSize: 13,
      diffMarkerStyle: "color",
      fontSmoothing: true,
    });
  });

  it("applies builtin theme colors when builtinTheme changes", () => {
    setAppearance({ builtinTheme: "notion" });
    const app = getAppearance();
    const notionTheme = BUILTIN_THEMES.find((t) => t.id === "notion")!;

    expect(app.builtinTheme).toBe("notion");
    expect(app.accentColor).toBe(notionTheme.settings.accentColor);
    expect(app.backgroundColor).toBe(notionTheme.settings.backgroundColor);
    expect(app.foregroundColor).toBe(notionTheme.settings.foregroundColor);
  });

  it("resets colors when switching back to default builtin theme", () => {
    setAppearance({ builtinTheme: "notion" });
    expect(getAppearance().accentColor).toBeTruthy();

    setAppearance({ builtinTheme: "default" });
    const app = getAppearance();
    expect(app.builtinTheme).toBe("default");
    expect(app.accentColor).toBe("");
    expect(app.backgroundColor).toBe("");
    expect(app.foregroundColor).toBe("");
    expect(app.contrast).toBe(50);
  });

  it("marks builtinTheme as custom when modifying accentColor explicitly", () => {
    setAppearance({ builtinTheme: "notion" });
    expect(getAppearance().builtinTheme).toBe("notion");

    setAppearance({ accentColor: "#ff0077" });
    expect(getAppearance().builtinTheme).toBe("custom");
    expect(getAppearance().accentColor).toBe("#ff0077");
  });

  it("persists grayscale color schemes correctly", () => {
    setAppearance({ colorScheme: "stone" });
    expect(getAppearance().colorScheme).toBe("stone");

    setAppearance({ colorScheme: "zinc" });
    expect(getAppearance().colorScheme).toBe("zinc");

    setAppearance({ colorScheme: "slate" });
    expect(getAppearance().colorScheme).toBe("slate");

    setAppearance({ colorScheme: "gray" });
    expect(getAppearance().colorScheme).toBe("gray");
  });

  it("handles importThemeJson properly (null on success, string on error)", () => {
    const validJson = exportThemeJson();
    const successResult = importThemeJson(validJson);
    expect(successResult).toBeNull();

    const invalidResult = importThemeJson("{ not valid json }");
    expect(typeof invalidResult).toBe("string");
    expect(invalidResult).toBeTruthy();
  });

  it("computes reduce motion properly", () => {
    setAppearance({ reduceMotion: "on" });
    expect(effectiveReduceMotion()).toBe(true);

    setAppearance({ reduceMotion: "off" });
    expect(effectiveReduceMotion()).toBe(false);
  });
});
