import type {
  CliModelInfo,
  CodexModelList,
  GlobalProviderCredential,
  GlobalProviderModel,
} from "$lib/types";

/** Stable id for the keyless provider backed by AgentCabin's own ChatGPT login. */
// Keep this distinct from Pi's native provider id (`openai-codex`). This is the
// AgentCabin-wide subscription profile backed by its own local OAuth secret.
export const CODEX_SUBSCRIPTION_PROVIDER_ID = "openai-chatgpt-subscription";
export const CODEX_SUBSCRIPTION_PROVIDER_NAME = "OpenAI / ChatGPT 订阅";
export const CODEX_SUBSCRIPTION_BASE_URL = "https://chatgpt.com/backend-api/codex";

function toGlobalModel(model: CliModelInfo): GlobalProviderModel {
  return {
    id: model.value,
    name: model.displayName && model.displayName !== model.value ? model.displayName : undefined,
    context_window: model.contextWindow,
    supports_reasoning: model.supportsEffort || undefined,
    supports_xhigh: model.supportedEffortLevels?.includes("xhigh") || undefined,
    supported_effort_levels: model.supportedEffortLevels?.length
      ? [...model.supportedEffortLevels]
      : undefined,
  };
}

/**
 * Materialize the ChatGPT subscription catalog as the canonical, keyless global Provider.
 * The actual bearer remains in AgentCabin's local host secret; this record is only a model
 * catalog and a routing marker for the runtime bridge.
 */
export function buildCodexSubscriptionProvider(
  catalog: CodexModelList,
  existing?: GlobalProviderCredential,
): GlobalProviderCredential {
  const models = catalog.models.map(toGlobalModel);
  const testModel = catalog.defaultModel ?? models[0]?.id ?? existing?.test_model;
  return {
    id: CODEX_SUBSCRIPTION_PROVIDER_ID,
    name: CODEX_SUBSCRIPTION_PROVIDER_NAME,
    protocol: "openai-responses",
    base_url: CODEX_SUBSCRIPTION_BASE_URL,
    models: models.length ? models : existing?.models,
    test_model: testModel,
    supports_developer_role: true,
    supports_reasoning_effort: true,
    keyless: true,
  };
}
