import { describe, expect, it } from "vitest";
import {
  detectWorkPlanConfirmation,
  parseWorkPlanDecision,
} from "$lib/utils/work-plan-confirmation";

const confirmation = `请确认：是否同意以上修复计划？
1. 确认执行
2. 修改计划
3. 取消`;

describe("Work text plan confirmation fallback", () => {
  it("recognizes a model-written confirmation prompt", () => {
    expect(detectWorkPlanConfirmation(confirmation)).toBe(true);
  });

  it("maps numeric replies to plan decisions", () => {
    expect(parseWorkPlanDecision("1")).toBe("approve");
    expect(parseWorkPlanDecision("2")).toBe("revise");
    expect(parseWorkPlanDecision("3")).toBe("cancel");
  });
});
