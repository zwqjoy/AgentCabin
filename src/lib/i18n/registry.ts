/**
 * Locale registry — single source of truth for supported languages.
 *
 * To add a new language:
 * 1. Add a LocaleEntry here
 * 2. Create messages/<code>.json
 * 3. Add a loader line in index.svelte.ts
 */

export interface LocaleEntry {
  code: string;
  nativeName: string;
  shortLabel: string;
  dir: "ltr" | "rtl";
  status: "stable" | "beta";
}

export const LOCALE_REGISTRY = [
  {
    code: "zh-CN",
    nativeName: "简体中文",
    shortLabel: "中",
    dir: "ltr",
    status: "stable",
  },
] as const satisfies readonly LocaleEntry[];

export type Locale = (typeof LOCALE_REGISTRY)[number]["code"];

export const SUPPORTED_LOCALES: Locale[] = LOCALE_REGISTRY.map((e) => e.code) as Locale[];
export const BASE_LOCALE: Locale = "zh-CN";

export function isLocale(code: string): code is Locale {
  return (SUPPORTED_LOCALES as string[]).includes(code);
}

export function getEntry(code: string): LocaleEntry | undefined {
  return LOCALE_REGISTRY.find((e) => e.code === code);
}
