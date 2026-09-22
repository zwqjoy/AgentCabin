import { describe, expect, it } from "vitest";
import type {
  WorkArtifactCheckStatus,
  WorkArtifactRequirement,
  WorkArtifactSummary,
  WorkTask,
} from "$lib/types/work";

describe("work task center & acceptance criteria logic", () => {
  function getTaskRequiredArtifactsCount(task: Partial<WorkTask>): number {
    const paths = new Set<string>();
    for (const path of task.requiredArtifacts ?? []) {
      if (path) paths.add(path);
    }
    for (const req of task.artifactRequirements ?? []) {
      if (req.path) paths.add(req.path);
    }
    return paths.size;
  }

  function isTaskScheduled(task: Partial<WorkTask>): boolean {
    return task.status === "scheduled" || Boolean(task.schedule?.enabled);
  }

  function synthesizeCriteria(
    artifactRequirements: WorkArtifactRequirement[] = [],
    requiredArtifacts: string[] = [],
    artifacts: WorkArtifactSummary[] = [],
  ) {
    const seenPaths = new Set<string>();
    const allRequirements: WorkArtifactRequirement[] = [];

    for (const req of artifactRequirements) {
      if (!seenPaths.has(req.path)) {
        seenPaths.add(req.path);
        allRequirements.push({
          path: req.path,
          title: req.title || req.path,
          artifactType: req.artifactType,
          required: req.required !== false,
        });
      }
    }

    for (const path of requiredArtifacts) {
      if (path && !seenPaths.has(path)) {
        seenPaths.add(path);
        allRequirements.push({
          path,
          title: path,
          artifactType: null,
          required: true,
        });
      }
    }

    const criteria = allRequirements.map((r) => {
      const matchingArtifact = artifacts.find(
        (a) => a.path.endsWith(r.path) || a.title === r.title || a.path === r.path,
      );
      let status: WorkArtifactCheckStatus = "missing";
      let message = "等待生成";
      if (matchingArtifact) {
        if (matchingArtifact.status === "delivered") {
          status = "satisfied";
          message = "已生成并通过交付验收";
        } else if (matchingArtifact.status === "validated") {
          status = "missing";
          message = "文件格式检查通过，等待最终交付";
        } else if (matchingArtifact.status === "invalid" || matchingArtifact.status === "failed") {
          status = "invalid";
          message = "文件格式检查未通过";
        } else {
          status = "missing";
          message = "生成中 / 待检查";
        }
      }
      return {
        path: r.path,
        title: r.title || r.path,
        required: r.required !== false,
        status,
        message,
      };
    });

    const requiredCriteria = criteria.filter((c) => c.required);
    const satisfiedRequired = requiredCriteria.filter((c) => c.status === "satisfied").length;
    const satisfiedTotal = criteria.filter((c) => c.status === "satisfied").length;

    return {
      criteria,
      requiredCriteria,
      satisfiedRequired,
      satisfiedTotal,
      isFullySatisfied:
        requiredCriteria.length > 0
          ? satisfiedRequired === requiredCriteria.length
          : satisfiedTotal === criteria.length,
    };
  }

  describe("Task required artifacts deduplication", () => {
    it("deduplicates identical paths in requiredArtifacts and artifactRequirements", () => {
      const task: Partial<WorkTask> = {
        requiredArtifacts: ["output/report.pdf", "output/data.csv"],
        artifactRequirements: [
          { path: "output/report.pdf", title: "PDF Report", required: true },
          { path: "output/summary.md", title: "Markdown Summary", required: true },
        ],
      };
      expect(getTaskRequiredArtifactsCount(task)).toBe(3);
    });

    it("handles tasks with only legacy requiredArtifacts", () => {
      const task: Partial<WorkTask> = {
        requiredArtifacts: ["output/file1.docx", "output/file2.xlsx"],
      };
      expect(getTaskRequiredArtifactsCount(task)).toBe(2);
    });
  });

  describe("Scheduled task boundary alignment", () => {
    it("identifies scheduled tasks by schedule.enabled or status === 'scheduled'", () => {
      expect(
        isTaskScheduled({
          status: "active",
          schedule: {
            enabled: true,
            cronExpression: "0 9 * * *",
            kind: "daily",
            timeOfDay: "09:00",
            dayOfWeek: null,
            timezone: "Asia/Shanghai",
            nextRunAt: null,
            lastRunAt: null,
          },
        }),
      ).toBe(true);

      expect(isTaskScheduled({ status: "scheduled", schedule: null })).toBe(true);
      expect(isTaskScheduled({ status: "active", schedule: { enabled: false } as any })).toBe(
        false,
      );
      expect(isTaskScheduled({ status: "completed", schedule: null })).toBe(false);
    });
  });

  describe("Goal → Acceptance → Delivery synthesis", () => {
    it("merges legacy requiredArtifacts and artifactRequirements properly", () => {
      const result = synthesizeCriteria(
        [{ path: "output/final.pdf", title: "PDF", required: true }],
        ["output/legacy.txt"],
        [],
      );
      expect(result.criteria).toHaveLength(2);
      expect(result.criteria.map((c) => c.path)).toEqual(["output/final.pdf", "output/legacy.txt"]);
    });

    it("marks only delivered artifacts as satisfied, not validated", () => {
      const artifacts: WorkArtifactSummary[] = [
        {
          id: "a1",
          workspaceId: "w1",
          artifactType: "pdf",
          category: "pdf",
          mimeType: "application/pdf",
          previewKind: "pdf",
          canPreview: true,
          version: 1,
          title: "PDF",
          path: "output/final.pdf",
          status: "validated",
          size: 1024,
          createdAt: "",
          updatedAt: "",
        },
      ];

      const result = synthesizeCriteria(
        [{ path: "output/final.pdf", title: "PDF", required: true }],
        [],
        artifacts,
      );

      expect(result.criteria[0].status).toBe("missing");
      expect(result.satisfiedRequired).toBe(0);
      expect(result.isFullySatisfied).toBe(false);
    });

    it("marks delivered artifacts as satisfied", () => {
      const artifacts: WorkArtifactSummary[] = [
        {
          id: "a1",
          workspaceId: "w1",
          artifactType: "pdf",
          category: "pdf",
          mimeType: "application/pdf",
          previewKind: "pdf",
          canPreview: true,
          version: 1,
          title: "PDF",
          path: "output/final.pdf",
          status: "delivered",
          size: 1024,
          createdAt: "",
          updatedAt: "",
        },
      ];

      const result = synthesizeCriteria(
        [{ path: "output/final.pdf", title: "PDF", required: true }],
        [],
        artifacts,
      );

      expect(result.criteria[0].status).toBe("satisfied");
      expect(result.satisfiedRequired).toBe(1);
      expect(result.isFullySatisfied).toBe(true);
    });

    it("does not block satisfaction if an optional deliverable is missing", () => {
      const artifacts: WorkArtifactSummary[] = [
        {
          id: "a1",
          workspaceId: "w1",
          artifactType: "pdf",
          category: "pdf",
          mimeType: "application/pdf",
          previewKind: "pdf",
          canPreview: true,
          version: 1,
          title: "PDF",
          path: "output/final.pdf",
          status: "delivered",
          size: 1024,
          createdAt: "",
          updatedAt: "",
        },
      ];

      const result = synthesizeCriteria(
        [
          { path: "output/final.pdf", title: "PDF", required: true },
          { path: "output/optional.csv", title: "CSV Data", required: false },
        ],
        [],
        artifacts,
      );

      expect(result.requiredCriteria).toHaveLength(1);
      expect(result.satisfiedRequired).toBe(1);
      expect(result.satisfiedTotal).toBe(1);
      expect(result.isFullySatisfied).toBe(true);
    });
  });
});
