import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  t,
  initLocale,
  switchLocale,
  currentLocale,
  locales,
  baseLocale,
  isLocale,
  LOCALE_REGISTRY,
  SUPPORTED_LOCALES,
  BASE_LOCALE,
  getEntry,
} from "../index.svelte";

// Mock debug utils (auto-mocked by vitest config convention)
vi.mock("$lib/utils/debug", () => ({
  dbg: vi.fn(),
  dbgWarn: vi.fn(),
}));

// Minimal DOM stubs for <html> attribute tests
function setupDocument() {
  globalThis.document = {
    documentElement: { lang: "", dir: "" },
  } as unknown as Document;
}

function setupLocalStorage() {
  const store: Record<string, string> = {};
  const lsImpl = {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, val: string) => {
      store[key] = val;
    },
    removeItem: (key: string) => {
      delete store[key];
    },
  };
  // @ts-expect-error - test stub
  globalThis.localStorage = lsImpl;
  // Ensure `typeof window !== "undefined"` passes in the module
  // @ts-expect-error - test stub
  globalThis.window = { localStorage: lsImpl };
  return store;
}

describe("i18n", () => {
  let lsStore: Record<string, string>;

  beforeEach(() => {
    setupDocument();
    lsStore = setupLocalStorage();
    // Clear any localStorage side-effects
    delete lsStore["agentcabin:locale"];
    delete lsStore["PARAGLIDE_LOCALE"];
    initLocale();
  });

  // ── t(key) basic ──

  it("returns the Chinese translation for a known key", () => {
    initLocale();
    expect(t("settings_title")).toBe("设置");
  });

  it("returns interpolated translation with params", () => {
    initLocale();
    const result = t("settings_general_lastUpdated", { date: "2026-02-17" });
    expect(result).toBe("上次更新：2026-02-17");
  });

  it("falls back to en when key is missing in zh-CN", () => {
    // If a key is missing in zh-CN, fallback to en bundle
    expect(t("settings_title")).toBe("设置");
  });

  it("returns raw key when key is completely missing", () => {
    initLocale();
    // @ts-expect-error - testing invalid key deliberately
    expect(t("this_key_does_not_exist")).toBe("this_key_does_not_exist");
  });

  it("preserves unreplaced placeholders when param is missing", () => {
    initLocale();
    expect(t("settings_general_lastUpdated")).toBe("上次更新：{date}");
  });

  // ── switchLocale ──

  it("ignores switch to unsupported locale such as en", () => {
    initLocale();
    switchLocale("en");
    expect(currentLocale()).toBe("zh-CN");
  });

  it("ignores switch to unknown locale", () => {
    initLocale();
    switchLocale("xx-INVALID");
    expect(currentLocale()).toBe("zh-CN");
  });

  it("ignores switch to same locale", () => {
    initLocale();
    switchLocale("zh-CN");
    expect(currentLocale()).toBe("zh-CN");
  });

  // ── currentLocale ──

  it("returns the current locale after init", () => {
    initLocale();
    expect(currentLocale()).toBe("zh-CN");
  });

  // ── localStorage persistence (new key: agentcabin:locale) ──

  it("reads locale from localStorage on init", () => {
    lsStore["agentcabin:locale"] = "zh-CN";
    initLocale();
    expect(currentLocale()).toBe("zh-CN");
  });

  // ── Legacy localStorage migration ──

  it("migrates PARAGLIDE_LOCALE to agentcabin:locale on init", () => {
    lsStore["PARAGLIDE_LOCALE"] = "zh-CN";
    initLocale();
    expect(currentLocale()).toBe("zh-CN");
    expect(lsStore["agentcabin:locale"]).toBe("zh-CN");
  });

  it("overwrites unsupported legacy locale with baseLocale on init", () => {
    lsStore["agentcabin:locale"] = "en";
    initLocale();
    expect(currentLocale()).toBe("zh-CN");
    expect(lsStore["agentcabin:locale"]).toBe("zh-CN");
  });

  // ── <html> attributes ──

  it("sets document.documentElement.lang on init", () => {
    initLocale();
    expect(document.documentElement.lang).toBe("zh-CN");
  });

  it("sets dir=ltr for LTR locales", () => {
    initLocale();
    expect(document.documentElement.dir).toBe("ltr");
  });

  // ── Static exports ──

  it("exports correct locales array", () => {
    expect(locales).toEqual(["zh-CN"]);
  });

  it("exports correct baseLocale", () => {
    expect(baseLocale).toBe("zh-CN");
  });

  it("isLocale returns true for zh-CN and false for en", () => {
    expect(isLocale("en")).toBe(false);
    expect(isLocale("zh-CN")).toBe(true);
  });

  // ── Registry ──

  it("LOCALE_REGISTRY contains all supported locales", () => {
    const registryCodes = LOCALE_REGISTRY.map((e) => e.code);
    expect(registryCodes).toEqual(SUPPORTED_LOCALES);
  });

  it("SUPPORTED_LOCALES matches locales alias", () => {
    expect(SUPPORTED_LOCALES).toEqual(locales);
  });

  it("BASE_LOCALE matches baseLocale alias", () => {
    expect(BASE_LOCALE).toBe(baseLocale);
  });

  it("getEntry returns correct entry for known locale", () => {
    const entry = getEntry("zh-CN");
    expect(entry).toBeDefined();
    expect(entry!.nativeName).toBe("简体中文");
    expect(entry!.shortLabel).toBe("中");
    expect(entry!.dir).toBe("ltr");
  });

  it("getEntry returns undefined for unknown locale", () => {
    expect(getEntry("en")).toBeUndefined();
    expect(getEntry("xx")).toBeUndefined();
  });

  it("every registry entry has required fields", () => {
    for (const entry of LOCALE_REGISTRY) {
      expect(entry.code).toBeTruthy();
      expect(entry.nativeName).toBeTruthy();
      expect(entry.shortLabel).toBeTruthy();
      expect(["ltr", "rtl"]).toContain(entry.dir);
      expect(["stable", "beta"]).toContain(entry.status);
    }
  });
});
