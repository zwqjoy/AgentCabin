import { describe, expect, it } from "vitest";
import {
  computeGoalProgress,
  formatCriterionStatus,
  formatGoalStatus,
  formatVerifierType,
} from "../work-goal";
import type { AcceptanceCriterion } from "$lib/types/work";

describe("work-goal utility", () => {
  it("computes goal progress correctly with mixed criteria", () => {
    const criteria: AcceptanceCriterion[] = [
      {
        id: "c1",
        description: "生成分析表",
        verifierType: "artifact",
        status: "passed",
        evidenceRefs: ["artifact:123"],
      },
      {
        id: "c2",
        description: "生成 PPT",
        verifierType: "artifact",
        status: "passed",
        evidenceRefs: ["artifact:456"],
      },
      {
        id: "c3",
        description: "数据一致性检查",
        verifierType: "structured",
        status: "failed",
        evidenceRefs: [],
        failureReason: "数据差额 2.5%",
      },
      {
        id: "c4",
        description: "官网来源核验",
        verifierType: "composite",
        status: "insufficient_evidence",
        evidenceRefs: [],
      },
      {
        id: "c5",
        description: "管理层总结",
        verifierType: "machine",
        status: "pending",
        evidenceRefs: [],
      },
    ];

    const progress = computeGoalProgress(criteria);
    expect(progress.total).toBe(5);
    expect(progress.passed).toBe(2);
    expect(progress.failed).toBe(1);
    expect(progress.insufficient).toBe(1);
    expect(progress.pending).toBe(1);
    expect(progress.percent).toBe(40); // 2 / 5 = 40%
  });

  it("handles empty criteria list cleanly", () => {
    const progress = computeGoalProgress([]);
    expect(progress.total).toBe(0);
    expect(progress.percent).toBe(100);
  });

  it("formats goal statuses properly", () => {
    expect(formatGoalStatus("passed").label).toBe("目标达成");
    expect(formatGoalStatus("failed").label).toBe("验收未通过");
    expect(formatGoalStatus("checking").label).toBe("系统验收中");
    expect(formatGoalStatus("insufficient_evidence").label).toBe("证据不足");
    expect(formatGoalStatus("pending").label).toBe("进行中");
    expect(formatGoalStatus("not_applicable").label).toBe("机器验收不适用");
  });

  it("formats criterion statuses properly", () => {
    expect(formatCriterionStatus("passed").icon).toBe("✓");
    expect(formatCriterionStatus("failed").icon).toBe("✕");
    expect(formatCriterionStatus("insufficient_evidence").icon).toBe("△");
    expect(formatCriterionStatus("checking").icon).toBe("◷");
    expect(formatCriterionStatus("pending").icon).toBe("○");
  });

  it("formats verifier types properly", () => {
    expect(formatVerifierType("artifact").label).toBe("成果校验");
    expect(formatVerifierType("file").label).toBe("文件校验");
    expect(formatVerifierType("structured").label).toBe("结构化校验");
    expect(formatVerifierType("machine").label).toBe("机器确定性");
    expect(formatVerifierType("composite").label).toBe("来源核验");
  });
});
