/**
 * Appearance Store — single source of truth for all visual customisation settings.
 *
 * Persistence : localStorage (keys prefixed with "agentcabin:")
 * Application : CSS custom properties on document.documentElement
 * Event bridge: dispatches "agentcabin:appearance-changed" so +layout.svelte
 *               can keep its reactive state (themeMode, colorScheme, reduceMotion) in sync.
 */

// ── Storage keys ──────────────────────────────────────────────────────────────
const KEYS = {
  themeMode: "agentcabin:theme",
  colorScheme: "agentcabin:colorScheme",
  accentColor: "agentcabin:accentColor",
  backgroundColor: "agentcabin:backgroundColor",
  foregroundColor: "agentcabin:foregroundColor",
  uiFontFamily: "agentcabin:uiFontFamily",
  codeFontFamily: "agentcabin:codeFontFamily",
  translucentSidebar: "agentcabin:translucentSidebar",
  contrast: "agentcabin:contrast",
  pointerCursor: "agentcabin:pointerCursor",
  reduceMotion: "agentcabin:reduceMotion",
  uiFontSize: "agentcabin:uiFontSize",
  codeFontSize: "agentcabin:codeFontSize",
  diffMarkerStyle: "agentcabin:diffMarkerStyle",
  fontSmoothing: "agentcabin:fontSmoothing",
  builtinTheme: "agentcabin:builtinTheme",
} as const;

export const UI_FONT_SIZE_LIMITS = { min: 12, max: 16 } as const;
export const CODE_FONT_SIZE_LIMITS = { min: 10, max: 20 } as const;

export type ThemeMode = "system" | "light" | "dark";
export type ColorScheme =
  | "neutral"
  | "zinc"
  | "slate"
  | "stone"
  | "gray"
  | "warm"
  | "emerald"
  | "violet"
  | "ocean"
  | "rose";
export type ReduceMotion = "system" | "on" | "off";
export type DiffMarkerStyle = "color" | "symbols";

export interface AppearanceSettings {
  themeMode: ThemeMode;
  colorScheme: ColorScheme;
  builtinTheme: string; // "default" | "codex" | ... | "custom"
  accentColor: string; // HEX or "" (use scheme preset)
  backgroundColor: string; // HEX or ""
  foregroundColor: string; // HEX or ""
  uiFontFamily: string; // CSS font-family or ""
  codeFontFamily: string; // CSS font-family or ""
  translucentSidebar: boolean;
  contrast: number; // 0–100
  pointerCursor: boolean;
  reduceMotion: ReduceMotion;
  uiFontSize: number; // px
  codeFontSize: number; // px
  diffMarkerStyle: DiffMarkerStyle;
  fontSmoothing: boolean;
}

// ── Defaults ──────────────────────────────────────────────────────────────────
const DEFAULTS: AppearanceSettings = {
  themeMode: "light",
  colorScheme: "neutral",
  builtinTheme: "default",
  accentColor: "",
  backgroundColor: "",
  foregroundColor: "",
  uiFontFamily: "",
  codeFontFamily: "",
  translucentSidebar: false,
  contrast: 50,
  pointerCursor: false,
  reduceMotion: "system",
  uiFontSize: 14,
  codeFontSize: 13,
  diffMarkerStyle: "color",
  fontSmoothing: true,
};

function formatCssPx(value: number): string {
  return `${Number(value.toFixed(2))}px`;
}

function clampFontSize(
  value: number,
  limits: { min: number; max: number },
  fallback: number,
): number {
  return Number.isFinite(value) ? Math.min(limits.max, Math.max(limits.min, value)) : fallback;
}

/**
 * Keep the product's typography hierarchy tied to the user-controlled UI
 * base size. The ratios preserve the existing 14/12/10/16px defaults while
 * allowing settings navigation and content labels to scale together.
 */
export function getUiTypographyCssVariables(uiFontSize: number): Record<string, string> {
  const base = clampFontSize(uiFontSize, UI_FONT_SIZE_LIMITS, DEFAULTS.uiFontSize);
  const scale = base / DEFAULTS.uiFontSize;
  return {
    "--ui-font-size": `${base}px`,
    "--ui-font-size-secondary": formatCssPx(12 * scale),
    "--ui-font-size-caption": formatCssPx(10 * scale),
    "--ui-font-size-micro": formatCssPx(11 * scale),
    "--ui-font-size-tiny": formatCssPx(9 * scale),
    "--ui-font-size-heading": formatCssPx(16 * scale),
  };
}

// ── Helpers ───────────────────────────────────────────────────────────────────
function ss(key: string): string | null {
  try {
    return typeof window !== "undefined" ? localStorage.getItem(key) : null;
  } catch {
    return null;
  }
}

function sw(key: string, value: string): void {
  try {
    if (typeof window !== "undefined") localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

function readBool(key: string, fallback: boolean): boolean {
  const v = ss(key);
  if (v === null) return fallback;
  return v === "true";
}

function readNumber(key: string, fallback: number, min: number, max: number): number {
  const v = ss(key);
  if (v === null) return fallback;
  const n = parseFloat(v);
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
}

/** Migrate legacy boolean reduceMotion → ReduceMotion string */
function readReduceMotion(): ReduceMotion {
  const v = ss(KEYS.reduceMotion);
  if (v === "system" || v === "on" || v === "off") return v;
  // Legacy migration: "true" → "on", "false" → "off"
  if (v === "true") return "on";
  if (v === "false") return "off";
  return "system";
}

function readColorScheme(): ColorScheme {
  const v = ss(KEYS.colorScheme);
  const allowed: ColorScheme[] = [
    "neutral",
    "zinc",
    "slate",
    "stone",
    "gray",
    "warm",
    "emerald",
    "violet",
    "ocean",
    "rose",
  ];
  return allowed.includes(v as ColorScheme) ? (v as ColorScheme) : "neutral";
}

function readThemeMode(): ThemeMode {
  const v = ss(KEYS.themeMode);
  if (v === "dark" || v === "system") return v;
  return "light";
}

// ── Load from storage ─────────────────────────────────────────────────────────
function loadSettings(): AppearanceSettings {
  return {
    themeMode: readThemeMode(),
    colorScheme: readColorScheme(),
    builtinTheme: ss(KEYS.builtinTheme) ?? "default",
    accentColor: ss(KEYS.accentColor) ?? "",
    backgroundColor: ss(KEYS.backgroundColor) ?? "",
    foregroundColor: ss(KEYS.foregroundColor) ?? "",
    uiFontFamily: ss(KEYS.uiFontFamily) ?? "",
    codeFontFamily: ss(KEYS.codeFontFamily) ?? "",
    translucentSidebar: readBool(KEYS.translucentSidebar, DEFAULTS.translucentSidebar),
    contrast: readNumber(KEYS.contrast, DEFAULTS.contrast, 0, 100),
    pointerCursor: readBool(KEYS.pointerCursor, DEFAULTS.pointerCursor),
    reduceMotion: readReduceMotion(),
    uiFontSize: readNumber(
      KEYS.uiFontSize,
      DEFAULTS.uiFontSize,
      UI_FONT_SIZE_LIMITS.min,
      UI_FONT_SIZE_LIMITS.max,
    ),
    codeFontSize: readNumber(
      KEYS.codeFontSize,
      DEFAULTS.codeFontSize,
      CODE_FONT_SIZE_LIMITS.min,
      CODE_FONT_SIZE_LIMITS.max,
    ),
    diffMarkerStyle: (ss(KEYS.diffMarkerStyle) as DiffMarkerStyle) ?? "color",
    fontSmoothing: readBool(KEYS.fontSmoothing, DEFAULTS.fontSmoothing),
  };
}

// ── Reactive state (Svelte 5 runes) ───────────────────────────────────────────
let _settings = $state<AppearanceSettings>(loadSettings());

// ── Computed: effective reduce-motion (resolves "system" against OS preference) ──
function _resolveReduceMotion(pref: ReduceMotion): boolean {
  if (pref === "on") return true;
  if (pref === "off") return false;
  // system: read prefers-reduced-motion
  return (
    typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

// ── Persist helpers ───────────────────────────────────────────────────────────
function _persist(patch: Partial<AppearanceSettings>): void {
  if (patch.themeMode !== undefined) sw(KEYS.themeMode, patch.themeMode);
  if (patch.colorScheme !== undefined) sw(KEYS.colorScheme, patch.colorScheme);
  if (patch.builtinTheme !== undefined) sw(KEYS.builtinTheme, patch.builtinTheme);
  if (patch.accentColor !== undefined) sw(KEYS.accentColor, patch.accentColor);
  if (patch.backgroundColor !== undefined) sw(KEYS.backgroundColor, patch.backgroundColor);
  if (patch.foregroundColor !== undefined) sw(KEYS.foregroundColor, patch.foregroundColor);
  if (patch.uiFontFamily !== undefined) sw(KEYS.uiFontFamily, patch.uiFontFamily);
  if (patch.codeFontFamily !== undefined) sw(KEYS.codeFontFamily, patch.codeFontFamily);
  if (patch.translucentSidebar !== undefined)
    sw(KEYS.translucentSidebar, String(patch.translucentSidebar));
  if (patch.contrast !== undefined) sw(KEYS.contrast, String(patch.contrast));
  if (patch.pointerCursor !== undefined) sw(KEYS.pointerCursor, String(patch.pointerCursor));
  if (patch.reduceMotion !== undefined) sw(KEYS.reduceMotion, patch.reduceMotion);
  if (patch.uiFontSize !== undefined) sw(KEYS.uiFontSize, String(patch.uiFontSize));
  if (patch.codeFontSize !== undefined) sw(KEYS.codeFontSize, String(patch.codeFontSize));
  if (patch.diffMarkerStyle !== undefined) sw(KEYS.diffMarkerStyle, patch.diffMarkerStyle);
  if (patch.fontSmoothing !== undefined) sw(KEYS.fontSmoothing, String(patch.fontSmoothing));
}

// ── CSS application ───────────────────────────────────────────────────────────
/**
 * Converts a 0–100 contrast value into adjustments for border/text alpha tokens.
 * contrast=50 is "neutral" (no override). <50 = softer, >50 = stronger.
 */
function _contrastFactor(contrast: number): number {
  // Map 0–100 to 0.4–1.8 (50 → 1.0)
  return 0.4 + (contrast / 100) * 1.4;
}

function hexToHslString(hex: string): string | null {
  if (!/^#[0-9a-fA-F]{6}$/.test(hex)) return null;
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b);
  let h = 0,
    s = 0;
  const l = (max + min) / 2;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r:
        h = (g - b) / d + (g < b ? 6 : 0);
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      case b:
        h = (r - g) / d + 4;
        break;
    }
    h /= 6;
  }
  return `${Math.round(h * 360)} ${Math.round(s * 100)}% ${Math.round(l * 100)}%`;
}

export function isDarkThemeEffective(s: AppearanceSettings): boolean {
  if (s.themeMode === "dark") return true;
  if (s.themeMode === "light") return false;
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function getHexLuminance(hex: string): number {
  if (!/^#[0-9a-fA-F]{6}$/.test(hex)) return 0.5;
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const [rr, gr, br] = [r, g, b].map((c) =>
    c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * rr + 0.7152 * gr + 0.0722 * br;
}

/** Apply all appearance settings as CSS custom properties + document classes. */
export function applyAppearanceCss(s: AppearanceSettings): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;

  const isDark = isDarkThemeEffective(s);

  // Keep .dark class on <html> synchronized
  root.classList.toggle("dark", isDark);

  // Accent color
  const accentHsl = hexToHslString(s.accentColor);
  if (accentHsl && s.accentColor) {
    root.style.setProperty("--user-accent", s.accentColor);
    root.style.setProperty("--primary", accentHsl);
    root.style.setProperty("--ring", accentHsl);
    root.style.setProperty("--sidebar-primary", accentHsl);
    root.style.setProperty("--accent-primary", s.accentColor);
    root.style.setProperty(
      "--accent-soft",
      `color-mix(in srgb, ${s.accentColor} 14%, transparent)`,
    );
    root.style.setProperty(
      "--accent-border",
      `color-mix(in srgb, ${s.accentColor} 38%, transparent)`,
    );
  } else {
    root.style.removeProperty("--user-accent");
    root.style.removeProperty("--primary");
    root.style.removeProperty("--ring");
    root.style.removeProperty("--sidebar-primary");
    root.style.removeProperty("--accent-primary");
    root.style.removeProperty("--accent-soft");
    root.style.removeProperty("--accent-border");
  }

  // Determine effective background and foreground with luminance safeguard
  let effectiveBg = s.backgroundColor;
  let effectiveFg = s.foregroundColor;

  const activeTheme = s.builtinTheme ? BUILTIN_THEMES.find((t) => t.id === s.builtinTheme) : null;

  if (effectiveBg) {
    const bgLum = getHexLuminance(effectiveBg);
    if (isDark && bgLum > 0.5) {
      // In dark mode, but background is light (e.g. leftover #FFFFFF from light preset)
      if (activeTheme && activeTheme.id !== "default" && activeTheme.darkSettings.backgroundColor) {
        effectiveBg = activeTheme.darkSettings.backgroundColor;
      } else {
        effectiveBg = ""; // fall back to CSS .dark variables
      }
    } else if (!isDark && bgLum < 0.2) {
      // In light mode, but background is dark
      if (activeTheme && activeTheme.id !== "default" && activeTheme.settings.backgroundColor) {
        effectiveBg = activeTheme.settings.backgroundColor;
      } else {
        effectiveBg = ""; // fall back to CSS light variables
      }
    }
  }

  if (effectiveFg) {
    const fgLum = getHexLuminance(effectiveFg);
    if (isDark && fgLum < 0.25) {
      if (activeTheme && activeTheme.id !== "default" && activeTheme.darkSettings.foregroundColor) {
        effectiveFg = activeTheme.darkSettings.foregroundColor;
      } else {
        effectiveFg = "";
      }
    } else if (!isDark && fgLum > 0.75) {
      if (activeTheme && activeTheme.id !== "default" && activeTheme.settings.foregroundColor) {
        effectiveFg = activeTheme.settings.foregroundColor;
      } else {
        effectiveFg = "";
      }
    }
  }

  // Background color
  const bgHsl = hexToHslString(effectiveBg);
  if (bgHsl && effectiveBg) {
    root.style.setProperty("--user-bg", effectiveBg);
    root.style.setProperty("--background", bgHsl);
    root.style.setProperty("--card", bgHsl);
    root.style.setProperty("--popover", bgHsl);
    root.style.setProperty("--bg-app", effectiveBg);
    root.style.setProperty("--bg-surface", effectiveBg);
  } else {
    root.style.removeProperty("--user-bg");
    root.style.removeProperty("--background");
    root.style.removeProperty("--card");
    root.style.removeProperty("--popover");
    root.style.removeProperty("--bg-app");
    root.style.removeProperty("--bg-surface");
  }

  // Foreground color
  const fgHsl = hexToHslString(effectiveFg);
  if (fgHsl && effectiveFg) {
    root.style.setProperty("--user-fg", effectiveFg);
    root.style.setProperty("--foreground", fgHsl);
    root.style.setProperty("--card-foreground", fgHsl);
    root.style.setProperty("--text-primary", effectiveFg);
  } else {
    root.style.removeProperty("--user-fg");
    root.style.removeProperty("--foreground");
    root.style.removeProperty("--card-foreground");
    root.style.removeProperty("--text-primary");
  }

  // UI font
  if (s.uiFontFamily && s.uiFontFamily.trim()) {
    root.style.setProperty("--font-ui-override", s.uiFontFamily.trim());
  } else {
    root.style.removeProperty("--font-ui-override");
  }

  // Code font
  if (s.codeFontFamily && s.codeFontFamily.trim()) {
    root.style.setProperty("--font-code-override", s.codeFontFamily.trim());
  } else {
    root.style.removeProperty("--font-code-override");
  }

  // Font sizes
  for (const [property, value] of Object.entries(getUiTypographyCssVariables(s.uiFontSize))) {
    root.style.setProperty(property, value);
  }
  const codeFontSize = clampFontSize(s.codeFontSize, CODE_FONT_SIZE_LIMITS, DEFAULTS.codeFontSize);
  root.style.setProperty("--code-font-size", `${codeFontSize}px`);

  // Contrast factor (0–100 -> factor 0.4 to 1.8)
  const factor = _contrastFactor(s.contrast);
  root.style.setProperty("--contrast-factor", String(factor));

  if (isDark) {
    const borderL = Math.round(22 * factor);
    const mutedL = Math.round(100 - (100 - 62) / Math.max(0.4, factor));
    root.style.setProperty("--border", `220 10% ${borderL}%`);
    root.style.setProperty("--input", `220 10% ${Math.min(50, borderL + 1)}%`);
    root.style.setProperty("--muted-foreground", `220 8% ${mutedL}%`);
    root.style.setProperty(
      "--border-default",
      `rgba(255, 255, 255, ${Number((0.11 * factor).toFixed(3))})`,
    );
    root.style.setProperty(
      "--border-subtle",
      `rgba(255, 255, 255, ${Number((0.075 * factor).toFixed(3))})`,
    );
    root.style.setProperty(
      "--border-strong",
      `rgba(255, 255, 255, ${Number((0.17 * factor).toFixed(3))})`,
    );
    root.style.setProperty(
      "--divider",
      `rgba(255, 255, 255, ${Number((0.075 * factor).toFixed(3))})`,
    );
  } else {
    // Light mode base: border = 220 14% 88%, muted-fg = 220 9% 41%
    const borderL = Math.round(100 - (100 - 88) * factor);
    const mutedL = Math.round(41 / Math.max(0.4, factor));
    root.style.setProperty("--border", `220 14% ${borderL}%`);
    root.style.setProperty("--input", `220 14% ${Math.max(60, borderL - 2)}%`);
    root.style.setProperty("--muted-foreground", `220 9% ${Math.min(75, Math.max(15, mutedL))}%`);
    root.style.setProperty(
      "--border-default",
      `rgba(17, 24, 39, ${Number((0.08 * factor).toFixed(3))})`,
    );
    root.style.setProperty(
      "--border-subtle",
      `rgba(17, 24, 39, ${Number((0.065 * factor).toFixed(3))})`,
    );
    root.style.setProperty(
      "--border-strong",
      `rgba(17, 24, 39, ${Number((0.13 * factor).toFixed(3))})`,
    );
    root.style.setProperty("--divider", `rgba(17, 24, 39, ${Number((0.065 * factor).toFixed(3))})`);
  }

  // Scheme classes
  const ALL_SCHEMES = [
    "scheme-neutral",
    "scheme-zinc",
    "scheme-slate",
    "scheme-stone",
    "scheme-gray",
    "scheme-warm",
    "scheme-emerald",
    "scheme-violet",
    "scheme-ocean",
    "scheme-rose",
  ];
  root.classList.remove(...ALL_SCHEMES);
  if (s.colorScheme && s.colorScheme !== "neutral") {
    root.classList.add(`scheme-${s.colorScheme}`);
  }

  // Motion preference
  root.classList.toggle("reduce-motion", _resolveReduceMotion(s.reduceMotion));

  // Body classes
  root.classList.toggle("use-pointer-cursor", s.pointerCursor);
  root.classList.toggle("translucent-sidebar", s.translucentSidebar);
  root.classList.toggle("diff-symbols", s.diffMarkerStyle === "symbols");

  // Font smoothing
  root.classList.toggle("font-smoothing-enabled", s.fontSmoothing);
  root.classList.toggle("font-smoothing-disabled", !s.fontSmoothing);
}

// ── Dispatch compatibility event ──────────────────────────────────────────────
export function dispatchAppearanceChanged(s: AppearanceSettings): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent("agentcabin:appearance-changed", {
      detail: {
        // Fields consumed by +layout.svelte
        themeMode: s.themeMode,
        colorScheme: s.colorScheme,
        builtinTheme: s.builtinTheme,
        reduceMotion: s.reduceMotion, // layout will handle "system"/"on"/"off"
        // Extended fields
        pointerCursor: s.pointerCursor,
        translucentSidebar: s.translucentSidebar,
        fontSmoothing: s.fontSmoothing,
        diffMarkerStyle: s.diffMarkerStyle,
        uiFontSize: s.uiFontSize,
        codeFontSize: s.codeFontSize,
        contrast: s.contrast,
        accentColor: s.accentColor,
        backgroundColor: s.backgroundColor,
        foregroundColor: s.foregroundColor,
        uiFontFamily: s.uiFontFamily,
        codeFontFamily: s.codeFontFamily,
      },
    }),
  );
}

// ── Public API ────────────────────────────────────────────────────────────────

/** Read the current appearance settings (reactive). */
export function getAppearance(): AppearanceSettings {
  return _settings;
}

/** Update one or more appearance fields, persist, apply CSS, and dispatch event. */
export function setAppearance(patch: Partial<AppearanceSettings>): void {
  const normalizedPatch = { ...patch };
  if (normalizedPatch.uiFontSize !== undefined) {
    normalizedPatch.uiFontSize = clampFontSize(
      normalizedPatch.uiFontSize,
      UI_FONT_SIZE_LIMITS,
      _settings.uiFontSize,
    );
  }
  if (normalizedPatch.codeFontSize !== undefined) {
    normalizedPatch.codeFontSize = clampFontSize(
      normalizedPatch.codeFontSize,
      CODE_FONT_SIZE_LIMITS,
      _settings.codeFontSize,
    );
  }

  // If builtinTheme is changed explicitly:
  if (normalizedPatch.builtinTheme !== undefined) {
    if (normalizedPatch.builtinTheme === "default") {
      normalizedPatch.accentColor = "";
      normalizedPatch.backgroundColor = "";
      normalizedPatch.foregroundColor = "";
      normalizedPatch.contrast = 50;
      normalizedPatch.translucentSidebar = false;
    } else if (normalizedPatch.builtinTheme !== "custom") {
      const theme = BUILTIN_THEMES.find((t) => t.id === normalizedPatch.builtinTheme);
      if (theme) {
        const isDark = isDarkThemeEffective({ ..._settings, ...normalizedPatch });
        const colors = resolveThemeColors(theme, isDark);
        normalizedPatch.accentColor = colors.accentColor;
        normalizedPatch.backgroundColor = colors.backgroundColor;
        normalizedPatch.foregroundColor = colors.foregroundColor;
        normalizedPatch.contrast = colors.contrast;
        if (colors.translucentSidebar !== undefined) {
          normalizedPatch.translucentSidebar = colors.translucentSidebar;
        }
      }
    }
  } else if (
    normalizedPatch.themeMode !== undefined &&
    normalizedPatch.themeMode !== _settings.themeMode &&
    _settings.builtinTheme &&
    _settings.builtinTheme !== "default" &&
    _settings.builtinTheme !== "custom" &&
    normalizedPatch.backgroundColor === undefined
  ) {
    const theme = BUILTIN_THEMES.find((t) => t.id === _settings.builtinTheme);
    if (theme) {
      const isDark = isDarkThemeEffective({ ..._settings, ...normalizedPatch });
      const colors = resolveThemeColors(theme, isDark);
      normalizedPatch.accentColor = colors.accentColor;
      normalizedPatch.backgroundColor = colors.backgroundColor;
      normalizedPatch.foregroundColor = colors.foregroundColor;
      normalizedPatch.contrast = colors.contrast;
      if (colors.translucentSidebar !== undefined) {
        normalizedPatch.translucentSidebar = colors.translucentSidebar;
      }
    }
  }

  // If user explicitly modifies accentColor/backgroundColor without specifying builtinTheme, mark as custom
  if (
    normalizedPatch.builtinTheme === undefined &&
    (patch.accentColor !== undefined ||
      patch.backgroundColor !== undefined ||
      patch.foregroundColor !== undefined)
  ) {
    normalizedPatch.builtinTheme = "custom";
  }

  _settings = { ..._settings, ...normalizedPatch };
  _persist(normalizedPatch);
  applyAppearanceCss(_settings);
  dispatchAppearanceChanged(_settings);
}

/**
 * Compute the effective "reduce motion" boolean from the three-way setting.
 * Safe to call from any context.
 */
export function effectiveReduceMotion(): boolean {
  return _resolveReduceMotion(_settings.reduceMotion);
}

/**
 * Re-read all settings from localStorage (e.g. after another tab changes them)
 * and re-apply CSS. Useful for cross-tab sync if needed in the future.
 */
export function reloadAppearance(): void {
  _settings = loadSettings();
  applyAppearanceCss(_settings);
}

export interface ThemeColorSet {
  accentColor: string;
  backgroundColor: string;
  foregroundColor: string;
  contrast: number;
  translucentSidebar?: boolean;
}

export interface BuiltinTheme {
  id: string;
  name: string;
  badgeBg: string;
  badgeFg: string;
  settings: ThemeColorSet;
  darkSettings: ThemeColorSet;
}

export const BUILTIN_THEMES: BuiltinTheme[] = [
  {
    id: "default",
    name: "AgentCabin 默认",
    badgeBg: "#10b981",
    badgeFg: "#ffffff",
    settings: {
      accentColor: "",
      backgroundColor: "",
      foregroundColor: "",
      contrast: 50,
      translucentSidebar: false,
    },
    darkSettings: {
      accentColor: "",
      backgroundColor: "",
      foregroundColor: "",
      contrast: 50,
      translucentSidebar: false,
    },
  },
  {
    id: "codex",
    name: "Codex",
    badgeBg: "#3b82f6",
    badgeFg: "#ffffff",
    settings: {
      accentColor: "#339CFF",
      backgroundColor: "#FFFFFF",
      foregroundColor: "#1A1C1F",
      contrast: 76,
      translucentSidebar: true,
    },
    darkSettings: {
      accentColor: "#339CFF",
      backgroundColor: "#151718",
      foregroundColor: "#ECEDEE",
      contrast: 76,
      translucentSidebar: true,
    },
  },
  {
    id: "absolutely",
    name: "Absolutely",
    badgeBg: "#ff79c6",
    badgeFg: "#282a36",
    settings: {
      accentColor: "#ff79c6",
      backgroundColor: "#f8f8f2",
      foregroundColor: "#282a36",
      contrast: 70,
      translucentSidebar: false,
    },
    darkSettings: {
      accentColor: "#ff79c6",
      backgroundColor: "#282a36",
      foregroundColor: "#f8f8f2",
      contrast: 70,
      translucentSidebar: false,
    },
  },
  {
    id: "catppuccin",
    name: "Catppuccin",
    badgeBg: "#cba6f7",
    badgeFg: "#1e1e2e",
    settings: {
      accentColor: "#8839ef",
      backgroundColor: "#eff1f5",
      foregroundColor: "#4c4f69",
      contrast: 65,
      translucentSidebar: true,
    },
    darkSettings: {
      accentColor: "#cba6f7",
      backgroundColor: "#1e1e2e",
      foregroundColor: "#cdd6f4",
      contrast: 65,
      translucentSidebar: true,
    },
  },
  {
    id: "everforest",
    name: "Everforest",
    badgeBg: "#a7c080",
    badgeFg: "#2d353b",
    settings: {
      accentColor: "#8da101",
      backgroundColor: "#fdf6e3",
      foregroundColor: "#5c6a72",
      contrast: 68,
      translucentSidebar: true,
    },
    darkSettings: {
      accentColor: "#a7c080",
      backgroundColor: "#2d353b",
      foregroundColor: "#d3c6aa",
      contrast: 68,
      translucentSidebar: true,
    },
  },
  {
    id: "github",
    name: "GitHub",
    badgeBg: "#0969da",
    badgeFg: "#ffffff",
    settings: {
      accentColor: "#0969da",
      backgroundColor: "#ffffff",
      foregroundColor: "#24292f",
      contrast: 60,
      translucentSidebar: false,
    },
    darkSettings: {
      accentColor: "#58a6ff",
      backgroundColor: "#0d1117",
      foregroundColor: "#c9d1d9",
      contrast: 60,
      translucentSidebar: false,
    },
  },
  {
    id: "gruvbox",
    name: "Gruvbox",
    badgeBg: "#458588",
    badgeFg: "#fbf1c7",
    settings: {
      accentColor: "#b57614",
      backgroundColor: "#fbf1c7",
      foregroundColor: "#3c3836",
      contrast: 72,
      translucentSidebar: false,
    },
    darkSettings: {
      accentColor: "#fabd2f",
      backgroundColor: "#282828",
      foregroundColor: "#ebdbb2",
      contrast: 72,
      translucentSidebar: false,
    },
  },
  {
    id: "linear",
    name: "Linear",
    badgeBg: "#5e6ad2",
    badgeFg: "#ffffff",
    settings: {
      accentColor: "#5e6ad2",
      backgroundColor: "#ffffff",
      foregroundColor: "#1c1d22",
      contrast: 75,
      translucentSidebar: true,
    },
    darkSettings: {
      accentColor: "#5e6ad2",
      backgroundColor: "#121316",
      foregroundColor: "#e5e7eb",
      contrast: 75,
      translucentSidebar: true,
    },
  },
  {
    id: "notion",
    name: "Notion",
    badgeBg: "#2383e2",
    badgeFg: "#ffffff",
    settings: {
      accentColor: "#2383e2",
      backgroundColor: "#ffffff",
      foregroundColor: "#37352f",
      contrast: 50,
      translucentSidebar: false,
    },
    darkSettings: {
      accentColor: "#2383e2",
      backgroundColor: "#191919",
      foregroundColor: "#d4d4d4",
      contrast: 50,
      translucentSidebar: false,
    },
  },
  {
    id: "one",
    name: "One",
    badgeBg: "#61afef",
    badgeFg: "#21252b",
    settings: {
      accentColor: "#4078f2",
      backgroundColor: "#fafafa",
      foregroundColor: "#383a42",
      contrast: 60,
      translucentSidebar: true,
    },
    darkSettings: {
      accentColor: "#61afef",
      backgroundColor: "#21252b",
      foregroundColor: "#abb2bf",
      contrast: 60,
      translucentSidebar: true,
    },
  },
];

export function resolveThemeColors(theme: BuiltinTheme, isDark: boolean): ThemeColorSet {
  return isDark ? theme.darkSettings : theme.settings;
}

/** Validate and import a theme JSON or codex-theme-v1 string. Returns error string or null on success. */
export function importThemeJson(raw: string): string | null {
  let cleanRaw = raw.trim();
  if (cleanRaw.startsWith("codex-theme-v1:")) {
    cleanRaw = cleanRaw.slice("codex-theme-v1:".length);
  }
  let obj: unknown;
  try {
    obj = JSON.parse(cleanRaw);
  } catch {
    return "Invalid JSON string";
  }
  if (typeof obj !== "object" || obj === null || Array.isArray(obj)) {
    return "Expected a JSON object";
  }
  const o = obj as Record<string, unknown>;
  const patch: Partial<AppearanceSettings> = {};
  const hexRe = /^#[0-9a-fA-F]{6}$/;

  // Parse Codex structure { theme: { accent, surface, ink, contrast, opaqueWindows } }
  if (typeof o["theme"] === "object" && o["theme"] !== null) {
    const tObj = o["theme"] as Record<string, unknown>;
    if (typeof tObj["accent"] === "string" && hexRe.test(tObj["accent"])) {
      patch.accentColor = tObj["accent"];
    }
    if (typeof tObj["surface"] === "string" && hexRe.test(tObj["surface"])) {
      patch.backgroundColor = tObj["surface"];
    }
    if (typeof tObj["ink"] === "string" && hexRe.test(tObj["ink"])) {
      patch.foregroundColor = tObj["ink"];
    }
    if (typeof tObj["contrast"] === "number") {
      patch.contrast = Math.min(100, Math.max(0, tObj["contrast"]));
    }
    if (typeof tObj["opaqueWindows"] === "boolean") {
      patch.translucentSidebar = !tObj["opaqueWindows"];
    }
  }

  // Parse standard AgentCabin colors
  const colors = o["colors"];
  if (typeof colors === "object" && colors !== null) {
    const c = colors as Record<string, unknown>;
    if (typeof c["accent"] === "string" && hexRe.test(c["accent"])) {
      patch.accentColor = c["accent"];
    }
    if (typeof c["background"] === "string" && hexRe.test(c["background"])) {
      patch.backgroundColor = c["background"];
    }
    if (typeof c["foreground"] === "string" && hexRe.test(c["foreground"])) {
      patch.foregroundColor = c["foreground"];
    }
  }

  const fonts = o["fonts"];
  if (typeof fonts === "object" && fonts !== null) {
    const f = fonts as Record<string, unknown>;
    if (typeof f["ui"] === "string" && f["ui"]) patch.uiFontFamily = f["ui"];
    if (typeof f["code"] === "string" && f["code"]) patch.codeFontFamily = f["code"];
  }

  if (typeof o["translucentSidebar"] === "boolean") {
    patch.translucentSidebar = o["translucentSidebar"];
  }
  if (typeof o["contrast"] === "number") {
    patch.contrast = Math.min(100, Math.max(0, o["contrast"]));
  }

  setAppearance(patch);
  return null;
}

/** Export current theme as a JSON string. */
export function exportThemeJson(name: string = "My Theme"): string {
  const s = _settings;
  return JSON.stringify(
    {
      name,
      version: 1,
      colors: {
        accent: s.accentColor || "#20b982",
        background: s.backgroundColor || "#ffffff",
        foreground: s.foregroundColor || "#17191c",
      },
      fonts: {
        ui: s.uiFontFamily || "-apple-system, BlinkMacSystemFont, sans-serif",
        code: s.codeFontFamily || "ui-monospace, SFMono-Regular, monospace",
      },
      translucentSidebar: s.translucentSidebar,
      contrast: s.contrast,
    },
    null,
    2,
  );
}
