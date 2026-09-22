import type { CliModelInfo, PiProviderCredential } from "$lib/types";

export interface PiProviderPreset {
  id: string;
  name: string;
  description: string;
  base_url: string;
  api: PiProviderCredential["api"];
  model: string;
  custom?: boolean;
}

export const PI_THINKING_LEVELS = ["off", "low", "medium", "high", "xhigh", "max"];
const PI_MANAGED_STANDARD_THINKING_LEVELS = ["off", "low", "medium", "high"];
const PI_MANAGED_XHIGH_THINKING_LEVELS = ["off", "low", "medium", "high", "xhigh"];
const PI_CONFIGURED_THINKING_LEVELS = ["off", "low", "medium", "high", "xhigh", "max"];

export function getPiModelEffortLevels(
  managedProvider: boolean,
  supportsXHigh?: boolean,
  configuredLevels?: readonly string[],
): string[] {
  if (configuredLevels?.length) {
    const levels = configuredLevels.filter((level) =>
      PI_CONFIGURED_THINKING_LEVELS.includes(level),
    );
    if (levels.length > 0) {
      return levels.filter((level, index, values) => values.indexOf(level) === index);
    }
  }
  if (!managedProvider) return [...PI_THINKING_LEVELS];
  return supportsXHigh
    ? [...PI_MANAGED_XHIGH_THINKING_LEVELS]
    : [...PI_MANAGED_STANDARD_THINKING_LEVELS];
}

/** Default native Pi model used when migrating the old managed-provider placeholder. */
export const PI_CODEX_DEFAULT_MODEL = "openai-codex/gpt-5.5";

/**
 * Keep the native Pi/Codex login path from booting with the legacy GLM placeholder.
 * An empty model is intentional: Pi can use the default model from its own settings.json.
 * Custom providers are deliberately left untouched because `glm-5.2` can be valid there.
 */
export function normalizePiCliModel(
  provider: string | null | undefined,
  model: string | null | undefined,
): string {
  const value = model?.trim() ?? "";
  const providerFromModel = value.split("/", 1)[0]?.trim().toLowerCase();
  const effectiveProvider = provider?.trim().toLowerCase() || providerFromModel;
  const modelId = value.includes("/") ? value.slice(value.indexOf("/") + 1) : value;

  if (effectiveProvider === "openai-codex" && value && modelId === "glm-5.2") {
    return PI_CODEX_DEFAULT_MODEL;
  }
  return value;
}

/**
 * Preserve a model explicitly selected in the current composer when a new Pi session starts.
 * The managed provider model is only a fallback; it may be stale while the project default or
 * current draft has already been changed.
 */
export function resolvePiStartupModel(
  currentModel: string | null | undefined,
  configuredModel: string | null | undefined,
): string {
  return currentModel?.trim() || configuredModel?.trim() || "";
}

export function getPiModelOptions(
  provider: PiProviderCredential | null | undefined,
  activeModel: string,
): CliModelInfo[] {
  const model = provider?.model?.trim() || activeModel.trim();
  const configuredModels = provider?.models?.length
    ? provider.models
    : model
      ? [{ id: model, context_window: provider?.context_window }]
      : [];

  return configuredModels.map((configured) => ({
    value: configured.id,
    displayName: configured.name || configured.id,
    description: configured.id === model ? "Default" : "Configured",
    contextWindow:
      configured.context_window ?? (configured.id === model ? provider?.context_window : undefined),
    supportsEffort: configured.supports_reasoning !== false,
    supportedEffortLevels: getPiModelEffortLevels(
      Boolean(provider),
      configured.supports_xhigh,
      configured.supported_effort_levels,
    ),
  }));
}

export const PI_PROVIDER_PRESETS: PiProviderPreset[] = [
  {
    id: "anthropic",
    name: "Anthropic",
    description: "Anthropic Messages API",
    base_url: "https://api.anthropic.com/v1",
    api: "anthropic-messages",
    model: "",
  },
  {
    id: "openai",
    name: "OpenAI",
    description: "OpenAI Responses API",
    base_url: "https://api.openai.com/v1",
    api: "openai-responses",
    model: "",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    description: "OpenAI-compatible gateway",
    base_url: "https://openrouter.ai/api/v1",
    api: "openai-completions",
    model: "",
  },
  {
    id: "custom-openai",
    name: "Custom OpenAI-compatible",
    description: "Chat Completions endpoint",
    base_url: "",
    api: "openai-completions",
    model: "",
    custom: true,
  },
  {
    id: "custom-anthropic",
    name: "Custom Anthropic-compatible",
    description: "Messages endpoint",
    base_url: "",
    api: "anthropic-messages",
    model: "",
    custom: true,
  },
];
