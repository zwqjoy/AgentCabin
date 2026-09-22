import type { AgentCapabilities, AgentUiCapabilities, ExecutionPath } from "$lib/types";

/**
 * AgentCabin-local execution/transport profile.
 *
 * Unlike protocol/runtime/ui capabilities, these fields describe how AgentCabin
 * hosts the integration. They are static provider metadata and are never replaced
 * by live protocol negotiation.
 */
export interface AgentExecutionProfile {
  busEvents: boolean;
  sessionInitEvents: boolean;
  snapshots: boolean;
  liveAddDir: boolean;
  sessionActor: boolean;
  /** Pre-run UI inference. "backend" waits for the materialized TaskRun path. */
  preRunExecutionPath: ExecutionPath | "backend";
}

/** Static + negotiated capabilities together with the local execution profile. */
export interface EffectiveAgentCapabilities extends AgentCapabilities {
  execution: AgentExecutionProfile;
}

/**
 * Static AgentCabin capability baseline for an agent integration.
 *
 * Live protocol negotiation (currently Grok ACP) is authoritative and overlays
 * these defaults. Runtime flags describe the AgentCabin integration surface and
 * can be narrowed or enabled by live negotiation for a particular session.
 */
const CLAUDE_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    sessionLoad: true,
    sessionSetModel: true,
    sessionModeControl: false,
    permissionRequest: true,
    permissionModeControl: true,
    slashCommands: true,
    planMode: true,
    effortControl: true,
    goalState: true,
    structuredTaskState: false,
  },
  runtime: {
    attachments: true,
    remote: true,
    fork: true,
    steer: true,
    followUp: true,
  },
  ui: {
    slashCommandMenu: true,
    planModeToggle: true,
    goalPanel: true,
    effortSelector: true,
    permissionModeSwitch: true,
    addDirAction: true,
  },
  execution: {
    busEvents: true,
    sessionInitEvents: true,
    snapshots: true,
    liveAddDir: true,
    sessionActor: true,
    preRunExecutionPath: "session_actor",
  },
};

const CODEX_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    sessionLoad: true,
    sessionSetModel: true,
    sessionModeControl: false,
    permissionRequest: false,
    permissionModeControl: true,
    slashCommands: true,
    planMode: true,
    effortControl: true,
    goalState: true,
    structuredTaskState: false,
  },
  runtime: {
    attachments: true,
    remote: false,
    fork: true,
    steer: true,
    followUp: true,
  },
  ui: {
    slashCommandMenu: true,
    planModeToggle: true,
    goalPanel: true,
    effortSelector: true,
    permissionModeSwitch: true,
    addDirAction: true,
  },
  execution: {
    busEvents: true,
    sessionInitEvents: false,
    snapshots: false,
    liveAddDir: false,
    sessionActor: true,
    // Codex app-server vs exec is backend-selected from user settings.
    // TaskRun.execution_path becomes authoritative when the run starts.
    preRunExecutionPath: "backend",
  },
};

const PI_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    sessionLoad: true,
    sessionSetModel: true,
    sessionModeControl: false,
    permissionRequest: true,
    permissionModeControl: true,
    slashCommands: true,
    planMode: true,
    effortControl: true,
    goalState: false,
    structuredTaskState: false,
  },
  runtime: {
    attachments: true,
    remote: false,
    fork: true,
    steer: true,
    followUp: true,
  },
  ui: {
    slashCommandMenu: true,
    planModeToggle: true,
    goalPanel: false,
    effortSelector: true,
    permissionModeSwitch: true,
    addDirAction: false,
  },
  execution: {
    busEvents: true,
    sessionInitEvents: true,
    snapshots: false,
    liveAddDir: false,
    sessionActor: true,
    preRunExecutionPath: "session_actor",
  },
};

const GROK_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    // Grok's load/model/command/native-mode support is negotiated from ACP
    // initialize and session updates. Plan also has a host startup fallback.
    sessionLoad: false,
    sessionSetModel: false,
    sessionModeControl: false,
    permissionRequest: true,
    permissionModeControl: false,
    slashCommands: false,
    planMode: true,
    // Grok's verified ACP effort path is `session/set_model` with
    // `_meta.reasoningEffort`; the live actor narrows this per selected model.
    effortControl: true,
    goalState: false,
    structuredTaskState: false,
  },
  runtime: {
    attachments: false,
    remote: false,
    fork: false,
    steer: false,
    followUp: false,
  },
  ui: {
    slashCommandMenu: false,
    planModeToggle: true,
    goalPanel: true,
    effortSelector: true,
    // Grok persists a permission policy for the next ACP session. Its actor
    // does not expose a live permission-mode mutation primitive.
    permissionModeSwitch: true,
    addDirAction: false,
  },
  execution: {
    busEvents: true,
    sessionInitEvents: true,
    snapshots: false,
    liveAddDir: false,
    sessionActor: true,
    preRunExecutionPath: "session_actor",
  },
};

const DSH_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    sessionLoad: true,
    // Code and Work both use the official Harness Session Controller bridge;
    // model/provider changes are installed with selectModel for the next turn.
    sessionSetModel: true,
    sessionModeControl: true,
    permissionRequest: true,
    // DSH maps the shared mode control to its native /permission presets.
    permissionModeControl: true,
    slashCommands: false,
    planMode: false,
    // Harness Session Controller applies effort changes through the same
    // session-local next-request selection used for model/provider changes.
    effortControl: true,
    goalState: false,
    structuredTaskState: false,
  },
  runtime: {
    // DSH Harness accepts ACP image content blocks. The selected model still
    // has to advertise `supports_images` before the provider route is usable.
    attachments: true,
    remote: false,
    fork: false,
    steer: true,
    followUp: true,
  },
  ui: {
    slashCommandMenu: false,
    planModeToggle: false,
    goalPanel: false,
    effortSelector: true,
    permissionModeSwitch: true,
    addDirAction: false,
  },
  execution: {
    busEvents: true,
    sessionInitEvents: true,
    snapshots: false,
    liveAddDir: false,
    sessionActor: true,
    preRunExecutionPath: "session_actor",
  },
};

const MINIMAL_CAPABILITIES: EffectiveAgentCapabilities = {
  protocol: {
    sessionLoad: false,
    sessionSetModel: false,
    sessionModeControl: false,
    permissionRequest: false,
    permissionModeControl: false,
    slashCommands: false,
    planMode: false,
    effortControl: false,
    goalState: false,
    structuredTaskState: false,
  },
  runtime: {
    attachments: false,
    remote: false,
    fork: false,
    steer: false,
    followUp: false,
  },
  ui: {
    slashCommandMenu: false,
    planModeToggle: false,
    goalPanel: false,
    effortSelector: false,
    permissionModeSwitch: false,
    addDirAction: false,
  },
  execution: {
    busEvents: false,
    sessionInitEvents: false,
    snapshots: false,
    liveAddDir: false,
    sessionActor: false,
    preRunExecutionPath: "pipe_exec",
  },
};

const CAPABILITY_PROVIDERS: Record<string, EffectiveAgentCapabilities> = {
  claude: CLAUDE_CAPABILITIES,
  codex: CODEX_CAPABILITIES,
  pi: PI_CAPABILITIES,
  grok: GROK_CAPABILITIES,
  dsh: DSH_CAPABILITIES,
};

function cloneCapabilities(capabilities: EffectiveAgentCapabilities): EffectiveAgentCapabilities {
  return {
    protocol: { ...capabilities.protocol },
    runtime: { ...capabilities.runtime },
    ui: { ...capabilities.ui },
    execution: { ...capabilities.execution },
  };
}

/** Return the static AgentCabin integration baseline for an agent. */
export function getDefaultAgentCapabilities(agent: string): EffectiveAgentCapabilities {
  return cloneCapabilities(CAPABILITY_PROVIDERS[agent] ?? MINIMAL_CAPABILITIES);
}

/**
 * Resolve effective capabilities for a session.
 *
 * Negotiated capabilities overlay protocol/runtime/ui only. The local execution
 * profile is provider-owned and cannot be replaced by protocol negotiation. A fresh
 * object is always returned so callers cannot mutate provider constants.
 */
export function getAgentCapabilities(
  agent: string,
  negotiated?: AgentCapabilities | null,
): EffectiveAgentCapabilities {
  const baseline = getDefaultAgentCapabilities(agent);
  if (!negotiated) return baseline;
  return {
    protocol: { ...baseline.protocol, ...negotiated.protocol },
    runtime: { ...baseline.runtime, ...negotiated.runtime },
    ui: { ...baseline.ui, ...negotiated.ui },
    execution: { ...baseline.execution },
  };
}

export function getAgentUiCapabilities(
  agent: string,
  negotiated?: AgentCapabilities | null,
): AgentUiCapabilities {
  return getAgentCapabilities(agent, negotiated).ui;
}

export function hasAgentCapabilityProvider(agent: string): boolean {
  return agent in CAPABILITY_PROVIDERS;
}
