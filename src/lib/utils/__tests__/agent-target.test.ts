import { describe, expect, it } from "vitest";
import {
  getAgentTarget,
  getRunRoute,
  getRunAppMode,
  getRunRuntime,
  isWorkRun,
  isNativeTarget,
  isPiTarget,
  isWorkTarget,
  getTargetRoute,
  getConversationDeleteFallbackRoute,
  getTargetBadgeName,
} from "../agent-target";

describe("agent-target utils", () => {
  it("resolves target directly when explicit agent_target is present", () => {
    expect(getAgentTarget({ agent_target: "native:codex" })).toBe("native:codex");
    expect(getAgentTarget({ agent_target: "pi:code" })).toBe("pi:code");
    expect(getAgentTarget({ agent_target: "pi:work" })).toBe("work");
  });

  it("resolves target from legacy app_mode and agent fields", () => {
    expect(getAgentTarget({ app_mode: "work", agent: "pi" })).toBe("work");
    expect(getAgentTarget({ app_mode: "code", agent: "pi" })).toBe("pi:code");
    expect(getAgentTarget({ app_mode: "code", agent: "codex" })).toBe("native:codex");
    expect(getAgentTarget({ app_mode: "code", agent: "grok" })).toBe("native:grok");
    expect(getAgentTarget({ app_mode: "code", agent: "claude" })).toBe("native:claude");
    expect(getAgentTarget({})).toBe("native:claude");
  });

  it("checks native vs pi realms", () => {
    expect(isNativeTarget("native:codex")).toBe(true);
    expect(isNativeTarget("native:claude")).toBe(true);
    expect(isNativeTarget("native:grok")).toBe(true);
    expect(isNativeTarget("pi:code")).toBe(false);
    expect(isNativeTarget("pi:work")).toBe(false);

    expect(isPiTarget("pi:code")).toBe(true);
    expect(isPiTarget("pi:work")).toBe(false);
    expect(isPiTarget("work")).toBe(false);
    expect(isWorkTarget("pi:work")).toBe(true);
    expect(isWorkTarget("work")).toBe(true);
    expect(isPiTarget("native:codex")).toBe(false);
  });

  it("uses app_mode as the Work identity independently of the runtime", () => {
    expect(isWorkRun({ app_mode: "work", agent: "dsh" })).toBe(true);
    expect(isWorkRun({ app_mode: "work", agent: "claude", agent_target: "native:claude" })).toBe(
      true,
    );
    expect(isWorkRun({ app_mode: "code", agent: "dsh", agent_target: "pi:work" })).toBe(false);
    expect(isWorkRun({ agent_target: "pi:work" })).toBe(true);
  });

  it("builds correct target routes", () => {
    expect(getTargetRoute("pi:code", { runId: "r-1" })).toBe("/chat/pi?run=r-1");
    expect(getTargetRoute("pi:work", { workspaceId: "ws-1" })).toBe("/chat/work?workspace=ws-1");
    expect(getTargetRoute("native:codex", { runId: "r-2" })).toBe("/chat?run=r-2");
    expect(getTargetRoute("native:codex")).toBe("/chat?agent=codex");
    expect(getTargetRoute("native:grok")).toBe("/chat?agent=grok");
    expect(getTargetRoute("native:claude")).toBe("/chat");
  });

  it("routes Work runs by mode while retaining the runtime in the run", () => {
    expect(
      getRunRoute({ app_mode: "work", agent: "dsh", workspace_id: "ws-1" }, { runId: "r-1" }),
    ).toBe("/chat/work?run=r-1&workspace=ws-1");
  });

  it("keeps Pi Code after deleting the active conversation", () => {
    expect(getConversationDeleteFallbackRoute("pi:code")).toBe("/chat/pi");
  });

  it("resolves run product mode and runtime provider independently", () => {
    expect(getRunAppMode({ app_mode: "work", agent: "dsh" })).toBe("work");
    expect(getRunAppMode({ app_mode: "code", agent: "codex" })).toBe("code");
    expect(getRunAppMode({ agent_target: "pi:work" })).toBe("work");

    expect(getRunRuntime({ agent: "dsh" })).toBe("dsh");
    expect(getRunRuntime({ agent: "codex" })).toBe("codex");
    expect(getRunRuntime({ agent_target: "native:claude" })).toBe("claude");
    expect(getRunRuntime({ agent_target: "pi:work" })).toBe("pi");
  });

  it("provides human-readable badge names", () => {
    expect(getTargetBadgeName("native:codex")).toBe("Codex");
    expect(getTargetBadgeName("native:claude")).toBe("Claude Code");
    expect(getTargetBadgeName("native:grok")).toBe("Grok");
    expect(getTargetBadgeName("pi:code")).toBe("Pi Code");
    expect(getTargetBadgeName("work")).toBe("Work");
  });
});
