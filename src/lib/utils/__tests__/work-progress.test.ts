import { describe, expect, it } from "vitest";
import type { StructuredTask } from "$lib/types";
import {
  deriveWorkProgressPhase,
  isAgentTurnCompleted,
  isWorkTaskCompleted,
  workProgressPercent,
} from "$lib/utils/work-progress";

const task = (status: StructuredTask["status"], id?: string): StructuredTask => ({
  id: id ?? status,
  text: id ?? status,
  status,
});

const base = {
  sessionPhase: "idle",
  runStatus: "idle" as const,
  tasks: [task("completed")],
  artifacts: [],
  hasPendingPermission: false,
  hasElicitation: false,
};

describe("work progress", () => {
  it("keeps approval and elicitation waits visible above the run phase", () => {
    expect(
      deriveWorkProgressPhase({ ...base, sessionPhase: "running", hasPendingPermission: true }),
    ).toBe("waiting_approval");
    expect(
      deriveWorkProgressPhase({ ...base, sessionPhase: "running", hasElicitation: true }),
    ).toBe("waiting_input");
  });

  it("lets a stopped or cancelled run win over stale pending interaction flags", () => {
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "stopped",
        runStatus: "stopped",
        hasPendingPermission: true,
        hasElicitation: true,
      }),
    ).toBe("stopped");
    expect(
      deriveWorkProgressPhase({
        ...base,
        workRunStatus: "cancelled",
        hasPendingPermission: true,
      }),
    ).toBe("cancelled");
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        runStatus: "completed",
        hasPendingPermission: true,
        hasElicitation: true,
      }),
    ).toBe("completed");
  });

  it("treats an undelivered Artifact as validation or delivery work", () => {
    expect(
      deriveWorkProgressPhase({
        ...base,
        artifacts: [{ status: "ready" }],
      }),
    ).toBe("idle");
    expect(
      deriveWorkProgressPhase({
        ...base,
        artifacts: [{ status: "validated" }],
      }),
    ).toBe("idle");
  });

  it("does not report completion while an Artifact is not delivered", () => {
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        runStatus: "completed",
        artifacts: [{ status: "delivered" }],
      }),
    ).toBe("completed");
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        tasks: [],
        artifacts: [{ status: "ready" }],
      }),
    ).toBe("awaiting_delivery");
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        runStatus: "completed",
        tasks: [],
        artifacts: [{ status: "delivered" }],
      }),
    ).toBe("completed");
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        tasks: [],
        artifacts: [],
      }),
    ).toBe("completed");
  });

  it("keeps pending delivery visible even when plan bookkeeping is incomplete", () => {
    const input = {
      ...base,
      sessionPhase: "completed",
      tasks: [task("pending")],
      artifacts: [{ status: "ready" as const }],
    };
    expect(deriveWorkProgressPhase(input)).toBe("awaiting_delivery");
    expect(isWorkTaskCompleted(input)).toBe(false);
    expect(isAgentTurnCompleted("completed")).toBe(true);
  });

  it("does not project a terminal WorkRun as completed while planned steps remain", () => {
    expect(
      deriveWorkProgressPhase({
        ...base,
        sessionPhase: "completed",
        runStatus: "completed",
        tasks: [task("in_progress"), task("pending", "next")],
        artifacts: [],
      }),
    ).toBe("planning");
  });

  it("calculates a determinate task percentage", () => {
    expect(workProgressPercent([task("completed", "one"), task("pending", "two")])).toBe(50);
    expect(workProgressPercent([])).toBeNull();
  });
});
