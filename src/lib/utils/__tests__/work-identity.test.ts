import { describe, it, expect, vi } from "vitest";
import { normalizeWorkIdentity } from "../work-identity";

describe("Work Identity Normalization (normalizeWorkIdentity)", () => {
  // Case 1: 正常 workspace Work
  it("Case 1: normal workspace Work retains distinct taskId and workRunId", () => {
    const identity = normalizeWorkIdentity({
      progressView: {
        taskId: "task-100",
        workRunId: "run-200",
      },
      workspaceId: "ws-1",
    });

    expect(identity.taskId).toBe("task-100");
    expect(identity.workRunId).toBe("run-200");
    expect(identity.taskId).not.toBe(identity.workRunId);
    expect(identity.canLoadWorkspaceReceipt).toBe(true);
    expect(identity.isStandalone).toBe(false);
  });

  // Case 2: session.run 有完整 identity (run.id != work_run_id)
  it("Case 2: session.run identity is strictly preserved and run.id never overwrites work_task_id or work_run_id", () => {
    const identity = normalizeWorkIdentity({
      run: {
        id: "session-xyz",
        work_task_id: "task-100",
        work_run_id: "run-200",
        workspace_id: "ws-1",
      },
      progressView: {
        taskId: "task-100",
        workRunId: "run-200",
      },
      workspaceId: "ws-1",
    });

    expect(identity.taskId).toBe("task-100");
    expect(identity.workRunId).toBe("run-200");
    expect(identity.sessionRunId).toBe("session-xyz");
    expect(identity.workRunId).not.toBe("session-xyz");
    expect(identity.taskId).not.toBe("session-xyz");
    expect(identity.canLoadWorkspaceReceipt).toBe(true);
  });

  // Case 3: 缺 work_task_id
  it("Case 3: missing work_task_id does not forge taskId from run.id, but falls back to automationTaskId or leaves undefined", () => {
    // 3a: automationTaskId available from WorkRun manifest
    const identityWithManifest = normalizeWorkIdentity({
      run: {
        id: "run-200",
        work_run_id: "run-200",
        workspace_id: "ws-1",
      },
      progressView: {
        workRunId: "run-200",
        automationTaskId: "task-recovered-from-manifest",
      },
      workspaceId: "ws-1",
    });

    expect(identityWithManifest.taskId).toBe("task-recovered-from-manifest");
    expect(identityWithManifest.workRunId).toBe("run-200");
    expect(identityWithManifest.canLoadWorkspaceReceipt).toBe(true);

    // 3b: no taskId at all (e.g. interactive conversation in workspace)
    const identityInteractive = normalizeWorkIdentity({
      run: {
        id: "run-interactive-1",
        workspace_id: "ws-1",
      },
      progressView: {
        workRunId: "run-interactive-1",
      },
      workspaceId: "ws-1",
    });

    expect(identityInteractive.taskId).toBeUndefined();
    expect(identityInteractive.workRunId).toBe("run-interactive-1");
    // Crucially: never forged as run.id ("run-interactive-1")
    expect(identityInteractive.taskId).not.toBe("run-interactive-1");
    expect(identityInteractive.canLoadWorkspaceReceipt).toBe(false);
  });

  // Case 4: Standalone Work
  it("Case 4: standalone Work correctly identified without corrupting workspace identity", () => {
    const identity = normalizeWorkIdentity({
      run: {
        id: "standalone-run-123",
      },
      conversationRunId: "standalone-run-123",
      workspaceId: null,
    });

    expect(identity.isStandalone).toBe(true);
    expect(identity.sessionRunId).toBe("standalone-run-123");
    expect(identity.workRunId).toBe("standalone-run-123");
    expect(identity.taskId).toBeUndefined();
    // Standalone runs must NOT use workspace receipt
    expect(identity.canLoadWorkspaceReceipt).toBe(false);
  });

  // Case 5: Receipt lookup simulation
  it("Case 5: receipt lookup resolves correct (taskId, workRunId) arguments", () => {
    const progressView = {
      taskId: "task-1",
      workRunId: "run-1",
    };
    const identity = normalizeWorkIdentity({
      progressView,
      workspaceId: "ws-alpha",
    });

    const mockGetWorkRunReceipt = vi.fn();
    if (identity.canLoadWorkspaceReceipt && identity.taskId) {
      mockGetWorkRunReceipt(identity.taskId, identity.workRunId);
    }

    expect(mockGetWorkRunReceipt).toHaveBeenCalledWith("task-1", "run-1");
    expect(mockGetWorkRunReceipt).not.toHaveBeenCalledWith("run-1", "run-1");
  });

  // Case 6: 错误 identity 不静默吞掉 (taskId === workRunId rejected)
  it("Case 6: identical taskId and workRunId (the buggy same-id) is rejected and does not trigger bogus receipt lookup", () => {
    const buggyUUID = "df8edb58-9039-4307-abe6-4b6b9344e8c4";
    const identity = normalizeWorkIdentity({
      run: {
        id: buggyUUID,
        workspace_id: "AgentCabin",
      },
      progressView: {
        taskId: buggyUUID, // buggy forged taskId === workRunId
        workRunId: buggyUUID,
      },
      workspaceId: "AgentCabin",
      conversationRunId: buggyUUID,
    });

    // The forged taskId is rejected because it equals workRunId / sessionRunId
    expect(identity.taskId).toBeUndefined();
    expect(identity.workRunId).toBe(buggyUUID);
    expect(identity.canLoadWorkspaceReceipt).toBe(false);

    const mockGetWorkRunReceipt = vi.fn();
    if (identity.canLoadWorkspaceReceipt && identity.taskId) {
      mockGetWorkRunReceipt(identity.taskId, identity.workRunId);
    }

    // Must NOT call getWorkRunReceipt with the same id
    expect(mockGetWorkRunReceipt).not.toHaveBeenCalled();
  });
});
