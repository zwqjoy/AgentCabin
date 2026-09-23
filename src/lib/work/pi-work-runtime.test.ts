import { describe, it, expect, vi } from "vitest";
import * as api from "$lib/api";
import {
  canCloneWorkSession,
  canOpenWorkSessionTree,
  getWorkAgentDisplayName,
  getWorkComposerCapabilities,
  getWorkModels,
  getWorkSlashCommands,
  getWorkStartingLabel,
  isLegacyWorkRun,
  isMissingPiWorkSessionError,
  isPiWorkRuntimeConfirmation,
  isWorkDiagnosticTimelineEntry,
  isWorkRunReadOnly,
  isWorkSubagentActivityEvent,
  loadWorkModels,
  loadWorkPreferences,
  normalizeWorkQueuedText,
  persistWorkEffort,
} from "./pi-work-runtime";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import type { TaskRun, TimelineEntry } from "$lib/types";

vi.mock("$lib/api", () => ({
  getAgentSettings: vi.fn().mockResolvedValue({}),
  getUserSettings: vi.fn().mockResolvedValue({}),
  updateAgentSettings: vi.fn().mockResolvedValue({}),
}));

describe("Pi Work Runtime Helpers", () => {
  it("identifies legacy non-Pi Work runs as read-only", () => {
    const dshRun: TaskRun = { id: "run-1", agent: "dsh" } as never;
    const claudeRun: TaskRun = { id: "run-2", agent: "claude" } as never;
    const piRun: TaskRun = { id: "run-3", agent: "pi" } as never;

    expect(isLegacyWorkRun(dshRun)).toBe(true);
    expect(isWorkRunReadOnly(dshRun)).toBe(true);
    expect(isLegacyWorkRun(claudeRun)).toBe(true);
    expect(isWorkRunReadOnly(claudeRun)).toBe(true);

    expect(isLegacyWorkRun(piRun)).toBe(false);
    expect(isWorkRunReadOnly(piRun)).toBe(false);
    expect(isLegacyWorkRun(null)).toBe(false);
  });

  it("returns human readable display names for agents", () => {
    expect(getWorkAgentDisplayName("pi")).toBe("本地 Pi");
    expect(getWorkAgentDisplayName("dsh")).toBe("DeepSeek Harness");
    expect(getWorkAgentDisplayName("claude")).toBe("Claude Code");
    expect(getWorkAgentDisplayName(undefined)).toBe("本地 Pi");
  });

  it("returns cold start starting label and confirmation check", () => {
    expect(getWorkStartingLabel()).toBe("正在启动 Pi");
    expect(isPiWorkRuntimeConfirmation("pi_extension_confirm")).toBe(true);
    expect(isPiWorkRuntimeConfirmation("other")).toBe(false);
  });

  it("detects missing session errors", () => {
    expect(isMissingPiWorkSessionError(new Error("no session found matching id"))).toBe(true);
    expect(isMissingPiWorkSessionError(new Error("pi session required"))).toBe(true);
    expect(isMissingPiWorkSessionError(new Error("unrelated error"))).toBe(false);
  });

  it("filters diagnostic timeline entries", () => {
    const diagEntry: TimelineEntry = {
      kind: "command_output",
      id: "1",
      anchorId: "1",
      content: "[pi rpc stderr] debug log",
      ts: "2026-01-01T00:00:00Z",
    };
    const userEntry: TimelineEntry = {
      kind: "user",
      id: "2",
      anchorId: "2",
      content: "hello",
      ts: "2026-01-01T00:00:01Z",
    };

    expect(isWorkDiagnosticTimelineEntry(diagEntry)).toBe(true);
    expect(isWorkDiagnosticTimelineEntry(userEntry)).toBe(false);
  });

  it("identifies subagent activity events", () => {
    expect(isWorkSubagentActivityEvent({ type: "tool_start" } as never)).toBe(true);
    expect(isWorkSubagentActivityEvent({ type: "run_state" } as never)).toBe(true);
    expect(isWorkSubagentActivityEvent({ type: "text_delta" } as never)).toBe(false);
  });

  it("evaluates clone and session tree availability", () => {
    const mockSession = {
      agent: "pi",
      run: { id: "run-pi", agent: "pi" },
      piCapabilities: { cloneAvailable: true, sessionTreeAvailable: true },
    } as unknown as SessionStore;

    expect(canCloneWorkSession(mockSession)).toBe(true);
    expect(canOpenWorkSessionTree(mockSession)).toBe(true);

    const legacySession = {
      agent: "dsh",
      run: { id: "run-dsh", agent: "dsh" },
      piCapabilities: { cloneAvailable: true, sessionTreeAvailable: true },
    } as unknown as SessionStore;

    expect(canCloneWorkSession(legacySession)).toBe(false);
    expect(canOpenWorkSessionTree(legacySession)).toBe(false);
  });

  describe("Composer capabilities projection", () => {
    it("returns read-only baseline for legacy runs", () => {
      const mockSession = {
        agent: "dsh",
        run: { id: "run-dsh", agent: "dsh" },
      } as unknown as SessionStore;

      const caps = getWorkComposerCapabilities(mockSession);
      expect(caps.runtime.steer).toBe(false);
      expect(caps.runtime.followUp).toBe(false);
      expect(caps.runtime.fork).toBe(false);
    });

    it("projects negotiated capabilities for Pi runs", () => {
      const mockSession = {
        agent: "pi",
        run: { id: "run-pi", agent: "pi" },
        capabilities: {
          runtime: { fork: true, steer: true, followUp: true },
          protocol: {},
          ui: {},
          execution: {},
        },
        piCapabilities: {
          steerAvailable: true,
          followUpAvailable: false,
          forkAvailable: true,
        },
      } as unknown as SessionStore;

      const caps = getWorkComposerCapabilities(mockSession);
      expect(caps.runtime.steer).toBe(true);
      expect(caps.runtime.followUp).toBe(false);
      expect(caps.runtime.fork).toBe(true);
    });
  });

  describe("Pi Work runtime preferences", () => {
    it("resolves managed provider default model from agent_provider_bindings", async () => {
      vi.mocked(api.getAgentSettings).mockResolvedValue({ agent: "pi" } as never);
      vi.mocked(api.getUserSettings).mockResolvedValue({
        default_agent: "pi",
        allowed_tools: [],
        provider_mode: "local",
        auth_mode: "cli",
        permission_mode: "ask",
        keybinding_overrides: [],
        onboarding_completed: true,
        global_providers: [
          {
            id: "gp-1",
            name: "Custom Provider",
            protocol: "openai-responses",
            base_url: "https://api.example.com",
            models: [
              { id: "Deepseek-V4-Flash", name: "Deepseek-V4-Flash" },
              { id: "Qwen3.8-27B", name: "Qwen3.8-27B" },
            ],
          },
        ],
        agent_provider_bindings: {
          claude: { mode: "cli" },
          codex: { mode: "cli" },
          pi: {
            mode: "custom",
            provider_id: "gp-1",
            models: ["Deepseek-V4-Flash", "Qwen3.8-27B"],
            model: "Qwen3.8-27B",
          },
          grok: { mode: "cli" },
        },
      } as never);

      const prefs = await loadWorkPreferences();
      expect(prefs.model).toBe("Qwen3.8-27B");
    });

    it("persists effort using updateAgentSettings", async () => {
      await persistWorkEffort("high");
      expect(api.updateAgentSettings).toHaveBeenCalledWith("pi", { effort: "high" });
    });
  });
});
