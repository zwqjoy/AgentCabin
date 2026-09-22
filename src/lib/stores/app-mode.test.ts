import { describe, expect, it, beforeEach } from "vitest";
import {
  getAppMode,
  getAppRealm,
  getAgentTargetFromUrl,
  getModeHref,
  getRealmHref,
  getSavedAppMode,
  setSavedAppMode,
  getSavedRealm,
  setSavedRealm,
  getSavedPiSubMode,
  setSavedPiSubMode,
  APP_MODE_STORAGE_KEY,
  APP_REALM_STORAGE_KEY,
  PI_SUBMODE_STORAGE_KEY,
} from "./app-mode.svelte";

describe("app mode and realm routing", () => {
  let storage: Map<string, string>;

  beforeEach(() => {
    storage = new Map<string, string>();
    const lsImpl = {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => storage.set(k, String(v)),
      removeItem: (k: string) => storage.delete(k),
      clear: () => storage.clear(),
    };
    (globalThis as unknown as { localStorage: unknown }).localStorage = lsImpl;
    (globalThis as unknown as { window: unknown }).window = { localStorage: lsImpl };
  });

  it("defaults to Native realm and Code mode", () => {
    expect(getAppMode(new URL("http://localhost/chat"))).toBe("code");
    expect(getAppRealm(new URL("http://localhost/chat"))).toBe("native");
    expect(getAgentTargetFromUrl(new URL("http://localhost/chat"))).toBe("native:claude");
    expect(getAgentTargetFromUrl(new URL("http://localhost/chat?agent=codex"))).toBe(
      "native:codex",
    );
    expect(getAgentTargetFromUrl(new URL("http://localhost/chat?agent=grok"))).toBe("native:grok");
  });

  it("recognizes Pi Code and Pi Work routes", () => {
    expect(getAppRealm(new URL("http://localhost/chat/pi"))).toBe("pi");
    expect(getAgentTargetFromUrl(new URL("http://localhost/chat/pi"))).toBe("pi:code");

    expect(getAppMode(new URL("http://localhost/chat/work"))).toBe("work");
    expect(getAppRealm(new URL("http://localhost/chat/work"))).toBe("pi");
    expect(getAgentTargetFromUrl(new URL("http://localhost/chat/work"))).toBe("work");
  });

  it("recognizes Work from the product route independently of its runtime", () => {
    expect(getAppMode(new URL("http://localhost/chat/work?agent=dsh"))).toBe("work");
    expect(getAppMode(new URL("http://localhost/chat?mode=work&agent=dsh"))).toBe("work");
  });

  it("keeps Work mode for the plugin scope route", () => {
    expect(getAppMode(new URL("http://localhost/chat/plugins?scope=work"))).toBe("work");
    expect(getAppMode(new URL("http://localhost/chat/plugins?scope=pi-work"))).toBe("work");
  });

  it("builds realm links correctly", () => {
    expect(getRealmHref("native")).toBe("/chat");
    expect(getRealmHref("pi", "code")).toBe("/chat/pi");
    expect(getRealmHref("pi", "work")).toBe("/chat/work");
    expect(getRealmHref("pi", "work", "ws-123")).toBe("/chat/work?workspace=ws-123");
  });

  it("persists and restores saved realms and submodes", () => {
    expect(getSavedRealm()).toBe("native");
    setSavedRealm("pi");
    expect(localStorage.getItem(APP_REALM_STORAGE_KEY)).toBe("pi");
    expect(getSavedRealm()).toBe("pi");

    expect(getSavedPiSubMode()).toBe("code");
    setSavedPiSubMode("work");
    expect(localStorage.getItem(PI_SUBMODE_STORAGE_KEY)).toBe("work");
    expect(getSavedPiSubMode()).toBe("work");
  });

  it("persists work mode across app restart / root redirect", () => {
    // User selects work mode
    setSavedAppMode("work");
    expect(getSavedAppMode()).toBe("work");
    expect(getSavedRealm()).toBe("pi");
    expect(getSavedPiSubMode()).toBe("work");

    // Next time app opens at root route "/"
    const rootUrl = new URL("http://localhost/");
    expect(getAgentTargetFromUrl(rootUrl)).toBe("work");
    expect(getAppMode(rootUrl)).toBe("work");
    expect(getAppRealm(rootUrl)).toBe("pi");
  });

  it("persists code mode across app restart / root redirect", () => {
    // User was in work mode, then selects code mode with Pi
    setSavedAppMode("work");
    setSavedRealm("pi");
    setSavedAppMode("code");
    expect(getSavedAppMode()).toBe("code");
    expect(getSavedPiSubMode()).toBe("code");
    expect(getSavedRealm()).toBe("pi");

    // Next time app opens at root route "/"
    const rootUrl = new URL("http://localhost/");
    expect(getAgentTargetFromUrl(rootUrl)).toBe("pi:code");
    expect(getAppMode(rootUrl)).toBe("code");
    expect(getAppRealm(rootUrl)).toBe("pi");

    // User selects native code mode
    setSavedRealm("native");
    expect(getAgentTargetFromUrl(rootUrl)).toBe("native:claude");
    expect(getAppMode(rootUrl)).toBe("code");
    expect(getAppRealm(rootUrl)).toBe("native");
  });
});
