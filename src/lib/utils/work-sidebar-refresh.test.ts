import { describe, expect, it } from "vitest";
import {
  getExpandedWorkSessionLoadTargets,
  getWorkSidebarRunStatus,
  getWorkSessionRefreshTargets,
} from "$lib/utils/work-sidebar-refresh";

describe("Work sidebar session refresh targets", () => {
  it.each([
    ["spawning", "running"],
    ["running", "running"],
    ["idle", "idle"],
    ["completed", "completed"],
    ["failed", "failed"],
    ["stopped", "stopped"],
  ])("maps runtime state %s to sidebar status %s", (state, status) => {
    expect(getWorkSidebarRunStatus(state)).toBe(status);
  });

  it("ignores unknown runtime state values", () => {
    expect(getWorkSidebarRunStatus("authenticating")).toBeNull();
    expect(getWorkSidebarRunStatus(undefined)).toBeNull();
  });

  it("loads sessions for workspaces restored as expanded before a workspace is selected", () => {
    expect(
      getExpandedWorkSessionLoadTargets(
        ["workspace-1", "workspace-2"],
        new Set(["workspace-1"]),
        new Set<string>(),
      ),
    ).toEqual(["workspace-1"]);
  });

  it("refreshes standalone tasks when their run changes state", () => {
    expect(
      getWorkSessionRefreshTargets(
        "standalone-run-1",
        {
          "workspace-1": [{ id: "workspace-run-1" }],
        },
        [{ id: "standalone-run-1" }],
      ),
    ).toEqual({
      workspaceIds: [],
      refreshStandalone: true,
    });
  });

  it("keeps workspace refresh behavior for workspace-scoped runs", () => {
    expect(
      getWorkSessionRefreshTargets(
        "workspace-run-1",
        {
          "workspace-1": [{ id: "workspace-run-1" }],
          "workspace-2": [{ id: "other-run" }],
        },
        [{ id: "standalone-run-1" }],
      ),
    ).toEqual({
      workspaceIds: ["workspace-1"],
      refreshStandalone: false,
    });
  });
});
