import { getTransport } from "$lib/transport";
import type { CliModelInfo } from "$lib/types";

export interface GrokStatus {
  found: boolean;
  path?: string;
  version?: string;
  authenticated: boolean;
  authError?: string;
  currentModel?: string;
  configPath?: string;
  models: CliModelInfo[];
}

/** Non-secret settings that AgentCabin is allowed to read/write in Grok's config.toml. */
export interface GrokCliConfig {
  autoUpdate?: boolean;
  permissionMode?: string;
  screenMode?: string;
  simpleMode?: boolean;
  vimMode?: boolean;
  showThinkingBlocks?: boolean;
  groupToolVerbs?: boolean;
  rememberToolApprovals?: boolean;
  telemetry?: boolean;
  codebaseIndexing?: boolean;
  remoteFetch?: boolean;
  autoCompactThresholdPercent?: number;
  loadEnvrc?: boolean;
  respectGitignore?: boolean;
}

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

export function getGrokStatus(): Promise<GrokStatus> {
  return invoke<GrokStatus>("get_grok_status");
}

export function getGrokModels(): Promise<CliModelInfo[]> {
  return invoke<CliModelInfo[]>("get_grok_models");
}

export function getGrokCliConfig(): Promise<GrokCliConfig> {
  return invoke<GrokCliConfig>("get_grok_cli_config");
}

export function updateGrokCliConfig(patch: Partial<GrokCliConfig>): Promise<GrokCliConfig> {
  return invoke<GrokCliConfig>("update_grok_cli_config", { patch });
}

export function runGrokLogin(): Promise<boolean> {
  return invoke<boolean>("run_grok_login");
}
