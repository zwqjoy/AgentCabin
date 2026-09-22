import { afterEach, describe, expect, it, vi } from "vitest";
import {
  aggregatePetState,
  FAILED_HOLD_MS,
  mapSessionPhaseToPetMode,
  PetStateMachine,
  REVIEW_HOLD_MS,
} from "./pet-state";

describe("desktop pet state mapping", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("maps every SessionPhase to the public pet mode", () => {
    expect(mapSessionPhaseToPetMode("empty")).toBe("idle");
    expect(mapSessionPhaseToPetMode("ready")).toBe("idle");
    expect(mapSessionPhaseToPetMode("idle")).toBe("idle");
    expect(mapSessionPhaseToPetMode("stopped")).toBe("idle");
    expect(mapSessionPhaseToPetMode("loading")).toBe("waiting");
    expect(mapSessionPhaseToPetMode("spawning")).toBe("waiting");
    expect(mapSessionPhaseToPetMode("running")).toBe("running");
    expect(mapSessionPhaseToPetMode("completed")).toBe("review");
    expect(mapSessionPhaseToPetMode("failed")).toBe("failed");
  });

  it("aggregates sessions with failed > running > waiting priority", () => {
    const state = aggregatePetState([
      { phase: "spawning", runId: "waiting-run" },
      { phase: "running", runId: "running-run" },
      { phase: "failed", runId: "failed-run" },
    ]);

    expect(state.mode).toBe("failed");
    expect(state.runningCount).toBe(1);
    expect(state.errorCount).toBe(1);
    expect(state.activeRunId).toBe("failed-run");
  });

  it("holds review for four seconds after running becomes idle", () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const updates: string[] = [];
    const machine = new PetStateMachine((state) => updates.push(state.mode));

    machine.update({
      mode: "running",
      runningCount: 1,
      errorCount: 0,
      activeRunId: "run-1",
      timestamp: 0,
    });
    vi.setSystemTime(700);
    machine.update({
      mode: "idle",
      runningCount: 0,
      errorCount: 0,
      activeRunId: null,
      timestamp: 700,
    });

    expect(machine.getCurrentState()?.mode).toBe("review");
    vi.advanceTimersByTime(REVIEW_HOLD_MS - 1);
    expect(machine.getCurrentState()?.mode).toBe("review");
    vi.advanceTimersByTime(1);
    expect(machine.getCurrentState()?.mode).toBe("idle");
    expect(updates).toEqual(["running", "review", "idle"]);
    machine.destroy();
  });

  it("holds failed for four seconds before returning to idle", () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const machine = new PetStateMachine();
    const failed = {
      mode: "failed" as const,
      runningCount: 0,
      errorCount: 1,
      activeRunId: "run-error",
      timestamp: 0,
    };

    machine.update(failed);
    expect(machine.getCurrentState()?.mode).toBe("failed");
    vi.advanceTimersByTime(FAILED_HOLD_MS - 1);
    expect(machine.getCurrentState()?.mode).toBe("failed");
    vi.advanceTimersByTime(1);
    expect(machine.getCurrentState()?.mode).toBe("idle");
    machine.destroy();
  });

  it("deduplicates repeated visible states", () => {
    const listener = vi.fn();
    const machine = new PetStateMachine(listener);
    const state = {
      mode: "waiting" as const,
      runningCount: 0,
      errorCount: 0,
      activeRunId: "run-1",
      timestamp: 1,
    };

    machine.update(state);
    machine.update({ ...state, timestamp: 2 });

    expect(listener).toHaveBeenCalledOnce();
    machine.destroy();
  });
});
