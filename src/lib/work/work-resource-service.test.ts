import { beforeEach, describe, expect, it, vi } from "vitest";
import * as workApi from "$lib/api/work";
import {
  deliverArtifact,
  deleteArtifact,
  exportArtifact,
  getReceipt,
  listArtifacts,
  saveOfficeArtifact,
  validateArtifact,
} from "./work-resource-service";
import { standaloneScope, workspaceScope } from "./work-scope";

vi.mock("$lib/api/work", () => ({
  addWorkAccessRoot: vi.fn(),
  copyWorkArtifactToPrimary: vi.fn(),
  deleteStandaloneWorkArtifact: vi.fn(),
  deleteWorkArtifact: vi.fn(),
  deliverStandaloneWorkArtifact: vi.fn(),
  deliverWorkArtifact: vi.fn(),
  exportStandaloneWorkArtifact: vi.fn(),
  exportWorkArtifact: vi.fn(),
  getStandaloneWorkRunReceipt: vi.fn(),
  getWorkRunReceipt: vi.fn(),
  getWorkRunRecovery: vi.fn(),
  importWorkFile: vi.fn(),
  listStandaloneWorkArtifacts: vi.fn(),
  listWorkAccessRoots: vi.fn(),
  listWorkArtifacts: vi.fn(),
  listWorkFiles: vi.fn(),
  openWorkDirectory: vi.fn(),
  openWorkFile: vi.fn(),
  recoverWorkRun: vi.fn(),
  registerStandaloneWorkArtifact: vi.fn(),
  registerWorkArtifact: vi.fn(),
  removeWorkAccessRoot: vi.fn(),
  removeWorkFile: vi.fn(),
  setWorkAccessRootWritable: vi.fn(),
  updateStandaloneWorkOfficeArtifact: vi.fn(),
  updateWorkOfficeArtifact: vi.fn(),
  validateStandaloneWorkArtifact: vi.fn(),
  validateWorkArtifact: vi.fn(),
}));

describe("work-resource-service", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("listArtifacts", () => {
    it("routes workspace scope to the workspace API", async () => {
      vi.mocked(workApi.listWorkArtifacts).mockResolvedValue([{ id: "a1" }] as any);
      const result = await listArtifacts(workspaceScope("ws-1"), "run-1");
      expect(workApi.listWorkArtifacts).toHaveBeenCalledWith("ws-1", "run-1");
      expect(workApi.listStandaloneWorkArtifacts).not.toHaveBeenCalled();
      expect(result).toEqual([{ id: "a1" }]);
    });

    it("routes standalone scope to the standalone API and skips empty run ids", async () => {
      await listArtifacts(standaloneScope(), "");
      expect(workApi.listStandaloneWorkArtifacts).not.toHaveBeenCalled();

      vi.mocked(workApi.listStandaloneWorkArtifacts).mockResolvedValue([{ id: "a2" }] as any);
      const result = await listArtifacts(standaloneScope(), "run-9");
      expect(workApi.listStandaloneWorkArtifacts).toHaveBeenCalledWith("run-9");
      expect(workApi.listWorkArtifacts).not.toHaveBeenCalled();
      expect(result).toEqual([{ id: "a2" }]);
    });
  });

  describe("deleteArtifact", () => {
    it("deletes workspace artifacts idempotently when already reconciled", async () => {
      vi.mocked(workApi.deleteWorkArtifact).mockRejectedValue(
        new Error("work artifact art-1 not found"),
      );
      await deleteArtifact({ scope: workspaceScope("ws-1"), runId: "run-1", artifactId: "art-1" });
      expect(workApi.deleteWorkArtifact).toHaveBeenCalledWith("ws-1", "art-1");
    });

    it("prefers the artifact's own run id for standalone deletes", async () => {
      vi.mocked(workApi.deleteStandaloneWorkArtifact).mockResolvedValue(undefined);
      await deleteArtifact(
        { scope: standaloneScope(), runId: "run-current", artifactId: "art-2" },
        { runId: "run-old", workspaceId: "run-old" } as any,
      );
      expect(workApi.deleteStandaloneWorkArtifact).toHaveBeenCalledWith("run-old", "art-2");
    });

    it("skips standalone deletes without any usable run id", async () => {
      await deleteArtifact({ scope: standaloneScope(), runId: "", artifactId: "art-3" });
      expect(workApi.deleteStandaloneWorkArtifact).not.toHaveBeenCalled();
    });
  });

  describe("validate/deliver/export/save office", () => {
    it("validates standalone artifacts against the conversation run", async () => {
      vi.mocked(workApi.validateStandaloneWorkArtifact).mockResolvedValue({ id: "art-4" } as any);
      const result = await validateArtifact({
        scope: standaloneScope(),
        runId: "run-5",
        artifactId: "art-4",
      });
      expect(workApi.validateStandaloneWorkArtifact).toHaveBeenCalledWith("run-5", "art-4");
      expect(result).toEqual({ id: "art-4" });
    });

    it("throws for standalone validation before a run exists", async () => {
      await expect(
        validateArtifact({ scope: standaloneScope(), runId: "", artifactId: "art-4" }),
      ).rejects.toThrow();
      expect(workApi.validateStandaloneWorkArtifact).not.toHaveBeenCalled();
    });

    it("delivers workspace artifacts with the artifact's run id", async () => {
      vi.mocked(workApi.deliverWorkArtifact).mockResolvedValue({ id: "art-6" } as any);
      await deliverArtifact({
        scope: workspaceScope("ws-2"),
        runId: "run-6",
        artifactId: "art-6",
        artifactRunId: "run-producer",
      });
      expect(workApi.deliverWorkArtifact).toHaveBeenCalledWith("ws-2", "art-6", "run-producer");
    });

    it("exports via the matching backend family", async () => {
      vi.mocked(workApi.exportStandaloneWorkArtifact).mockResolvedValue("/tmp/out" as any);
      vi.mocked(workApi.exportWorkArtifact).mockResolvedValue("/tmp/out2" as any);

      await exportArtifact(standaloneScope(), "run-7", "art-7", "/dest");
      expect(workApi.exportStandaloneWorkArtifact).toHaveBeenCalledWith("run-7", "art-7", "/dest");

      await exportArtifact(workspaceScope("ws-3"), "run-8", "art-8", "/dest2", "run-producer");
      expect(workApi.exportWorkArtifact).toHaveBeenCalledWith(
        "ws-3",
        "art-8",
        "/dest2",
        "run-producer",
      );
    });

    it("saves office artifacts for both scopes", async () => {
      vi.mocked(workApi.updateStandaloneWorkOfficeArtifact).mockResolvedValue({} as any);
      vi.mocked(workApi.updateWorkOfficeArtifact).mockResolvedValue({} as any);

      await saveOfficeArtifact(standaloneScope(), "run-1", "art-1", "base64");
      expect(workApi.updateStandaloneWorkOfficeArtifact).toHaveBeenCalledWith(
        "run-1",
        "art-1",
        "base64",
      );

      await saveOfficeArtifact(workspaceScope("ws-1"), "run-2", "art-1", "base64");
      expect(workApi.updateWorkOfficeArtifact).toHaveBeenCalledWith(
        "ws-1",
        "art-1",
        "base64",
        "run-2",
      );
    });
  });

  describe("getReceipt", () => {
    it("loads standalone receipts from the conversation run id", async () => {
      vi.mocked(workApi.getStandaloneWorkRunReceipt).mockResolvedValue({ runId: "run-1" } as any);
      const receipt = await getReceipt({ scope: standaloneScope(), conversationRunId: "run-1" });
      expect(workApi.getStandaloneWorkRunReceipt).toHaveBeenCalledWith("run-1");
      expect(receipt).toEqual({ runId: "run-1" });
    });

    it("returns null for standalone conversations without a run", async () => {
      const receipt = await getReceipt({ scope: standaloneScope(), conversationRunId: "" });
      expect(receipt).toBeNull();
      expect(workApi.getStandaloneWorkRunReceipt).not.toHaveBeenCalled();
    });

    it("resolves the workspace identity before loading a receipt", async () => {
      vi.mocked(workApi.getWorkRunReceipt).mockResolvedValue({ runId: "run-2" } as any);
      const receipt = await getReceipt({
        scope: workspaceScope("ws-1"),
        conversationRunId: "session-1",
        run: { id: "session-1", work_task_id: "task-1", work_run_id: "run-2" },
      });
      expect(workApi.getWorkRunReceipt).toHaveBeenCalledWith("task-1", "run-2");
      expect(receipt).toEqual({ runId: "run-2" });
    });

    it("returns null when the workspace identity cannot load a receipt", async () => {
      const receipt = await getReceipt({
        scope: workspaceScope("ws-1"),
        conversationRunId: "session-1",
        run: { id: "session-1" },
      });
      expect(receipt).toBeNull();
      expect(workApi.getWorkRunReceipt).not.toHaveBeenCalled();
    });
  });
});
