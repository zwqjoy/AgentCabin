import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UserSettings } from "$lib/types";

const getGrokStatus = vi.hoisted(() => vi.fn());
const getUserSettings = vi.hoisted(() => vi.fn());

vi.mock("$lib/grok-api", () => ({ getGrokStatus }));
vi.mock("$lib/api", () => ({ getUserSettings }));

import {
  fetchRuntimeProviderStatus,
  resolveManagedProviderStatus,
  runtimeProviderAuthLabel,
} from "./runtime-status";

describe("fetchRuntimeProviderStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("treats a complete AgentCabin-managed Grok binding as chat-ready without CLI auth", async () => {
    getGrokStatus.mockResolvedValue({
      found: true,
      authenticated: false,
      models: [],
    });
    getUserSettings.mockResolvedValue({
      agent_provider_bindings: {
        claude: { mode: "cli" },
        codex: { mode: "cli" },
        pi: { mode: "cli" },
        grok: {
          mode: "custom",
          provider_id: "newapi-openai",
          models: ["deepseek/deepseek-v4-flash"],
          model: "deepseek/deepseek-v4-flash",
        },
      },
      global_providers: [
        {
          id: "newapi-openai",
          name: "NewAPI-OpenAI",
          protocol: "openai-completions",
          base_url: "http://127.0.0.1:3000/v1",
          api_key: "managed-secret",
          models: [{ id: "deepseek/deepseek-v4-flash" }],
        },
      ],
    } as Pick<UserSettings, "agent_provider_bindings" | "global_providers">);

    const status = await fetchRuntimeProviderStatus("grok");

    expect(status).toMatchObject({
      installed: true,
      authenticated: true,
      ready: true,
      authSource: "agentcabin",
    });
    expect(status.reason).toBeUndefined();
    expect(runtimeProviderAuthLabel(status)).toBe("AgentCabin 已配置");
  });

  it("does not fall back to native auth when an explicit managed binding is incomplete", () => {
    const status = resolveManagedProviderStatus("grok", {
      agent_provider_bindings: {
        claude: { mode: "cli" },
        codex: { mode: "cli" },
        pi: { mode: "cli" },
        grok: { mode: "custom", provider_id: "missing-provider" },
      },
      global_providers: [],
    });

    expect(status).toEqual({
      ready: false,
      reason: "找不到 AgentCabin Provider：missing-provider",
    });
  });

  it("keeps an incomplete managed binding unavailable even when native auth exists", async () => {
    getGrokStatus.mockResolvedValue({
      found: true,
      authenticated: true,
      models: [],
    });
    getUserSettings.mockResolvedValue({
      agent_provider_bindings: {
        claude: { mode: "cli" },
        codex: { mode: "cli" },
        pi: { mode: "cli" },
        grok: { mode: "custom", provider_id: "missing-provider" },
      },
      global_providers: [],
    } as Pick<UserSettings, "agent_provider_bindings" | "global_providers">);

    const status = await fetchRuntimeProviderStatus("grok");

    expect(status).toMatchObject({
      installed: true,
      authenticated: false,
      ready: false,
      authSource: "agentcabin",
      reason: "找不到 AgentCabin Provider：missing-provider",
    });
  });
});
