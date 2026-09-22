import { describe, expect, it } from "vitest";
import {
  getWorkActiveRunId,
  getWorkConversationSurfaceKey,
  getWorkConversationSurfaceKeyAfterRouteChange,
  getWorkspaceRowAction,
} from "$lib/utils/work-sidebar-navigation";

describe("Work conversation surface identity", () => {
  it("builds route-specific keys for workspace conversations", () => {
    expect(getWorkConversationSurfaceKey("workspace-1", "")).toBe("workspace-1:new");
    expect(getWorkConversationSurfaceKey("workspace-1", "run-1")).toBe("workspace-1:run-1");
  });

  it("keeps a new workspace conversation mounted when it adopts its first run", () => {
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange(
        "workspace-1:new",
        "workspace-1",
        "",
        "workspace-1",
        "run-1",
        true,
      ),
    ).toBe("workspace-1:new");
  });

  it("replaces the surface for an actual run or workspace selection", () => {
    // Navigating from new conversation to an existing run in the same workspace (sidebar click)
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange(
        "workspace-1:new",
        "workspace-1",
        "",
        "workspace-1",
        "run-1",
        false,
      ),
    ).toBe("workspace-1:run-1");
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange(
        "workspace-1:run-1",
        "workspace-1",
        "run-1",
        "workspace-1",
        "run-2",
        false,
      ),
    ).toBe("workspace-1:run-2");
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange(
        "workspace-1:run-2",
        "workspace-1",
        "run-2",
        "workspace-2",
        "",
        false,
      ),
    ).toBe("workspace-2:new");
  });

  it("uses the route run id for standalone conversations", () => {
    expect(getWorkConversationSurfaceKey("", "")).toBe("standalone-new");
    expect(getWorkConversationSurfaceKey("", "run-1")).toBe("run-1");
    // Adopting a newly started standalone run keeps the surface key
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange("standalone-new", "", "", "", "run-1", true),
    ).toBe("standalone-new");
    // Clicking an existing standalone run from the sidebar replaces the surface key
    expect(
      getWorkConversationSurfaceKeyAfterRouteChange("standalone-new", "", "", "", "run-1", false),
    ).toBe("run-1");
  });
});

describe("Work sidebar workspace navigation", () => {
  it("keeps the workspace configuration visible when clicking its name again", () => {
    expect(getWorkspaceRowAction("workspace-1", "", false, "workspace-1")).toEqual({
      kind: "stay",
    });
  });

  it("returns to configuration when clicking the active workspace from a conversation", () => {
    expect(getWorkspaceRowAction("workspace-1", "run-1", false, "workspace-1")).toEqual({
      kind: "navigate",
      href: "/chat/work?workspace=workspace-1",
    });
  });

  it("selects another workspace and opens its configuration", () => {
    expect(getWorkspaceRowAction("workspace-1", "", false, "workspace-2")).toEqual({
      kind: "navigate",
      href: "/chat/work?workspace=workspace-2",
    });
  });
});

describe("Work sidebar active run resolution", () => {
  it("resolves to started run before route navigation completes", () => {
    expect(getWorkActiveRunId("", "run-123")).toBe("run-123");
  });

  it("resolves to route run id once navigated", () => {
    expect(getWorkActiveRunId("run-123", "")).toBe("run-123");
    expect(getWorkActiveRunId("run-123", "run-123")).toBe("run-123");
  });

  it("returns empty string when no run is active", () => {
    expect(getWorkActiveRunId("", "")).toBe("");
  });
});
