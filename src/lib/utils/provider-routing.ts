export type ProviderAgent = "claude" | "codex" | "pi" | "grok" | "dsh";

import type { AgentProviderBinding } from "$lib/types";

export function isProviderCompatible(agent: ProviderAgent, protocol: string): boolean {
  if (agent === "claude") return protocol === "anthropic-messages";
  if (agent === "codex") {
    return protocol === "openai-responses" || protocol === "openai-completions";
  }
  if (agent === "grok") {
    return protocol === "openai-responses" || protocol === "openai-completions";
  }
  if (agent === "dsh") {
    return (
      protocol === "openai-completions" ||
      protocol === "openai-responses" ||
      protocol === "anthropic-messages"
    );
  }
  return true;
}

export function filterCustomProviderModels<T extends { value: string }>(
  allModels: T[],
  allowed: string[],
): T[] {
  if (!allowed.length) return allModels;
  return allModels.filter((item) => allowed.includes(item.value));
}

/** Resolve the model that should be shown when a managed provider starts a new chat. */
export function resolveManagedProviderDefaultModel(
  binding: AgentProviderBinding | undefined,
  providerModels: string[],
): string {
  if (binding?.mode !== "custom") return "";
  const configured = (binding.models ?? []).map((model) => model.trim()).filter(Boolean);
  const catalog = providerModels.map((model) => model.trim()).filter(Boolean);
  const explicit = binding.model?.trim() ?? "";

  // Prefer the saved default when it still belongs to the configured/catalogued models.
  // If either list is unavailable, the remaining source is authoritative (legacy settings
  // may not have a provider catalog yet).
  if (
    explicit &&
    (!configured.length || configured.includes(explicit)) &&
    (!catalog.length || catalog.includes(explicit))
  ) {
    return explicit;
  }
  return configured[0] ?? catalog[0] ?? explicit;
}
