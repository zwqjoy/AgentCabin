import type { CliModelInfo } from "$lib/types";

// Pi's managed-provider bridge has historically used the same conservative fallback.
// Keep the shared UI consistent when a custom OpenAI/Anthropic-compatible provider does
// not publish model metadata of its own.
export const DEFAULT_CUSTOM_CONTEXT_WINDOW = 128_000;

export interface ContextWindowResolution {
  contextWindow: number;
  estimated: boolean;
}

function normalizedModelIds(value: string): string[] {
  const normalized = value.trim().toLowerCase();
  if (!normalized) return [];
  const slash = normalized.lastIndexOf("/");
  return slash >= 0 ? [normalized, normalized.slice(slash + 1)] : [normalized];
}

function modelsMatch(left: string, right: string): boolean {
  const leftIds = normalizedModelIds(left);
  const rightIds = new Set(normalizedModelIds(right));
  return leftIds.some((id) => rightIds.has(id));
}

export function resolveContextWindow({
  reportedWindow = 0,
  model = "",
  models = [],
  customProvider = false,
}: {
  reportedWindow?: number;
  model?: string;
  models?: CliModelInfo[];
  customProvider?: boolean;
}): ContextWindowResolution {
  if (reportedWindow > 0) return { contextWindow: reportedWindow, estimated: false };

  const catalogMatch = models.find(
    (candidate) => (candidate.contextWindow ?? 0) > 0 && modelsMatch(candidate.value, model),
  );
  if (catalogMatch?.contextWindow) {
    return { contextWindow: catalogMatch.contextWindow, estimated: false };
  }

  if (customProvider && model.trim()) {
    return { contextWindow: DEFAULT_CUSTOM_CONTEXT_WINDOW, estimated: true };
  }
  return { contextWindow: 0, estimated: false };
}
