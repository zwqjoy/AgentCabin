import { describe, expect, it } from "vitest";
import { classifyWorkExecutionFailure } from "./work-execution-failure";

describe("classifyWorkExecutionFailure", () => {
  it.each([
    [
      "sandbox denied",
      { status: "failed", failureKind: "sandbox_denied", exitCode: 1 },
      "沙箱拒绝",
    ],
    [
      "process start",
      { status: "failed", stderr: "Failed to spawn command 'soffice': No such file" },
      "进程启动失败",
    ],
    ["non-zero exit", { status: "failed", exitCode: 7, stderr: "boom" }, "非零退出"],
    [
      "missing output",
      {
        status: "failed",
        stderr: "Obligation failed: expected output file 'output/a.xlsx' was not created",
      },
      "输出缺失",
    ],
  ])("classifies %s", (_name, output, label) => {
    expect(classifyWorkExecutionFailure("error", output)?.label).toBe(label);
  });

  it("keeps successful empty stdout successful", () => {
    expect(classifyWorkExecutionFailure("success", { status: "success", exitCode: 0 })).toBeNull();
  });
});
