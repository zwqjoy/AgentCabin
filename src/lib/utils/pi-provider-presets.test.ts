import { describe, expect, it } from "vitest";
import type { PiProviderCredential } from "$lib/types";
import {
  getPiModelOptions,
  normalizePiCliModel,
  PI_CODEX_DEFAULT_MODEL,
  PI_THINKING_LEVELS,
  resolvePiStartupModel,
} from "./pi-provider-presets";
import { filterPiThinkingLevelsForUi } from "./pi-thinking";

const effortCapabilities = {
  supportsEffort: true,
  supportedEffortLevels: PI_THINKING_LEVELS,
};

describe("getPiModelOptions", () => {
  it("uses the managed Pi model instead of a stale Claude platform model", () => {
    const provider: PiProviderCredential = {
      id: "custom-openai",
      name: "Custom OpenAI-compatible",
      base_url: "http://127.0.0.1:3000/v1",
      api: "openai-completions",
      model: "Qwen3.6-27B",
    };

    expect(getPiModelOptions(provider, "qwen3.5-plus")).toEqual([
      {
        value: "Qwen3.6-27B",
        displayName: "Qwen3.6-27B",
        description: "Default",
        supportsEffort: true,
        supportedEffortLevels: ["off", "low", "medium", "high"],
      },
    ]);
  });

  it("uses the RPC model when Pi CLI configuration is active", () => {
    expect(getPiModelOptions(undefined, "local-pi-model")).toEqual([
      {
        value: "local-pi-model",
        displayName: "local-pi-model",
        description: "Default",
        ...effortCapabilities,
      },
    ]);
  });

  it("keeps every configured managed-provider model available", () => {
    const provider: PiProviderCredential = {
      id: "custom-openai",
      name: "Custom OpenAI-compatible",
      base_url: "http://127.0.0.1:3000/v1",
      api: "openai-completions",
      model: "glm-5.2",
      models: [{ id: "glm-5.2" }, { id: "Qwen3.6-27B" }],
    };

    expect(getPiModelOptions(provider, "glm-5.2").map((item) => item.value)).toEqual([
      "glm-5.2",
      "Qwen3.6-27B",
    ]);
  });

  it("advertises the Pi thinking levels to the chat status selector", () => {
    const [model] = getPiModelOptions(undefined, "model-a");
    expect(model.supportsEffort).toBe(true);
    expect(model.supportedEffortLevels).toEqual(["off", "low", "medium", "high", "xhigh", "max"]);
  });

  it("advertises xhigh-style levels only for a managed model that supports xhigh", () => {
    const [model] = getPiModelOptions(
      {
        id: "custom-openai",
        name: "Custom OpenAI-compatible",
        base_url: "http://127.0.0.1:3000/v1",
        api: "openai-completions",
        model: "Qwen3.8-27B",
        models: [{ id: "Qwen3.8-27B", supports_reasoning: true, supports_xhigh: true }],
      },
      "Qwen3.8-27B",
    );

    expect(model.supportedEffortLevels).toEqual(["off", "low", "medium", "high", "xhigh"]);
  });

  it("uses explicitly configured model effort levels when present", () => {
    const [model] = getPiModelOptions(
      {
        id: "custom-openai",
        name: "Custom OpenAI-compatible",
        base_url: "http://127.0.0.1:3000/v1",
        api: "openai-completions",
        model: "Qwen3.8-27B",
        models: [
          {
            id: "Qwen3.8-27B",
            supports_reasoning: true,
            supported_effort_levels: ["low", "medium", "xhigh"],
          },
        ],
      },
      "Qwen3.8-27B",
    );

    expect(model.supportedEffortLevels).toEqual(["low", "medium", "xhigh"]);
  });

  it("supports max level and configured off level", () => {
    const [model] = getPiModelOptions(
      {
        id: "custom-openai",
        name: "Custom OpenAI-compatible",
        base_url: "http://127.0.0.1:3000/v1",
        api: "openai-completions",
        model: "GLM-5.3-Flash",
        models: [
          {
            id: "GLM-5.3-Flash",
            supports_reasoning: true,
            supported_effort_levels: ["low", "high", "max"],
          },
        ],
      },
      "GLM-5.3-Flash",
    );

    expect(model.supportedEffortLevels).toEqual(["low", "high", "max"]);
  });
});

describe("filterPiThinkingLevelsForUi", () => {
  it("removes legacy none/minimal aliases without dropping high or xhigh", () => {
    expect(
      filterPiThinkingLevelsForUi(["none", "off", "minimal", "low", "medium", "high", "xhigh"]),
    ).toEqual(["off", "low", "medium", "high", "xhigh"]);
  });
});

describe("normalizePiCliModel", () => {
  it("migrates the legacy GLM placeholder after native Codex login", () => {
    expect(normalizePiCliModel("openai-codex", "glm-5.2")).toBe(PI_CODEX_DEFAULT_MODEL);
    expect(normalizePiCliModel("openai-codex", "")).toBe("");
  });

  it("does not overwrite a valid Codex model or a custom provider model", () => {
    expect(normalizePiCliModel("openai-codex", "openai-codex/gpt-5.6-luna")).toBe(
      "openai-codex/gpt-5.6-luna",
    );
    expect(normalizePiCliModel("custom-openai", "glm-5.2")).toBe("glm-5.2");
  });
});

describe("resolvePiStartupModel", () => {
  it("preserves the model selected in the current draft over a stale provider default", () => {
    expect(resolvePiStartupModel("glm-5.2", "deepseek/deepseek-v4-flash")).toBe("glm-5.2");
  });

  it("falls back to the configured provider model when the draft has no model", () => {
    expect(resolvePiStartupModel("", "deepseek/deepseek-v4-flash")).toBe(
      "deepseek/deepseek-v4-flash",
    );
  });
});
