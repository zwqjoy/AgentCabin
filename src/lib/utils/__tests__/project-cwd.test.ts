import { describe, it, expect, beforeEach } from "vitest";
import {
  getProjectCwdKey,
  getPinnedCwdsKey,
  getExpandedProjectsKey,
  getSavedProjectCwd,
  setSavedProjectCwd,
  getSavedPinnedCwds,
  setSavedPinnedCwds,
  getSavedExpandedProjects,
  setSavedExpandedProjects,
  PROJECT_CWD_NATIVE_KEY,
  PROJECT_CWD_PI_KEY,
  PINNED_CWDS_NATIVE_KEY,
  PINNED_CWDS_PI_KEY,
  EXPANDED_PROJECTS_NATIVE_KEY,
  EXPANDED_PROJECTS_PI_KEY,
} from "../project-cwd";

describe("project-cwd realm scoping", () => {
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

  it("returns distinct keys for native vs pi", () => {
    expect(getProjectCwdKey("native")).toBe(PROJECT_CWD_NATIVE_KEY);
    expect(getProjectCwdKey("pi")).toBe(PROJECT_CWD_PI_KEY);
    expect(getPinnedCwdsKey("native")).toBe(PINNED_CWDS_NATIVE_KEY);
    expect(getPinnedCwdsKey("pi")).toBe(PINNED_CWDS_PI_KEY);
    expect(getExpandedProjectsKey("native")).toBe(EXPANDED_PROJECTS_NATIVE_KEY);
    expect(getExpandedProjectsKey("pi")).toBe(EXPANDED_PROJECTS_PI_KEY);
  });

  it("saves and retrieves project CWD independently per realm", () => {
    setSavedProjectCwd("/Users/dev/native-app", "native");
    setSavedProjectCwd("/Users/dev/pi-app", "pi");

    expect(getSavedProjectCwd("native")).toBe("/Users/dev/native-app");
    expect(getSavedProjectCwd("pi")).toBe("/Users/dev/pi-app");

    setSavedProjectCwd("", "native");
    expect(getSavedProjectCwd("native")).toBe("");
    expect(getSavedProjectCwd("pi")).toBe("/Users/dev/pi-app");
  });

  it("saves and retrieves pinned CWDs independently per realm", () => {
    setSavedPinnedCwds(["/Users/dev/native-1", "/Users/dev/native-2"], "native");
    setSavedPinnedCwds(["/Users/dev/pi-1"], "pi");

    expect(getSavedPinnedCwds("native")).toEqual(["/Users/dev/native-1", "/Users/dev/native-2"]);
    expect(getSavedPinnedCwds("pi")).toEqual(["/Users/dev/pi-1"]);
  });

  it("saves and retrieves expanded projects independently per realm", () => {
    setSavedExpandedProjects(new Set(["cwd:/Users/dev/native-1"]), "native");
    setSavedExpandedProjects(new Set(["cwd:/Users/dev/pi-1"]), "pi");

    expect(getSavedExpandedProjects("native").has("cwd:/Users/dev/native-1")).toBe(true);
    expect(getSavedExpandedProjects("native").has("cwd:/Users/dev/pi-1")).toBe(false);

    expect(getSavedExpandedProjects("pi").has("cwd:/Users/dev/pi-1")).toBe(true);
    expect(getSavedExpandedProjects("pi").has("cwd:/Users/dev/native-1")).toBe(false);
  });
});
