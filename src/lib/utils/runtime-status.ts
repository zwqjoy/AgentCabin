import * as api from "$lib/api";
import { getGrokStatus } from "$lib/grok-api";
import type { CliCheckResult, UserSettings } from "$lib/types";
import type { RuntimeProviderId } from "$lib/utils/agent-metadata";
import { isProviderCompatible } from "$lib/utils/provider-routing";

export type RuntimeProviderAuthSource = "cli" | "agentcabin";

/** The settings needed to resolve whether a managed provider can start a chat. */
export type RuntimeProviderSettings = Pick<
  UserSettings,
  "agent_provider_bindings" | "global_providers"
>;

export interface RuntimeProviderStatus {
  installed: boolean;
  /** Effective authentication/readiness for the selected auth source. */
  authenticated: boolean;
  /** Whether the provider can be selected for a new chat. */
  ready?: boolean;
  /** The source that made the provider ready, or the source needing setup. */
  authSource?: RuntimeProviderAuthSource;
  reason?: string;
}

interface ManagedProviderStatus {
  ready: boolean;
  reason?: string;
}

function hasValue(value?: string | null): boolean {
  return Boolean(value?.trim());
}

/**
 * Resolve AgentCabin's canonical provider binding without looking at native CLI credentials.
 * A custom binding is authoritative: an incomplete managed binding must not silently fall back
 * to a user's native CLI login, because the selected provider/profile is the intended boundary.
 */
export function resolveManagedProviderStatus(
  pid: RuntimeProviderId,
  settings?: RuntimeProviderSettings | null,
): ManagedProviderStatus | null {
  const binding = settings?.agent_provider_bindings?.[pid];
  const providerId =
    binding?.mode === "custom" && binding.provider_id?.trim()
      ? binding.provider_id.trim()
      : settings?.global_providers?.find((item) => isProviderCompatible(pid, item.protocol))?.id;

  if (binding?.mode === "custom" && !binding.provider_id?.trim()) {
    return { ready: false, reason: "AgentCabin 尚未选择 Provider" };
  }
  if (!providerId) {
    return { ready: false, reason: "AgentCabin 尚未配置 Provider" };
  }

  const provider = settings?.global_providers?.find((item) => item.id === providerId);
  if (!provider) {
    return { ready: false, reason: `找不到 AgentCabin Provider：${providerId}` };
  }

  if (!hasValue(provider.base_url)) {
    return { ready: false, reason: "AgentCabin Provider 尚未配置 Base URL" };
  }

  // Credentials stay opaque here. A configured key, keyless provider, or explicit environment
  // key is enough to establish that AgentCabin owns the auth source; the runtime performs the
  // actual request/handshake and reports expired or invalid credentials separately.
  const hasCredentialSource =
    provider.keyless === true ||
    hasValue(provider.api_key) ||
    hasValue(provider.auth_env_var) ||
    hasValue(provider.env_key) ||
    Object.values(provider.extra_env ?? {}).some(hasValue);
  if (!hasCredentialSource) {
    return { ready: false, reason: "AgentCabin Provider 尚未配置凭证" };
  }

  return { ready: true };
}

export function isRuntimeProviderReady(status: RuntimeProviderStatus): boolean {
  return status.ready ?? (status.installed && status.authenticated);
}

export function runtimeProviderAuthLabel(status: RuntimeProviderStatus): string {
  if (status.authSource === "agentcabin") {
    return isRuntimeProviderReady(status) ? "AgentCabin 已配置" : "AgentCabin 配置不完整";
  }
  return status.authenticated ? "已认证" : "待认证";
}

function withEffectiveReadiness(
  native: Omit<RuntimeProviderStatus, "ready" | "authSource">,
  managed: ManagedProviderStatus | null,
): RuntimeProviderStatus {
  if (managed) {
    const ready = native.installed && managed.ready;
    return {
      ...native,
      authenticated: ready,
      ready,
      authSource: "agentcabin",
      reason: native.installed ? managed.reason : native.reason,
    };
  }

  return {
    ...native,
    ready: native.installed && native.authenticated,
    authSource: "cli",
  };
}

export async function fetchRuntimeProviderStatus(
  pid: RuntimeProviderId,
  settings?: RuntimeProviderSettings | null,
): Promise<RuntimeProviderStatus> {
  const resolvedSettings =
    settings === undefined ? await api.getUserSettings().catch(() => null) : settings;
  const managed = resolveManagedProviderStatus(pid, resolvedSettings);

  try {
    if (pid === "grok") {
      const grok = await getGrokStatus().catch(() => ({
        found: false,
        authenticated: false,
        authError: "未检测到 Grok CLI",
      }));
      return withEffectiveReadiness(
        {
          installed: grok.found,
          authenticated: grok.found,
          reason: grok.found ? undefined : "未检测到 Grok CLI",
        },
        managed,
      );
    } else if (pid === "codex") {
      const cli = await api.checkAgentCli("codex").catch(() => ({ found: false, agent: "codex" }));
      return withEffectiveReadiness(
        {
          installed: cli.found,
          authenticated: cli.found,
          reason: cli.found ? undefined : "未检测到 Codex CLI",
        },
        managed,
      );
    } else if (pid === "pi") {
      const cli = await api
        .checkAgentCli("pi")
        .catch((): CliCheckResult => ({ found: false, agent: "pi" }));
      const installed = cli.found && cli.version_supported !== false;
      const reason =
        cli.found && cli.version_supported === false
          ? `Pi ${cli.version ?? "未知版本"} 过旧，需要升级到 ${cli.minimum_version ?? "0.84.4"} 或更高版本`
          : cli.found
            ? undefined
            : "未检测到 Pi CLI";
      return withEffectiveReadiness({ installed, authenticated: installed, reason }, managed);
    } else if (pid === "dsh") {
      const cli = await api
        .checkAgentCli("dsh")
        .catch((): CliCheckResult => ({ found: false, agent: "dsh" }));
      return withEffectiveReadiness(
        {
          installed: cli.found,
          authenticated: cli.found,
          reason: cli.found ? undefined : "未检测到 DSH CLI",
        },
        managed,
      );
    } else {
      // claude
      const cli = await api
        .checkAgentCli("claude")
        .catch(() => ({ found: false, agent: "claude" }));
      return withEffectiveReadiness(
        {
          installed: cli.found,
          authenticated: cli.found,
          reason: cli.found ? undefined : "未检测到 Claude CLI",
        },
        managed,
      );
    }
  } catch (e) {
    return withEffectiveReadiness(
      {
        installed: false,
        authenticated: false,
        reason: (e as Error)?.message || "检查失败",
      },
      managed,
    );
  }
}
