import { describe, expect, it, vi } from "vitest";
import type { TaskRun } from "$lib/types";
import { startContinuationSession } from "./continuation";

function makeRun(id: string): TaskRun {
  return {
    id,
    prompt: "",
    cwd: "/workspace",
    agent: "claude",
    auth_mode: "cli",
    status: "pending",
    started_at: "2026-08-06T00:00:00.000Z",
    execution_path: "session_actor",
  };
}

describe("startContinuationSession", () => {
  it("copies history through the anchor and starts the target without an automatic prompt", async () => {
    const targetRun = makeRun("target-run");
    const deps = {
      getContinuationContext: vi.fn().mockResolvedValue("historical context"),
      startRun: vi.fn().mockResolvedValue(targetRun),
      copyRunHistory: vi.fn().mockResolvedValue(undefined),
      resumeSession: vi.fn().mockResolvedValue(targetRun.id),
    };

    await startContinuationSession(
      {
        sourceRun: makeRun("source-run"),
        anchorId: "assistant-2",
        targetCwd: "/workspace",
      },
      deps,
    );

    expect(deps.getContinuationContext).toHaveBeenCalledWith("source-run", "assistant-2");
    expect(deps.startRun).toHaveBeenCalledWith(
      "",
      "/workspace",
      "claude",
      undefined,
      undefined,
      undefined,
      "session_actor",
      "historical context",
    );
    expect(deps.copyRunHistory).toHaveBeenCalledWith("source-run", "target-run", "assistant-2");
    expect(deps.resumeSession).toHaveBeenCalledWith("target-run", "new");
  });

  it("can branch the full history into another Agent without sending a prompt", async () => {
    const targetRun = makeRun("target-run");
    const deps = {
      getContinuationContext: vi.fn().mockResolvedValue("full historical context"),
      startRun: vi.fn().mockResolvedValue(targetRun),
      copyRunHistory: vi.fn().mockResolvedValue(undefined),
      resumeSession: vi.fn().mockResolvedValue(targetRun.id),
    };

    await startContinuationSession(
      {
        sourceRun: makeRun("source-run"),
        targetAgent: "codex",
        targetCwd: "/workspace",
      },
      deps,
    );

    expect(deps.getContinuationContext).toHaveBeenCalledWith("source-run", undefined);
    expect(deps.startRun).toHaveBeenCalledWith(
      "",
      "/workspace",
      "codex",
      undefined,
      undefined,
      undefined,
      "session_actor",
      "full historical context",
    );
    expect(deps.copyRunHistory).toHaveBeenCalledWith("source-run", "target-run", undefined);
    expect(deps.resumeSession).toHaveBeenCalledWith("target-run", "new");
  });
});
