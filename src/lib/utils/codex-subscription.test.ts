import { describe, expect, it } from "vitest";
import type { CodexModelList } from "$lib/types";
import {
  buildCodexSubscriptionProvider,
  CODEX_SUBSCRIPTION_PROVIDER_ID,
} from "./codex-subscription";

describe("Codex subscription provider", () => {
  it("materializes the subscription catalog without copying credentials", () => {
    const catalog: CodexModelList = {
      defaultModel: "gpt-5.6-sol",
      models: [
        {
          value: "gpt-5.6-sol",
          displayName: "GPT-5.6 Sol",
          description: "",
          supportsEffort: true,
          supportedEffortLevels: ["low", "medium", "high", "xhigh"],
          contextWindow: 200000,
        },
      ],
    };

    const provider = buildCodexSubscriptionProvider(catalog);

    expect(provider.id).toBe(CODEX_SUBSCRIPTION_PROVIDER_ID);
    expect(provider.protocol).toBe("openai-responses");
    expect(provider.keyless).toBe(true);
    expect(provider.api_key).toBeUndefined();
    expect(provider.base_url).toContain("chatgpt.com/backend-api/codex");
    expect(provider.models?.[0]).toMatchObject({
      id: "gpt-5.6-sol",
      context_window: 200000,
      supports_reasoning: true,
      supports_xhigh: true,
    });
  });
});
