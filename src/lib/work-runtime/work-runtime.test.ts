import { describe, it, expect, vi } from "vitest";
import * as api from "$lib/api";
import {
  getWorkRuntimeClient,
  getWorkRuntimeClientOrReadOnly,
  UnsupportedWorkRuntimeError,
  piWorkRuntimeClient,
  dshWorkRuntimeClient,
  FakeWorkRuntimeClient,
} from "./index";

vi.mock("$lib/api", () => ({
  getAgentSettings: vi.fn().mockResolvedValue({}),
  getUserSettings: vi.fn().mockResolvedValue({}),
  updateAgentSettings: vi.fn().mockResolvedValue({}),
}));

describe("Work Runtime Client Router", () => {
  it("routes 'pi' to piWorkRuntimeClient", () => {
    const client = getWorkRuntimeClient("pi");
    expect(client).toBe(piWorkRuntimeClient);
    expect(client.provider).toBe("pi");
    expect(client.getDisplayName()).toBe("本地 Pi");
    expect(client.capabilities.supportsResume).toBe(true);
  });

  it("defaults to piWorkRuntimeClient when provider is unset or empty", () => {
    expect(getWorkRuntimeClient()).toBe(piWorkRuntimeClient);
    expect(getWorkRuntimeClient("")).toBe(piWorkRuntimeClient);
    expect(getWorkRuntimeClient("   ")).toBe(piWorkRuntimeClient);
  });

  it("routes dsh to the DSH Work runtime client", () => {
    const client = getWorkRuntimeClient("dsh");
    expect(client).toBe(dshWorkRuntimeClient);
    expect(client.provider).toBe("dsh");
    expect(client.getDisplayName()).toBe("DeepSeek Harness (DSH)");
    expect(client.capabilities.supportsResume).toBe(true);
    expect(client.capabilities.supportsSubagents).toBe(false);
  });

  it("throws UnsupportedWorkRuntimeError for claude without fallback to pi", () => {
    expect(() => getWorkRuntimeClient("claude")).toThrow(UnsupportedWorkRuntimeError);
  });

  it("throws UnsupportedWorkRuntimeError for codex without fallback to pi", () => {
    expect(() => getWorkRuntimeClient("codex")).toThrow(UnsupportedWorkRuntimeError);
  });

  it("throws UnsupportedWorkRuntimeError for grok without fallback to pi", () => {
    expect(() => getWorkRuntimeClient("grok")).toThrow(UnsupportedWorkRuntimeError);
  });

  it("throws UnsupportedWorkRuntimeError for unknown providers without fallback to pi", () => {
    expect(() => getWorkRuntimeClient("mystery_agent")).toThrow(UnsupportedWorkRuntimeError);
  });

  it("returns neutral ReadOnlyWorkRuntimeClient for unsupported agents via getWorkRuntimeClientOrReadOnly", () => {
    const client = getWorkRuntimeClientOrReadOnly("claude");
    expect(client).not.toBe(piWorkRuntimeClient);
    expect(client.provider).toBe("claude");
    expect(client.capabilities.supportsResume).toBe(false);
    expect(client.capabilities.supportsContinuation).toBe(false);
    expect(client.capabilities.supportsSubagents).toBe(false);
    expect(client.getModels()).toEqual([]);
    expect(client.getDisplayName()).toBe("Claude Code");
    expect(client.isRuntimeConfirmation("pi_extension_confirm")).toBe(false);
    expect(client.canClone({} as any)).toBe(false);
    expect(client.canOpenSessionTree({} as any)).toBe(false);
  });

  it("supports a provider-neutral Fake Work Runtime readiness fixture", async () => {
    const client = new FakeWorkRuntimeClient({
      displayName: "Fake DSH",
      models: [{ value: "fake-model", displayName: "Fake Model", description: "fixture" }],
      preferences: { model: "fake-model" },
      capabilities: { supportsResume: true, supportsSubagents: true },
    });

    expect(client.provider).toBe("fake");
    expect(client.getDisplayName()).toBe("Fake DSH");
    expect(client.getModels()[0]?.value).toBe("fake-model");
    expect((await client.loadRuntimePreferences()).model).toBe("fake-model");
    await client.persistEffort("high");
    expect((await client.loadRuntimePreferences()).effort).toBe("high");
    expect(client.capabilities.supportsResume).toBe(true);
    expect(client.capabilities.supportsSubagents).toBe(true);
  });

  describe("PiWorkRuntimeClient preferences", () => {
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

      const prefs = await piWorkRuntimeClient.loadRuntimePreferences();
      expect(prefs.model).toBe("Qwen3.8-27B");
    });

    it("falls back to pi_provider.model when no custom binding model exists", async () => {
      vi.mocked(api.getAgentSettings).mockResolvedValue({ agent: "pi" } as never);
      vi.mocked(api.getUserSettings).mockResolvedValue({
        default_agent: "pi",
        allowed_tools: [],
        provider_mode: "local",
        auth_mode: "cli",
        permission_mode: "ask",
        keybinding_overrides: [],
        onboarding_completed: true,
        pi_provider: {
          id: "pi-legacy",
          base_url: "https://api.example.com",
          model: "legacy-pi-model",
        },
      } as never);

      const prefs = await piWorkRuntimeClient.loadRuntimePreferences();
      expect(prefs.model).toBe("legacy-pi-model");
    });

    it("falls back to agentSettings.model when provider settings have no model", async () => {
      vi.mocked(api.getAgentSettings).mockResolvedValue({
        agent: "pi",
        model: "cli-saved-model",
        effort: "high",
      } as never);
      vi.mocked(api.getUserSettings).mockResolvedValue({
        default_agent: "pi",
        allowed_tools: [],
        provider_mode: "local",
        auth_mode: "cli",
        permission_mode: "ask",
        keybinding_overrides: [],
        onboarding_completed: true,
      } as never);

      const prefs = await piWorkRuntimeClient.loadRuntimePreferences();
      expect(prefs.model).toBe("cli-saved-model");
      expect(prefs.effort).toBe("high");
    });

    it("falls back to userSettings.default_model when no agent-specific model is set", async () => {
      vi.mocked(api.getAgentSettings).mockResolvedValue({ agent: "pi" } as never);
      vi.mocked(api.getUserSettings).mockResolvedValue({
        default_agent: "pi",
        default_model: "fallback-default-model",
        allowed_tools: [],
        provider_mode: "local",
        auth_mode: "cli",
        permission_mode: "ask",
        keybinding_overrides: [],
        onboarding_completed: true,
      } as never);

      const prefs = await piWorkRuntimeClient.loadRuntimePreferences();
      expect(prefs.model).toBe("fallback-default-model");
    });
  });
});
