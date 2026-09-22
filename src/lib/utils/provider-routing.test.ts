import { describe, expect, it } from "vitest";
import {
  filterCustomProviderModels,
  isProviderCompatible,
  resolveManagedProviderDefaultModel,
} from "./provider-routing";

describe("third-party Codex provider routing", () => {
  it("accepts Chat Completions providers when saving a Codex binding", () => {
    expect(isProviderCompatible("codex", "openai-completions")).toBe(true);
  });

  it("does not leak the current native Codex model into a custom provider catalog", () => {
    const models = [
      { value: "DeepSeek-V4-Flash" },
      { value: "glm-5.2" },
      { value: "GPT-5.6-Luna" },
    ];
    expect(filterCustomProviderModels(models, ["DeepSeek-V4-Flash", "glm-5.2"])).toEqual([
      { value: "DeepSeek-V4-Flash" },
      { value: "glm-5.2" },
    ]);
  });

  it("uses the explicit binding model when it is included in configured models", () => {
    expect(
      resolveManagedProviderDefaultModel(
        {
          mode: "custom",
          models: ["glm-5.2", "Qwen3.8-27B", "ZHIPU/GLM-5.2"],
          model: "Qwen3.8-27B",
        },
        ["deepseek/deepseek-v4-flash", "glm-5.2", "Qwen3.8-27B", "ZHIPU/GLM-5.2"],
      ),
    ).toBe("Qwen3.8-27B");
  });

  it("falls back to the first configured model if explicit default is not in configured list", () => {
    expect(
      resolveManagedProviderDefaultModel(
        {
          mode: "custom",
          models: ["glm-5.2", "ZHIPU/GLM-5.2"],
          model: "Qwen3.8-27B",
        },
        ["deepseek/deepseek-v4-flash", "glm-5.2", "Qwen3.8-27B", "ZHIPU/GLM-5.2"],
      ),
    ).toBe("glm-5.2");
  });

  it("checks compatibility correctly for claude and grok", () => {
    expect(isProviderCompatible("claude", "anthropic-messages")).toBe(true);
    expect(isProviderCompatible("claude", "openai-responses")).toBe(false);
    expect(isProviderCompatible("grok", "openai-completions")).toBe(true);
    expect(isProviderCompatible("grok", "openai-responses")).toBe(true);
  });

  it("checks compatibility correctly for dsh", () => {
    expect(isProviderCompatible("dsh", "openai-completions")).toBe(true);
    expect(isProviderCompatible("dsh", "openai-responses")).toBe(true);
    expect(isProviderCompatible("dsh", "anthropic-messages")).toBe(true);
    expect(isProviderCompatible("dsh", "unsupported-protocol")).toBe(false);
  });
});
