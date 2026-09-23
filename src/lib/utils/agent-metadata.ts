import { hasAgentCapabilityProvider } from "$lib/utils/agent-capabilities";

/** Shared display order for Runtime Providers: Pi Agent, Codex, Claude Code, Grok, DSH. */
export const ALL_RUNTIME_PROVIDERS = ["pi", "codex", "claude", "grok", "dsh"] as const;
export type RuntimeProviderId = (typeof ALL_RUNTIME_PROVIDERS)[number];

/** Visible runtime providers in the UI (only Pi Agent is visible in AgentCabin). */
export const VISIBLE_RUNTIME_PROVIDERS: readonly RuntimeProviderId[] = ["pi"];

export const DEFAULT_ENABLED_RUNTIME_PROVIDERS: readonly RuntimeProviderId[] = ["pi"];
export const DEFAULT_RUNTIME_PROVIDER: RuntimeProviderId = "pi";

export const NATIVE_AGENT_ORDER = ALL_RUNTIME_PROVIDERS;
export const AGENT_ORDER = ALL_RUNTIME_PROVIDERS;

/** Product identity shown for assistant messages in the Code and Work transcripts. */
export const CONVERSATION_ASSISTANT_NAME = "AgentCabin";

export interface RuntimeProviderMeta {
  id: RuntimeProviderId;
  name: string;
  color: string;
  dotClass: string;
  desc: string;
}

export const RUNTIME_PROVIDERS_CONFIG: Record<RuntimeProviderId, RuntimeProviderMeta> = {
  pi: {
    id: "pi",
    name: "Pi Agent",
    color: "bg-purple-500",
    dotClass: "bg-purple-500",
    desc: "内置调度引擎，深度集成扩展、MCP与工具系统",
  },
  codex: {
    id: "codex",
    name: "Codex",
    color: "bg-emerald-500",
    dotClass: "bg-emerald-500",
    desc: "OpenAI Codex 智能开发运行时，适合代码实现与审查",
  },
  claude: {
    id: "claude",
    name: "Claude Code",
    color: "bg-amber-500",
    dotClass: "bg-amber-500",
    desc: "Anthropic Claude Code 命令行运行时，支持长上下文与复杂子任务",
  },
  grok: {
    id: "grok",
    name: "Grok",
    color: "bg-violet-500",
    dotClass: "bg-violet-500",
    desc: "xAI Grok 高性能编码运行时，支持 ACP 协议与流式交互",
  },
  dsh: {
    id: "dsh",
    name: "DeepSeek Harness (DSH)",
    color: "bg-cyan-500",
    dotClass: "bg-cyan-500",
    desc: "DeepSeek Harness 智能开发运行时，支持 JSON-RPC 协议与多模式分发",
  },
};

/** Canonical labels for agent IDs. IDs remain internal; these labels are UI-only. */
const AGENT_DISPLAY_NAMES: Record<string, string> = {
  pi: "Pi Agent",
  codex: "Codex",
  claude: "Claude Code",
  grok: "Grok",
  dsh: "DeepSeek Harness",
};

export function isKnownAgent(agent: string): boolean {
  return (
    (ALL_RUNTIME_PROVIDERS as readonly string[]).includes(agent) ||
    hasAgentCapabilityProvider(agent)
  );
}

export function getAgentDisplayName(agent: string): string {
  const normalized = agent.trim();
  if (!normalized || normalized === "unknown") return "Agent";
  return AGENT_DISPLAY_NAMES[normalized] ?? normalized;
}

/** Return the assistant label used by both live and completed messages. */
export function getAssistantDisplayName(agent: string, _claudeLabel?: string): string {
  return getAgentDisplayName(agent);
}
