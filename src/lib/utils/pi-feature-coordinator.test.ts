import { describe, expect, it } from "vitest";
import {
  classifyPiGoalCommand,
  coercePiGoalStateForComposer,
  isPiFeatureAvailable,
  planEntryNeedsGoalPause,
  shouldBlockPiGoalCommand,
} from "./pi-feature-coordinator";

describe("Pi Plan/Goal coordinator rules", () => {
  it("treats a discovered Goal extension with no state entry as idle", () => {
    expect(coercePiGoalStateForComposer({ phase: "unavailable" }, { goalAvailable: true })).toEqual(
      { phase: "inactive" },
    );
  });

  it("blocks Goal start and resume while Plan is active", () => {
    expect(shouldBlockPiGoalCommand("active", "ship this feature")).toBe(true);
    expect(shouldBlockPiGoalCommand("active", "resume")).toBe(true);
    expect(shouldBlockPiGoalCommand("active", "pause")).toBe(false);
  });

  it("requires Goal pause before entering Plan", () => {
    expect(planEntryNeedsGoalPause("inactive", "active", "")).toBe(true);
    expect(planEntryNeedsGoalPause("inactive", "paused", "")).toBe(false);
    expect(planEntryNeedsGoalPause("active", "active", "exit")).toBe(false);
  });

  it("keeps unrelated slash commands outside the coordinator", () => {
    expect(classifyPiGoalCommand("status")).toBe("status");
    expect(classifyPiGoalCommand("/other-command")).toBe("start");
  });

  it("keeps configured Pi controls visible while runtime discovery is pending", () => {
    expect(isPiFeatureAvailable(null, "goalAvailable")).toBe(true);
    expect(
      isPiFeatureAvailable(
        {
          commandsReady: true,
          planAvailable: true,
          goalAvailable: false,
          permissionAvailable: true,
          sessionTreeAvailable: true,
          forkAvailable: true,
          cloneAvailable: true,
          steerAvailable: true,
          followUpAvailable: true,
          clearQueueAvailable: true,
          navigateTreeAvailable: true,
        },
        "goalAvailable",
      ),
    ).toBe(false);
  });
});
