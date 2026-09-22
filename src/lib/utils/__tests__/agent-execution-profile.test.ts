import { describe, expect, it } from "vitest";
import { getAgentCapabilities } from "../agent-capabilities";

describe("agent execution profiles", () => {
  it("keeps Claude live-session execution semantics", () => {
    const caps = getAgentCapabilities("claude");
    expect(caps.execution.busEvents).toBe(true);
    expect(caps.execution.sessionInitEvents).toBe(true);
    expect(caps.execution.snapshots).toBe(true);
    expect(caps.execution.liveAddDir).toBe(true);
    expect(caps.execution.sessionActor).toBe(true);
    expect(caps.execution.preRunExecutionPath).toBe("session_actor");
  });

  it("keeps Codex backend-selected pre-run execution semantics", () => {
    const caps = getAgentCapabilities("codex");
    expect(caps.execution.busEvents).toBe(true);
    expect(caps.execution.sessionInitEvents).toBe(false);
    expect(caps.execution.snapshots).toBe(false);
    expect(caps.execution.liveAddDir).toBe(false);
    expect(caps.execution.sessionActor).toBe(true);
    expect(caps.execution.preRunExecutionPath).toBe("backend");
  });

  it("keeps Pi, Grok, and DSH on the session actor path", () => {
    expect(getAgentCapabilities("pi").execution.preRunExecutionPath).toBe("session_actor");
    expect(getAgentCapabilities("grok").execution.preRunExecutionPath).toBe("session_actor");
    expect(getAgentCapabilities("dsh").execution.preRunExecutionPath).toBe("session_actor");
  });

  it("uses a conservative execution fallback for unknown agents", () => {
    const caps = getAgentCapabilities("future-agent");
    expect(caps.execution.busEvents).toBe(false);
    expect(caps.execution.sessionInitEvents).toBe(false);
    expect(caps.execution.snapshots).toBe(false);
    expect(caps.execution.liveAddDir).toBe(false);
    expect(caps.execution.sessionActor).toBe(false);
    expect(caps.execution.preRunExecutionPath).toBe("pipe_exec");
  });
});
