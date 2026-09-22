import { describe, expect, it, vi } from "vitest";
import { applyPiPermissionMode, normalizePiPermissionMode } from "./pi-permission";
import { resolveStartupPermissionMode } from "./permission-mode";

describe("resolveStartupPermissionMode", () => {
  it.each([
    ["claude", "bypassPermissions", "guarded", "bypassPermissions"],
    ["claude", "auto-grant", "guarded", "bypassPermissions"],
    ["claude", "ask-all", "guarded", "default"],
    ["codex", "plan", "guarded", "plan"],
    ["grok", "acceptEdits", "guarded", "acceptEdits"],
    ["pi", "default", "auto_approve", "auto_approve"],
  ])("uses the selected mode for %s", (agent, genericMode, piMode, expected) => {
    expect(resolveStartupPermissionMode(agent, genericMode, piMode)).toBe(expected);
  });

  it("gives an explicit session override precedence over the UI mode", () => {
    expect(
      resolveStartupPermissionMode("claude", "bypassPermissions", "guarded", "acceptEdits"),
    ).toBe("acceptEdits");
  });

  it("keeps backend settings as the fallback when a generic mode is not loaded yet", () => {
    expect(resolveStartupPermissionMode("codex", "", "guarded")).toBeUndefined();
  });
});

describe("normalizePiPermissionMode", () => {
  it("maps the Claude-style edit mode to the Pi edit policy", () => {
    expect(normalizePiPermissionMode("accept_edits")).toBe("accept_edits");
    expect(normalizePiPermissionMode("acceptEdits")).toBe("accept_edits");
    expect(normalizePiPermissionMode("auto_read")).toBe("accept_edits");
  });

  it("maps legacy bypass names to the Pi yolo policy", () => {
    expect(normalizePiPermissionMode("auto_approve")).toBe("auto_approve");
    expect(normalizePiPermissionMode("bypassPermissions")).toBe("auto_approve");
    expect(normalizePiPermissionMode("bypass")).toBe("auto_approve");
  });

  it("falls back to guarded mode", () => {
    expect(normalizePiPermissionMode("default")).toBe("guarded");
    expect(normalizePiPermissionMode("unknown")).toBe("guarded");
  });
});

describe("applyPiPermissionMode", () => {
  it("persists a mode without starting a session when no Pi run exists", async () => {
    const persistMode = vi.fn().mockResolvedValue(undefined);
    const setLiveMode = vi.fn().mockResolvedValue(undefined);

    const result = await applyPiPermissionMode("auto_approve", {
      runId: null,
      sessionAlive: false,
      persistMode,
      setLiveMode,
    });

    expect(result).toEqual({ mode: "auto_approve", appliedToLiveSession: false });
    expect(persistMode).toHaveBeenCalledWith("auto_approve");
    expect(setLiveMode).not.toHaveBeenCalled();
  });

  it("applies the edit policy to a live RPC session", async () => {
    const persistMode = vi.fn().mockResolvedValue(undefined);
    const setLiveMode = vi.fn().mockResolvedValue(undefined);

    const result = await applyPiPermissionMode("acceptEdits", {
      runId: "pi-run-1",
      sessionAlive: true,
      persistMode,
      setLiveMode,
    });

    expect(result).toEqual({ mode: "accept_edits", appliedToLiveSession: true });
    expect(setLiveMode).toHaveBeenCalledWith("pi-run-1", "accept_edits");
    expect(persistMode).toHaveBeenCalledWith("accept_edits");
  });

  it("persists instead of sending a live request when host control is unavailable", async () => {
    const persistMode = vi.fn().mockResolvedValue(undefined);
    const setLiveMode = vi.fn().mockResolvedValue(undefined);

    const result = await applyPiPermissionMode("auto_approve", {
      runId: "pi-run-1",
      sessionAlive: true,
      liveControlAvailable: false,
      persistMode,
      setLiveMode,
    });

    expect(result).toEqual({ mode: "auto_approve", appliedToLiveSession: false });
    expect(setLiveMode).not.toHaveBeenCalled();
    expect(persistMode).toHaveBeenCalledWith("auto_approve");
  });
});
