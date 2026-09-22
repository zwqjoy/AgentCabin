import { describe, expect, it } from "vitest";
import type { AgentCapabilities } from "$lib/types";
import {
  getAgentCapabilities,
  getDefaultAgentCapabilities,
  getAgentUiCapabilities,
  hasAgentCapabilityProvider,
} from "../agent-capabilities";

describe("agent capability providers", () => {
  it("provides static baselines for all built-in agents", () => {
    expect(hasAgentCapabilityProvider("claude")).toBe(true);
    expect(hasAgentCapabilityProvider("codex")).toBe(true);
    expect(hasAgentCapabilityProvider("pi")).toBe(true);
    expect(hasAgentCapabilityProvider("grok")).toBe(true);
    expect(hasAgentCapabilityProvider("dsh")).toBe(true);

    expect(getAgentUiCapabilities("claude").addDirAction).toBe(true);
    expect(getAgentUiCapabilities("codex").permissionModeSwitch).toBe(true);
    expect(getAgentUiCapabilities("pi").addDirAction).toBe(false);
    expect(getAgentUiCapabilities("dsh").permissionModeSwitch).toBe(true);
    expect(getAgentCapabilities("dsh").execution.preRunExecutionPath).toBe("session_actor");
    expect(getAgentUiCapabilities("grok").planModeToggle).toBe(true);
    expect(getAgentUiCapabilities("grok").permissionModeSwitch).toBe(true);
    expect(getAgentUiCapabilities("grok").goalPanel).toBe(true);
    expect(getAgentCapabilities("claude").runtime.attachments).toBe(true);
    expect(getAgentCapabilities("claude").runtime.remote).toBe(true);
    expect(getAgentCapabilities("codex").runtime.remote).toBe(false);
    expect(getAgentCapabilities("pi").runtime.fork).toBe(true);
    expect(getAgentCapabilities("grok").runtime.steer).toBe(false);
    expect(getAgentCapabilities("grok").protocol.effortControl).toBe(true);
    expect(getAgentCapabilities("grok").ui.effortSelector).toBe(true);
    expect(getAgentCapabilities("claude").protocol.goalState).toBe(true);
    expect(getAgentCapabilities("grok").protocol.goalState).toBe(false);
  });

  it("keeps permission requests distinct from host permission-mode control", () => {
    expect(getAgentCapabilities("claude").protocol.permissionRequest).toBe(true);
    expect(getAgentCapabilities("claude").protocol.permissionModeControl).toBe(true);
    expect(getAgentCapabilities("codex").protocol.permissionRequest).toBe(false);
    expect(getAgentCapabilities("codex").protocol.permissionModeControl).toBe(true);
    expect(getAgentCapabilities("pi").protocol.permissionRequest).toBe(true);
    expect(getAgentCapabilities("pi").protocol.permissionModeControl).toBe(true);
    expect(getAgentCapabilities("grok").protocol.permissionRequest).toBe(true);
    expect(getAgentCapabilities("grok").protocol.permissionModeControl).toBe(false);
    expect(getAgentCapabilities("grok").protocol.sessionModeControl).toBe(false);
    expect(getAgentCapabilities("dsh").protocol.permissionRequest).toBe(true);
    expect(getAgentCapabilities("dsh").protocol.permissionModeControl).toBe(true);
  });

  it("lets negotiated session capabilities override the static baseline", () => {
    const negotiated: AgentCapabilities = {
      protocol: {
        sessionLoad: true,
        sessionSetModel: true,
        sessionModeControl: true,
        permissionRequest: true,
        permissionModeControl: false,
        slashCommands: true,
        planMode: true,
        effortControl: false,
        goalState: false,
        structuredTaskState: false,
      },
      runtime: {
        attachments: true,
        remote: true,
        fork: true,
        steer: true,
        followUp: true,
      },
      ui: {
        slashCommandMenu: true,
        planModeToggle: true,
        goalPanel: true,
        effortSelector: false,
        permissionModeSwitch: false,
        addDirAction: false,
      },
    };

    const effective = getAgentCapabilities("grok", negotiated);
    expect(effective.protocol.sessionLoad).toBe(true);
    expect(effective.protocol.slashCommands).toBe(true);
    expect(effective.ui.slashCommandMenu).toBe(true);
    expect(effective.ui.planModeToggle).toBe(true);
    expect(effective.runtime.attachments).toBe(true);
    expect(effective.runtime.remote).toBe(true);
    expect(effective.runtime.fork).toBe(true);
    expect(effective.runtime.steer).toBe(true);
    expect(effective.execution.preRunExecutionPath).toBe("session_actor");
    expect(effective.execution.busEvents).toBe(true);

    // Provider constants stay immutable after resolving a live session.
    expect(getDefaultAgentCapabilities("grok").ui.planModeToggle).toBe(true);
  });

  it("uses a conservative protocol/runtime fallback for unknown agents", () => {
    const capabilities = getAgentCapabilities("future-agent");
    expect(hasAgentCapabilityProvider("future-agent")).toBe(false);
    expect(capabilities.protocol.sessionLoad).toBe(false);
    expect(capabilities.protocol.permissionModeControl).toBe(false);
    expect(capabilities.runtime.remote).toBe(false);
    expect(capabilities.execution.busEvents).toBe(false);
    expect(capabilities.execution.preRunExecutionPath).toBe("pipe_exec");
    expect(capabilities.ui.slashCommandMenu).toBe(false);
  });
});
