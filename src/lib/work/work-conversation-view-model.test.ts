import { describe, it, expect } from "vitest";
import { computeConversationStatus } from "./work-conversation-view-model";

describe("computeConversationStatus", () => {
  it("prioritizes waiting attention over running", () => {
    expect(
      computeConversationStatus({
        isRunning: true,
        sessionAlive: true,
        pendingCount: 2,
      }),
    ).toBe("waiting");
  });

  it("returns running when active without pending items", () => {
    expect(
      computeConversationStatus({
        isRunning: true,
        sessionAlive: true,
        pendingCount: 0,
      }),
    ).toBe("running");
  });

  it("maps terminal statuses correctly", () => {
    expect(
      computeConversationStatus({
        isRunning: false,
        sessionAlive: false,
        pendingCount: 0,
        runStatus: "completed",
      }),
    ).toBe("completed");

    expect(
      computeConversationStatus({
        isRunning: false,
        sessionAlive: false,
        pendingCount: 0,
        runStatus: "failed",
      }),
    ).toBe("failed");
  });
});
