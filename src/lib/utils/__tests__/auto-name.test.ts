import { describe, it, expect } from "vitest";
import {
  deriveAutoName,
  derivePiShellTitle,
  shouldAutoName,
  type AutoNameState,
} from "../auto-name";

// ── deriveAutoName ──

describe("deriveAutoName", () => {
  it("returns first line trimmed", () => {
    expect(deriveAutoName("Hello world\nSecond line")).toBe("Hello world");
  });

  it("truncates at 40 chars with ellipsis", () => {
    const long = "A".repeat(50);
    expect(deriveAutoName(long)).toBe("A".repeat(40) + "…");
  });

  it("returns full string when <= 40 chars", () => {
    const short = "A".repeat(40);
    expect(deriveAutoName(short)).toBe(short);
  });

  it("returns empty string for empty prompt", () => {
    expect(deriveAutoName("")).toBe("");
    expect(deriveAutoName("   ")).toBe("");
    expect(deriveAutoName("\n\n")).toBe("");
  });
});

describe("derivePiShellTitle", () => {
  it("names an empty Pi shell from a skill command message", () => {
    expect(
      derivePiShellTitle({
        agent: "pi",
        prompt: "",
        name: undefined,
        message: "/skill:code-review 分析下最新这次提交",
      }),
    ).toBe("/skill:code-review 分析下最新这次提交");
  });

  it("does not overwrite an existing Pi session name or prompt", () => {
    expect(
      derivePiShellTitle({
        agent: "pi",
        prompt: "已有首条消息",
        name: undefined,
        message: "/skill:code-review 继续检查",
      }),
    ).toBe("");
    expect(
      derivePiShellTitle({
        agent: "pi",
        prompt: "",
        name: "自定义名称",
        message: "/skill:code-review 继续检查",
      }),
    ).toBe("");
  });
});

// ── shouldAutoName ──

describe("shouldAutoName", () => {
  const base: AutoNameState = {
    phase: "idle",
    runId: "run-123",
    runName: undefined,
    prompt: "Fix the login bug",
    autoNameDone: false,
  };

  it("fires when all conditions are met", () => {
    const result = shouldAutoName(base);
    expect(result.fire).toBe(true);
    expect(result.autoName).toBe("Fix the login bug");
  });

  it("does not fire when autoNameDone is true", () => {
    expect(shouldAutoName({ ...base, autoNameDone: true }).fire).toBe(false);
  });

  it("does not fire when runName is already set", () => {
    expect(shouldAutoName({ ...base, runName: "My Session" }).fire).toBe(false);
  });

  it("does not fire when phase is not idle", () => {
    expect(shouldAutoName({ ...base, phase: "running" }).fire).toBe(false);
    expect(shouldAutoName({ ...base, phase: "ready" }).fire).toBe(false);
  });

  it("does not fire when runId is undefined", () => {
    expect(shouldAutoName({ ...base, runId: undefined }).fire).toBe(false);
  });

  it("does not fire when prompt is undefined", () => {
    expect(shouldAutoName({ ...base, prompt: undefined }).fire).toBe(false);
  });

  it("does not fire when prompt is empty", () => {
    expect(shouldAutoName({ ...base, prompt: "" }).fire).toBe(false);
  });

  it("truncates long prompts in autoName", () => {
    const long = "A".repeat(50) + "\nline2";
    const result = shouldAutoName({ ...base, prompt: long });
    expect(result.fire).toBe(true);
    expect(result.autoName).toBe("A".repeat(40) + "…");
  });

  // Regression: sidebar rename sets name externally. If the runs-changed
  // listener sets autoNameDone=true, shouldAutoName must not fire even
  // if store.run.name hasn't propagated yet.
  it("does not fire after external rename sets autoNameDone", () => {
    // Simulate: sidebar renamed → listener set autoNameDone, but name not yet synced
    expect(shouldAutoName({ ...base, runName: undefined, autoNameDone: true }).fire).toBe(false);
  });
});
