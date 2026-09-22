import { describe, expect, it } from "vitest";
import {
  applyAppearanceCss,
  BUILTIN_THEMES,
  getAppearance,
  getHexLuminance,
  isDarkThemeEffective,
  resolveThemeColors,
  setAppearance,
} from "./appearance.svelte";

describe("appearance theme mode and builtin themes", () => {
  it("defines dual-mode palettes for all builtin themes", () => {
    const defaultTheme = BUILTIN_THEMES.find((t) => t.id === "default");
    expect(defaultTheme).toBeDefined();
    expect(defaultTheme?.settings.backgroundColor).toBe("");
    expect(defaultTheme?.darkSettings.backgroundColor).toBe("");

    const codex = BUILTIN_THEMES.find((t) => t.id === "codex");
    expect(codex).toBeDefined();
    expect(codex?.settings.backgroundColor.toUpperCase()).toBe("#FFFFFF");
    expect(codex?.darkSettings.backgroundColor).toBe("#151718");
    expect(codex?.darkSettings.foregroundColor).toBe("#ECEDEE");

    for (const theme of BUILTIN_THEMES) {
      expect(theme.settings).toBeDefined();
      expect(theme.darkSettings).toBeDefined();
      if (theme.id !== "default") {
        // Dark settings should have dark background (low luminance)
        const darkLum = getHexLuminance(theme.darkSettings.backgroundColor);
        const lightLum = getHexLuminance(theme.settings.backgroundColor);
        expect(darkLum).toBeLessThan(0.3);
        expect(lightLum).toBeGreaterThan(0.7);
      }
    }
  });

  it("calculates relative luminance accurately", () => {
    expect(getHexLuminance("#FFFFFF")).toBeCloseTo(1.0, 2);
    expect(getHexLuminance("#000000")).toBeCloseTo(0.0, 2);
    expect(getHexLuminance("#151718")).toBeLessThan(0.05);
  });

  it("resolves dark theme effectiveness based on themeMode", () => {
    expect(
      isDarkThemeEffective({
        ...getAppearance(),
        themeMode: "dark",
      }),
    ).toBe(true);

    expect(
      isDarkThemeEffective({
        ...getAppearance(),
        themeMode: "light",
      }),
    ).toBe(false);
  });

  it("adapts builtin theme colors when switching themeMode", () => {
    const codex = BUILTIN_THEMES.find((t) => t.id === "codex")!;

    // Start with codex in light mode
    setAppearance({
      builtinTheme: "codex",
      themeMode: "light",
      ...codex.settings,
    });

    expect(getAppearance().themeMode).toBe("light");
    expect(getAppearance().backgroundColor.toUpperCase()).toBe("#FFFFFF");

    // Switch to dark mode: should automatically adapt to codex.darkSettings
    setAppearance({ themeMode: "dark" });
    expect(getAppearance().themeMode).toBe("dark");
    expect(getAppearance().backgroundColor).toBe(codex.darkSettings.backgroundColor);
    expect(getAppearance().foregroundColor).toBe(codex.darkSettings.foregroundColor);

    // Switch back to light mode: should adapt back to codex.settings
    setAppearance({ themeMode: "light" });
    expect(getAppearance().themeMode).toBe("light");
    expect(getAppearance().backgroundColor).toBe(codex.settings.backgroundColor);
    expect(getAppearance().foregroundColor).toBe(codex.settings.foregroundColor);
  });

  it("protects against light background overriding dark mode in applyAppearanceCss", () => {
    if (typeof document === "undefined") return;

    const root = document.documentElement;

    // Simulate dark mode with a forced white background (e.g. leftover from old state)
    applyAppearanceCss({
      ...getAppearance(),
      themeMode: "dark",
      builtinTheme: "codex",
      backgroundColor: "#FFFFFF",
      foregroundColor: "#1A1C1F",
    });

    // In dark mode with codex, it should resolve to codex darkSettings background (#151718), NOT #FFFFFF
    expect(root.style.getPropertyValue("--bg-app")).toBe("#151718");
    expect(root.classList.contains("dark")).toBe(true);

    // When background is reset to empty string, custom inline styles are removed
    applyAppearanceCss({
      ...getAppearance(),
      themeMode: "dark",
      builtinTheme: "default",
      backgroundColor: "",
      foregroundColor: "",
    });

    expect(root.style.getPropertyValue("--bg-app")).toBe("");
    expect(root.style.getPropertyValue("--bg-surface")).toBe("");
    expect(root.style.getPropertyValue("--user-bg")).toBe("");
    expect(root.classList.contains("dark")).toBe(true);
  });
});
