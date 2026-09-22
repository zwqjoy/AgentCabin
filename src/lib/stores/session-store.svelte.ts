/**
 * SessionStore: single source of truth for chat session state.
 *
 * Replaces 25 scattered $state variables and 3 booleans (running x sending x sessionStarted)
 * with a state-machine (SessionPhase) + organized fields + idempotent reducers.
 */
import * as api from "$lib/api";
import type {
  TaskRun,
  RunStatus,
  HookEvent,
  BusEvent,
  BusToolItem,
  TimelineEntry,
  Attachment,
  CliCommand,
  McpServerInfo,
  ElicitationSchema,
  SessionMode,
  TodoItem,
  PanelTask,
  QueuedMessage,
} from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";
import { yieldToMain } from "$lib/utils/yield";
import { IMAGE_TYPES } from "$lib/utils/file-types";
import { uuid } from "$lib/utils/uuid";
import {
  type SessionPhase,
  type UsageState,
  type TurnUsage,
  ACTIVE_PHASES,
  TERMINAL_PHASES,
  SESSION_ALIVE_PHASES,
  assertTransition,
} from "./types";
import { getEventMiddleware } from "./event-middleware";
import { updateInstalledVersion, getCliCommands } from "./cli-info.svelte";
import { getKnownVirtualNames, parseSlashCommand } from "$lib/utils/slash-commands";
import { getAgentDisplayName } from "$lib/utils/agent-metadata";
import * as snapshotCache from "$lib/utils/snapshot-cache";
import { getTransport } from "$lib/transport";
import { withTimeout } from "$lib/utils/async-utils";
import { dispatchRunMutation, RUNS_CHANGED_EVENT } from "$lib/utils/run-mutations";
import {
  getAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import { resolveStartupPermissionMode } from "$lib/utils/permission-mode";
import { resolvePiStartupModel } from "$lib/utils/pi-provider-presets";
import { resolveManagedProviderDefaultModel } from "$lib/utils/provider-routing";
import { dedupeMcpServersByName } from "$lib/utils/mcp";
import {
  SCHEDULING_TOOLS,
  pickSchedule,
  pickCronId,
  extractOutputText,
} from "$lib/utils/tool-rendering";

// Lifecycle controls are recovery actions. They must always release the
// frontend state even if the desktop process is still unwinding a provider or
// tool handler in the background.
const SESSION_CONTROL_TIMEOUT_MS = 5_000;
const SESSION_STOP_TIMEOUT_MS = 10_000;

// ── CLI permission mode normalization ──
// CLI may return different names for the same mode across versions.
// Normalize to the canonical names used throughout the app.
const CLI_PERM_MODE_ALIASES: Record<string, string> = {
  delegate: "acceptEdits", // CLI v2.1.81+ renamed acceptEdits → delegate
};

// Internal first-turn prompt used to seed a cross-agent branch. It is sent to the
// target model but is intentionally omitted from the visible timeline.
const AGENT_HANDOFF_CONTEXT_MARKER = "[[AGENTCABIN_AGENT_HANDOFF]]";
// Internal Work continuation prompt. It keeps the runtime resume explicit while
// avoiding a synthetic user bubble that splits the original conversation turn.
const AGENT_WORK_RESUME_CONTEXT_MARKER = "[[AGENTCABIN_WORK_RESUME]]";

function isAgentInternalContext(text: string): boolean {
  return (
    text.startsWith(AGENT_HANDOFF_CONTEXT_MARKER) ||
    text.startsWith(AGENT_WORK_RESUME_CONTEXT_MARKER)
  );
}

function normalizePermissionMode(mode: string): string {
  return CLI_PERM_MODE_ALIASES[mode] ?? mode;
}

/** Pi's permission extension may publish its current yolo mode through the
 * generic extension UI channel. It is already represented by the composer
 * permission indicator, so it must not become a persistent chat card. */
function isPiPermissionModeIndicator(value: unknown): boolean {
  return typeof value === "string" && /^(?:yolo|yolo mode)$/i.test(value.trim());
}

/** pi-context-prune emits routine lifecycle/progress diagnostics as info notices.
 * They are useful in a terminal log, but too noisy and implementation-specific
 * for the chat toast. Warnings/errors still pass through to the user. */
function isPiContextPruneRoutineNotice(value: unknown, noticeType: string): boolean {
  if (noticeType !== "info" || typeof value !== "string") return false;
  const message = value.trim();
  return (
    (message.startsWith("pruner loaded — pruning ") && message.includes(" | model: ")) ||
    /^pruner:\s+\d[\d,]*\s+turns?\s+queued\b/i.test(message)
  );
}

/** Pi exposes a user-initiated abort as a low-level command output. Hide both
 * new and replayed variants; stopping a turn is normal user interaction. */
function isPiAbortCommandOutput(content: string): boolean {
  const normalized = content.trim().toLowerCase();
  return (
    (normalized.startsWith("[pi error]") || normalized.startsWith("[pi rpc] abort")) &&
    normalized.includes("abort")
  );
}

/** Codex emits this low-level metadata diagnostic for custom provider model IDs. The bridge
 * already supplies safe fallback metadata, so showing the implementation detail in chat only
 * adds noise and is not actionable for the user. */
function isCodexModelMetadataNotice(content: string): boolean {
  const normalized = content.trim().toLowerCase();
  return (
    normalized.startsWith("[notice] model metadata for") &&
    normalized.includes("not found") &&
    normalized.includes("defaulting to fallback metadata")
  );
}

/** Pi emits transport/npm/node diagnostic lines to stderr during session startup and tool execution.
 *  These are internal transport/runtime details and should not pollute the chat timeline. */
function isPiRpcStderrCommandOutput(content: string): boolean {
  return /^\[pi rpc stderr\]/i.test(content.trim());
}

/** Extract `hookSpecificOutput.sessionTitle` from a SessionStart hook's raw stdout.
 *  The CLI delivers the hook's stdout verbatim as a JSON string (verified over stream-json,
 *  CLI 2.1.x). Returns the trimmed title, or null if absent/unparseable. */
function extractSessionTitle(stdout: string | undefined): string | null {
  if (!stdout) return null;
  try {
    const parsed = JSON.parse(stdout) as {
      hookSpecificOutput?: { sessionTitle?: unknown };
    };
    const title = parsed.hookSpecificOutput?.sessionTitle;
    if (typeof title === "string" && title.trim()) return title.trim();
  } catch {
    // Hook stdout is not always JSON (plain-text additionalContext etc.) — not an error.
  }
  return null;
}

// ── OpGuard: async operation guard with mounted check ──

class OpGuard {
  private _active = false;
  private _mounted = true;

  get busy(): boolean {
    return this._active;
  }
  get isMounted(): boolean {
    return this._mounted;
  }

  acquire(): boolean {
    if (this._active) return false;
    this._active = true;
    return true;
  }
  release(): void {
    this._active = false;
  }
  unmount(): void {
    this._mounted = false;
  }
}

// ── Helpers ──

function eventTs(ev: BusEvent): string {
  const r = ev as Record<string, unknown>;
  return (r.ts as string) ?? (r.timestamp as string) ?? new Date().toISOString();
}

/** Backfill anchorId for old snapshots/entries that predate the anchor system. Recursive for subTimelines. */
function backfillAnchorId(entry: TimelineEntry): TimelineEntry {
  const e = entry as Record<string, unknown>;
  if (e.anchorId) return entry; // already has anchorId
  const anchor = (e.cliUuid as string) || (e.id as string);
  const patched = { ...entry, anchorId: anchor } as TimelineEntry;
  if (patched.kind === "tool" && patched.subTimeline) {
    (patched as { subTimeline: TimelineEntry[] }).subTimeline =
      patched.subTimeline.map(backfillAnchorId);
  }
  return patched;
}

/** Parse event timestamp to epoch milliseconds (falls back to Date.now()). */
function eventTsMs(ev: BusEvent): number {
  const iso = eventTs(ev);
  const ms = new Date(iso).getTime();
  return Number.isFinite(ms) ? ms : Date.now();
}

// ── Internal batch state (plain objects, no reactivity) ──

interface ReduceCtx {
  tl: TimelineEntry[];
  he: HookEvent[];
  streamText: string;
  thinkingText: string;
  thinkingStartMs: number;
  thinkingEndMs: number;
  model: string;
  phase: SessionPhase;
  usage: UsageState;
  error: string;
  seenMessageIds: Set<string>;
  seenToolIds: Set<string>;
  /** Track run.status changes from non-terminal run_state events (running/idle). */
  runStatus: RunStatus | null;
  /** New session_id from session_init (e.g. fork generates a new CLI session). */
  sessionId: string | null;
  /** Whether this run uses stream-json mode (skip tools mirror writes). */
  isStream: boolean;
  /** Per-turn usage snapshots. */
  turnUsages: TurnUsage[];
  /** High-water context tokens / window since last compaction (drives contextUtilization). */
  contextHwTokens: number;
  contextHwWindow: number;
  /** Context tokens of the last request in the current turn (drives context occupancy, #149). */
  lastReqContextTokens: number;
  /** tool_use_id → tl[] index (only tool entries, first-match semantics). */
  toolTlIndex: Map<string, number>;
  /** tool_use_id → he[] index (only HookEvent entries with tool_use_id). */
  toolHeIndex: Map<string, number>;
  /** Local scheduled-task accumulator. Snapshotted from store at ctx
   *  creation; only committed in _commitReduceCtx. Keeps async replay
   *  stale-safe (mid-batch abort doesn't pollute live store). */
  scheduledTasks: ScheduledTask[];
}

// ── Helpers ──

/** Strip contentBase64 from non-image attachments to avoid storing MB of data in reactive state.
 *  Images keep base64 for inline <img> preview; PDF/other show as file chip (metadata only). */
function timelineAttachments(atts: Attachment[]): Attachment[] | undefined {
  if (atts.length === 0) return undefined;
  return atts.map((a) =>
    (IMAGE_TYPES as readonly string[]).includes(a.type) ? a : { ...a, contentBase64: "" },
  );
}

/** Replay RunEvent[] into an xterm terminal.
 *  Handles: user (prompt), system (status), stdout (agent output),
 *  stderr (error stream), assistant (final reply). */
function replayTerminalEvents(
  events: import("$lib/types").RunEvent[],
  xtermRef: { writeText(s: string): void },
  isRunning: boolean,
): void {
  let hasHistory = false;
  for (const event of events) {
    const text = String(
      (event.payload as Record<string, unknown>).text ??
        (event.payload as Record<string, unknown>).message ??
        "",
    );
    if (!text) continue;
    switch (event.type) {
      case "user":
        xtermRef.writeText(`\x1b[1;36m> ${text}\x1b[0m\r\n`);
        hasHistory = true;
        break;
      case "system":
        xtermRef.writeText(`\x1b[90m${text}\x1b[0m\r\n`);
        break;
      case "stdout":
        xtermRef.writeText(text);
        hasHistory = true;
        break;
      case "stderr":
        xtermRef.writeText(`\x1b[31m${text}\x1b[0m\r\n`);
        hasHistory = true;
        break;
      case "assistant":
        xtermRef.writeText(`${text}\r\n`);
        hasHistory = true;
        break;
    }
  }
  if (hasHistory && !isRunning) {
    xtermRef.writeText(`\r\n\x1b[90m--- Session ended ---\x1b[0m\r\n`);
  }
}

/** Map frontend Attachment[] to backend AttachmentData format for IPC. */
function mapAttachments(
  atts: Attachment[],
): Array<{ content_base64: string; media_type: string; filename: string }> | null {
  if (atts.length === 0) return null;
  return atts.map((a) => ({
    content_base64: a.contentBase64,
    media_type: a.type,
    filename: a.name,
  }));
}

// ── Exported types ──

export interface ElicitationState {
  requestId: string;
  mcpServerName: string;
  message: string;
  elicitationId?: string;
  mode?: string;
  url?: string;
  requestedSchema?: ElicitationSchema;
  expiresAt?: string;
}

export interface TaskNotificationItem {
  task_id: string;
  status: string;
  message: string;
  startedAt: number;
  data: unknown;
  output_file?: string;
  task_type?: string;
  summary?: string;
  tool_use_id?: string;
}

/** Scheduled task created via CronCreate (CLI /loop infrastructure).
 *  prompt/cron are best-effort: only available when the matching tool_start
 *  input is present in the same batch or snapshot. */
export interface ScheduledTask {
  id: string;
  humanSchedule: string;
  recurring: boolean;
  durable: boolean;
  prompt?: string;
  cron?: string;
  toolUseId: string;
}

// ── Store ──

export class SessionStore {
  // ── State fields ──
  phase: SessionPhase = $state("empty");
  run: TaskRun | null = $state(null);
  timeline: TimelineEntry[] = $state([]);
  streamingText: string = $state("");
  /** Accumulated thinking/reasoning text (extended thinking). Cleared on new turn. */
  thinkingText: string = $state("");
  /** Timestamp (ms) of the first thinking_delta event in the current turn. 0 = no thinking yet. */
  thinkingStartMs: number = $state(0);
  /** Timestamp (ms) when thinking ended (first message_delta after thinking). 0 = still thinking or no thinking. */
  thinkingEndMs: number = $state(0);
  tools: HookEvent[] = $state([]);
  usage: UsageState = $state({
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    cost: 0,
  });
  model: string = $state("");
  error: string = $state("");
  agent: string = $state("claude");
  authMode: string = $state("cli");

  // ── Protocol extension fields ──
  systemStatus = $state<{ status?: string } | null>(null);
  authStatus = $state<{ is_authenticating: boolean; output: string[] } | null>(null);
  hookEvents = $state<
    Array<{
      type: string;
      hook_id: string;
      data: unknown;
      request_id?: string;
      status?: "hook_pending" | "allowed" | "denied" | "cancelled";
      hook_name?: string;
      stdout?: string;
      stderr?: string;
      exit_code?: number;
    }>
  >([]);
  taskNotifications = $state<Map<string, TaskNotificationItem>>(new Map());
  /** Pending MCP elicitation prompts keyed by request_id. */
  pendingElicitations = $state<Map<string, ElicitationState>>(new Map());
  /** Current non-modal UI surface published by Pi extensions, keyed by extensionId. */
  piExtensionUiMap = $state<Record<string, import("$lib/types").PiExtensionUiState>>({});
  piExtensionUi = $state<import("$lib/types").PiExtensionUiState | null>(null);
  piExtensionHostState = $state<import("$lib/types").PiExtensionHostState>({
    statuses: {},
    widgets: {},
    pendingRequests: [],
  });
  /** Latest structured context composition from AgentCabin's built-in Pi extension. */
  contextBreakdown = $state<import("$lib/types").ContextUsageBreakdown | null>(null);
  pendingEditorText = $state<string | null>(null);
  latestNotice = $state<{ noticeType: string; message: string; timestamp: number } | null>(null);
  persistedFiles = $state<unknown[]>([]);

  /** Ralph loop state — null when no loop has been active. */
  ralphLoop = $state<{
    active: boolean;
    prompt: string;
    iteration: number;
    maxIterations: number;
    completionPromise: string | null;
    startedAt: string;
    reason: import("$lib/types").RalphCompleteReason | "interrupted" | null;
  } | null>(null);

  /** Codex Wave-3 session goal. Populated by `getGoal` on panel open and
   *  kept live by the `goal_update` reducer (from `thread/goal/updated`).
   *  null = no objective set / non-Codex session. */
  goal = $state<import("$lib/types").ThreadGoal | null>(null);

  setGoal(newGoal: import("$lib/types").ThreadGoal | null) {
    this.goal = newGoal;
  }

  /** Scheduled tasks created via CronCreate. Updated by tool_end reducer
   *  branches; replayed via snapshot. UI lives in ScheduledTasksChip.svelte. */
  scheduledTasks = $state<ScheduledTask[]>([]);

  /** CLI slash commands from session_init (session-specific, includes custom commands). */
  sessionCommands = $state<CliCommand[]>([]);
  /** Agent-neutral capabilities negotiated for the live session. */
  sessionCapabilities = $state<import("$lib/types").AgentCapabilities | null>(null);
  /** Agent-native protocol mode (e.g. Grok ACP default/plan/ask). */
  sessionMode = $state<string>("");
  sessionModes = $state<import("$lib/types").AgentSessionMode[]>([]);
  /** Model catalog announced by the live session protocol. */
  sessionModels = $state<import("$lib/types").CliModelInfo[]>([]);
  /** Pi command discovery is separate from the general session_init handshake. */
  piCommandsState = $state<"idle" | "loading" | "ready" | "failed">("idle");
  /** True only after Pi's get_commands response has completed successfully. */
  piCommandsReady = $state(false);
  piCapabilities = $state<import("$lib/types").PiFeatureCapabilities | null>(null);
  piPlanState = $state<import("$lib/types").PiPlanState>({ phase: "unavailable" });
  piGoalState = $state<import("$lib/types").PiGoalState>({ phase: "unavailable" });
  piTodoState = $state<import("$lib/types").PiTodoState>({ phases: [] });
  /** Hide a completed Todo panel after the user starts the next turn. */
  todoPanelDismissed = $state(false);
  /**
   * Agent-neutral authoritative task snapshot (e.g. Grok ACP `plan`).
   * `null` = no snapshot received; `[]` = authoritatively empty (all tasks cleared).
   * Each update is a full replacement, never a merge.
   */
  structuredTaskState = $state<import("$lib/types").StructuredTask[] | null>(null);
  /** Authoritative Work-owned goal, plan and checkpoint for the current Run. */
  workTaskState = $state<import("$lib/types/work").WorkTaskState | null>(null);
  /** Authoritative Work Context Plan for the current Run / Turn. */
  workContextPlan = $state<import("$lib/types/work").WorkContextPlan | null>(null);
  piPermissionState = $state<import("$lib/types").PiPermissionState>({
    available: false,
    mode: "accept_edits",
    pendingRequestCount: 0,
    restartRequired: false,
  });
  piQueueState = $state<import("$lib/types").PiMessageQueueState>({
    steering: [],
    followUp: [],
    pendingCount: 0,
    detailsKnown: false,
  });
  /** Messages created by the composer. Kept client-side so each row can be edited or removed. */
  queuedMessages = $state<QueuedMessage[]>([]);
  private queueFlushInFlight = false;
  piSessionTree = $state<import("$lib/types").PiSessionTreeState>({
    roots: [],
    leafId: null,
    loading: false,
  });
  /** MCP servers from session_init (per-session state). */
  mcpServers = $state<McpServerInfo[]>([]);
  /** Codex Wave-4: latest cumulative turn diff (`turn/diff/updated`). Live-only,
   *  not replayed — cleared at the start of each turn (run_state→running) and on reset. */
  turnDiff = $state<string>("");

  // ── CLI verbose fields (from session_init / usage_update) ──
  cliVersion = $state<string>("");
  permissionMode = $state<string>("");
  /** True when permissionMode was set by user/settings AND successfully persisted.
   *  Prevents session_init / snapshot from overwriting.
   *  NOT cleared by _clearContentState() unless permissionModePersistFailed is true. */
  permissionModeSetByUser = $state<boolean>(false);
  /** True when mode was switched via control protocol but settings persist failed.
   *  Cleared on _clearContentState() (new run/session), allowing session_init to
   *  re-sync from CLI's actual startup mode. */
  permissionModePersistFailed = $state<boolean>(false);
  fastModeState = $state<string>("");
  apiKeySource = $state<string>("");
  availableAgents = $state<string[]>([]);
  availableSkills = $state<string[]>([]);
  availablePlugins = $state<unknown[]>([]);
  /** Rate limit status: "allowed", "allowed_warning", "rejected" */
  rateLimitStatus = $state<string>("");
  /** Which rate limit: "five_hour", "seven_day", etc. */
  rateLimitType = $state<string>("");
  /** Utilization 0.0-1.0 */
  rateLimitUtilization = $state<number | null>(null);
  /** When the rate limit resets (epoch seconds) */
  rateLimitResetsAt = $state<number | null>(null);
  /** CLI's current working directory (updated from session_init). */
  sessionCwd = $state<string>("");
  /** CLI's available tools (updated from session_init). */
  sessionTools = $state<string[]>([]);
  /** CLI's output style (updated from session_init). */
  outputStyle = $state<string>("");
  /** Saved permission mode before plan mode (restored on ExitPlanMode). */
  previousPermissionMode = $state<string>("");
  /** Override mode after ExitPlanMode completes (user chose specific mode via approval card). */
  pendingPermissionModeOverride = $state<string | null>(null);
  /** Plan content for "clear context" restart (set before allow, consumed after tool_end). */
  pendingClearContextPlan = $state<string | null>(null);
  /** True after first session_init received for this session (gates sessionCommands exposure). */
  sessionInitReceived = $state<boolean>(false);
  /** Native slash catalogs discovered in earlier sessions, retained across new-chat reset. */
  nativeSlashCommandsByAgent = $state<Record<string, CliCommand[]>>({});
  numTurns = $state<number>(0);
  durationMs = $state<number>(0);
  /** Count of unknown event types hitting _reduce default case. */
  unknownEventCount = $state<number>(0);
  /** Count of Raw events with non-stdout/stderr source (fallback path). */
  rawFallbackCount = $state<number>(0);
  /** When true, throw on unknown/fallback events instead of counting. Test-only. */
  strictMode = false;
  /** Per-turn usage snapshots (append-only, one per usage_update event). */
  turnUsages: TurnUsage[] = $state([]);
  /** High-water mark of current-context tokens (input+cacheRead+cacheWrite) since the last
   *  compaction. contextUtilization reads this instead of the latest raw usage so that a
   *  resumed / partial / cross-process usage_update reporting a smaller snapshot can't crater
   *  the gauge (Issue #135). Reset on compact_boundary. contextHwWindow remembers the matching
   *  window so a dip event without modelUsage doesn't zero the denominator. */
  contextHwTokens: number = $state(0);
  contextHwWindow: number = $state(0);
  /** Context tokens of the LAST request in the current turn (input + cache_read +
   *  cache_creation from the final assistant message_usage). Context occupancy is a
   *  point-in-time measure, so it must use the last request — NOT the turn-summed
   *  result usage, which multiplies cache_read by the request count (#149). 0 until
   *  a message with usage arrives; the result handler falls back to summed usage then. */
  lastReqContextTokens: number = $state(0);
  /** Timestamp of the most recent compact_boundary event (0 = never). */
  lastCompactedAt: number = $state(0);
  /** Number of full compaction events in this session. */
  compactCount: number = $state(0);
  /** Number of micro-compaction events in this session. */
  microcompactCount: number = $state(0);

  /** Remote host name (if this session runs on a remote machine). */
  remoteHostName = $state<string | null>(null);
  /** Derived: true if running on a remote host via SSH. */
  get isRemote(): boolean {
    return !!this.remoteHostName;
  }

  /** Per-session platform_id — set before first message, locked after. */
  platformId = $state<string | null>(null);

  /** True while stop() is in progress — suppresses RunState error display from dying CLI. */
  private _stopping = false;

  // Internal dedup sets (not reactive — only used inside reducers)
  private _seenMessageIds = new Set<string>();
  private _seenToolIds = new Set<string>();

  /** Highest _seq processed — used for WS checkpoint on reconnect/subscribe */
  private _lastProcessedSeq = 0;

  // ── Reducer tool indexes (runtime-only, not serialized) ──
  /** tool_use_id → timeline[] index for tool entries (first-match, reducer fast-path). */
  private _toolTlIndex = new Map<string, number>();
  /** tool_use_id → tools[] (HookEvent) index (first-match, reducer fast-path). */
  private _toolHeIndex = new Map<string, number>();
  /** Runtime state: whether the current run uses chat timeline (bus-events) rendering.
   *  Only meaningful when `this.run` is set. Set by loadRun/startSession/resumeSession. */
  _useChatTimelineForRun = $state(false);

  // Generation counter: prevents stale async loadRun from overwriting state
  private _loadGen = 0;
  /** True while loadRun is replaying events — suppresses isThinking flash on session switch. */
  private _isLoadingReplay = false;
  /** Run currently being hydrated after a route switch. */
  private _loadingRunId: string | null = null;
  /** Live events received while the run metadata/history is being hydrated. */
  private _loadingEvents: BusEvent[] = [];
  private _pendingOptimisticUsers = new Map<string, Extract<TimelineEntry, { kind: "user" }>[]>();

  // Spawn timeout: fail if CLI never emits session_init
  private _spawnTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly _SPAWN_TIMEOUT_MS = 30_000;

  // Response timeout: warn if no content after sending a message
  private _responseTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly _RESPONSE_TIMEOUT_MS = 60_000;
  /** True when current error was set by the response timeout (cleared when content arrives). */
  private _isTimeoutError = false;

  /** Set phase with dev-mode transition guard. */
  private _setPhase(to: SessionPhase): void {
    assertTransition(this.phase, to);
    this.phase = to;
    // Any phase change away from spawning clears the spawn timeout
    if (to !== "spawning") {
      this._clearSpawnTimeout();
    }
    // Clear response timeout on terminal/idle phases
    if (to !== "running") {
      this._clearResponseTimeout();
    }
  }

  /** Start a timeout that fails the session if phase stays at spawning.
   *  Also kills the backend process to prevent orphan CLI processes. */
  private _startSpawnTimeout(runId: string): void {
    this._clearSpawnTimeout();
    this._spawnTimer = setTimeout(async () => {
      if (this.phase === "spawning" && this.run?.id === runId) {
        dbgWarn(
          "store",
          "spawn timeout: CLI did not respond within",
          SessionStore._SPAWN_TIMEOUT_MS,
          "ms",
        );
        this.error =
          "Session failed to start (CLI did not respond). Try again or check CLI installation.";
        // Kill the hung backend process to prevent orphans
        try {
          await api.stopSession(runId);
        } catch (e) {
          dbgWarn("store", "spawn timeout: failed to stop session:", e);
        }
        this._setPhase("failed");
        if (this.run) {
          this.run = { ...this.run, status: "failed" };
        }
      }
    }, SessionStore._SPAWN_TIMEOUT_MS);
  }

  private _clearSpawnTimeout(): void {
    if (this._spawnTimer) {
      clearTimeout(this._spawnTimer);
      this._spawnTimer = null;
    }
  }

  /** Start a timeout that warns if no response content arrives after sending a message. */
  private _startResponseTimeout(runId: string): void {
    this._clearResponseTimeout();
    this._responseTimer = setTimeout(() => {
      if (
        this.run?.id === runId &&
        this.phase === "running" &&
        !this.streamingText &&
        !this.thinkingText &&
        // Don't flag an API hang while the turn is paused waiting on the user
        // (approval card, elicitation, or a pending multiple-choice question).
        !this._waitingOnUser
      ) {
        this._isTimeoutError = true;
        this.error = "No response after 60s — still waiting for API.";
        dbgWarn("store", "response timeout: no content after 60s");
      }
    }, SessionStore._RESPONSE_TIMEOUT_MS);
  }

  private _clearResponseTimeout(): void {
    if (this._responseTimer) {
      clearTimeout(this._responseTimer);
      this._responseTimer = null;
    }
  }

  /** Clear response timeout error (only if it was set by the timeout, not a real error). */
  private _clearTimeoutError(): void {
    this._clearResponseTimeout();
    if (this._isTimeoutError) {
      this.error = "";
      this._isTimeoutError = false;
    }
  }

  /** Check if text is a known CLI slash command (not path like /home/user). */
  isKnownSlashCommand(text: string): boolean {
    const parsed = parseSlashCommand(text);
    if (!parsed?.command) return false;
    if (!this.capabilities.protocol.slashCommands) return false;
    const agent = this.run?.agent ?? this.agent;
    // A slash-containing path is not a Claude/Codex command, but Pi command names
    // are intentionally opaque and may contain punctuation such as `/` or `:`.
    if (agent !== "pi" && parsed.command.includes("/")) return false;
    const name = parsed.command.toLowerCase();
    // Use effective agent (run-level overrides store-level) — same as UI's effectiveAgent
    // Codex: only non-excluded virtual commands (no skills, CLI, or project commands)
    if (agent === "codex") {
      return getKnownVirtualNames("codex").has(name);
    }
    // Pi reports its command set through RPC session_init. Never consult the
    // static Claude CLI catalog while a Pi session is cold or starting.
    if (agent === "pi") {
      if (!this.piCommandsReady) return false;
      const cmds = this.sessionCommands;
      if (cmds.length > 0) {
        return cmds.some(
          (c) =>
            c.name.toLowerCase() === name ||
            (c.aliases ?? []).some((a) => a.toLowerCase() === name),
        );
      }
      return getKnownVirtualNames("pi").has(name);
    }
    // A live negotiated catalog is authoritative. Never fall back to a static
    // provider catalog after a session has announced its own command contract.
    if (this.sessionCapabilities) {
      return this.sessionCommands.some(
        (c) =>
          c.name.toLowerCase() === name || (c.aliases ?? []).some((a) => a.toLowerCase() === name),
      );
    }
    // Claude: skills + session/CLI commands
    if (this.availableSkills.some((s) => s.toLowerCase() === name)) return true;
    // Check session commands (available after session_init) or static CLI info
    const cmds = this.sessionCommands.length > 0 ? this.sessionCommands : getCliCommands();
    if (cmds.length > 0) {
      return cmds.some(
        (c) =>
          c.name.toLowerCase() === name || (c.aliases ?? []).some((a) => a.toLowerCase() === name),
      );
    }
    // Cold start: no command list available yet — trust the regex pattern.
    // The (?:\s|$) boundary already filters out paths like /home/user.
    // False positive risk (e.g. "/hello") only skips 60s timeout, acceptable.
    dbg("store", "isKnownSlashCommand cold-start fallback", { name });
    return true;
  }

  // ── Derived getters ──

  get isRunning(): boolean {
    return ACTIVE_PHASES.includes(this.phase);
  }

  get isIdle(): boolean {
    return this.phase === "idle";
  }

  get sessionAlive(): boolean {
    return SESSION_ALIVE_PHASES.includes(this.phase);
  }

  get canSend(): boolean {
    return ["empty", "ready", "idle"].includes(this.phase);
  }

  get activeToolName(): string {
    // Stream mode: scan timeline (top-level only, equivalent to previous HookEvent behavior)
    if (this.useStreamSession) {
      for (let i = this.timeline.length - 1; i >= 0; i--) {
        const e = this.timeline[i];
        if (e.kind === "tool" && e.tool.status === "running") return e.tool.tool_name;
      }
      return "";
    }
    // Pipe/PTY fallback: use HookEvent tools array
    return this.tools.filter((e) => e.status === "running").at(-1)?.tool_name ?? "";
  }

  /**
   * The most recent TodoWrite checklist (top-level timeline only, matching the
   * `/todos` command). Empty when no TodoWrite has run — including all Codex
   * sessions, since Codex's exec protocol never emits TodoWrite.
   */
  get latestTodos(): TodoItem[] {
    for (let i = this.timeline.length - 1; i >= 0; i--) {
      const e = this.timeline[i];
      if (
        e.kind === "tool" &&
        e.tool.tool_name === "TodoWrite" &&
        e.tool.status === "success" &&
        e.tool.tool_use_result != null &&
        typeof e.tool.tool_use_result === "object" &&
        Array.isArray((e.tool.tool_use_result as Record<string, unknown>).newTodos)
      ) {
        return (e.tool.tool_use_result as unknown as { newTodos: TodoItem[] }).newTodos;
      }
    }
    return [];
  }

  /**
   * Current task list aggregated from the Tasks system (TaskCreate/TaskUpdate),
   * which is incremental — there is no single full-snapshot event. We replay the
   * timeline in order: each TaskCreate adds a row (pending), each TaskUpdate mutates
   * its status by taskId; a "deleted" status removes the row. Creation order is kept.
   */
  get taskList(): PanelTask[] {
    const byId = new Map<string, PanelTask>();
    const order: string[] = [];
    for (const e of this.timeline) {
      if (e.kind !== "tool" || e.tool.status !== "success") continue;
      const r = e.tool.tool_use_result as Record<string, unknown> | undefined;
      if (r == null || typeof r !== "object") continue;
      if (e.tool.tool_name === "TaskCreate") {
        const task = r.task as { id?: string; subject?: string } | undefined;
        if (task?.id) {
          if (!byId.has(task.id)) order.push(task.id);
          byId.set(task.id, { id: task.id, text: task.subject ?? "", status: "pending" });
        }
      } else if (e.tool.tool_name === "TaskUpdate") {
        const taskId = r.taskId as string | undefined;
        const to = (r.statusChange as { to?: string } | undefined)?.to;
        if (taskId && to && byId.has(taskId)) {
          if (to === "deleted") byId.delete(taskId);
          else byId.get(taskId)!.status = to as PanelTask["status"];
        }
      }
    }
    return order.map((id) => byId.get(id)).filter((t): t is PanelTask => t != null);
  }

  /**
   * Unified source for the TodoPanel and /todos: prefer the Tasks system, fall back
   * to legacy TodoWrite snapshots. Empty for Codex (neither tool exists there).
   */
  get panelTasks(): PanelTask[] {
    const tasks = this.taskList;
    if (tasks.length > 0) return tasks;
    return this.latestTodos.map((td, i) => ({
      id: String(i),
      text: td.content,
      status: td.status,
    }));
  }

  get todoPanelVisible(): boolean {
    if (this.todoPanelDismissed) return false;
    return (
      this.panelTasks.length > 0 || this.piTodoState.phases.some((phase) => phase.tasks.length > 0)
    );
  }

  private _allTodoTasksCompleted(): boolean {
    const tasks = this.panelTasks;
    if (tasks.length > 0) return tasks.every((task) => task.status === "completed");

    const piTasks = this.piTodoState.phases.flatMap((phase) => phase.tasks);
    return (
      piTasks.length > 0 &&
      piTasks.every((task) => task.status === "completed" || task.status === "cancelled")
    );
  }

  /** Hide a completed checklist when the user starts a new turn. */
  private _dismissCompletedTodoPanel(): void {
    if (this._allTodoTasksCompleted()) this.todoPanelDismissed = true;
  }

  /** A new task update starts a fresh visible checklist. */
  private _showTodoPanel(): void {
    this.todoPanelDismissed = false;
  }

  /** Recursive walk: short-circuit check for any permission_prompt in timeline + subTimelines. */
  private _hasPermission(predicate?: (t: BusToolItem) => boolean): boolean {
    function walk(entries: TimelineEntry[]): boolean {
      for (const entry of entries) {
        if (entry.kind !== "tool") continue;
        if (
          entry.tool.status === "permission_prompt" &&
          entry.tool.permission_request_id &&
          (!predicate || predicate(entry.tool))
        )
          return true;
        if (entry.subTimeline && walk(entry.subTimeline)) return true;
      }
      return false;
    }
    return walk(this.timeline);
  }

  /** Whether any permission prompt is pending user approval (recursive, includes subTimelines). */
  get hasPendingPermission(): boolean {
    return this._hasPermission();
  }

  /** Whether any MCP elicitation prompt is pending user response. */
  get hasElicitation(): boolean {
    return this.pendingElicitations.size > 0;
  }

  /** True when the turn is legitimately paused waiting on the user (permission approval,
   *  MCP elicitation, or an AskUserQuestion/multiple-choice selection) rather than the API.
   *  Used to suppress the "no response" timeout during these waits. */
  private get _waitingOnUser(): boolean {
    if (this.hasPendingPermission || this.hasElicitation) return true;
    const walk = (entries: TimelineEntry[]): boolean => {
      for (const e of entries) {
        if (e.kind !== "tool") continue;
        if (e.tool.status === "ask_pending") return true;
        if (e.subTimeline && walk(e.subTimeline)) return true;
      }
      return false;
    };
    return walk(this.timeline);
  }

  /** Whether an inline-only permission (AskUserQuestion / ExitPlanMode) is pending. */
  get hasInlinePermission(): boolean {
    return this._hasPermission(
      (t) => t.tool_name === "AskUserQuestion" || t.tool_name === "ExitPlanMode",
    );
  }

  /** Pending generic tool permission prompts (recursive, excludes AskUserQuestion/ExitPlanMode). */
  get pendingToolPermissions(): Array<{ tool: BusToolItem; requestId: string }> {
    const map = new Map<string, BusToolItem>();
    function walk(entries: TimelineEntry[]) {
      for (const entry of entries) {
        if (entry.kind !== "tool") continue;
        if (
          entry.tool.status === "permission_prompt" &&
          entry.tool.permission_request_id &&
          entry.tool.tool_name !== "AskUserQuestion" &&
          entry.tool.tool_name !== "ExitPlanMode"
        ) {
          const rid = entry.tool.permission_request_id;
          map.delete(rid);
          map.set(rid, entry.tool);
        }
        if (entry.subTimeline) walk(entry.subTimeline);
      }
    }
    walk(this.timeline);
    return Array.from(map, ([requestId, tool]) => ({ tool, requestId }));
  }

  get isThinking(): boolean {
    if (!this.isRunning || this.streamingText) return false;
    // During loadRun replay, phase is set to "running" before events are loaded.
    // Without this guard, isThinking flashes true on session switch (especially on
    // Windows where replay is slower). Suppress during the loading window.
    if (this._isLoadingReplay) return false;
    return !this.hasPendingPermission && !this.hasElicitation;
  }

  /** isRunning but not blocked on a permission prompt or elicitation.
   *  Used for UI elements (stop button, spinner) that should hide during approval. */
  get isActivelyRunning(): boolean {
    return this.isRunning && !this.hasPendingPermission && !this.hasElicitation;
  }

  /** Duration of extended thinking in seconds (0 if no thinking happened). */
  get thinkingDurationSec(): number {
    if (!this.thinkingStartMs) return 0;
    const end = this.thinkingEndMs || Date.now();
    return Math.floor((end - this.thinkingStartMs) / 1000);
  }

  get totalTokens(): number {
    return (
      this.usage.inputTokens +
      this.usage.outputTokens +
      this.usage.cacheReadTokens +
      this.usage.cacheWriteTokens
    );
  }

  get contextWindow(): number {
    // Prefer the latest event's window; fall back to the high-water window so a dip
    // event without modelUsage (resume/synth) doesn't zero the denominator. #135
    if (this.usage.modelUsage) {
      let max = 0;
      for (const e of Object.values(this.usage.modelUsage)) {
        if (e.context_window && e.context_window > max) max = e.context_window;
      }
      if (max > 0) return max;
    }
    return this.contextHwWindow;
  }

  get contextUtilization(): number {
    // Fill % = current-context tokens (input + cache_read + cache_write of the latest main
    // turn) / context window. We read the high-water mark since last compaction, NOT the
    // latest raw usage: a resumed/partial/cross-process usage_update can report a much smaller
    // snapshot (different process, cold cache, error result) that must not crater the gauge. #135
    const cw = this.contextWindow;
    if (cw <= 0 || this.contextHwTokens <= 0) return 0;
    return Math.min(this.contextHwTokens / cw, 1);
  }

  /** Conversation turn count = user messages in the timeline. This is the user-facing turn
   *  number; it matches the conversation list. Distinct from CLI `num_turns`, which counts
   *  per-prompt internal API steps and resets across process re-invocations (Issue #135). */
  get userTurnCount(): number {
    return this.timeline.filter((e) => e.kind === "user").length;
  }

  /** Current context size in tokens (high-water since last compaction). */
  get contextTokens(): number {
    return this.contextHwTokens;
  }

  get contextWarningLevel(): "none" | "moderate" | "high" | "critical" {
    const u = this.contextUtilization;
    if (u >= 0.9) return "critical";
    if (u >= 0.75) return "high";
    if (u >= 0.5) return "moderate";
    return "none";
  }

  /** Background tasks that are still running/started (not completed/failed). */
  get activeBackgroundTasks(): TaskNotificationItem[] {
    const active: TaskNotificationItem[] = [];
    for (const item of this.taskNotifications.values()) {
      if (item.status !== "completed" && item.status !== "failed" && item.status !== "error") {
        active.push(item);
      }
    }
    return active;
  }

  /** Whether there are any background tasks (active or recently completed). */
  get hasBackgroundTasks(): boolean {
    return this.taskNotifications.size > 0;
  }

  get effectiveCwd(): string {
    return this.sessionCwd || this.run?.cwd || "";
  }

  get isApiMode(): boolean {
    return this.run ? this.run.auth_mode === "api" : this.authMode === "api";
  }

  get useStreamSession(): boolean {
    // Run-level: the persisted execution path is authoritative.
    if (this.run) return this.run.execution_path === "session_actor";
    // Pre-run: infer only when the provider owns the path. "backend" waits for
    // TaskRun.execution_path, which is authoritative after run creation.
    return this.capabilities.execution.preRunExecutionPath === "session_actor";
  }

  /** Whether to use chat timeline rendering (bus-events) vs terminal.
   *  Without a run: derives from the execution profile. With a run: from runtime state.
   *  useStreamSession always implies timeline (session_actor = bus events). */
  get useChatTimeline(): boolean {
    if (!this.run) {
      // No run: infer from agent capabilities
      return this.useStreamSession || this.capabilities.execution.busEvents;
    }
    // session_actor always uses timeline; for pipe_exec, check runtime flag
    return this.useStreamSession || this._useChatTimelineForRun;
  }

  /** Effective agent contract: static baseline overlaid by live negotiation. */
  get capabilities(): EffectiveAgentCapabilities {
    return getAgentCapabilities(this.run?.agent ?? this.agent, this.sessionCapabilities);
  }

  /** Whether Plan is controlled by the agent-native session-mode protocol. */
  get usesSessionPlanMode(): boolean {
    return (
      this.capabilities.protocol.planMode && this.sessionModes.some((mode) => mode.id === "plan")
    );
  }

  get planModeActive(): boolean {
    return this.usesSessionPlanMode ? this.sessionMode === "plan" : this.permissionMode === "plan";
  }

  /** CLI-reported authentication source label. Empty before session_init. */
  get authSourceLabel(): string {
    if (!this.apiKeySource) return "";
    // When auth_mode="api", the App may inject ANTHROPIC_AUTH_TOKEN (Bearer auth)
    // and explicitly clear ANTHROPIC_API_KEY. CLI only tracks ANTHROPIC_API_KEY,
    // so it reports "none" even though auth works via ANTHROPIC_AUTH_TOKEN.
    if (this.apiKeySource === "none" && this.isApiMode) return "API Key";
    const map: Record<string, string> = {
      "/login managed key": "Login Key",
      ANTHROPIC_API_KEY: "API Key",
      apiKeyHelper: "Key Helper",
      none: "No Auth",
    };
    return map[this.apiKeySource] ?? this.apiKeySource;
  }

  /** Authentication source category for badge coloring. */
  get authSourceCategory(): string {
    if (!this.apiKeySource) return "unknown";
    if (this.apiKeySource === "/login managed key") return "login";
    if (this.apiKeySource === "ANTHROPIC_API_KEY") return "env_key";
    // Same ANTHROPIC_AUTH_TOKEN case — treat as env_key (blue badge)
    if (this.apiKeySource === "none" && this.isApiMode) return "env_key";
    if (this.apiKeySource === "none") return "none";
    return "other";
  }

  // ── Reducer index helpers ──

  /** Append a timeline entry and update tool index if applicable.
   *  Index uses first-match semantics (matching findIndex behavior) — only set if not already present. */
  private _pushTimeline(ctx: ReduceCtx | null, entry: TimelineEntry): void {
    if (ctx) {
      ctx.tl.push(entry);
      if (entry.kind === "tool" && !ctx.toolTlIndex.has(entry.id)) {
        ctx.toolTlIndex.set(entry.id, ctx.tl.length - 1);
      }
    } else {
      this.timeline = [...this.timeline, entry];
      if (entry.kind === "tool" && !this._toolTlIndex.has(entry.id)) {
        this._toolTlIndex.set(entry.id, this.timeline.length - 1);
      }
    }
  }

  /** Push an optimistic user message to the timeline (deduped by content in _reduce). */
  pushOptimisticUser(content: string, attachments?: Attachment[]): string {
    const id = uuid();
    this._pushTimeline(null, {
      kind: "user",
      id,
      anchorId: id,
      content,
      ts: new Date().toISOString(),
      ...(attachments && attachments.length > 0
        ? { attachments: timelineAttachments(attachments) }
        : {}),
    });
    return id;
  }

  /** Private alias for backward compatibility. */
  private _pushOptimisticUser(content: string, attachments?: Attachment[]): string {
    return this.pushOptimisticUser(content, attachments);
  }

  /** Remove an optimistic user entry when the backend rejected the send. */
  removeOptimisticUser(id: string): void {
    this.timeline = this.timeline.filter((entry) => entry.id !== id);
  }

  /** Private alias for backward compatibility. */
  private _removeOptimisticUser(id: string): void {
    this.removeOptimisticUser(id);
  }

  /** Append a hook event entry and update tool index if applicable.
   *  Index uses first-match semantics — only set if not already present. */
  private _pushHookEntry(ctx: ReduceCtx | null, heEntry: HookEvent): void {
    const toolUseId = (heEntry as Record<string, unknown>).tool_use_id as string | undefined;
    if (ctx) {
      ctx.he.push(heEntry);
      if (toolUseId && !ctx.toolHeIndex.has(toolUseId))
        ctx.toolHeIndex.set(toolUseId, ctx.he.length - 1);
    } else {
      this.tools = [...this.tools, heEntry];
      if (toolUseId && !this._toolHeIndex.has(toolUseId))
        this._toolHeIndex.set(toolUseId, this.tools.length - 1);
    }
  }

  /** Find tool timeline entry by tool_use_id. Map fast-path + findIndex fallback. */
  private _findToolIdx(ctx: ReduceCtx | null, toolUseId: string): number {
    const tl = ctx ? ctx.tl : this.timeline;
    const idx = ctx ? ctx.toolTlIndex.get(toolUseId) : this._toolTlIndex.get(toolUseId);
    // Fast path: Map hit + validation
    if (idx !== undefined && tl[idx]?.kind === "tool" && tl[idx].id === toolUseId) return idx;
    // Fallback: linear scan (covers stale/missing index entries)
    const fallback = tl.findIndex((e) => e.kind === "tool" && e.id === toolUseId);
    if (fallback >= 0) {
      dbgWarn("store", "_findToolIdx: index miss, found via scan", {
        toolUseId,
        mapIdx: idx,
        scanIdx: fallback,
      });
    }
    return fallback;
  }

  /** Simple id-only lookup for hook events. Map fast-path + findIndex fallback. */
  private _findHeIdx(ctx: ReduceCtx | null, toolUseId: string): number {
    const he = ctx ? ctx.he : this.tools;
    const idx = ctx ? ctx.toolHeIndex.get(toolUseId) : this._toolHeIndex.get(toolUseId);
    if (
      idx !== undefined &&
      he[idx] &&
      (he[idx] as Record<string, unknown>).tool_use_id === toolUseId
    )
      return idx;
    const fallback = he.findIndex((e) => (e as Record<string, unknown>).tool_use_id === toolUseId);
    if (fallback >= 0) {
      dbgWarn("store", "_findHeIdx: index miss, found via scan", {
        toolUseId,
        mapIdx: idx,
        scanIdx: fallback,
      });
    }
    return fallback;
  }

  /** Status-aware hook event lookup: Map fast-path + status validation + scan fallback.
   *  Used by user_message and tool_end which filter by status==="running". */
  private _findHeIdxByStatus(ctx: ReduceCtx | null, toolUseId: string, status: string): number {
    const he = ctx ? ctx.he : this.tools;
    const idx = ctx ? ctx.toolHeIndex.get(toolUseId) : this._toolHeIndex.get(toolUseId);
    // Fast path: Map hit + status match
    if (
      idx !== undefined &&
      he[idx] &&
      (he[idx] as Record<string, unknown>).tool_use_id === toolUseId &&
      he[idx].status === status
    ) {
      return idx;
    }
    // Fallback: linear scan (covers status mismatch or stale index)
    return he.findIndex(
      (e) => (e as Record<string, unknown>).tool_use_id === toolUseId && e.status === status,
    );
  }

  // ── SubTimeline helpers (subagent routing) ──

  /** Find the parent tool entry in timeline by tool_use_id; return index or -1.
   *  Uses _findToolIdx for Map fast-path with findIndex fallback. */
  private _findParentToolIdx(ctx: ReduceCtx | null, parentToolUseId: string): number {
    return this._findToolIdx(ctx, parentToolUseId);
  }

  /** Lookup the tool_start input for a given tool_use_id. Used by tool_end
   *  branches (e.g. CronCreate) to join with the originating input. Reads from
   *  the timeline — same-batch tool_start is committed by _pushTimeline before
   *  tool_end runs. */
  private _lookupToolStartInput(
    ctx: ReduceCtx | null,
    toolUseId: string,
  ): Record<string, unknown> | undefined {
    const tl = ctx?.tl ?? this.timeline;
    const idx = this._findToolIdx(ctx, toolUseId);
    if (idx >= 0) {
      const e = tl[idx];
      if (e.kind === "tool") return e.tool.input as Record<string, unknown> | undefined;
    }
    return undefined;
  }

  /** Search ALL subTimelines (one level deep) for a tool with the given id.
   *  Used when parent_tool_use_id is missing but the tool exists in a subTimeline.
   *  Returns true if found and updated; false if not found. */
  private _updateToolInAnySubTimeline(
    toolUseId: string,
    updater: (old: BusToolItem) => BusToolItem,
    ctx: ReduceCtx | null,
  ): boolean {
    const tl = ctx ? ctx.tl : this.timeline;
    for (let pIdx = 0; pIdx < tl.length; pIdx++) {
      const entry = tl[pIdx];
      if (entry.kind !== "tool" || !entry.subTimeline) continue;
      const sub = entry.subTimeline;
      const cIdx = sub.findIndex((e) => e.kind === "tool" && e.id === toolUseId);
      if (cIdx < 0) continue;
      // Found in this parent's subTimeline — update it
      const oldChild = sub[cIdx] as Extract<TimelineEntry, { kind: "tool" }>;
      const newSub = [...sub];
      newSub[cIdx] = { ...oldChild, tool: updater(oldChild.tool) };
      const updatedParent: TimelineEntry = { ...entry, subTimeline: newSub };
      if (ctx) {
        ctx.tl[pIdx] = updatedParent;
      } else {
        const u = [...this.timeline];
        u[pIdx] = updatedParent;
        this.timeline = u;
      }
      dbg("store", "found tool in subTimeline (missing parent_tool_use_id)", {
        tool: toolUseId,
        parent: entry.id,
      });
      return true;
    }
    return false;
  }

  /** Append an entry to a parent tool's subTimeline. */
  private _appendToSubTimeline(
    tl: TimelineEntry[],
    parentIdx: number,
    entry: TimelineEntry,
    ctx: ReduceCtx | null,
  ): void {
    const old = tl[parentIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const updated: TimelineEntry = { ...old, subTimeline: [...(old.subTimeline ?? []), entry] };
    if (ctx) {
      ctx.tl[parentIdx] = updated;
    } else {
      const u = [...this.timeline];
      u[parentIdx] = updated;
      this.timeline = u;
    }
  }

  /** Update a tool entry inside a parent tool's subTimeline (3-level immutable update). */
  private _updateSubTimelineTool(
    parentToolUseId: string,
    childToolUseId: string,
    updater: (old: BusToolItem) => BusToolItem,
    ctx: ReduceCtx | null,
  ): boolean {
    const tl = ctx ? ctx.tl : this.timeline;
    const pIdx = this._findParentToolIdx(ctx, parentToolUseId);
    if (pIdx < 0) return false;
    const parent = tl[pIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const sub = parent.subTimeline ?? [];
    const cIdx = sub.findIndex((e) => e.kind === "tool" && e.id === childToolUseId);
    if (cIdx < 0) return false;
    const oldChild = sub[cIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const newSub = [...sub];
    newSub[cIdx] = { ...oldChild, tool: updater(oldChild.tool) };
    const updatedParent: TimelineEntry = { ...parent, subTimeline: newSub };
    if (ctx) {
      ctx.tl[pIdx] = updatedParent;
    } else {
      const u = [...this.timeline];
      u[pIdx] = updatedParent;
      this.timeline = u;
    }
    return true;
  }

  /** Append/update a synthetic assistant entry in a parent tool's subTimeline for streaming deltas.
   *  Single-active-stream per parent: synthetic ID = `__sub_stream_{parentToolUseId}`.
   *  If the entry doesn't exist yet, creates it; otherwise appends to content or thinkingText. */
  private _appendSubTimelineStreamingDelta(
    parentToolUseId: string,
    field: "content" | "thinkingText",
    text: string,
    ctx: ReduceCtx | null,
  ): void {
    const tl = ctx ? ctx.tl : this.timeline;
    const pIdx = this._findParentToolIdx(ctx, parentToolUseId);
    if (pIdx < 0) return;
    const parent = tl[pIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const sub = parent.subTimeline ?? [];
    const syntheticId = `__sub_stream_${parentToolUseId}`;
    const sIdx = sub.findIndex((e) => e.kind === "assistant" && e.id === syntheticId);
    let newSub: TimelineEntry[];
    if (sIdx >= 0) {
      // Update existing synthetic entry
      const old = sub[sIdx] as Extract<TimelineEntry, { kind: "assistant" }>;
      newSub = [...sub];
      if (field === "content") {
        newSub[sIdx] = { ...old, content: old.content + text };
      } else {
        newSub[sIdx] = { ...old, thinkingText: (old.thinkingText ?? "") + text };
      }
    } else {
      // Create new synthetic entry
      const entry: TimelineEntry =
        field === "content"
          ? {
              kind: "assistant",
              id: syntheticId,
              anchorId: syntheticId,
              content: text,
              ts: new Date().toISOString(),
            }
          : {
              kind: "assistant",
              id: syntheticId,
              anchorId: syntheticId,
              content: "",
              thinkingText: text,
              ts: new Date().toISOString(),
            };
      newSub = [...sub, entry];
    }
    const updatedParent: TimelineEntry = { ...parent, subTimeline: newSub };
    if (ctx) {
      ctx.tl[pIdx] = updatedParent;
    } else {
      const u = [...this.timeline];
      u[pIdx] = updatedParent;
      this.timeline = u;
    }
  }

  /** Extract thinkingText from a parent tool's synthetic streaming entry (before removal). */
  private _extractSubTimelineThinking(
    parentToolUseId: string,
    ctx: ReduceCtx | null,
  ): string | undefined {
    const tl = ctx ? ctx.tl : this.timeline;
    const pIdx = this._findParentToolIdx(ctx, parentToolUseId);
    if (pIdx < 0) return undefined;
    const parent = tl[pIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const sub = parent.subTimeline ?? [];
    const syntheticId = `__sub_stream_${parentToolUseId}`;
    const entry = sub.find((e) => e.kind === "assistant" && e.id === syntheticId);
    if (!entry || entry.kind !== "assistant") return undefined;
    return entry.thinkingText;
  }

  /** Remove the synthetic streaming entry from a parent tool's subTimeline (called on message_complete). */
  private _removeSubTimelineStreamingEntry(parentToolUseId: string, ctx: ReduceCtx | null): void {
    const tl = ctx ? ctx.tl : this.timeline;
    const pIdx = this._findParentToolIdx(ctx, parentToolUseId);
    if (pIdx < 0) return;
    const parent = tl[pIdx] as Extract<TimelineEntry, { kind: "tool" }>;
    const sub = parent.subTimeline ?? [];
    const syntheticId = `__sub_stream_${parentToolUseId}`;
    const sIdx = sub.findIndex((e) => e.kind === "assistant" && e.id === syntheticId);
    if (sIdx < 0) return;
    const newSub = [...sub];
    newSub.splice(sIdx, 1);
    const updatedParent: TimelineEntry = { ...parent, subTimeline: newSub };
    if (ctx) {
      ctx.tl[pIdx] = updatedParent;
    } else {
      const u = [...this.timeline];
      u[pIdx] = updatedParent;
      this.timeline = u;
    }
  }

  /** Accumulate partial JSON and try to parse. Returns merged tool fields. */
  private static _accumulateJsonInput(
    tool: Record<string, unknown>,
    partialJson: string,
  ): { input?: Record<string, unknown>; _inputJsonAccum: string } {
    const prevAccum = ((tool as Record<string, unknown>)._inputJsonAccum as string) ?? "";
    const newAccum = prevAccum + partialJson;
    let parsed: Record<string, unknown> | null = null;
    try {
      parsed = JSON.parse(newAccum);
    } catch {
      /* incomplete JSON */
    }
    return { ...(parsed ? { input: parsed } : {}), _inputJsonAccum: newAccum };
  }

  /** Route tool_input_delta to a child tool inside a parent's subTimeline. */
  private _updateSubTimelineToolInput(
    parentToolUseId: string,
    childToolUseId: string,
    partialJson: string,
    ctx: ReduceCtx | null,
  ): void {
    this._updateSubTimelineTool(
      parentToolUseId,
      childToolUseId,
      (t) => {
        const accum = SessionStore._accumulateJsonInput(t as Record<string, unknown>, partialJson);
        return { ...t, ...accum } as typeof t;
      },
      ctx,
    );
  }

  // ── Reducers ──

  /** Apply a single live bus event (mutates $state directly). */
  applyEvent(ev: BusEvent): void {
    // The chat route subscribes before loadRun() has fetched run metadata. Keep
    // events for that target instead of dropping them while `this.run` still
    // points at the previous chat (or is null after reset()).
    if (this._loadingRunId === ev.run_id) {
      this._loadingEvents.push(ev);
      dbg("store", "buffer event during load", ev.type, "run_id=", ev.run_id);
      return;
    }
    // Guard: drop events for a run we're no longer viewing
    if (!this.run || ev.run_id !== this.run.id) {
      dbg("store", "drop stale event", ev.type, "run_id=", ev.run_id, "current=", this.run?.id);
      return;
    }
    // Track WS sequence checkpoint — skip already-processed events (dedup)
    const evSeq = ((ev as Record<string, unknown>)._seq as number) ?? 0;
    if (evSeq > 0) {
      if (evSeq <= this._lastProcessedSeq) {
        dbg(
          "store",
          "drop duplicate event",
          ev.type,
          "seq=",
          evSeq,
          "last=",
          this._lastProcessedSeq,
        );
        return;
      }
      this._lastProcessedSeq = evSeq;
    }
    this._reduce(ev, null);
  }

  /** Build a reducer context snapshotted from current store state. Caller drives the
   * reduce loop and then passes the ctx to `_commitReduceCtx`. */
  private _createReduceCtx(): ReduceCtx {
    const batchTlIndex = new Map<string, number>();
    for (let i = 0; i < this.timeline.length; i++) {
      const e = this.timeline[i];
      if (e.kind === "tool" && !batchTlIndex.has(e.id)) batchTlIndex.set(e.id, i);
    }
    const batchHeIndex = new Map<string, number>();
    for (let i = 0; i < this.tools.length; i++) {
      const tid = (this.tools[i] as Record<string, unknown>).tool_use_id as string | undefined;
      if (tid && !batchHeIndex.has(tid)) batchHeIndex.set(tid, i);
    }
    return {
      tl: [...this.timeline],
      he: [...this.tools],
      streamText: this.streamingText,
      thinkingText: this.thinkingText,
      thinkingStartMs: this.thinkingStartMs,
      thinkingEndMs: this.thinkingEndMs,
      model: this.model,
      phase: this.phase,
      usage: { ...this.usage },
      error: this.error,
      seenMessageIds: new Set(this._seenMessageIds),
      seenToolIds: new Set(this._seenToolIds),
      runStatus: null,
      sessionId: null,
      isStream: this.useChatTimeline,
      turnUsages: [...this.turnUsages],
      contextHwTokens: this.contextHwTokens,
      contextHwWindow: this.contextHwWindow,
      lastReqContextTokens: this.lastReqContextTokens,
      toolTlIndex: batchTlIndex,
      toolHeIndex: batchHeIndex,
      scheduledTasks: [...this.scheduledTasks],
    };
  }

  /** Apply the reducer ctx to live store state. Runs synchronously — no awaits inside,
   * so a stale check before this call is sufficient to skip publishing. */
  private _commitReduceCtx(ctx: ReduceCtx, replayOnly: boolean): void {
    // If the session ended, resolve any leftover incomplete tools
    // (running, ask_pending, permission_prompt — these will never receive results)
    const runStatus = this.run?.status;
    const sessionDead =
      runStatus === "stopped" || runStatus === "completed" || runStatus === "failed";
    if (sessionDead) {
      const staleStatuses = new Set(["running", "ask_pending", "permission_prompt"]);
      const finalizeTools = (tl: TimelineEntry[]): TimelineEntry[] => {
        let changed = false;
        const result = tl.map((e) => {
          if (e.kind !== "tool") return e;
          const newSub = e.subTimeline ? finalizeTools(e.subTimeline) : e.subTimeline;
          const needsFinalize = staleStatuses.has(e.tool.status);
          if (!needsFinalize && newSub === e.subTimeline) return e;
          changed = true;
          return {
            ...e,
            ...(newSub !== e.subTimeline ? { subTimeline: newSub } : {}),
            tool: needsFinalize
              ? { ...e.tool, status: "error" as const, output: { error: "Session ended" } }
              : e.tool,
          };
        });
        return changed ? result : tl;
      };
      ctx.tl = finalizeTools(ctx.tl);

      // If a terminal session has uncompleted streaming text (e.g. app restarted
      // or turn completed without message_complete event), seal it as a permanent
      // assistant entry so historical replies are never lost or blank on replay.
      if (ctx.streamText.trim()) {
        const entryId = uuid();
        const entry: TimelineEntry = {
          kind: "assistant",
          id: entryId,
          anchorId: entryId,
          content: ctx.streamText,
          ts: new Date().toISOString(),
          ...(ctx.model ? { model: ctx.model } : {}),
          ...(ctx.thinkingText ? { thinkingText: ctx.thinkingText } : {}),
        };
        ctx.tl.push(entry);
        ctx.streamText = "";
        ctx.thinkingText = "";
      }
    }

    this.timeline = ctx.tl;
    this.tools = ctx.he;
    this.streamingText = ctx.streamText;
    this.thinkingText = ctx.thinkingText;
    this.model = ctx.model;
    this.usage = ctx.usage;
    this.turnUsages = ctx.turnUsages;
    this.contextHwTokens = ctx.contextHwTokens;
    this.contextHwWindow = ctx.contextHwWindow;
    this.lastReqContextTokens = ctx.lastReqContextTokens;
    this._seenMessageIds = ctx.seenMessageIds;
    this._seenToolIds = ctx.seenToolIds;
    this._toolTlIndex = ctx.toolTlIndex;
    this._toolHeIndex = ctx.toolHeIndex;
    this.scheduledTasks = ctx.scheduledTasks;
    if (!replayOnly) {
      this._setPhase(ctx.phase);
      this.error = ctx.error;
      if ((ctx.runStatus || ctx.sessionId) && this.run) {
        const updates: Partial<TaskRun> = {};
        if (ctx.runStatus) updates.status = ctx.runStatus as RunStatus;
        if (ctx.sessionId) {
          dbg("store", "batch: updating session_id", {
            old: this.run.session_id,
            new: ctx.sessionId,
          });
          updates.session_id = ctx.sessionId;
        }
        this.run = { ...this.run, ...updates };
      }
    }

    if (this.ralphLoop?.active && replayOnly) {
      this.ralphLoop = { ...this.ralphLoop, active: false, reason: "interrupted" };
      dbg("store", "ralph loop marked interrupted after replay");
    }

    // Live bus events are commonly committed as one batch (message_complete,
    // usage_update, run_state: idle). The reducer uses a temporary context for
    // that batch, so the idle reducer cannot flush the composer queue itself.
    // Flush after the batch has been committed to the authoritative store state.
    if (!replayOnly && ctx.phase === "idle") {
      void this.flushQueuedMessages();
    }
  }

  /** Apply a batch of events (e.g. during loadRun replay). Avoids N reactive updates.
   *  opts.replayOnly=true skips phase and error assignments (used during resume).
   *  Returns elapsed milliseconds. Synchronous — for chunked replay use
   *  `applyEventBatchAsync` which yields between chunks. */
  applyEventBatch(events: BusEvent[], opts?: { replayOnly?: boolean }): number {
    const t0 = performance.now();
    const replayOnly = opts?.replayOnly ?? false;
    const ctx = this._createReduceCtx();
    const checkpoint = this._lastProcessedSeq;
    const seenBatchSeqs = new Set<number>();
    for (const ev of events) {
      const evSeq = ((ev as Record<string, unknown>)._seq as number) ?? 0;
      if (evSeq > 0) {
        // A reconnect/replay can put an already-delivered event in the same
        // microbatch as its live copy. Delta events have no message ID, so the
        // sequence checkpoint is the only safe dedup key for them.
        if (evSeq <= checkpoint || seenBatchSeqs.has(evSeq)) continue;
        seenBatchSeqs.add(evSeq);
        this._lastProcessedSeq = Math.max(this._lastProcessedSeq, evSeq);
      }
      this._reduce(ev, ctx, replayOnly);
    }
    const cpuMs = performance.now() - t0;
    dbg(
      "store",
      `applyEventBatch:sync: ${events.length} events in ${cpuMs.toFixed(1)}ms cpu, timeline=${ctx.tl.length}`,
    );
    this._commitReduceCtx(ctx, replayOnly);
    return cpuMs;
  }

  /**
   * Async variant for replay paths (loadRun / resume / fork / idle catchup).
   *
   * Always reduces into a local ctx + `localSeq`; on stale abort neither
   * `this.timeline` nor `this._lastProcessedSeq` is touched (so a quick run
   * switch can't pollute the next subscribe's seq checkpoint).
   *
   * Large batches (> CHUNK_THRESHOLD) yield between chunks to keep the main
   * thread responsive. Small batches use the same code path but skip the
   * yields — slightly more overhead than the public sync `applyEventBatch`,
   * but worth it for the stale-safety guarantee.
   *
   * Returns elapsed ms (wall-time when chunked, cpu-time otherwise), or `null`
   * if `opts.isStale()` reported stale — caller must skip subsequent
   * subscribe/snapshot work in that case.
   */
  async applyEventBatchAsync(
    events: BusEvent[],
    opts: { replayOnly?: boolean; isStale?: () => boolean } = {},
  ): Promise<number | null> {
    if (opts.isStale?.()) return null;
    const t0 = performance.now();
    const isStale = opts.isStale ?? (() => false);
    const replayOnly = opts.replayOnly ?? false;
    // Local accumulators — both ctx and `_lastProcessedSeq` stay isolated until commit.
    // This prevents stale aborts from polluting store state mid-replay.
    const ctx = this._createReduceCtx();
    const checkpoint = this._lastProcessedSeq;
    let localSeq = this._lastProcessedSeq;
    const seenBatchSeqs = new Set<number>();
    const CHUNK = 200;
    const CHUNK_THRESHOLD = 500;
    const shouldYield = events.length > CHUNK_THRESHOLD;
    for (let i = 0; i < events.length; i += CHUNK) {
      if (isStale()) return null;
      const end = Math.min(i + CHUNK, events.length);
      for (let j = i; j < end; j++) {
        const ev = events[j];
        const evSeq = ((ev as Record<string, unknown>)._seq as number) ?? 0;
        if (evSeq > 0) {
          if (evSeq <= checkpoint || seenBatchSeqs.has(evSeq)) continue;
          seenBatchSeqs.add(evSeq);
          localSeq = Math.max(localSeq, evSeq);
        }
        this._reduce(ev, ctx, replayOnly);
      }
      if (shouldYield) await yieldToMain();
    }
    if (isStale()) return null;
    // Atomic commit: seq + timeline + tools + phase land together.
    this._lastProcessedSeq = localSeq;
    this._commitReduceCtx(ctx, replayOnly);
    const wallMs = performance.now() - t0;
    dbg(
      "store",
      `applyEventBatch:async: ${events.length} events in ${wallMs.toFixed(1)}ms ${shouldYield ? "wall" : "cpu"}, timeline=${ctx.tl.length}`,
    );
    return wallMs;
  }

  /** Apply a hook event (from hook-event Tauri listener). */
  applyHookEvent(event: HookEvent): void {
    if (!this.run || event.run_id !== this.run.id) return;

    // In stream session mode, bus events already handle tool tracking
    if (
      (this.useStreamSession || this.sessionAlive) &&
      (event.hook_type === "PreToolUse" || event.hook_type === "PostToolUse")
    ) {
      dbg("store", "skip hook tool event (stream mode):", event.hook_type, event.tool_name);
      return;
    }

    // PostToolUse should update matching PreToolUse entry
    if (event.hook_type === "PostToolUse" && event.tool_name) {
      const idx = this.tools.findLastIndex(
        (e) =>
          e.tool_name === event.tool_name && e.hook_type === "PreToolUse" && e.status === "running",
      );
      if (idx >= 0) {
        const updated = [...this.tools];
        updated[idx] = {
          ...updated[idx],
          status: "done",
          hook_type: "PostToolUse",
          tool_output: event.tool_output,
        };
        this.tools = updated;
        return;
      }
    }

    this.tools = [...this.tools, event];
  }

  /** Apply hook usage (cumulative += not overwrite). */
  applyHookUsage(usage: {
    run_id: string;
    input_tokens: number;
    output_tokens: number;
    cost: number;
  }): void {
    if (!this.run || usage.run_id !== this.run.id) return;
    this.usage = {
      ...this.usage,
      inputTokens: this.usage.inputTokens + usage.input_tokens,
      outputTokens: this.usage.outputTokens + usage.output_tokens,
      cost: this.usage.cost + usage.cost,
    };
  }

  // ── Actions ──

  /** Clear all content/display state fields. Does not touch phase, run, or agent. */
  private _clearContentState(): void {
    this._useChatTimelineForRun = false;
    this.timeline = [];
    this.streamingText = "";
    this.thinkingText = "";
    this.thinkingStartMs = 0;
    this.thinkingEndMs = 0;
    this.tools = [];
    this.usage = {
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      cost: 0,
    };
    this.model = "";
    this.error = "";

    this.systemStatus = null;
    this.authStatus = null;
    this.hookEvents = [];
    this.taskNotifications = new Map();
    this.pendingElicitations = new Map();
    this.piExtensionUiMap = {};
    this.piExtensionUi = null;
    this.piExtensionHostState = { statuses: {}, widgets: {}, pendingRequests: [] };
    this.contextBreakdown = null;
    this.pendingEditorText = null;
    this.latestNotice = null;
    this.persistedFiles = [];
    this.ralphLoop = null;
    this.goal = null;
    this.scheduledTasks = [];
    this.sessionCommands = [];
    this.sessionCapabilities = null;
    this.sessionMode = "";
    this.sessionModes = [];
    this.sessionModels = [];
    this.piCommandsState = "idle";
    this.piCommandsReady = false;
    this.piCapabilities = null;
    this.piPlanState = { phase: "unavailable" };
    this.piGoalState = { phase: "unavailable" };
    this.piTodoState = { phases: [] };
    this.todoPanelDismissed = false;
    this.structuredTaskState = null;
    this.workTaskState = null;
    this.workContextPlan = null;
    this.piPermissionState = {
      available: false,
      mode: "accept_edits",
      pendingRequestCount: 0,
      restartRequired: false,
    };
    this.piQueueState = { steering: [], followUp: [], pendingCount: 0, detailsKnown: false };
    this.queuedMessages = [];
    this.queueFlushInFlight = false;
    this.piSessionTree = { roots: [], leafId: null, loading: false };
    this.mcpServers = [];
    this.turnDiff = "";
    this.cliVersion = "";
    // Pi permission modes are session-scoped in AgentCabin. A new chat must not
    // inherit a previous Pi bypass/auto choice, even if an older version wrote it
    // to agent settings.
    if (this.agent === "pi") {
      this.permissionMode = "default";
      this.permissionModeSetByUser = false;
      this.permissionModePersistFailed = false;
    }
    // For Claude/Codex, permissionMode remains a user-level preference. If persist
    // had failed, reset the flag so the next session_init can re-sync.
    if (this.permissionModePersistFailed) {
      this.permissionModeSetByUser = false;
      this.permissionModePersistFailed = false;
      dbg("store", "permissionMode flag reset — persist had failed, allowing session_init re-sync");
    }
    this.fastModeState = "";
    this.apiKeySource = "";
    this.rateLimitStatus = "";
    this.rateLimitType = "";
    this.rateLimitUtilization = null;
    this.rateLimitResetsAt = null;
    this.availableAgents = [];
    this.availableSkills = [];
    this.availablePlugins = [];
    this.numTurns = 0;
    this.durationMs = 0;
    this.turnUsages = [];
    this.contextHwTokens = 0;
    this.contextHwWindow = 0;
    this.lastReqContextTokens = 0;
    this.lastCompactedAt = 0;
    this.compactCount = 0;
    this.microcompactCount = 0;
    this.sessionCwd = "";
    this.sessionTools = [];
    this.outputStyle = "";
    // If agent entered plan mode (previousPermissionMode is non-empty), restore the user's
    // actual preference. If user manually selected plan (previousPermissionMode is empty),
    // leave it alone — it's a user-level preference.
    if (this.permissionMode === "plan" && this.previousPermissionMode) {
      const restored = this.previousPermissionMode;
      this.permissionMode = restored;
      dbg("store", "permissionMode restored from agent plan on clear", { restored });
    }
    this.previousPermissionMode = "";
    this.pendingPermissionModeOverride = null;
    this.pendingClearContextPlan = null;
    this.sessionInitReceived = false;
    this.unknownEventCount = 0;
    this.rawFallbackCount = 0;
    // NOTE: remoteHostName and platformId are intentionally NOT cleared here —
    // they are run-level properties restored from run metadata, not per-session state.
    this._seenMessageIds.clear();
    this._seenToolIds.clear();
    this._lastProcessedSeq = 0;
    this._toolTlIndex.clear();
    this._toolHeIndex.clear();
  }

  /** Optimistically remove an elicitation after responding.
   *  Called by UI before the IPC call returns. */
  removeElicitation(requestId: string): void {
    if (!this.pendingElicitations.has(requestId)) return;
    const updated = new Map(this.pendingElicitations);
    updated.delete(requestId);
    this.pendingElicitations = updated;
  }

  /** Clear Pi extension host surfaces when a live run is no longer usable. */
  clearPiExtensionUiState(): void {
    this.piExtensionUiMap = {};
    this.piExtensionUi = null;
    this.piExtensionHostState = { statuses: {}, widgets: {}, pendingRequests: [] };
    this.pendingEditorText = null;
    this.latestNotice = null;
  }

  /** Reset all state to empty. */
  reset(): void {
    this._setPhase("empty");
    this.run = null;
    this._isLoadingReplay = false;
    this._loadingRunId = null;
    this._loadingEvents = [];
    this._clearContentState();
  }

  /**
   * Codex turn-based rewind: drop the timeline tail starting at the
   * `dropFromTurnIndex`-th top-level user message (0-based). Keeps turns
   * [0, dropFromTurnIndex) and removes everything from that user message
   * onward. Mirrors a successful `thread/rollback` on the backend — history
   * only; file changes are NOT reverted (caller surfaces that warning).
   *
   * Returns the number of turns dropped (0 if the index is out of range).
   */
  truncateToTurn(dropFromTurnIndex: number): number {
    if (dropFromTurnIndex < 0) return 0;
    let userSeen = 0;
    let cutAt = -1;
    for (let i = 0; i < this.timeline.length; i++) {
      if (this.timeline[i].kind === "user") {
        if (userSeen === dropFromTurnIndex) {
          cutAt = i;
          break;
        }
        userSeen++;
      }
    }
    if (cutAt < 0) return 0; // index past the last turn — nothing to drop
    const droppedTurns = this.timeline.slice(cutAt).filter((e) => e.kind === "user").length;
    this.timeline = this.timeline.slice(0, cutAt);
    // Rebuild numTurns from surviving user messages so the status bar/turn list agree.
    this.numTurns = this.timeline.filter((e) => e.kind === "user").length;
    this.turnUsages = this.turnUsages.slice(0, dropFromTurnIndex);
    // Streaming/thinking buffers belong to the (now-removed) latest turn.
    this.streamingText = "";
    this.thinkingText = "";
    // The current Pi context is branch-specific; wait for the next runtime
    // measurement instead of showing a breakdown from the removed branch.
    this.contextBreakdown = null;
    dbg("store", "truncateToTurn", { dropFromTurnIndex, droppedTurns, kept: this.numTurns });
    return droppedTurns;
  }

  // ── Snapshot cache helpers ──

  /** Serialize current store state into a JSON string for IDB caching. */
  private _buildSnapshot(): string {
    const obj: Record<string, unknown> = {
      // A group (ReduceCtx-derived)
      timeline: this.timeline,
      tools: this.tools,
      hookEvents: this.hookEvents,
      streamingText: this.streamingText,
      thinkingText: this.thinkingText,
      model: this.model,
      usage: this.usage,
      turnUsages: this.turnUsages,
      contextHwTokens: this.contextHwTokens,
      contextHwWindow: this.contextHwWindow,
      lastReqContextTokens: this.lastReqContextTokens,
      _seenMessageIds: [...this._seenMessageIds],
      _seenToolIds: [...this._seenToolIds],
      // B group (direct fields)
      systemStatus: this.systemStatus,
      piExtensionUi: this.piExtensionUi,
      contextBreakdown: this.contextBreakdown,
      authStatus: this.authStatus,
      cliVersion: this.cliVersion,
      // NOTE: permissionMode intentionally excluded — user-level preference, not snapshot state.
      fastModeState: this.fastModeState,
      apiKeySource: this.apiKeySource,
      sessionCommands: this.sessionCommands,
      sessionCapabilities: this.sessionCapabilities,
      sessionMode: this.sessionMode,
      sessionModes: this.sessionModes,
      sessionModels: this.sessionModels,
      piCommandsState: this.piCommandsState,
      piCommandsReady: this.piCommandsReady,
      piCapabilities: this.piCapabilities,
      piPlanState: this.piPlanState,
      piGoalState: this.piGoalState,
      piTodoState: this.piTodoState,
      todoPanelDismissed: this.todoPanelDismissed,
      structuredTaskState: this.structuredTaskState,
      workTaskState: this.workTaskState,
      workContextPlan: this.workContextPlan,
      piPermissionState: this.piPermissionState,
      piQueueState: this.piQueueState,
      piSessionTree: this.piSessionTree,
      mcpServers: this.mcpServers,
      sessionTools: this.sessionTools,
      availableAgents: this.availableAgents,
      availableSkills: this.availableSkills,
      availablePlugins: this.availablePlugins,
      sessionCwd: this.sessionCwd,
      outputStyle: this.outputStyle,
      sessionInitReceived: this.sessionInitReceived,
      numTurns: this.numTurns,
      durationMs: this.durationMs,
      compactCount: this.compactCount,
      microcompactCount: this.microcompactCount,
      lastCompactedAt: this.lastCompactedAt,
      persistedFiles: this.persistedFiles,
      unknownEventCount: this.unknownEventCount,
      rawFallbackCount: this.rawFallbackCount,
      taskNotifications: [...this.taskNotifications.entries()],
      scheduledTasks: this.scheduledTasks,
      // Reducer-written run state that idle-snapshot restore must preserve (a snapshot-hit
      // loadRun skips event replay, so anything not serialized here is lost on revisit). #135-class
      ralphLoop: this.ralphLoop,
      rateLimitStatus: this.rateLimitStatus,
      rateLimitType: this.rateLimitType,
      rateLimitUtilization: this.rateLimitUtilization,
      rateLimitResetsAt: this.rateLimitResetsAt,
      thinkingStartMs: this.thinkingStartMs,
      thinkingEndMs: this.thinkingEndMs,
      _lastProcessedSeq: this._lastProcessedSeq,
    };
    return JSON.stringify(obj);
  }

  /** Parse snapshot body string. Returns parsed object or null if invalid JSON. */
  private _parseSnapshotBody(body: string): Record<string, unknown> | null {
    try {
      return JSON.parse(body) as Record<string, unknown>;
    } catch {
      return null;
    }
  }

  async syncPiExtensionUiState(): Promise<void> {
    if (!this.run?.id || this.run.agent !== "pi") return;
    try {
      const snap = await api.getPiExtensionUiState(this.run.id);
      if (snap && typeof snap === "object") {
        this.applyHostSnapshot(snap);
      }
    } catch (e) {
      dbgWarn("store", "syncPiExtensionUiState failed", String(e));
    }
  }

  applyHostSnapshot(
    snapshot: Record<string, unknown> | import("$lib/types").PiExtensionHostState,
  ): void {
    const s = snapshot as Record<string, unknown>;
    const rawStatuses =
      typeof s.statuses === "object" && s.statuses ? (s.statuses as Record<string, unknown>) : {};
    // The permission extension exposes its internal `yolo` flag through the
    // generic status channel. It is already represented by the Pi permission
    // control in the composer and must never become a visible extension card,
    // including when restoring a snapshot after opening an existing run.
    const statuses = Object.fromEntries(
      Object.entries(rawStatuses).filter(([, value]) => !isPiPermissionModeIndicator(value)),
    ) as Record<string, string>;
    const pendingReqs = Array.isArray(s.pendingRequests)
      ? (s.pendingRequests as import("$lib/types").PiExtensionUiRequest[])
      : Array.isArray(s.pending_requests)
        ? (s.pending_requests as import("$lib/types").PiExtensionUiRequest[])
        : [];

    this.piExtensionHostState = {
      title: typeof s.title === "string" ? s.title : undefined,
      statuses,
      widgets:
        typeof s.widgets === "object" && s.widgets
          ? (s.widgets as Record<string, import("$lib/types").PiExtensionWidget>)
          : {},
      pendingRequests: pendingReqs,
    };

    // Restore uncompleted Dialogs (pending requests) into pendingElicitations
    const map = new Map(this.pendingElicitations);
    for (const [id, el] of map.entries()) {
      if (el.mode?.startsWith("pi_extension_") && !pendingReqs.some((r) => r.id === id)) {
        map.delete(id);
      }
    }

    for (const req of pendingReqs) {
      const method = req.method;
      const requestedSchema =
        method === "select"
          ? {
              type: "object",
              properties: {
                value: {
                  type: "string",
                  title: "Selection",
                  enum: req.options,
                },
              },
              required: ["value"],
            }
          : method === "input"
            ? {
                type: "object",
                properties: {
                  value: {
                    type: "string",
                    title: req.placeholder || "Value",
                  },
                },
                required: ["value"],
              }
            : method === "editor"
              ? {
                  type: "object",
                  properties: {
                    value: {
                      type: "string",
                      title: "Text",
                      default: req.prefill || "",
                    },
                  },
                  required: ["value"],
                }
              : undefined;

      const detail = req.message || "";
      const titleStr = req.title || "Pi extension request";
      const msgText = detail ? (titleStr ? `${titleStr}\n${detail}` : detail) : titleStr;

      map.set(req.id, {
        requestId: req.id,
        mcpServerName: "Pi extension UI",
        message: msgText,
        elicitationId: req.id,
        mode: `pi_extension_${method}`,
        requestedSchema: requestedSchema as ElicitationSchema,
        expiresAt: req.expiresAt,
      });
    }
    this.pendingElicitations = map;
  }

  /** Try to restore store state from a pre-parsed snapshot object (or string for compat).
   *  Returns true on success, false if shape validation fails. */
  private _tryApplySnapshot(bodyOrObj: string | Record<string, unknown>): boolean {
    try {
      const obj =
        typeof bodyOrObj === "string"
          ? (JSON.parse(bodyOrObj) as Record<string, unknown>)
          : bodyOrObj;
      // Shape validation: timeline must be array, usage must be object
      if (!Array.isArray(obj.timeline) || typeof obj.usage !== "object" || obj.usage === null) {
        dbgWarn("snapshot", "apply:shape-fail", {
          hasTimeline: Array.isArray(obj.timeline),
          hasUsage: typeof obj.usage,
        });
        return false;
      }

      // A group
      // Backfill anchorId for old snapshots that predate the anchor system
      this.timeline = (obj.timeline as TimelineEntry[]).map(backfillAnchorId);
      this.tools = (obj.tools ?? []) as HookEvent[];
      this.hookEvents = (obj.hookEvents ?? []) as typeof this.hookEvents;
      this.streamingText = (obj.streamingText as string) ?? "";
      this.thinkingText = (obj.thinkingText as string) ?? "";
      this.model = (obj.model as string) ?? "";
      this.usage = obj.usage as UsageState;
      this.turnUsages = (obj.turnUsages ?? []) as TurnUsage[];
      this.contextHwTokens = (obj.contextHwTokens as number) ?? 0;
      this.contextHwWindow = (obj.contextHwWindow as number) ?? 0;
      this.lastReqContextTokens = (obj.lastReqContextTokens as number) ?? 0;
      this._seenMessageIds = new Set((obj._seenMessageIds ?? []) as string[]);
      this._seenToolIds = new Set((obj._seenToolIds ?? []) as string[]);

      // B group
      this.systemStatus = (obj.systemStatus as typeof this.systemStatus) ?? null;
      this.piExtensionUi = (obj.piExtensionUi as import("$lib/types").PiExtensionUiState) ?? null;
      this.contextBreakdown =
        (obj.contextBreakdown as import("$lib/types").ContextUsageBreakdown | undefined) ?? null;
      this.authStatus = (obj.authStatus as typeof this.authStatus) ?? null;
      this.cliVersion = (obj.cliVersion as string) ?? "";
      // NOTE: permissionMode intentionally NOT restored from snapshot — user-level preference.
      this.fastModeState = (obj.fastModeState as string) ?? "";
      this.apiKeySource = (obj.apiKeySource as string) ?? "";
      this.sessionCommands = (obj.sessionCommands ?? []) as CliCommand[];
      this.sessionCapabilities =
        (obj.sessionCapabilities as import("$lib/types").AgentCapabilities | undefined) ?? null;
      this.sessionMode = (obj.sessionMode as string | undefined) ?? "";
      this.sessionModes =
        (obj.sessionModes as import("$lib/types").AgentSessionMode[] | undefined) ?? [];
      this.sessionModels =
        (obj.sessionModels as import("$lib/types").CliModelInfo[] | undefined) ?? [];
      const snapshotPiCommandsState = obj.piCommandsState;
      this.piCommandsState =
        snapshotPiCommandsState === "loading" ||
        snapshotPiCommandsState === "ready" ||
        snapshotPiCommandsState === "failed"
          ? snapshotPiCommandsState
          : "idle";
      this.piCommandsReady = this.piCommandsState === "ready";
      this.piCapabilities =
        (obj.piCapabilities as import("$lib/types").PiFeatureCapabilities | null) ?? null;
      this.piPlanState = (obj.piPlanState as import("$lib/types").PiPlanState) ?? {
        phase: "unavailable",
      };
      this.piGoalState = (obj.piGoalState as import("$lib/types").PiGoalState) ?? {
        phase: "unavailable",
      };
      this.piTodoState = (obj.piTodoState as import("$lib/types").PiTodoState) ?? {
        phases: [],
      };
      this.todoPanelDismissed = (obj.todoPanelDismissed as boolean) ?? false;
      this.structuredTaskState =
        (obj.structuredTaskState as import("$lib/types").StructuredTask[] | null) ?? null;
      this.workTaskState =
        (obj.workTaskState as import("$lib/types/work").WorkTaskState | null) ?? null;
      this.workContextPlan =
        (obj.workContextPlan as import("$lib/types/work").WorkContextPlan | null) ?? null;
      this.piPermissionState =
        (obj.piPermissionState as import("$lib/types").PiPermissionState) ?? this.piPermissionState;
      this.piQueueState = (obj.piQueueState as import("$lib/types").PiMessageQueueState) ?? {
        steering: [],
        followUp: [],
        pendingCount: 0,
        detailsKnown: false,
      };
      this.piSessionTree = (obj.piSessionTree as import("$lib/types").PiSessionTreeState) ?? {
        roots: [],
        leafId: null,
        loading: false,
      };
      this.mcpServers = dedupeMcpServersByName((obj.mcpServers ?? []) as McpServerInfo[]);
      this.sessionTools = (obj.sessionTools ?? []) as string[];
      this.availableAgents = (obj.availableAgents ?? []) as string[];
      this.availableSkills = (obj.availableSkills ?? []) as string[];
      this.availablePlugins = (obj.availablePlugins ?? []) as unknown[];
      this.sessionCwd = (obj.sessionCwd as string) ?? "";
      this.outputStyle = (obj.outputStyle as string) ?? "";
      this.sessionInitReceived = (obj.sessionInitReceived as boolean) ?? false;
      this.numTurns = (obj.numTurns as number) ?? 0;
      this.durationMs = (obj.durationMs as number) ?? 0;
      this.compactCount = (obj.compactCount as number) ?? 0;
      this.microcompactCount = (obj.microcompactCount as number) ?? 0;
      this.lastCompactedAt = (obj.lastCompactedAt as number) ?? 0;
      this.persistedFiles = (obj.persistedFiles ?? []) as unknown[];
      this.unknownEventCount = (obj.unknownEventCount as number) ?? 0;
      this.rawFallbackCount = (obj.rawFallbackCount as number) ?? 0;
      // Reducer-written run state (see _buildSnapshot). ?? keeps old snapshots valid.
      this.ralphLoop = (obj.ralphLoop as typeof this.ralphLoop) ?? null;
      this.rateLimitStatus = (obj.rateLimitStatus as string) ?? "";
      this.rateLimitType = (obj.rateLimitType as string) ?? "";
      this.rateLimitUtilization = (obj.rateLimitUtilization as number | null) ?? null;
      this.rateLimitResetsAt = (obj.rateLimitResetsAt as number | null) ?? null;
      this.thinkingStartMs = (obj.thinkingStartMs as number) ?? 0;
      this.thinkingEndMs = (obj.thinkingEndMs as number) ?? 0;
      this.taskNotifications = new Map(
        (obj.taskNotifications ?? []) as Array<[string, TaskNotificationItem]>,
      );
      this.scheduledTasks = Array.isArray(obj.scheduledTasks)
        ? (obj.scheduledTasks as unknown[]).filter((t): t is ScheduledTask => {
            if (!t || typeof t !== "object") return false;
            const o = t as Record<string, unknown>;
            return (
              typeof o.id === "string" &&
              typeof o.humanSchedule === "string" &&
              typeof o.recurring === "boolean" &&
              typeof o.durable === "boolean" &&
              typeof o.toolUseId === "string"
            );
          })
        : [];
      this._lastProcessedSeq = (obj._lastProcessedSeq as number) ?? 0;

      // Rebuild runtime tool indexes from restored state
      this._toolTlIndex.clear();
      for (let i = 0; i < this.timeline.length; i++) {
        const e = this.timeline[i];
        if (e.kind === "tool" && !this._toolTlIndex.has(e.id)) this._toolTlIndex.set(e.id, i);
      }
      this._toolHeIndex.clear();
      for (let i = 0; i < this.tools.length; i++) {
        const tid = (this.tools[i] as Record<string, unknown>).tool_use_id as string | undefined;
        if (tid && !this._toolHeIndex.has(tid)) this._toolHeIndex.set(tid, i);
      }

      dbg("snapshot", "apply:ok", { timeline: this.timeline.length });
      return true;
    } catch (err) {
      dbgWarn("snapshot", "apply:error", err);
      return false;
    }
  }

  /** Fire-and-forget: serialize current state and write to IDB.
   *  Deferred to next event-loop tick so JSON.stringify doesn't block loadRun.
   *  Caller must check write guard before calling. */
  private _saveSnapshotToIdb(runId: string): void {
    if (!this.run) return;
    const runStatus = this.run.status;
    const gen = this._loadGen;
    setTimeout(() => {
      // Guard: still viewing the same run (user may have navigated away)
      if (this._loadGen !== gen || this.run?.id !== runId) return;
      // Guard: status must still match (prevents stale write after idle→running transition)
      if (this.run.status !== runStatus) {
        dbg("snapshot", "save:skipped (status changed)", {
          runId,
          expected: runStatus,
          actual: this.run.status,
        });
        return;
      }
      const body = this._buildSnapshot();
      dbg("snapshot", "save", { runId, runStatus, bytes: body.length });
      snapshotCache.writeSnapshot(runId, runStatus, body).catch(() => {});
    }, 0);
  }

  /** Load a run by ID. Handles replay of bus events / run events. */
  private _loadingEventKey(ev: BusEvent): string {
    const payload = { ...(ev as unknown as Record<string, unknown>) };
    // `_seq`/`ts` are transport/storage metadata. Tauri live events do not have
    // them, while replayed events do, so they must not affect deduplication.
    delete payload._seq;
    delete payload.ts;
    return JSON.stringify(payload);
  }

  /**
   * Merge events that arrived while run metadata/history was being loaded.
   * Durable replay wins when it already contains the same event; otherwise the
   * live event is applied after replay so it cannot be overwritten by the batch
   * commit. Returns without touching state if a newer load owns the store.
   */
  private _drainLoadingEvents(runId: string, replayEvents: BusEvent[]): void {
    if (this._loadingRunId !== runId) return;

    const pending = this._loadingEvents;
    this._loadingEvents = [];
    this._loadingRunId = null;

    if (pending.length === 0 || !this.run || this.run.id !== runId) return;

    const replayCounts = new Map<string, number>();
    for (const event of replayEvents) {
      const key = this._loadingEventKey(event);
      replayCounts.set(key, (replayCounts.get(key) ?? 0) + 1);
    }

    const liveOnly = pending.filter((event) => {
      const key = this._loadingEventKey(event);
      const count = replayCounts.get(key) ?? 0;
      if (count === 0) return true;
      replayCounts.set(key, count - 1);
      return false;
    });

    if (liveOnly.length === 1) {
      this.applyEvent(liveOnly[0]);
    } else if (liveOnly.length > 1) {
      this.applyEventBatch(liveOnly);
    }
  }

  /**
   * Reconstructs the initial run prompt if the durable `user_message` bus event
   * hasn't arrived or been processed yet (e.g. during actor startup handshake or
   * right after creating/switching into a running session). Live events are
   * content-deduped against this placeholder; terminal replay removes it before
   * applying the authoritative event batch.
   */
  private _restorePendingInitialPrompt(includeTerminal = false): void {
    const run = this.run;
    const allowedStatuses = includeTerminal
      ? ["pending", "running", "idle", "completed", "failed", "stopped"]
      : ["pending", "running", "idle"];
    if (
      !run ||
      !run.prompt.trim() ||
      !allowedStatuses.includes(run.status) ||
      this.timeline.some((entry) => entry.kind === "user")
    ) {
      return;
    }

    const id = uuid();
    const entry: TimelineEntry = {
      kind: "user",
      id,
      anchorId: id,
      content: run.prompt,
      ts: run.started_at,
    };
    this.timeline = [entry, ...this.timeline];

    // Prepending shifts tool positions. Rebuild the runtime index so subsequent
    // ACP tool/permission events still target the correct timeline entries.
    this._toolTlIndex.clear();
    for (let index = 0; index < this.timeline.length; index++) {
      const current = this.timeline[index];
      if (current.kind === "tool" && !this._toolTlIndex.has(current.id)) {
        this._toolTlIndex.set(current.id, index);
      }
    }
  }

  /** Remove the temporary metadata prompt when replay contains the authoritative event. */
  private _removeInitialPromptPlaceholderIfReplayed(events: BusEvent[]): void {
    const prompt = this.run?.prompt.trim();
    if (
      !prompt ||
      !events.some((event) => event.type === "user_message" && event.text === prompt)
    ) {
      return;
    }
    const index = this.timeline.findIndex(
      (entry) => entry.kind === "user" && entry.content === prompt && !entry.cliUuid,
    );
    if (index < 0) return;
    this.timeline = this.timeline.filter((_, entryIndex) => entryIndex !== index);
    this._toolTlIndex.clear();
    for (let entryIndex = 0; entryIndex < this.timeline.length; entryIndex++) {
      const entry = this.timeline[entryIndex];
      if (entry.kind === "tool" && !this._toolTlIndex.has(entry.id)) {
        this._toolTlIndex.set(entry.id, entryIndex);
      }
    }
  }

  /**
   * Mark the current run stopped without replaying its entire event log.
   *
   * The Work stop command already persists the terminal state and returns the
   * authoritative metadata. Re-entering loadRun() here would clear the visible
   * transcript and synchronously wait for events.jsonl, which makes a stopped
   * conversation look frozen (especially after a restart). Keep the transcript
   * the user just saw, invalidate any in-flight hydration, and cache this
   * terminal state for the next open.
   */
  adoptStoppedRun(run?: TaskRun): void {
    const current = this.run;
    if (run && current?.id === run.id) {
      this.run = { ...current, ...run, status: "stopped" };
    } else if (run) {
      this.run = { ...run, status: "stopped" };
      this.agent = run.agent;
      this.remoteHostName = run.remote_host_name ?? null;
      this.platformId = run.platform_id ?? null;
      this.sessionCwd = run.cwd;
      this._useChatTimelineForRun = true;
    } else if (current) {
      this.run = { ...current, status: "stopped" };
    } else {
      return;
    }

    ++this._loadGen;
    this._loadingRunId = null;
    this._loadingEvents = [];
    this._isLoadingReplay = false;
    this._restorePendingInitialPrompt(true);
    this._setPhase("stopped");
    this.error = "";
    this._saveSnapshotToIdb(this.run.id);
  }

  private _restorePendingOptimisticUsers(runId: string): void {
    const pending = this._pendingOptimisticUsers.get(runId);
    if (!pending || pending.length === 0) return;
    const missing = pending.filter(
      (p) => !this.timeline.some((e) => e.kind === "user" && e.content === p.content),
    );
    for (const entry of missing) {
      this._pushTimeline(null, entry);
    }
    this._pendingOptimisticUsers.delete(runId);
  }

  async loadRun(
    id: string,
    xtermRef?: { clear(): void; writeText(s: string): void },
  ): Promise<void> {
    const gen = ++this._loadGen;
    const loadStart = performance.now();
    dbg("store", "loadRun id=", id, "gen=", gen);

    // Establish the loading target before the first await. The middleware may
    // deliver a bus event immediately after the route switches to this run.
    this._loadingRunId = id || null;
    this._loadingEvents = [];
    this._isLoadingReplay = Boolean(id);

    if (!id) {
      this.reset();
      return;
    }

    // Reset state for new load
    this._setPhase("loading");
    const prevRunId = this.run?.id;
    if (prevRunId) {
      const pending = this.timeline.filter(
        (e): e is Extract<TimelineEntry, { kind: "user" }> => e.kind === "user" && !e.cliUuid,
      );
      if (pending.length > 0) this._pendingOptimisticUsers.set(prevRunId, pending);
    }
    this._clearContentState();

    if (xtermRef) {
      xtermRef.clear();
      xtermRef.writeText("\x1b[0m\x1b[2J\x1b[H");
    }

    try {
      this.run = await api.getRun(id);
      if (gen !== this._loadGen) {
        dbg("store", "stale after getRun, gen=", gen);
        return;
      }

      // Auto-sync CLI imports to pick up events written after the initial import
      if (this.run.source === "cli_import") {
        try {
          const syncResult = await api.syncCliSession(id);
          if (syncResult.newEvents > 0) {
            dbg("store", "loadRun: auto-synced CLI import", {
              newEvents: syncResult.newEvents,
            });
            // Refresh run meta after sync (watermark/status may have updated)
            this.run = await api.getRun(id);
            // Sync appended events → IDB snapshot is stale
            snapshotCache.deleteSnapshot(id).catch(() => {});
          }
        } catch (e) {
          dbg("store", "loadRun: auto-sync failed (non-fatal)", String(e));
        }
        if (gen !== this._loadGen) {
          dbg("store", "stale after auto-sync, gen=", gen);
          return;
        }
      }

      this.agent = this.run.agent;
      this.remoteHostName = this.run.remote_host_name ?? null;
      this.platformId = this.run.platform_id ?? null;
      // Suppress isThinking during event replay (prevents "thinking" flash on session switch)
      this._isLoadingReplay = true;

      // Determine phase from run status
      const st = this.run.status;
      if (st === "running") {
        this._setPhase("running");
      } else if (st === "idle") {
        this._setPhase("idle");
      } else if (st === "completed" || st === "failed" || st === "stopped") {
        this._setPhase(st as SessionPhase);
      } else {
        this._setPhase("ready");
      }

      // Terminal runs use replayOnly — historical run_state events must not
      // overwrite the phase we just set from run.status. Same pattern as resumeSession.
      const isTerminal = TERMINAL_PHASES.includes(this.phase);

      // Pre-set _useChatTimelineForRun: session_actor always; pipe_exec only if
      // active Phase 2 (has conversation_ref). Overridden by bus-events branch below.
      this._useChatTimelineForRun =
        this.run!.execution_path === "session_actor" ||
        (this.run!.execution_path === "pipe_exec" && !isTerminal && !!this.run!.conversation_ref);

      if (this.useStreamSession) {
        // The run metadata already contains the user's prompt. Render it before
        // fetching/reducing the durable event log so a slow terminal replay
        // never presents an entirely blank conversation.
        if (isTerminal && this.run?.app_mode === "work") {
          this._restorePendingInitialPrompt(true);
        }
        let reducerMs = 0;
        let snapshotHit = false;

        // Only non-Work terminal sessions may use a snapshot. Idle sessions can
        // receive a follow-up turn, and Work recovery can mark a run terminal
        // before its Pi transcript has been hydrated. A prompt-only Work
        // snapshot would therefore hide an already persisted assistant reply;
        // replay the durable event log for Work so reopening is authoritative.
        const snapshotEligible = isTerminal && this.run?.app_mode !== "work";
        let snapshotBody: string | null = null;
        if (snapshotEligible) {
          try {
            snapshotBody = await snapshotCache.readSnapshot(id, this.run!.status);
          } catch {
            /* IDB unavailable → miss */
          }
          if (gen !== this._loadGen) return;
        }

        if (snapshotBody) {
          const parsed = this._parseSnapshotBody(snapshotBody);
          if (!parsed) {
            snapshotBody = null; // corrupted JSON
          } else if (this._tryApplySnapshot(parsed)) {
            snapshotHit = true;
            this._drainLoadingEvents(id, []);
            this._wsSubscribeWithSeq(id, this._lastProcessedSeq);
          } else {
            snapshotBody = null; // shape validation failed
          }
        }

        if (!snapshotHit) {
          // Miss or snapshot corrupted → normal path
          const busEvents = await api.getBusEvents(id);
          if (gen !== this._loadGen) {
            dbg("store", "stale after getBusEvents, gen=", gen);
            return;
          }
          this._removeInitialPromptPlaceholderIfReplayed(busEvents);
          const ms = await this.applyEventBatchAsync(busEvents, {
            replayOnly: isTerminal,
            isStale: () => gen !== this._loadGen,
          });
          // Stale: a newer load owns the store; do not touch _isLoadingReplay.
          if (ms === null) return;
          reducerMs = ms;
          this._drainLoadingEvents(id, busEvents);
          this._restorePendingInitialPrompt();
          this._wsSubscribeWithSeq(id, this._lastProcessedSeq);
          // Write guard: distinguish "legit empty session" from "reducer anomaly"
          if (snapshotEligible && (this.timeline.length > 0 || busEvents.length === 0)) {
            this._saveSnapshotToIdb(id);
          }
        }

        this._isLoadingReplay = false;
        if (this.agent === "pi") {
          // Sync Pi extension UI state in background — non-blocking, content is already rendered
          this.syncPiExtensionUiState().catch((e) =>
            dbgWarn("store", "syncPiExtensionUiState background failed", String(e)),
          );
        }
        dbg("store", "loadRun", {
          total: Math.round(performance.now() - loadStart),
          snapshotHit,
          reducer: Math.round(reducerMs),
          entries: this.timeline.length,
        });
      } else if (this.capabilities.execution.busEvents) {
        // Codex bus-events path: try bus-events, fall back to terminal
        const busEvents = await api.getBusEvents(id);
        if (gen !== this._loadGen) {
          dbg("store", "stale after getBusEvents (codex), gen=", gen);
          return;
        }
        let reducerMs = 0;
        if (busEvents.length > 0) {
          this._useChatTimelineForRun = true;
          // Always replayOnly for loadRun — these are persisted historical events.
          // Live events arrive via WS subscription (set up below).
          reducerMs = this.applyEventBatch(busEvents, { replayOnly: true });
        } else if (!isTerminal && this.run?.conversation_ref?.kind === "codex_thread") {
          // Active Phase 2 run with no bus-events yet (just started) → timeline, wait for real-time
          this._useChatTimelineForRun = true;
        } else {
          // Terminal run with no bus-events OR active Phase 1 run (no conversation_ref) → terminal
          this._useChatTimelineForRun = false;
          // Replay terminal history (same as legacy branch below)
          if (xtermRef) {
            const termEvents = await api.getRunEvents(id);
            if (gen !== this._loadGen) return;
            replayTerminalEvents(termEvents, xtermRef, this.isRunning);
          }
        }
        this._drainLoadingEvents(id, busEvents);
        // Non-terminal Phase 2 runs must subscribe regardless of history
        if (!isTerminal && this._useChatTimelineForRun) {
          this._wsSubscribeWithSeq(id, this._lastProcessedSeq);
        }
        this._isLoadingReplay = false;
        dbg("store", "loadRun (codex bus)", {
          events: busEvents.length,
          reducer: Math.round(reducerMs),
          timeline: this._useChatTimelineForRun,
        });
      } else {
        // CLI mode: replay history in terminal
        const events = await api.getRunEvents(id);
        if (gen !== this._loadGen) {
          dbg("store", "stale after getRunEvents, gen=", gen);
          return;
        }
        if (xtermRef) {
          replayTerminalEvents(events, xtermRef, this.isRunning);
        }
        this._drainLoadingEvents(id, []);
        this._isLoadingReplay = false;
      }

      // After replay, reconcile phase with run.status:
      // bus events may leave phase as "idle"/"running" even though the run
      // is actually terminal (e.g. process crashed without emitting run_state).
      const finalStatus = this.run?.status;
      if (finalStatus === "completed" || finalStatus === "failed" || finalStatus === "stopped") {
        if (!TERMINAL_PHASES.includes(this.phase as SessionPhase)) {
          dbg("store", "reconcile phase", this.phase, "→", finalStatus);
          this._setPhase(finalStatus as SessionPhase);
        }
        // Clear replayed errors for terminal runs — they're historical, not active
        this.error = "";
      }

      // Restore the model persisted on the run after replay. The run metadata represents
      // the authoritative, latest model selected by the user for this conversation.
      if (this.run?.model?.trim()) {
        dbg("store", "restore run model from meta:", this.run.model);
        this.model = this.run.model.trim();
      }

      if (id) this._restorePendingOptimisticUsers(id);
    } catch (e) {
      if (gen !== this._loadGen) return;
      this._loadingRunId = null;
      this._loadingEvents = [];
      this._isLoadingReplay = false;
      this.error = String(e);
      this._setPhase("failed");
    }
  }

  /**
   * Attach a Work run returned by the start command without replaying it through
   * loadRun(). Work starts its actor before returning the run, so the first bus
   * events can already exist (or arrive while they are being fetched). Clearing
   * the optimistic transcript here would make a successful first turn appear
   * empty and could race the live subscription.
   */
  adoptStartedRun(run: TaskRun, optimisticId?: string): void {
    const previousRunId = this.run?.id;
    const optimistic = optimisticId
      ? this.timeline.find((entry) => entry.kind === "user" && entry.id === optimisticId)
      : undefined;

    // Invalidate an older route load before changing the run identity. If this
    // is a fallback start after a stale/terminal run, keep only the new prompt.
    ++this._loadGen;
    this._loadingRunId = null;
    this._loadingEvents = [];
    this._isLoadingReplay = false;
    if (previousRunId && previousRunId !== run.id) {
      this._clearContentState();
      if (optimistic) this._pushTimeline(null, optimistic);
    }

    this.run = run;
    this.agent = run.agent;
    this.remoteHostName = run.remote_host_name ?? null;
    this.platformId = run.platform_id ?? null;
    this.sessionCwd = run.cwd;
    this._useChatTimelineForRun = true;
    if (run.model?.trim()) this.model = run.model;

    // A newly returned Work run is normally still marked pending in the
    // returned metadata. Represent the actor handshake explicitly until the
    // durable spawning/running event arrives.
    if (this.phase === "empty") {
      this._setPhase("spawning");
    } else if (this.phase !== "spawning" && !ACTIVE_PHASES.includes(this.phase)) {
      this._setPhase("spawning");
    }
  }

  /**
   * Catch up a just-started Work run after subscribing to its bus events. This
   * deliberately does not clear or replace current content; events received
   * during the fetch are buffered and merged after the durable replay.
   */
  async hydrateStartedRun(runId: string): Promise<void> {
    if (!runId || this.run?.id !== runId) return;

    const gen = ++this._loadGen;
    this._loadingRunId = runId;
    this._loadingEvents = [];
    this._isLoadingReplay = true;

    try {
      const busEvents = await withTimeout(
        api.getBusEvents(runId),
        10_000,
        "Work 首条消息事件同步超时",
      );
      if (gen !== this._loadGen || this.run?.id !== runId) return;

      const reduced = await this.applyEventBatchAsync(busEvents, {
        isStale: () => gen !== this._loadGen || this.run?.id !== runId,
      });
      if (reduced === null || gen !== this._loadGen || this.run?.id !== runId) return;

      this._drainLoadingEvents(runId, busEvents);
      this._restorePendingInitialPrompt();
      this._wsSubscribeWithSeq(runId, this._lastProcessedSeq);
      this._isLoadingReplay = false;

      // The initial TaskRun returned by Work is created before the actor emits
      // its final status. Refresh metadata in the background without holding
      // the transcript or input hostage to another IPC call.
      void api
        .getRun(runId)
        .then((latest) => {
          if (latest && this.run?.id === runId) this.run = latest;
        })
        .catch((error) => dbgWarn("store", "hydrateStartedRun metadata refresh failed:", error));
    } catch (error) {
      if (gen !== this._loadGen || this.run?.id !== runId) return;

      // The live subscription is still authoritative if history IPC times out.
      // Release the loading buffer and apply what arrived so the session cannot
      // remain in a permanently buffered state.
      const pending = this._loadingEvents;
      this._loadingRunId = null;
      this._loadingEvents = [];
      this._isLoadingReplay = false;
      if (pending.length > 0) this.applyEventBatch(pending);
      this._wsSubscribeWithSeq(runId, this._lastProcessedSeq);
      dbgWarn("store", "hydrateStartedRun replay skipped:", error);
    } finally {
      if (gen === this._loadGen) {
        if (this._loadingRunId === runId) {
          this._loadingRunId = null;
          this._loadingEvents = [];
        }
        this._isLoadingReplay = false;
      }
    }
  }

  /** Create a new run and start the session. Returns the run ID.
   *  permissionModeOverride: session-scoped permission mode (CLI name, e.g. "acceptEdits").
   *  When set, takes priority over persisted user settings for this spawn only —
   *  used by ExitPlanMode "clear context + auto-accept" flow. */
  async startSession(
    prompt: string,
    cwd: string,
    attachments: Attachment[],
    permissionModeOverride?: string,
    skills?: Array<{ name: string; path: string }>,
    effort?: string,
  ): Promise<string> {
    this.error = "";
    if (this.remoteHostName && !this.capabilities.runtime.remote) {
      throw new Error(`The ${this.agent} runtime does not support remote hosts`);
    }
    this._setPhase("spawning");
    if (this.agent === "pi") {
      this.piCommandsState = "loading";
      this.piCommandsReady = false;
    }

    try {
      // Refresh platformId and permissionMode from latest settings for new sessions.
      // Both may be stale (carried over from a previous run / earlier user selection)
      // if the user changed settings without navigating away from chat.
      try {
        const freshSettings = await api.getUserSettings();
        const managedPiBinding = freshSettings.agent_provider_bindings?.pi;
        let managedBindingModel = "";
        if (managedPiBinding?.mode === "custom") {
          const provider = managedPiBinding.provider_id
            ? freshSettings.global_providers?.find(
                (item) => item.id === managedPiBinding.provider_id,
              )
            : undefined;
          const bare = resolveManagedProviderDefaultModel(
            managedPiBinding,
            provider?.models?.map((item) => item.id) ?? [],
          );
          const pid = managedPiBinding.provider_id;
          managedBindingModel = pid && bare && !bare.includes("/") ? `${pid}/${bare}` : bare;
        }
        const managedPiModel = managedBindingModel || freshSettings.pi_provider?.model?.trim();
        if (this.agent === "pi" && managedPiModel) {
          const startupModel = resolvePiStartupModel(this.model, managedPiModel);
          const qualifiedStartupModel =
            managedPiBinding?.mode === "custom" &&
            managedPiBinding.provider_id &&
            !startupModel.includes("/")
              ? `${managedPiBinding.provider_id}/${startupModel}`
              : startupModel;
          if (qualifiedStartupModel !== this.model) {
            dbg("store", "startSession: falling back to managed Pi model", {
              from: this.model,
              to: qualifiedStartupModel,
            });
            this.model = qualifiedStartupModel;
          }
        }
        if (freshSettings.auth_mode === "api") {
          const freshPid = freshSettings.active_platform_id ?? "anthropic";
          if (freshPid !== this.platformId) {
            dbg("store", "startSession: refreshing platformId", {
              old: this.platformId,
              new: freshPid,
            });
            this.platformId = freshPid;
          }
        }
        // Settings stores app-internal names (auto_all, auto_read, etc.)
        // Store uses CLI names (bypassPermissions, acceptEdits, etc.)
        const APP_TO_CLI: Record<string, string> = {
          ask: "default",
          "ask-all": "default",
          ask_all: "default",
          auto_read: "acceptEdits",
          auto_all: "bypassPermissions",
          "auto-grant": "bypassPermissions",
          auto_grant: "bypassPermissions",
          plan: "plan",
          auto: "auto",
          dont_ask: "dontAsk",
        };
        if (permissionModeOverride) {
          // Session-scoped override wins — sync UI state to match what backend will spawn with.
          if (permissionModeOverride !== this.permissionMode) {
            this.permissionMode = permissionModeOverride;
            this.permissionModeSetByUser = true;
          }
        } else if (this.agent === "pi") {
          // Pi's top-level mode is owned by the permission extension config.
          // Keep the generic Claude/Codex CLI field neutral; the dedicated Pi
          // state is passed to the RPC session below.
          if (!this.permissionModeSetByUser) {
            this.permissionMode = "default";
          }
        } else if (!this.permissionModeSetByUser) {
          // Refresh permissionMode: agentSettings.plan_mode takes priority (consistent
          // with backend adapter.rs:102). Don't set permissionModeSetByUser — that flag
          // is only for user manual changes; session_init is the authority.
          let agentPlanMode = false;
          try {
            const as = await api.getAgentSettings(this.agent);
            agentPlanMode = !!as?.plan_mode;
          } catch {
            /* non-fatal */
          }

          if (agentPlanMode) {
            if (this.permissionMode !== "plan") {
              dbg("store", "startSession: agentSettings.plan_mode → plan", {
                old: this.permissionMode,
              });
              this.permissionMode = "plan";
            }
          } else if (freshSettings.permission_mode) {
            const freshPerm =
              APP_TO_CLI[freshSettings.permission_mode] ?? freshSettings.permission_mode;
            if (freshPerm !== this.permissionMode) {
              dbg("store", "startSession: refreshing permissionMode", {
                old: this.permissionMode,
                new: freshPerm,
              });
              this.permissionMode = freshPerm;
            }
          }
        }
      } catch {
        // Non-fatal: fall through with current store values
      }

      // Explicitly pass execution_path for Claude and Pi (source of truth for run mode).
      // For Codex, defer to the backend: it picks session_actor (app-server transport) vs
      // pipe_exec (legacy) from the `codex_transport` user setting. Forcing it here would
      // override that and pin Codex to exec.
      const executionPath =
        this.agent === "codex"
          ? undefined
          : this.agent === "pi" ||
              this.agent === "grok" ||
              this.agent === "dsh" ||
              this.useStreamSession
            ? "session_actor"
            : "pipe_exec";
      // Persist the composer's model for Codex even while the frontend still thinks it is on
      // the legacy transport: the backend may select app-server at start_run time. The run model
      // is also the source of truth for the managed provider's actual upstream model.
      const runModel =
        this.useStreamSession || this.agent === "codex" ? this.model || undefined : undefined;
      let run = await api.startRun(
        prompt,
        cwd,
        this.agent,
        runModel,
        this.remoteHostName || undefined,
        this.platformId || undefined,
        executionPath,
      );
      const startupEffort = effort?.trim();
      if (startupEffort) {
        // Persist before spawning the actor. Pi/DSH read the run-level effort while building
        // their launch settings, so writing it after startSession would miss the first request.
        await api.updateRunEffort(run.id, startupEffort);
        run = { ...run, effort: startupEffort };
      }
      this.run = run;
      // The backend validates managed-provider models against the latest canonical catalog.
      // Reflect that authoritative fallback immediately when this draft still held a deleted
      // model from a stale picker/cache.
      if (run.model?.trim() && run.model !== this.model) {
        dbg("store", "startSession: backend normalized model", {
          from: this.model,
          to: run.model,
        });
        this.model = run.model;
      }
      // Reflect the persisted run in the app-shell sidebar immediately. Session startup may
      // still spend seconds spawning the CLI/actor and accepting the first message.
      dispatchRunMutation({ kind: "create", run });
      // Compute once after the settings refresh and reuse it for either transport. This keeps
      // stream-session and pipe-exec startup under the same permission contract.
      const startupPermissionMode = resolveStartupPermissionMode(
        this.agent,
        this.permissionMode,
        this.piPermissionState.mode,
        permissionModeOverride,
      );

      if (this.useStreamSession) {
        this._useChatTimelineForRun = true;
        // Optimistic user message — the backend emits UserMessage during
        // api.startSession(), but the middleware subscription isn't set up
        // until after goto() triggers the URL $effect.  Content-based dedup
        // in _reduce(user_message) prevents double display.
        if (prompt.trim() || attachments.length > 0) {
          this._pushOptimisticUser(prompt, attachments);
        }
        // Subscribe middleware BEFORE spawning so no bus-events are dropped.
        // The $effect in chat page will call subscribeCurrent again (idempotent).
        const mw = getEventMiddleware();
        mw.subscribeCurrent(run.id, this);
        this._wsSubscribeNewSession(run.id);
        dbg("store", "stream session start, run=", run.id);
        const backendAtt = mapAttachments(attachments) ?? undefined;
        await api.startSession(
          run.id,
          undefined,
          undefined,
          undefined,
          backendAtt,
          this.platformId || undefined,
          startupPermissionMode,
          skills,
        );
        dbg("store", "startSession resolved");
        // phase will be set by run_state bus event
        this._startSpawnTimeout(run.id);
        if (this.isKnownSlashCommand(prompt)) {
          dbg("store", "skip response timeout for slash command", { cmd: prompt.split(" ")[0] });
        } else {
          this._startResponseTimeout(run.id);
        }
      } else if (this.capabilities.execution.busEvents) {
        // Codex bus-events pipe mode (Phase 2)
        this._useChatTimelineForRun = true;
        const clientUuid = this._pushOptimisticUser(prompt, attachments);
        const mw = getEventMiddleware();
        mw.subscribeCurrent(run.id, this);
        this._wsSubscribeNewSession(run.id);
        this._setPhase("running");
        // Don't pass model for Codex — it doesn't use Claude model names
        await api.sendChatMessage(
          run.id,
          prompt,
          attachments.length > 0 ? attachments : undefined,
          undefined,
          clientUuid,
          startupPermissionMode,
        );
      } else {
        // PipeExec path (Codex): useStreamSession is false iff the freshly
        // created run has execution_path=pipe_exec, so sendChatMessage is the
        // correct IPC. SessionActor runs always go through the branch above.
        this._setPhase("running");
        await api.sendChatMessage(
          run.id,
          prompt,
          attachments.length > 0 ? attachments : undefined,
          undefined,
          undefined,
          startupPermissionMode,
        );
      }

      return run.id;
    } catch (e) {
      this.error = String(e);
      if (this.agent === "pi") {
        this.piCommandsState = "failed";
        this.piCommandsReady = false;
      }
      // Refresh a newly-created pending run if startup failed before the normal post-start
      // refresh in chat/+page.svelte could run.
      if (typeof window !== "undefined" && this.run?.id) {
        window.dispatchEvent(new Event(RUNS_CHANGED_EVENT));
      }
      this._setPhase("failed");
      throw e;
    }
  }

  /** Start a Pi actor and its RPC discovery handshake without starting a model turn. */
  async startPiSessionShell(cwd: string, effort?: string): Promise<string> {
    if (this.agent !== "pi") throw new Error("Pi session shell requires the Pi agent");
    return this.startSession("", cwd, [], undefined, undefined, effort);
  }

  /** Wait for Pi get_commands, not merely the earlier get_state SessionInit. */
  async waitForPiCommandsReady(timeoutMs = 5000): Promise<boolean> {
    if (this.agent !== "pi") return true;
    const deadline = performance.now() + timeoutMs;
    while (!this.piCommandsReady && this.piCommandsState !== "failed") {
      if (!this.sessionAlive || performance.now() >= deadline) {
        this.piCommandsState = "failed";
        this.piCommandsReady = false;
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return this.piCommandsReady;
  }

  async loadPiSessionTree(): Promise<void> {
    if (this.agent !== "pi" || !this.run?.id) return;
    if (this.piCapabilities?.sessionTreeAvailable === false) return;
    this.piSessionTree = { ...this.piSessionTree, loading: true, error: undefined };
    try {
      await api.getPiSessionTree(this.run.id);
    } catch (error) {
      this.piSessionTree = { ...this.piSessionTree, loading: false, error: String(error) };
    }
  }

  async loadPiSessionEntries(): Promise<void> {
    if (this.agent !== "pi" || !this.run?.id) return;
    try {
      await api.getPiSessionEntries(this.run.id, this.piSessionTree.lastEntryId);
    } catch (error) {
      this.piSessionTree = { ...this.piSessionTree, error: String(error) };
    }
  }

  /** Send a subsequent message in an active session. */
  async sendMessage(
    text: string,
    attachments: Attachment[],
    // Structured Codex skill refs picked in the composer. Threaded to api.sendSessionMessage so
    // the agent triggers the skill via a {type:"skill"} UserInput item. Live-Codex only (the
    // picker is gated to that); empty/undefined = unchanged behavior for Claude and no-skill sends.
    skills?: { name: string; path: string }[],
  ): Promise<void> {
    if (!this.run) return;
    if (!this.capabilities.runtime.attachments) attachments = [];
    // Pi slash commands must go through the authoritative RPC command path in
    // the chat route. Never let an unvalidated slash-prefixed input become a
    // normal prompt if another caller reaches this lower-level method.
    if ((this.run.agent === "pi" || this.agent === "pi") && text.trim().startsWith("/")) {
      this.error = "Pi slash commands require command dispatch (CommandUnavailable).";
      return;
    }
    this._dismissCompletedTodoPanel();
    this.error = "";
    // Pi Plan mode can create a shell session before the user enters a prompt. Keep the first
    // real message as the run prompt so title fallbacks and later auto-naming have the actual
    // user request instead of the shell's empty prompt.
    if (!this.run.prompt.trim() && text.trim()) {
      this.run = { ...this.run, prompt: text };
    }
    // Remove any legacy snapshot before a new turn starts.
    snapshotCache.deleteSnapshot(this.run.id).catch(() => {});

    const hasSkills = !!skills && skills.length > 0;
    const startupPermissionMode = resolveStartupPermissionMode(
      this.agent,
      this.permissionMode,
      this.piPermissionState.mode,
    );

    try {
      if (
        this.useStreamSession &&
        this.sessionAlive &&
        this.capabilities.runtime.steer &&
        this.run.agent === "codex" &&
        this.isRunning &&
        attachments.length === 0 &&
        // Skills must ride on turn/start (the {type:"skill"} UserInput item) — turn/steer takes
        // plain text only, so a skill-bearing send falls through to the enqueue path below.
        !hasSkills
      ) {
        // Mid-turn steer (Codex app-server): a turn is RUNNING and the user sends from the
        // mid-turn send button. Inject the text into the CURRENT turn via turn/steer instead
        // of enqueueing a new turn. (turn/steer takes plain text input — attachments fall
        // through to the normal enqueue path below.) The steered text still shows as a user
        // entry via the optimistic push; the backend does not echo a separate UserMessage.
        this._pushOptimisticUser(text, attachments);
        await api.steerSession(this.run.id, text);
        dbg("store", "codex mid-turn steer", { len: text.length });
      } else if (
        this.useStreamSession &&
        this.sessionAlive &&
        this.capabilities.runtime.steer &&
        this.run.agent !== "codex" &&
        this.isRunning &&
        !hasSkills
      ) {
        // Mid-turn steer for session-actor agents (Claude interrupt+resend, Grok native
        // _x.ai/interject). Both route through steerSessionMessage -> ActorCommand::SteerMessage;
        // the actor decides whether to interrupt-and-resend (Claude) or interject (Grok).
        // Codex is excluded because it uses a distinct turn/steer RPC (handled above).
        this._pushOptimisticUser(text, attachments);
        await api.steerSessionMessage(this.run.id, text, mapAttachments(attachments) ?? undefined);
        dbg("store", "mid-turn steer", { agent: this.run.agent, len: text.length });
      } else if (this.useStreamSession && this.sessionAlive) {
        // Optimistic user message — matches the pattern in startSession().
        // Content-based dedup in _reduce(user_message) prevents double display
        // when the backend's UserMessage bus event arrives.
        this._pushOptimisticUser(text, attachments);
        await api.sendSessionMessage(
          this.run.id,
          text,
          mapAttachments(attachments) ?? undefined,
          hasSkills ? skills : undefined,
        );
        if (hasSkills) dbg("skills", "store send with skills", { count: skills!.length });
        if (this.isKnownSlashCommand(text)) {
          dbg("store", "skip response timeout for slash command", { cmd: text.split(" ")[0] });
        } else {
          this._startResponseTimeout(this.run.id);
        }
      } else if (this.useStreamSession && this.run.agent === "codex") {
        // Stopped Codex app-server session (user clicked Stop, then sent a new message):
        // re-spawn the actor with THIS message via start_session. The backend resumes the
        // thread from conversation_ref (thread/resume) and sends this message as the turn.
        // (Without this branch it falls through to send_chat_message and the backend rejects
        // it: "requires execution_path=pipe_exec, got SessionActor". Claude stopped runs are
        // resumed via resumeSession elsewhere, so this is scoped to Codex.)
        this._useChatTimelineForRun = true;
        this._pushOptimisticUser(text, attachments);
        if (this.run) this.run = { ...this.run, status: "running" };
        const mw = getEventMiddleware();
        mw.subscribeCurrent(this.run!.id, this);
        this._wsSubscribeNewSession(this.run!.id);
        await api.startSession(
          this.run!.id,
          undefined,
          undefined,
          text,
          mapAttachments(attachments) ?? undefined,
          this.platformId || undefined,
          startupPermissionMode,
          skills,
        );
        this._startSpawnTimeout(this.run!.id);
      } else if (
        !this.useStreamSession &&
        this.capabilities.execution.busEvents &&
        this.run?.conversation_ref?.kind === "codex_thread"
      ) {
        // Codex bus-events resume path: stopped run with codex_thread → auto resume
        if (!this._useChatTimelineForRun) {
          this._useChatTimelineForRun = true;
          const mw = getEventMiddleware();
          mw.subscribeCurrent(this.run!.id, this);
          this._wsSubscribeNewSession(this.run!.id);
        }
        const clientUuid = this._pushOptimisticUser(text, attachments);
        this._setPhase("running");
        await api.sendChatMessage(
          this.run!.id,
          text,
          attachments.length > 0 ? attachments : undefined,
          undefined,
          clientUuid,
          startupPermissionMode,
        );
      } else if (this.useStreamSession) {
        // Dead session_actor that's NOT a resumable Codex run (e.g. a stopped Claude
        // process): routing to sendChatMessage would be rejected (pipe_exec only), so
        // surface a clear error instead of a guaranteed-to-fail IPC. Codex stop→resend
        // is handled by the codex re-spawn branch above. (master: clear-error fallback)
        throw new Error("Session ended — start a new session to continue.");
      } else {
        // PipeExec (Codex): one-shot process per message, no liveness concept.
        this._setPhase("running");
        await api.sendChatMessage(
          this.run.id,
          text,
          attachments.length > 0 ? attachments : undefined,
          undefined,
          undefined,
          startupPermissionMode,
        );
      }
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  /** Send a queue operation while a turn is running. Steer routes to the agent's native
   * mid-turn injection (Pi steer, Claude interrupt+resend, Grok _x.ai/interject); follow-up
   * queues the message locally for delivery after the current turn completes. */
  async sendQueuedMessage(
    text: string,
    attachments: Attachment[],
    mode: "steer" | "followUp",
  ): Promise<void> {
    if (!this.capabilities.runtime.steer) return;
    if (!this.run) return;
    if (!this.capabilities.runtime.attachments) attachments = [];
    if (!this.sessionAlive) {
      this.error = "会话已结束，请先恢复会话再排队发送消息。";
      return;
    }
    this._dismissCompletedTodoPanel();
    this.error = "";
    // The composer can still have a stale `running` prop for one render after the
    // turn has become idle. Do not leave that follow-up stranded in the local queue;
    // send it through the normal idle-session path immediately.
    if (mode === "followUp" && !this.isRunning) {
      await this.sendMessage(text, attachments);
      return;
    }
    if (mode === "steer") {
      const runId = this.run.id;
      // Add the optimistic entry before the actor emits its authoritative UserMessage. This
      // keeps the reducer's content/cliUuid dedup path active; adding it after the IPC resolves
      // races the backend event and renders the same guidance twice.
      const optimisticId = this._pushOptimisticUser(text, attachments);
      try {
        if (this.run.agent === "pi") {
          await api.sendPiSteer(runId, text, mapAttachments(attachments) ?? undefined);
        } else {
          await api.steerSessionMessage(runId, text, mapAttachments(attachments) ?? undefined);
        }
      } catch (error) {
        if (this.run?.id === runId) this._removeOptimisticUser(optimisticId);
        throw error;
      }
    } else {
      this.queuedMessages = [
        ...this.queuedMessages,
        { id: uuid(), text, attachments: [...attachments] },
      ];
    }
  }

  /** Remove a locally queued message and return it to the composer for editing. */
  takeQueuedMessage(id: string): QueuedMessage | null {
    const item = this.queuedMessages.find((message) => message.id === id);
    if (!item) return null;
    this.queuedMessages = this.queuedMessages.filter((message) => message.id !== id);
    return { ...item, attachments: [...item.attachments] };
  }

  /** Delete a locally queued message without sending it. */
  deleteQueuedMessage(id: string): void {
    this.queuedMessages = this.queuedMessages.filter((message) => message.id !== id);
  }

  /** Immediately submit a previously queued message to the current turn. */
  async steerQueuedMessage(id: string): Promise<void> {
    const index = this.queuedMessages.findIndex((message) => message.id === id);
    if (
      index < 0 ||
      !this.capabilities.runtime.steer ||
      !this.run ||
      !["claude", "pi", "dsh"].includes(this.run.agent) ||
      !this.sessionAlive
    ) {
      return;
    }
    const runId = this.run.id;
    const item = this.queuedMessages[index];
    if (!this.capabilities.runtime.attachments) item.attachments = [];
    this.queuedMessages = this.queuedMessages.filter((message) => message.id !== id);
    let optimisticId: string | null = null;
    this._dismissCompletedTodoPanel();
    try {
      // The backend emits UserMessage before the steer IPC resolves. Push first so the live
      // reducer merges that event into this entry instead of appending a duplicate.
      if (this.run?.id === runId) {
        optimisticId = this._pushOptimisticUser(item.text, item.attachments);
      }
      if (this.run.agent === "pi") {
        await api.sendPiSteer(runId, item.text, mapAttachments(item.attachments) ?? undefined);
      } else {
        await api.steerSessionMessage(
          runId,
          item.text,
          mapAttachments(item.attachments) ?? undefined,
        );
      }
    } catch (error) {
      if (this.run?.id === runId) {
        if (optimisticId) this._removeOptimisticUser(optimisticId);
        this.queuedMessages = [
          ...this.queuedMessages.slice(0, index),
          item,
          ...this.queuedMessages.slice(index),
        ];
      }
      throw error;
    }
  }

  /** Send composer-owned messages after the current turn reports idle. */
  private async flushQueuedMessages(): Promise<void> {
    if (
      this.queueFlushInFlight ||
      !this.run ||
      !["claude", "pi", "dsh"].includes(this.run.agent) ||
      !this.sessionAlive ||
      this.queuedMessages.length === 0
    ) {
      return;
    }
    const runId = this.run.id;
    this.queueFlushInFlight = true;
    try {
      while (this.run?.id === runId && this.sessionAlive && this.queuedMessages.length > 0) {
        const item = this.queuedMessages[0];
        // The composer queue is deliberately flushed only after the previous turn is idle.
        // Pi's native follow_up RPC is for enqueueing while a turn is still streaming; using it
        // after idle can persist the user message without starting the next model turn. The
        // generic actor send path starts a fresh turn for Pi, DSH, and Claude alike.
        await api.sendSessionMessage(
          runId,
          item.text,
          mapAttachments(item.attachments) ?? undefined,
        );
        if (this.run?.id === runId && this.queuedMessages[0]?.id === item.id) {
          this._pushOptimisticUser(item.text, item.attachments);
          this.queuedMessages = this.queuedMessages.slice(1);
        }
      }
    } catch (error) {
      this.error = String(error);
    } finally {
      this.queueFlushInFlight = false;
    }
  }

  /** Interrupt current turn. Falls back to kill if interrupt fails. */
  async interrupt(): Promise<void> {
    if (!this.run || !this.isRunning) return;
    if (!this.sessionAlive) {
      // Phase shows running but session is not alive — force cleanup
      this._setPhase("stopped");
      this.run = { ...this.run, status: "stopped" };
      return;
    }
    try {
      dbg("store", "interrupt current turn");
      const runId = this.run.id;
      await withTimeout(
        Promise.resolve(api.sendSessionControl(runId, "interrupt")),
        SESSION_CONTROL_TIMEOUT_MS,
        "中断当前回合超时",
      );
      // A control acknowledgement means the agent accepted the interrupt. Keep
      // the UI responsive even if the lifecycle event is delayed or the realtime
      // transport drops it; the backend will persist the same idle boundary.
      if (this.run?.id === runId && this.useStreamSession && this.phase === "running") {
        this._setPhase("idle");
        this.run = { ...this.run, status: "idle" };
      }
    } catch (e) {
      // interrupt failed (timeout or actor dead) — kill process directly
      dbg("store", "interrupt failed, killing process:", e);
      try {
        await withTimeout(
          Promise.resolve(api.stopSession(this.run.id)),
          SESSION_STOP_TIMEOUT_MS,
          "停止会话超时",
        );
      } catch {
        // Session may already be dead
      }
      this._setPhase("stopped");

      this.run = { ...this.run, status: "stopped" };
    }
  }

  /** Stop the current session. */
  async stop(): Promise<void> {
    if (!this.run) return;
    this._stopping = true;
    this._clearResponseTimeout();
    try {
      if (this.sessionAlive) {
        // Try graceful interrupt first if agent is currently running.
        // Skip during "spawning" — CLI hasn't initialized yet, interrupt would
        // wait for a control_response that may never come.
        if (this.phase === "running") {
          try {
            dbg("store", "sending interrupt before stop");
            await withTimeout(
              Promise.resolve(api.sendSessionControl(this.run.id, "interrupt")),
              SESSION_CONTROL_TIMEOUT_MS,
              "中断当前回合超时",
            );
            // Brief wait for CLI to process the interrupt
            await new Promise((r) => setTimeout(r, 500));
          } catch (e) {
            dbg("store", "interrupt failed (proceeding to kill):", e);
          }
        }
        try {
          await withTimeout(
            Promise.resolve(api.stopSession(this.run.id)),
            SESSION_STOP_TIMEOUT_MS,
            "停止会话超时",
          );
        } catch (e) {
          // Session may already be dead (process exited, actor cleaned up).
          // Force frontend state to stopped regardless.
          dbgWarn("store", "stopSession failed (forcing stopped):", e);
        }
      } else {
        await api.stopRun(this.run.id);
      }
    } catch (e) {
      dbgWarn("store", "stop failed:", e);
    } finally {
      // Always clean up frontend state, even if backend calls failed.
      // If the process is already dead, the UI must not stay stuck in "running".
      this._setPhase("stopped");

      this.run = { ...this.run!, status: "stopped" };
      this._stopping = false;
    }
  }

  // ── Resume ──

  private _resumeGuard = new OpGuard();

  /** Whether a resume/continue/fork operation is currently in progress. */
  get resumeInFlight(): boolean {
    return this._resumeGuard.busy;
  }

  /** Resume/continue/fork a finished session. Returns the target run ID.
   *  Avoids flash by NOT calling reset() — clears content fields individually
   *  and uses replayOnly=true so replay doesn't overwrite phase.
   *  When initialMessage is provided, the message is written to CLI stdin atomically
   *  with the spawn — no separate send_session_message needed. */
  async resumeSession(
    runId: string,
    mode: SessionMode,
    initialMessage?: string,
    attachments?: Attachment[],
  ): Promise<string | null> {
    if (!this._resumeGuard.acquire()) return null;

    try {
      let run = await api.getRun(runId);
      if (!this._resumeGuard.isMounted) return runId;

      if (run.remote_host_name && !getAgentCapabilities(run.agent).runtime.remote) {
        throw new Error(`The ${run.agent} runtime does not support remote hosts`);
      }
      if (mode === "fork" && !getAgentCapabilities(run.agent).runtime.fork) {
        throw new Error(`Fork is not supported by the ${run.agent} runtime`);
      }
      let metaActive = ACTIVE_PHASES.includes(run.status as SessionPhase);
      if (metaActive && mode !== "fork") {
        // meta.json says "running" — likely a stale status from an orphaned/crashed session.
        // Try to stop it first (kills process if alive, updates meta if not), then proceed.
        dbg("store", "resumeSession: meta says active, attempting stop first", {
          runId,
          status: run.status,
        });
        try {
          await api.stopRun(runId);
          // Re-fetch meta after stop to get updated status
          const refreshed = await api.getRun(runId);
          run = refreshed;
          metaActive = ACTIVE_PHASES.includes(run.status as SessionPhase);
        } catch (e) {
          dbgWarn("store", "resumeSession: stop attempt failed:", e);
        }
        if (metaActive) {
          // Still running after stop attempt — genuinely active, can't resume
          throw new Error("Session is still running");
        }
      }
      // A brand-new session (including a cross-Agent handoff) intentionally has no
      // session_id yet. Only resume/continue require an existing provider session.
      if ((mode === "resume" || mode === "continue") && !run.session_id) {
        throw new Error("No session_id available for resume");
      }

      // Invalidate any concurrent loadRun, then snapshot the gen for our own
      // stale-check (a later loadRun would bump _loadGen and we'd see the change).
      const loadGen = ++this._loadGen;
      const resumeT0 = performance.now();

      // ★ Phase 1: async data fetch BEFORE clearing state (avoids flash)
      const isStream = run.execution_path === "session_actor"; // run-level, not agent identity
      let snapshotBody: string | null = null;
      let busEvents: BusEvent[] = [];

      if (isStream) {
        // Snapshot cache is terminal-only. A live/idle session can still change,
        // so resume must rebuild from the durable event log as well.
        if (TERMINAL_PHASES.includes(run.status as SessionPhase)) {
          try {
            snapshotBody = await snapshotCache.readSnapshot(runId, run.status);
          } catch {
            /* IDB unavailable */
          }
        }
        if (!this._resumeGuard.isMounted) return runId;
        if (!snapshotBody) {
          busEvents = await api.getBusEvents(runId);
          if (!this._resumeGuard.isMounted) return runId;
          dbg("store", "resumeSession: fetched", busEvents.length, "bus events for replay");
        }
      }

      // ★ Phase 2: clear + set run metadata (sync frame, no await)
      this.run = run;
      this.agent = run.agent;
      this.platformId = run.platform_id ?? null;
      this._clearContentState();
      this._useChatTimelineForRun = true; // resumed runs use the shared bus timeline

      // ★ Phase 3: apply snapshot or events + force invalidate
      let reducerMs = 0;
      let snapshotHit = false;
      if (isStream) {
        if (snapshotBody && this._tryApplySnapshot(snapshotBody)) {
          snapshotHit = true;
          this._wsSubscribeWithSeq(runId, this._lastProcessedSeq);
        } else {
          // Fallback: snapshot corrupted → re-fetch events if needed
          if (!busEvents.length && snapshotBody) {
            busEvents = await api.getBusEvents(runId);
            if (!this._resumeGuard.isMounted) return runId;
          }
          if (busEvents.length > 0) {
            const ms = await this.applyEventBatchAsync(busEvents, {
              replayOnly: true,
              isStale: () => !this._resumeGuard.isMounted || loadGen !== this._loadGen,
            });
            if (ms === null) return runId;
            reducerMs = ms;
          }
          // Always subscribe — even empty history needs real-time events
          this._wsSubscribeAfterLoad(runId, busEvents);
        }

        // Resume makes session go live → old snapshot is always stale
        snapshotCache.deleteSnapshot(runId).catch(() => {});
      }

      dbg("store", "resumeSession", {
        total: Math.round(performance.now() - resumeT0),
        snapshotHit,
        reducer: Math.round(reducerMs),
        entries: this.timeline.length,
      });

      // Restore the persisted per-run model after replay.
      if (run.model?.trim()) {
        dbg("store", "resume: restore run model from meta:", run.model);
        this.model = run.model.trim();
      }

      // Optimistic user message: add AFTER replay so it appears at the end of timeline.
      // Must be before startSession IPC so the user sees their message immediately.
      // Backend's UserMessage bus event will be deduped by content match in _reduce.
      if (initialMessage && !isAgentInternalContext(initialMessage)) {
        this._pushOptimisticUser(initialMessage, attachments ?? undefined);
      }

      // Explicitly set phase — replay didn't touch it
      this._setPhase("spawning");

      let targetRunId = runId;

      // A resumed run can be a brand-new run created by an Agent handoff. The
      // caller cannot subscribe after resumeSession() returns because
      // startSession() emits session_init/run_state while this method is still
      // awaiting the backend. Subscribe before spawning so the new run's
      // startup events are not routed to the previous Agent's subscription.
      getEventMiddleware().subscribeCurrent(targetRunId, this);

      if (mode === "fork") {
        targetRunId = await this._handleFork(runId);
      } else {
        const sessionId = run.session_id;
        const backendAtt = attachments ? (mapAttachments(attachments) ?? undefined) : undefined;
        dbg("store", "resumeSession", {
          runId,
          targetRunId,
          mode,
          sessionId,
          hasMessage: !!initialMessage,
          attachments: backendAtt?.length ?? 0,
        });
        await api.startSession(
          targetRunId,
          mode,
          sessionId ?? undefined,
          initialMessage,
          backendAtt,
          run.platform_id ?? undefined,
          resolveStartupPermissionMode(run.agent, this.permissionMode, this.piPermissionState.mode),
        );
      }
      // Bus events via applyEvent (live) will transition phase:
      // - With message: spawning → running → idle (from CLI result)
      // - Without message: spawning → idle (synthetic, waiting for user input)

      // Timeout guard: if CLI never emits session_init, the UI would spin forever.
      // Fork skips this — connectSession() handles its own spawn timeout.
      if (mode !== "fork") {
        this._startSpawnTimeout(targetRunId);
        if (
          initialMessage &&
          !isAgentInternalContext(initialMessage) &&
          !this.isKnownSlashCommand(initialMessage)
        ) {
          this._startResponseTimeout(targetRunId);
        } else if (isAgentInternalContext(initialMessage ?? "")) {
          // Internal context is not a user turn
          // waiting for a normal response, so the regular response timeout would
          // incorrectly fail a healthy target CLI.
          dbg("store", "skip response timeout for internal context");
        } else if (initialMessage) {
          dbg("store", "skip response timeout for slash command (resume)", {
            cmd: initialMessage.split(" ")[0],
          });
        }
        // No initialMessage → no response timeout (just waiting for user input)
      }

      return targetRunId;
    } catch (e) {
      if (!this._resumeGuard.isMounted) return null;
      this.error = String(e);
      this._setPhase("failed");
      dbgWarn("store", "resumeSession failed:", e);
      return null;
    } finally {
      this._resumeGuard.release();
    }
  }

  /** Step 1 of two-step fork: create forked run, replay parent events.
   *  Returns the new run ID. Called from resumeSession when mode === "fork".
   *  Step 2 (connectSession) is called by the frontend after dismissing the fork overlay. */
  private async _handleFork(runId: string): Promise<string> {
    dbg("store", "resumeSession: two-step fork", { runId });
    const loadGen = this._loadGen;

    // Clear any subscription to prevent source RunState(stopped) interference
    getEventMiddleware().subscribeCurrent("", this);

    // Step 1: One-shot fork (backend does fork_oneshot, returns new run_id with new session_id)
    const newRunId = await api.forkSession(runId);
    if (!this._resumeGuard.isMounted) throw new Error("Unmounted during fork");

    const newRun = await api.getRun(newRunId);
    if (!this._resumeGuard.isMounted) throw new Error("Unmounted during fork");

    this.run = newRun;

    // Reset display state — start fresh for the fork run.
    // Without this, the source session's timeline stays in state and
    // message_delta events accumulate as duplicate streamingText.
    this._clearContentState();
    this._useChatTimelineForRun = true;

    // Replay copied parent events for immediate display.
    // Subscribe to live events AFTER replay so a live applyEvent during chunked
    // replay can't slip in and be overwritten by `_commitReduceCtx` snapshotting
    // a now-stale `this.timeline`.
    const allForkEvents = await api.getBusEvents(newRunId);
    if (!this._resumeGuard.isMounted) throw new Error("Unmounted during fork");
    const newEvents = allForkEvents.filter((ev) => ev.run_id === newRunId);
    if (newEvents.length > 0) {
      dbg("store", "fork: replaying", newEvents.length, "parent events");
      const ms = await this.applyEventBatchAsync(newEvents, {
        replayOnly: true,
        isStale: () => !this._resumeGuard.isMounted || loadGen !== this._loadGen,
      });
      // Match the existing fork pattern (line ~2157): treat a stale/unmount
      // mid-replay as a fatal interruption so the caller's catch path runs.
      if (ms === null) throw new Error("Stale during fork replay");
    }
    // Subscribe to NEW run — live events from stream-json will route here.
    getEventMiddleware().subscribeCurrent(newRunId, this);
    dbg("store", "fork: middleware subscribed to new run", newRunId);
    this._wsSubscribeAfterLoad(newRunId, allForkEvents);

    // Step 2 (stream-json resume) is NOT started here.
    // handleResume will dismiss the overlay first, then call connectSession()
    // so the user sees normal "Starting session..." spinner instead of the fork overlay.
    dbg("store", "fork: step 1 complete, returning newRunId for step 2", {
      newRunId,
      sessionId: newRun.session_id,
    });
    return newRunId;
  }

  /**
   * Step 2 of two-step fork: establish stream-json connection to an already-forked session.
   * Called from handleResume AFTER the fork overlay is dismissed, so the user sees
   * the normal "Starting session..." spinner instead of the fork overlay.
   */
  async connectSession(
    runId: string,
    sessionId?: string,
    permissionModeOverride?: string,
  ): Promise<void> {
    const sid = sessionId ?? this.run?.session_id;
    if (!sid) throw new Error("No session_id available for connectSession");
    dbg("store", "connectSession: establishing stream-json connection", {
      runId,
      sessionId: sid,
      permissionModeOverride,
    });
    this._wsSubscribeNewSession(runId);
    this._setPhase("spawning");
    await api.startSession(
      runId,
      "resume",
      sid,
      undefined,
      undefined,
      this.platformId || undefined,
      resolveStartupPermissionMode(
        this.agent,
        this.permissionMode,
        this.piPermissionState.mode,
        permissionModeOverride,
      ),
    );
    this._startSpawnTimeout(runId);
  }

  // ── WS subscribe helpers (browser-only, no-op on desktop) ──

  /** Browser: notify WS server to start pushing real-time events after history load */
  private _wsSubscribeAfterLoad(runId: string, events: BusEvent[]): void {
    if (getTransport().isDesktop()) return;
    const maxSeq =
      events.length > 0
        ? (((events[events.length - 1] as Record<string, unknown>)._seq as number) ?? 0)
        : 0;
    getTransport().subscribeRun(runId, maxSeq);
  }

  private _wsSubscribeNewSession(runId: string): void {
    if (getTransport().isDesktop()) return;
    getTransport().subscribeRun(runId, 0);
  }

  private _wsSubscribeWithSeq(runId: string, lastSeq: number): void {
    if (getTransport().isDesktop()) return;
    getTransport().subscribeRun(runId, lastSeq);
  }

  /** Call from page cleanup to prevent stale async writes after unmount. */
  unmountGuards(): void {
    this._resumeGuard.unmount();
    this._clearSpawnTimeout();
  }

  /** Update MCP servers (e.g. after getMcpStatus refresh). Deduplicates by name. */
  updateMcpServers(servers: McpServerInfo[]): void {
    this.mcpServers = dedupeMcpServersByName(servers);
  }

  /** Resolve an AskUserQuestion tool: transition from ask_pending → success.
   *  `answersMap` (question-text → selected label) is stored on tool_use_result so the
   *  resolved card highlights each question's choice (needed for multi-question). */
  resolveAskQuestion(toolUseId: string, answer: string, answersMap?: Record<string, string>): void {
    dbg("store", "resolveAskQuestion", { toolUseId, answer });
    const tIdx = this._findToolIdx(null, toolUseId);
    if (tIdx >= 0) {
      const old = this.timeline[tIdx] as Extract<TimelineEntry, { kind: "tool" }>;
      const u = [...this.timeline];
      const tool = { ...old.tool, status: "success" as const, output: { answer } };
      if (answersMap) {
        tool.tool_use_result = {
          ...(old.tool.tool_use_result as Record<string, unknown> | undefined),
          answers: answersMap,
        };
      }
      u[tIdx] = { ...old, tool };
      this.timeline = u;
    }
    // Mirror to tools[] only in non-stream mode
    if (!this.useStreamSession) {
      const hIdx = this._findHeIdx(null, toolUseId);
      if (hIdx >= 0) {
        const u = [...this.tools];
        u[hIdx] = { ...u[hIdx], status: "done", hook_type: "PostToolUse" };
        this.tools = u;
      }
    }
  }

  /** Answer an AskUserQuestion tool.
   *  - Codex/Grok: the tool_use_id IS the pending request id → reply via the
   *    JSON-RPC `respond_user_input` channel (NOT a new user turn).
   *  - Claude: send the answer as a follow-up user message. */
  async answerToolQuestion(toolUseId: string, answer: string): Promise<void> {
    if (!this.run) return;
    dbg("store", "tool answer", { toolUseId, answer });

    // Multi-question answers arrive JSON-encoded from InlineToolCard (submitAllAskAnswers):
    // { __askMulti: true, byId: {qid: label}, byText: {questionText: label} }.
    let multi: { byId: Record<string, string>; byText: Record<string, string> } | null = null;
    try {
      const p = JSON.parse(answer);
      if (p && p.__askMulti) multi = { byId: p.byId ?? {}, byText: p.byText ?? {} };
    } catch {
      /* plain single answer */
    }

    // Capture the question id before resolving (resolveAskQuestion overwrites output).
    const tIdx = this._findToolIdx(null, toolUseId);
    const tool = tIdx >= 0 ? (this.timeline[tIdx] as { tool?: BusToolItem }).tool : undefined;

    // Display summary: readable for multi-question, raw for single.
    const display = multi
      ? Object.entries(multi.byText)
          .map(([q, a]) => `${q}: ${a}`)
          .join("\n")
      : answer;
    // Pass the per-question answers so the resolved card highlights each choice.
    this.resolveAskQuestion(toolUseId, display, multi ? multi.byText : undefined);

    if (this.run.agent === "codex" || this.run.agent === "grok") {
      try {
        const answers: Record<string, string[]> = {};
        if (multi && Object.keys(multi.byId).length > 0) {
          for (const [qid, label] of Object.entries(multi.byId)) answers[qid] = [label];
        } else {
          const questions = (tool?.input?.questions ?? []) as Array<{ id?: string }>;
          answers[questions[0]?.id ?? "0"] = [answer];
        }
        await api.respondUserInput(this.run.id, toolUseId, answers);
      } catch (e) {
        dbgWarn("store", "native respondUserInput failed:", e);
        this.error = String(e);
        throw e;
      }
      return;
    }

    try {
      // Send the user's answer as a follow-up message.
      // The session should be alive (idle phase) after the CLI auto-failed AskUserQuestion.
      if (this.sessionAlive) {
        await api.sendSessionMessage(this.run.id, display);
      } else {
        dbgWarn("store", "session not alive for tool answer, skipping send");
      }
    } catch (e) {
      dbgWarn("store", "tool answer failed:", e);
      this.error = String(e);
      throw e;
    }
  }

  /** Unified permission resolution: traverses ALL timeline + subTimeline entries
   *  (no early return) to handle duplicate requestId entries from fallback/synthetic sources.
   *  - "deny" → permission_denied
   *  - "allow" → running (skips AskUserQuestion to avoid double-submit) */
  private _resolvePermission(action: "allow" | "deny", requestId: string): void {
    dbg("store", `resolvePermission${action === "allow" ? "Allow" : "Deny"}`, { requestId });
    const targetStatus = action === "allow" ? ("running" as const) : ("permission_denied" as const);
    const skipAsk = action === "allow";
    let changed = false;
    const u = [...this.timeline];
    for (let i = 0; i < u.length; i++) {
      const entry = u[i];
      if (entry.kind !== "tool") continue;
      if (
        entry.tool.status === "permission_prompt" &&
        entry.tool.permission_request_id === requestId
      ) {
        if (!(skipAsk && entry.tool.tool_name === "AskUserQuestion")) {
          u[i] = { ...entry, tool: { ...entry.tool, status: targetStatus } };
          changed = true;
        }
      }
      if (entry.subTimeline) {
        let subChanged = false;
        const newSub = [...entry.subTimeline];
        for (let j = 0; j < newSub.length; j++) {
          const sub = newSub[j];
          if (
            sub.kind === "tool" &&
            sub.tool.status === "permission_prompt" &&
            sub.tool.permission_request_id === requestId &&
            !(skipAsk && sub.tool.tool_name === "AskUserQuestion")
          ) {
            newSub[j] = { ...sub, tool: { ...sub.tool, status: targetStatus } };
            subChanged = true;
          }
        }
        if (subChanged) {
          u[i] = { ...u[i], subTimeline: newSub } as TimelineEntry;
          changed = true;
        }
      }
    }
    if (changed) this.timeline = u;
  }

  resolvePermissionDeny(requestId: string): void {
    this._resolvePermission("deny", requestId);
  }

  resolvePermissionAllow(requestId: string): void {
    this._resolvePermission("allow", requestId);
  }

  /** Handle chat-done event (pipe mode). */
  handleChatDone(_done: { ok: boolean; code: number; error?: string }): void {
    if (!this.run) return;
    // When useChatTimeline is true, RunState bus event handles phase transition
    if (this.useChatTimeline) return;

    if (!this.useStreamSession) {
      this._setPhase("completed");
      api
        .getRun(this.run.id)
        .then((r) => {
          this.run = r;
        })
        .catch((e) => dbgWarn("store", "getRun after pipe-exec done failed:", e));
    }
  }

  /** Handle chat-delta event (pipe-exec mode). */
  handleChatDelta(text: string, xtermRef?: { writeText(s: string): void }): void {
    if (!this.run) return;
    // When useChatTimeline is true, bus events handle text rendering
    if (this.useChatTimeline) return;
    if (!this.useStreamSession && xtermRef) {
      xtermRef.writeText(text);
    }
  }

  // ── Private ──

  /** Whether to skip tools (HookEvent[]) mirror writes. Stream mode tools are in timeline only. */
  private _isStreamMode(ctx: ReduceCtx | null): boolean {
    return ctx ? ctx.isStream : this.useChatTimeline;
  }

  /**
   * Resolve stale tool entries to "error" across main timeline and all subTimelines.
   * Used by idle/spawning/control_cancelled cleanup.
   */
  private _resolveStaleTools(
    predicate: (tool: BusToolItem) => boolean,
    ctx: ReduceCtx | null,
  ): void {
    const tl = ctx ? ctx.tl : this.timeline;
    let cloned = !!ctx; // ctx.tl is already a mutable reference

    for (let i = 0; i < tl.length; i++) {
      const e = tl[i];
      if (e.kind !== "tool") continue;

      // Top-level tool
      let parentUpdated = e;
      if (predicate(e.tool)) {
        if (!cloned) {
          this.timeline = [...this.timeline];
          cloned = true;
        }
        parentUpdated = { ...e, tool: { ...e.tool, status: "error" as const } };
        const target = ctx ? ctx.tl : this.timeline;
        target[i] = parentUpdated;
        dbg("store", "resolved stale tool", { id: e.id, name: e.tool.tool_name });
        // Don't continue: even if top-level matched, still scan and converge subTimeline children
      }

      // subTimeline children
      const sub = parentUpdated.subTimeline;
      if (!sub) continue;
      let subChanged = false;
      let newSub = sub;
      for (let j = 0; j < newSub.length; j++) {
        const child = newSub[j];
        if (child.kind !== "tool" || !predicate(child.tool)) continue;
        if (!subChanged) {
          newSub = [...newSub];
          subChanged = true;
        }
        newSub[j] = { ...child, tool: { ...child.tool, status: "error" as const } };
        dbg("store", "resolved stale sub-tool", { id: child.id, name: child.tool.tool_name });
      }
      if (subChanged) {
        if (!cloned) {
          this.timeline = [...this.timeline];
          cloned = true;
        }
        const target = ctx ? ctx.tl : this.timeline;
        target[i] = { ...parentUpdated, subTimeline: newSub };
      }
    }
  }

  /** Core reducer: apply a single bus event. When ctx is null, mutates $state directly.
   *  replayOnly=true skips phase and error assignments (used during resume replay). */
  private _reduce(ev: BusEvent, ctx: ReduceCtx | null, replayOnly = false): void {
    // Shorthand accessors — either batch ctx or this (reactive)
    const getTl = () => (ctx ? ctx.tl : this.timeline);
    const getHe = () => (ctx ? ctx.he : this.tools);
    const getSeenMsg = () => (ctx ? ctx.seenMessageIds : this._seenMessageIds);
    const getSeenTool = () => (ctx ? ctx.seenToolIds : this._seenToolIds);

    switch (ev.type) {
      case "agent_mode_update":
        this.sessionMode = ev.current_mode_id;
        this.sessionModes = ev.available_modes ?? this.sessionModes;
        break;
      case "session_init":
        if (ev.model) {
          if (ctx) {
            // When replaying historical events, the run's persisted model reflects the latest
            // choice selected by the user. Only adopt ev.model if no run model exists yet.
            if (!this.run?.model) {
              ctx.model = ev.model;
            }
          } else if (this.agent === "pi") {
            this.model = ev.model;
            if (this.run) this.run = { ...this.run, model: ev.model };
          } else if (ev.capabilities?.protocol.sessionSetModel || ev.model_options?.length) {
            this.model = ev.model;
            if (this.run) this.run = { ...this.run, model: ev.model };
          } else if (!this.run?.model) {
            this.model = ev.model;
          }
        }
        // Persist the CLI's new session_id (important for fork — CLI generates a new ID)
        if (ev.session_id) {
          if (ctx) ctx.sessionId = ev.session_id;
          else if (this.run) {
            dbg("store", "session_init: updating session_id", {
              old: this.run.session_id,
              new: ev.session_id,
            });
            this.run = { ...this.run, session_id: ev.session_id };
          }
        }
        // Store CLI slash commands (session-specific, includes custom .claude/commands/)
        // CLI system/init returns slash_commands as string[] (just names) or CliCommand[]
        if (ev.slash_commands !== undefined) {
          this.sessionCommands = ev.slash_commands.map((c: unknown) =>
            typeof c === "string" ? { name: c, description: "", aliases: [] } : (c as CliCommand),
          );
          if (this.sessionCommands.length > 0) {
            // Replace the cached live catalog rather than accumulating stale plugin commands.
            this.nativeSlashCommandsByAgent[this.agent] = [...this.sessionCommands];
          }
        }
        if (ev.capabilities !== undefined) {
          this.sessionCapabilities = ev.capabilities;
        }
        if (ev.model_options !== undefined) {
          this.sessionModels = ev.model_options;
        }
        if (this.agent === "pi" && ev.commands_loaded === true) {
          // An empty array is still a successful discovery result.
          if (ev.slash_commands === undefined) this.sessionCommands = [];
          this.piCommandsState = "ready";
          this.piCommandsReady = true;
        }
        // Store MCP servers (per-session state, deduplicate by name)
        if (ev.mcp_servers && ev.mcp_servers.length > 0) {
          this.mcpServers = dedupeMcpServersByName(ev.mcp_servers);
        }
        // Store CLI verbose fields
        if (ev.claude_code_version) {
          this.cliVersion = ev.claude_code_version;
          // Only update global installed version from live sessions,
          // not from historical replay (which carries the old version).
          if (!replayOnly) {
            updateInstalledVersion(ev.claude_code_version);
            try {
              localStorage.setItem("agentcabin:cli-version", ev.claude_code_version);
            } catch {
              /* ignore */
            }
          }
        }
        // eslint-disable-next-line no-case-declarations -- scoped to session_init block
        const normalizedPermMode = ev.permissionMode
          ? normalizePermissionMode(ev.permissionMode)
          : undefined;
        if (normalizedPermMode && !this.permissionModeSetByUser) {
          this.permissionMode = normalizedPermMode;
        } else if (normalizedPermMode && this.permissionModeSetByUser) {
          dbg("store", "session_init permissionMode skipped — user already set", {
            cliValue: normalizedPermMode,
            userValue: this.permissionMode,
          });
          // CLI may have reset permission mode after compaction — re-send to resync.
          // Only in live mode (not batch replay) and when the run has a valid id.
          if (
            !ctx &&
            this.sessionAlive &&
            this.capabilities.protocol.permissionModeControl &&
            this.run?.id &&
            normalizedPermMode !== this.permissionMode
          ) {
            dbg("store", "resync permissionMode to CLI after compaction", {
              mode: this.permissionMode,
            });
            api.setPermissionMode(this.run.id, this.permissionMode).catch((e) => {
              dbgWarn("store", "permissionMode resync failed", e);
            });
          }
        }
        if (ev.fast_mode_state) this.fastModeState = ev.fast_mode_state;
        if (ev.apiKeySource) this.apiKeySource = ev.apiKeySource;
        if (ev.agents && ev.agents.length > 0) this.availableAgents = ev.agents;
        // Runtime discovery is authoritative. Claude omits an empty skills array from the
        // serialized event, so clear preloaded names explicitly when its session initializes.
        if (this.agent === "claude") this.availableSkills = ev.skills ?? [];
        else if (ev.skills) this.availableSkills = ev.skills;
        if (ev.plugins) this.availablePlugins = ev.plugins;
        // Always assign (not truthy-guarded) so CLI returning empty values clears stale state
        this.sessionCwd = ev.cwd ?? "";
        this.sessionTools = ev.tools ?? [];
        this.outputStyle = ev.output_style ?? "";
        this.sessionInitReceived = true;
        dbg("store", "session_init: cli verbose fields", {
          cliVersion: this.cliVersion,
          permissionMode: this.permissionMode,
          fastModeState: this.fastModeState,
          apiKeySource: this.apiKeySource,
          agents: this.availableAgents.length,
          skills: this.availableSkills.length,
          plugins: this.availablePlugins.length,
          sessionCwd: this.sessionCwd,
          sessionTools: this.sessionTools.length,
          outputStyle: this.outputStyle,
        });
        break;

      case "rate_limit_event":
        this.rateLimitStatus = ev.status;
        this.rateLimitType = ev.rate_limit_type ?? "";
        this.rateLimitUtilization = ev.utilization ?? null;
        this.rateLimitResetsAt = ev.resets_at ?? null;
        dbg("store", "rate_limit_event", {
          status: ev.status,
          type: ev.rate_limit_type,
          utilization: ev.utilization,
        });
        break;

      case "message_delta":
        this._clearTimeoutError();
        if (ev.parent_tool_use_id) {
          this._appendSubTimelineStreamingDelta(ev.parent_tool_use_id, "content", ev.text, ctx);
          break;
        }
        // Mark thinking end: first text delta after thinking started
        if (this.thinkingStartMs && !this.thinkingEndMs) {
          this.thinkingEndMs = eventTsMs(ev);
        }
        if (ctx) ctx.streamText += ev.text;
        else this.streamingText += ev.text;
        break;

      case "thinking_delta":
        this._clearTimeoutError();
        if (ev.parent_tool_use_id) {
          this._appendSubTimelineStreamingDelta(
            ev.parent_tool_use_id,
            "thinkingText",
            ev.text,
            ctx,
          );
          break;
        }
        if (!this.thinkingStartMs) this.thinkingStartMs = eventTsMs(ev);
        if (ctx) ctx.thinkingText += ev.text;
        else this.thinkingText += ev.text;
        break;

      case "tool_input_delta": {
        if (ev.parent_tool_use_id) {
          this._updateSubTimelineToolInput(
            ev.parent_tool_use_id,
            ev.tool_use_id,
            ev.partial_json,
            ctx,
          );
          break;
        }
        // Update matching tool entry's input in real-time with accumulated partial JSON
        const tl = getTl();
        const tIdx = this._findToolIdx(ctx, ev.tool_use_id);
        if (tIdx >= 0) {
          const old = tl[tIdx] as Extract<TimelineEntry, { kind: "tool" }>;
          const accum = SessionStore._accumulateJsonInput(
            old.tool as Record<string, unknown>,
            ev.partial_json,
          );
          const updated: TimelineEntry = {
            ...old,
            tool: { ...old.tool, ...accum } as typeof old.tool,
          };
          if (ctx) {
            ctx.tl[tIdx] = updated;
          } else {
            const u = [...this.timeline];
            u[tIdx] = updated;
            this.timeline = u;
          }
        }
        break;
      }

      case "message_complete": {
        // Dedup guard — but always clean up synthetic entry first to prevent leaks
        if (getSeenMsg().has(ev.message_id)) {
          if (ev.parent_tool_use_id)
            this._removeSubTimelineStreamingEntry(ev.parent_tool_use_id, ctx);
          break;
        }
        getSeenMsg().add(ev.message_id);
        if (getTl().some((e) => e.kind === "assistant" && e.id === ev.message_id)) {
          if (ev.parent_tool_use_id)
            this._removeSubTimelineStreamingEntry(ev.parent_tool_use_id, ctx);
          break;
        }

        // Subagent path: extract thinking → remove synthetic → create entry → append
        if (ev.parent_tool_use_id) {
          const subThinking = this._extractSubTimelineThinking(ev.parent_tool_use_id, ctx);
          this._removeSubTimelineStreamingEntry(ev.parent_tool_use_id, ctx);

          const entry: TimelineEntry = {
            kind: "assistant",
            id: ev.message_id,
            anchorId: ev.message_id,
            content: ev.text,
            ts: eventTs(ev),
            ...(ev.model ? { model: ev.model } : {}),
            ...(subThinking ? { thinkingText: subThinking } : {}),
          };
          dbg("store", "subagent thinking persisted", {
            parent: ev.parent_tool_use_id,
            len: subThinking?.length ?? 0,
          });

          const parentIdx = this._findParentToolIdx(ctx, ev.parent_tool_use_id);
          if (parentIdx >= 0) {
            this._appendToSubTimeline(getTl(), parentIdx, entry, ctx);
            break;
          }
          dbgWarn(
            "store",
            "subagent message_complete: parent not found, fallback to main timeline",
            { parent: ev.parent_tool_use_id },
          );
          this._pushTimeline(ctx, entry);
          break;
        }

        // Main session path: save thinking before clearing
        const savedThinking = ctx ? ctx.thinkingText : this.thinkingText;
        if (ctx) {
          ctx.streamText = "";
          ctx.thinkingText = "";
        } else {
          this.streamingText = "";
          this.thinkingText = "";
        }
        this.thinkingStartMs = 0;
        this.thinkingEndMs = 0;

        const entry: TimelineEntry = {
          kind: "assistant",
          id: ev.message_id,
          anchorId: ev.message_id,
          content: ev.text,
          ts: eventTs(ev),
          ...(ev.model ? { model: ev.model } : {}),
          ...(savedThinking ? { thinkingText: savedThinking } : {}),
        };
        if (savedThinking)
          dbg("store", "thinking persisted to timeline", {
            id: ev.message_id,
            len: savedThinking.length,
          });

        // Track this request's context size (input + cache_read + cache_creation). Context
        // occupancy is point-in-time, so the result handler uses the LAST request's value
        // instead of the turn-summed result usage, which multiplies cache_read by request
        // count (#149). Main-session messages only — subagent usage is its own context.
        if (ev.message_usage) {
          const mu = ev.message_usage;
          const num = (k: string) => (typeof mu[k] === "number" ? (mu[k] as number) : 0);
          const reqCtx =
            num("input_tokens") +
            num("cache_read_input_tokens") +
            num("cache_creation_input_tokens");
          if (reqCtx > 0) {
            if (ctx) ctx.lastReqContextTokens = reqCtx;
            else this.lastReqContextTokens = reqCtx;
            dbg("store", "lastReqContextTokens", { reqCtx });
          }
        }

        this._pushTimeline(ctx, entry);
        break;
      }

      case "user_message": {
        if (isAgentInternalContext(ev.text)) {
          dbg("store", "agent internal context omitted from visible timeline");
          break;
        }
        this._dismissCompletedTodoPanel();
        const tl = getTl();
        // Content-based dedup: only in live mode (replayOnly=false) where an optimistic
        // user entry was already added by startSession/sendMessage.  During replay
        // (replayOnly=true), every event from events.jsonl is authoritative —
        // the user may legitimately send the same text twice in different turns.
        if (!replayOnly) {
          let match: TimelineEntry | undefined;
          // Priority 1: exact match by client_uuid (Codex pipe path)
          if (ev.client_uuid) {
            match = tl.findLast((e) => e.kind === "user" && e.id === ev.client_uuid);
          }
          // Priority 2: backend uuid (stream-session path). A repeated live event may arrive
          // after the optimistic entry has already been confirmed and assigned cliUuid; matching
          // that stable backend id prevents the same user message from rendering twice.
          if (!match && ev.uuid) {
            match = tl.findLast((e) => e.kind === "user" && e.cliUuid === ev.uuid);
          }
          // DSH marks a steer echo with a transport-only prefix. Match the original optimistic
          // text so the same steer does not render once as `5 行` and again as `[Steer] 5 行`.
          const comparableEventText =
            this.run?.agent === "dsh" && ev.text.startsWith("[Steer] ")
              ? ev.text.slice("[Steer] ".length)
              : ev.text;
          // Priority 3: content-based match (Claude path / legacy compat)
          if (!match) {
            match = tl.findLast(
              (e) => e.kind === "user" && e.content === comparableEventText && !e.cliUuid,
            );
          }
          if (match && match.kind === "user") {
            // Merge the confirmed backend metadata into the optimistic entry. The bus only
            // carries attachment metadata, so retain the local base64 payload for image
            // previews instead of turning an already-rendered thumbnail into `image.png`.
            const confirmedAttachments =
              ev.attachments && ev.attachments.length > 0
                ? ev.attachments.map((a, index) => {
                    const local =
                      match.attachments?.find(
                        (item) => item.name === a.name && item.type === a.mime_type,
                      ) ?? match.attachments?.[index];
                    return {
                      name: a.name,
                      type: a.mime_type,
                      size: a.size,
                      contentBase64: local?.contentBase64 ?? "",
                    };
                  })
                : match.attachments;
            if (ev.uuid || confirmedAttachments) {
              const idx = tl.indexOf(match);
              const updated = {
                ...match,
                ...(ev.uuid ? { cliUuid: ev.uuid, anchorId: ev.uuid } : {}),
                ...(confirmedAttachments ? { attachments: confirmedAttachments } : {}),
              };
              if (ctx) ctx.tl[idx] = updated;
              else {
                const u = [...this.timeline];
                u[idx] = updated;
                this.timeline = u;
              }
            }
            break;
          }
        }
        const newId = uuid();
        // Convert bus-event attachments (name/mime_type/size) to timeline Attachment shape
        const busAttachments: Attachment[] | undefined =
          ev.attachments && ev.attachments.length > 0
            ? ev.attachments.map((a: { name: string; mime_type: string; size: number }) => ({
                name: a.name,
                type: a.mime_type,
                size: a.size,
                contentBase64: "",
              }))
            : undefined;
        const entry: TimelineEntry = {
          kind: "user",
          id: newId,
          anchorId: ev.uuid || newId,
          content: ev.text,
          ts: eventTs(ev),
          ...(ev.uuid ? { cliUuid: ev.uuid } : {}),
          ...(busAttachments ? { attachments: busAttachments } : {}),
        };
        this._pushTimeline(ctx, entry);

        // Resolve any ask_pending AskUserQuestion tool — the user_message following
        // a tool_end(AskUserQuestion) is the user's answer.  Without this, navigating
        // away and back replays bus events and resets the tool to ask_pending.
        const pendingIdx = tl.findIndex(
          (e) => e.kind === "tool" && e.tool.status === "ask_pending",
        );
        if (pendingIdx >= 0) {
          const old = tl[pendingIdx] as Extract<TimelineEntry, { kind: "tool" }>;
          const resolved: TimelineEntry = {
            ...old,
            tool: { ...old.tool, status: "success", output: { answer: ev.text } },
          };
          if (ctx) {
            ctx.tl[pendingIdx] = resolved;
          } else {
            const u = [...this.timeline];
            u[pendingIdx] = resolved;
            this.timeline = u;
          }
          // Also resolve the matching HookEvent (non-stream mode only)
          if (!this._isStreamMode(ctx)) {
            const he = getHe();
            const hIdx = this._findHeIdxByStatus(ctx, old.id, "running");
            if (hIdx >= 0) {
              const updatedHe: HookEvent = {
                ...he[hIdx],
                status: "done",
                hook_type: "PostToolUse",
              };
              if (ctx) {
                ctx.he[hIdx] = updatedHe;
              } else {
                const u = [...this.tools];
                u[hIdx] = updatedHe;
                this.tools = u;
              }
            }
          }
        }
        break;
      }

      case "agent_handoff": {
        const sepId = uuid();
        const srcLabel = getAgentDisplayName(ev.source_agent);
        const tgtLabel = getAgentDisplayName(ev.target_agent);
        const entry: TimelineEntry = {
          kind: "separator",
          id: sepId,
          anchorId: sepId,
          content: `已从 ${srcLabel} 切换到 ${tgtLabel}（新分支）`,
          ts: eventTs(ev),
        };
        this._pushTimeline(ctx, entry);
        break;
      }

      case "tool_start": {
        this._clearTimeoutError();
        if (getSeenTool().has(ev.tool_use_id)) break;
        getSeenTool().add(ev.tool_use_id);
        if (
          !ev.parent_tool_use_id &&
          ["TodoWrite", "TaskCreate", "TaskUpdate"].includes(ev.tool_name)
        ) {
          this._showTodoPanel();
        }
        // Subagent routing: nest inside parent tool's subTimeline
        if (ev.parent_tool_use_id) {
          const parentIdx = this._findParentToolIdx(ctx, ev.parent_tool_use_id);
          if (parentIdx >= 0) {
            const subEntry: TimelineEntry = {
              kind: "tool",
              id: ev.tool_use_id,
              anchorId: ev.tool_use_id,
              tool: {
                tool_use_id: ev.tool_use_id,
                tool_name: ev.tool_name,
                input: (ev.input as Record<string, unknown>) ?? {},
                status: "running",
              },
              ts: eventTs(ev),
            };
            this._appendToSubTimeline(getTl(), parentIdx, subEntry, ctx);
            break;
          }
          dbgWarn("store", "subagent tool_start: parent not found, fallback to main timeline", {
            parent: ev.parent_tool_use_id,
          });
        }
        if (this._findToolIdx(ctx, ev.tool_use_id) >= 0) break;

        const tlEntry: TimelineEntry = {
          kind: "tool",
          id: ev.tool_use_id,
          anchorId: ev.tool_use_id,
          tool: {
            tool_use_id: ev.tool_use_id,
            tool_name: ev.tool_name,
            input: (ev.input as Record<string, unknown>) ?? {},
            status: "running",
          },
          ts: eventTs(ev),
        };
        this._pushTimeline(ctx, tlEntry);

        // Mirror to tools[] (HookEvent) only in non-stream mode (pipe/PTY)
        if (!this._isStreamMode(ctx)) {
          const heEntry: HookEvent = {
            run_id: ev.run_id,
            hook_type: "PreToolUse",
            tool_name: ev.tool_name,
            tool_input: ev.input as Record<string, unknown>,
            status: "running",
            timestamp: new Date().toISOString(),
          };
          (heEntry as Record<string, unknown>).tool_use_id = ev.tool_use_id;
          this._pushHookEntry(ctx, heEntry);
        }
        break;
      }

      case "tool_end": {
        // AskUserQuestion handling:
        // - pipe mode: CLI returns error → ask_pending (frontend shows interactive options)
        // - stream-json mode: CLI returns success (answer provided via updatedInput) → success
        const isAskUser = ev.tool_name === "AskUserQuestion";
        const resolvedStatus =
          isAskUser && ev.status === "error"
            ? ("ask_pending" as const)
            : ev.status === "error"
              ? ("error" as const)
              : ("success" as const);

        // Subagent routing: update child tool inside parent's subTimeline
        if (ev.parent_tool_use_id) {
          if (
            this._updateSubTimelineTool(
              ev.parent_tool_use_id,
              ev.tool_use_id,
              (t) => ({
                ...t,
                status: resolvedStatus,
                output: ev.output as Record<string, unknown>,
                duration_ms: ev.duration_ms,
                tool_name: ev.tool_name || t.tool_name,
                tool_use_result: ev.tool_use_result as Record<string, unknown> | undefined,
              }),
              ctx,
            )
          ) {
            break;
          }
          dbgWarn(
            "store",
            "subagent tool_end: not found in subTimeline, fallback to main timeline",
            { parent: ev.parent_tool_use_id, tool: ev.tool_use_id },
          );
          // fall through to main timeline logic
        }

        const tl = getTl();
        const tIdx = this._findToolIdx(ctx, ev.tool_use_id);
        if (tIdx >= 0) {
          const old = tl[tIdx] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = {
            ...old,
            tool: {
              ...old.tool,
              status: resolvedStatus,
              output: ev.output as Record<string, unknown>,
              duration_ms: ev.duration_ms,
              tool_name: ev.tool_name || old.tool.tool_name,
              tool_use_result: ev.tool_use_result as Record<string, unknown> | undefined,
            },
          };
          if (ctx) {
            ctx.tl[tIdx] = updated;
          } else {
            const u = [...this.timeline];
            u[tIdx] = updated;
            this.timeline = u;
          }
        }

        // CronList is intentionally not consumed — output shape is unverified upstream.
        if (resolvedStatus === "success" && SCHEDULING_TOOLS.has(ev.tool_name)) {
          const readTasks = (): ScheduledTask[] => (ctx ? ctx.scheduledTasks : this.scheduledTasks);
          const writeTasks = (next: ScheduledTask[]): void => {
            if (ctx) ctx.scheduledTasks = next;
            else this.scheduledTasks = next;
          };

          if (ev.tool_name === "CronCreate") {
            const result = ev.tool_use_result as Record<string, unknown> | undefined;
            const id = typeof result?.id === "string" ? result.id : null;
            if (id && result) {
              const startInput = this._lookupToolStartInput(ctx, ev.tool_use_id);
              const humanSchedule =
                typeof result.humanSchedule === "string" ? result.humanSchedule : "";
              const task: ScheduledTask = {
                id,
                humanSchedule,
                recurring: result.recurring === true,
                durable: result.durable === true,
                prompt: typeof startInput?.prompt === "string" ? startInput.prompt : undefined,
                cron: pickSchedule(startInput),
                toolUseId: ev.tool_use_id,
              };
              // Replace-by-id: if CLI re-emits the same id (e.g. snapshot + live
              // overlap, or replay with newer input), keep the latest payload
              // rather than skipping. Preserves order.
              const cur = readTasks();
              const idx = cur.findIndex((t) => t.id === id);
              if (idx >= 0) {
                const next = [...cur];
                next[idx] = task;
                writeTasks(next);
              } else {
                writeTasks([...cur, task]);
              }
            } else {
              dbgWarn("store", "CronCreate tool_end missing id", { result });
            }
          } else if (ev.tool_name === "CronDelete") {
            const result = ev.tool_use_result as Record<string, unknown> | undefined;
            const startInput = this._lookupToolStartInput(ctx, ev.tool_use_id);
            const id =
              (typeof result?.id === "string" && result.id) || pickCronId(startInput) || null;
            if (id) {
              const cur = readTasks();
              const next = cur.filter((t) => t.id !== id);
              if (next.length !== cur.length) writeTasks(next);
            } else {
              dbgWarn("store", "CronDelete tool_end could not resolve id", { result, startInput });
            }
          }
        }

        // Plan mode inference: only top-level tools in live mode affect main session permissionMode.
        // Subagent EnterPlanMode should not change the parent session's mode.
        // replayOnly guard: replaying a historical session that ended mid-plan must not
        // pollute the current permissionMode (which is a user-level preference, not snapshot state).
        if (!replayOnly && ev.status !== "error" && !ev.parent_tool_use_id) {
          if (ev.tool_name === "EnterPlanMode") {
            this.previousPermissionMode = this.permissionMode || "default";
            this.permissionMode = "plan";
            dbg("store", "tool_end: EnterPlanMode → permissionMode=plan", {
              previous: this.previousPermissionMode,
            });
          } else if (ev.tool_name === "ExitPlanMode" && this.previousPermissionMode) {
            if (this.pendingPermissionModeOverride) {
              // User chose a specific mode via ExitPlanMode approval card
              this.permissionMode = this.pendingPermissionModeOverride;
              this.pendingPermissionModeOverride = null;
              dbg("store", "tool_end: ExitPlanMode → permissionMode overridden", {
                mode: this.permissionMode,
              });
            } else {
              const restored = this.previousPermissionMode;
              this.permissionMode = restored;
              dbg("store", "tool_end: ExitPlanMode → permissionMode restored", { restored });
            }
            this.previousPermissionMode = "";

            // "Clear context" deferred handling: extract plan from tool result
            if (this.pendingClearContextPlan === "__pending__") {
              const toolResult = ev.tool_use_result as Record<string, unknown> | undefined;
              const plan =
                (ev.output as Record<string, unknown> | undefined)?.plan || toolResult?.plan;
              if (plan && typeof plan === "string") {
                this.pendingClearContextPlan = plan;
                dbg("store", "ExitPlanMode: plan content captured for clear context");
              } else {
                this.pendingClearContextPlan = null;
                dbgWarn("store", "ExitPlanMode: no plan found in tool result for clear context");
              }
            }
          }
        }

        // Mirror to tools[] only in non-stream mode
        if (!isAskUser && !this._isStreamMode(ctx)) {
          const he = getHe();
          const hIdx = this._findHeIdxByStatus(ctx, ev.tool_use_id, "running");
          if (hIdx >= 0) {
            const updatedHe: HookEvent = {
              ...he[hIdx],
              status: "done",
              hook_type: "PostToolUse",
              tool_name: ev.tool_name || he[hIdx].tool_name,
              tool_output: ev.output as Record<string, unknown>,
            };
            if (ctx) {
              ctx.he[hIdx] = updatedHe;
            } else {
              const u = [...this.tools];
              u[hIdx] = updatedHe;
              this.tools = u;
            }
          }
        }
        break;
      }

      case "run_state":
        if (!replayOnly) {
          if (ev.state === "running" || ev.state === "spawning") {
            const newPhase: SessionPhase = ev.state === "spawning" ? "spawning" : "running";
            if (ctx) ctx.phase = newPhase;
            else this._setPhase(newPhase);
            // New turn starting — drop the previous turn's aggregated diff (live-only,
            // not snapshotted so a ctx replay path never reaches here).
            if (!ctx) this.turnDiff = "";
            // Remove any legacy snapshot — the session is now active.
            if (!ctx && this.run) {
              snapshotCache.deleteSnapshot(this.run.id).catch(() => {});
            }
          } else if (ev.state === "idle") {
            if (ctx) ctx.phase = "idle";
            else this._setPhase("idle");
            // Turn ended: clear the "in-progress thinking" field so the UI stops
            // showing "Pi 正在整理思路…" forever. The thinking content has already
            // been appended to the timeline; this field is only the live accumulator.
            if (ctx) {
              ctx.thinkingText = "";
              ctx.thinkingStartMs = 0;
              ctx.thinkingEndMs = 0;
            } else {
              this.thinkingText = "";
              this.thinkingStartMs = 0;
              this.thinkingEndMs = 0;
            }
            if (!ctx) void this.flushQueuedMessages();
          } else {
            // completed / failed / stopped
            const termPhase = ev.state as SessionPhase;
            if (ctx) ctx.phase = termPhase;
            else {
              this._setPhase(termPhase);
              if (this.run) {
                const snapId = this.run.id;
                api
                  .getRun(snapId)
                  .then((r) => {
                    // Guard: only update if we're still viewing the same run
                    if (this.run?.id === snapId) this.run = r;
                  })
                  .catch((e) => dbgWarn("store", "getRun after terminal state failed:", e));
              }
            }
          }
          // Sync run.status for non-terminal states so status bar reflects reality
          // (terminal states update run via api.getRun above)
          if (ev.state === "running" || ev.state === "idle") {
            if (ctx) ctx.runStatus = ev.state;
            else if (this.run) this.run = { ...this.run, status: ev.state };
          }
        }
        // Show error to user only for genuine failures, not user-initiated stops.
        // "stopped" = user clicked stop; "failed" after stop = CLI dying mid-request (expected).
        // _stopping flag: set by stop() before IPC call, covers the interrupt+kill window.
        if (!replayOnly && ev.error && ev.state !== "stopped" && !this._stopping) {
          if (ctx) ctx.error = ev.error;
          else this.error = ev.error;
        }
        // Resolve stale permission_prompt / optimistic-running tools on idle transition.
        // When CLI goes idle (turn complete), any remaining permission_prompt cards are stale
        // (e.g. user interrupted during a pending can_use_tool request).
        // Also resolve "optimistic running" tools (have permission_request_id) that never got a tool_end.
        // Covers both main timeline and subTimelines.
        if (ev.state === "idle") {
          this._resolveStaleTools(
            (t) =>
              t.status === "permission_prompt" ||
              (t.status === "running" && !!t.permission_request_id),
            ctx,
          );
        }
        // Resolve permission_denied / permission_prompt tools on session restart (spawning).
        // After approval, the session restarts — those cards are no longer actionable.
        // Runs in both live and replay mode so replayed sessions show resolved state.
        // Covers both main timeline and subTimelines.
        if (ev.state === "spawning") {
          this._resolveStaleTools(
            (t) =>
              t.status === "permission_denied" ||
              t.status === "permission_prompt" ||
              (t.status === "running" && !!t.permission_request_id),
            ctx,
          );
        }
        // Clear stale elicitations on state transitions — CLI won't send control_cancelled
        // for these if the session ends abnormally or restarts.
        if (
          ev.state === "idle" ||
          ev.state === "spawning" ||
          ev.state === "completed" ||
          ev.state === "failed" ||
          ev.state === "stopped"
        ) {
          if (this.pendingElicitations.size > 0) {
            dbg("store", "run_state clearing stale elicitations", {
              state: ev.state,
              count: this.pendingElicitations.size,
            });
            this.pendingElicitations = new Map();
          }
        }
        if (
          !replayOnly &&
          this.run?.app_mode === "work" &&
          (ev.state === "completed" || ev.state === "failed" || ev.state === "stopped")
        ) {
          this.clearPiExtensionUiState();
        }
        break;

      case "usage_update": {
        const u: UsageState = {
          inputTokens: ev.input_tokens,
          outputTokens: ev.output_tokens,
          cacheReadTokens: ev.cache_read_tokens ?? 0,
          cacheWriteTokens: ev.cache_write_tokens ?? 0,
          cost: ev.total_cost_usd,
          modelUsage: ev.model_usage,
          durationApiMs: ev.duration_api_ms,
        };
        // Don't let an all-zero-token usage (from error results) overwrite real data.
        // CLI sometimes sends cost-only usage on error — preserve previous token counts.
        const prev = ctx ? ctx.usage : this.usage;
        const hasTokens =
          u.inputTokens > 0 ||
          u.outputTokens > 0 ||
          u.cacheReadTokens > 0 ||
          u.cacheWriteTokens > 0;
        const merged = hasTokens ? u : { ...prev, cost: Math.max(prev.cost, u.cost) };
        // Preserve modelUsage/durationApiMs even on zero-token update (error results may still have them)
        if (!hasTokens && u.modelUsage) merged.modelUsage = u.modelUsage;
        if (!hasTokens && u.durationApiMs) merged.durationApiMs = u.durationApiMs;
        if (ctx) ctx.usage = merged;
        else this.usage = merged;
        // Store duration_ms and num_turns from result events
        if (ev.duration_ms != null) this.durationMs = ev.duration_ms;
        if (ev.num_turns != null) this.numTurns = ev.num_turns;

        // Append per-turn usage snapshot (raw event data, not merged)
        // Use backend-authoritative turn_index when available; fall back to counting
        // user entries in the timeline (for backwards compat with older events).
        const tl = getTl();
        const fallbackIdx = tl.filter((e) => e.kind === "user").length;
        const turnIdx = ev.turn_index ?? fallbackIdx;
        dbg("store", "usage_update turn_index", {
          backend: ev.turn_index,
          fallback: fallbackIdx,
          used: turnIdx,
        });
        const turnSnap: TurnUsage = {
          turnIndex: turnIdx,
          inputTokens: u.inputTokens,
          outputTokens: u.outputTokens,
          cacheReadTokens: u.cacheReadTokens,
          cacheWriteTokens: u.cacheWriteTokens,
          cost: u.cost,
          durationApiMs: u.durationApiMs,
          durationMs: ev.duration_ms,
        };
        if (ctx) {
          ctx.turnUsages.push(turnSnap);
        } else {
          this.turnUsages = [...this.turnUsages, turnSnap];
        }

        // Update context usage. Most agents only expose cumulative usage, so retain the
        // high-water behavior for those legacy events. Codex separately reports the current
        // prompt context (`context_tokens`); use that exact snapshot so total thread usage is
        // not mistaken for current context and compaction can move the meter back down. #135
        if (ev.context_tokens != null) {
          const contextTarget = ctx ?? this;
          contextTarget.contextHwTokens = ev.context_tokens;
          if (ev.context_window && ev.context_window > 0) {
            contextTarget.contextHwWindow = ev.context_window;
          }
          contextTarget.lastReqContextTokens = ev.context_tokens;
        } else if (hasTokens) {
          // Context occupancy = the LAST request's tokens, NOT the turn-summed result usage,
          // which inflates cache_read by the request count (#149). Fall back to the summed
          // usage for older runs whose message events carry no per-request usage.
          const lastReq = (ctx ?? this).lastReqContextTokens;
          const evUsed =
            lastReq > 0 ? lastReq : u.inputTokens + u.cacheReadTokens + u.cacheWriteTokens;
          let evWin = 0;
          if (u.modelUsage) {
            for (const e of Object.values(u.modelUsage)) {
              if (e.context_window && e.context_window > evWin) evWin = e.context_window;
            }
          }
          if (evWin > 0) {
            const hwTgt = ctx ?? this;
            const clamped = Math.min(evUsed, evWin);
            hwTgt.contextHwWindow = evWin;
            if (clamped > hwTgt.contextHwTokens) hwTgt.contextHwTokens = clamped;
          }
        }
        break;
      }

      case "permission_denied": {
        // Retroactively update: find matching tool, change to "permission_denied"
        const tl = getTl();
        const tIdx = this._findToolIdx(ctx, ev.tool_use_id);
        if (tIdx >= 0) {
          const old = tl[tIdx] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = {
            ...old,
            tool: { ...old.tool, status: "permission_denied" },
          };
          if (ctx) {
            ctx.tl[tIdx] = updated;
          } else {
            const u = [...this.timeline];
            u[tIdx] = updated;
            this.timeline = u;
          }
        } else {
          // Not in main timeline — search subTimelines (CLI may omit parent_tool_use_id)
          this._updateToolInAnySubTimeline(
            ev.tool_use_id,
            (t) => ({ ...t, status: "permission_denied" as const }),
            ctx,
          );
        }
        break;
      }

      case "permission_prompt": {
        dbg("store", "permission_prompt received", {
          tool_use_id: ev.tool_use_id,
          request_id: ev.request_id,
          tool_name: ev.tool_name,
          parent: ev.parent_tool_use_id,
          batch: !!ctx,
        });
        // Subagent routing: update child tool inside parent's subTimeline
        if (ev.parent_tool_use_id) {
          if (
            this._updateSubTimelineTool(
              ev.parent_tool_use_id,
              ev.tool_use_id,
              (t) => ({
                ...t,
                status: "permission_prompt" as const,
                permission_request_id: ev.request_id,
                ...(ev.suggestions && ev.suggestions.length > 0
                  ? { suggestions: ev.suggestions }
                  : {}),
              }),
              ctx,
            )
          ) {
            break;
          }
          dbgWarn(
            "store",
            "subagent permission_prompt: not found in subTimeline, fallback to main timeline",
            { parent: ev.parent_tool_use_id, tool: ev.tool_use_id },
          );
          // fall through to main timeline logic
        }
        // Inline permission prompt from --permission-prompt-tool stdio.
        // Find matching tool (should be "running") and update to "permission_prompt" with request_id.
        const tl = getTl();
        const tIdx = this._findToolIdx(ctx, ev.tool_use_id);
        if (tIdx >= 0) {
          const old = tl[tIdx] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = {
            ...old,
            tool: {
              ...old.tool,
              status: "permission_prompt",
              permission_request_id: ev.request_id,
              // Merge suggestions from permission_prompt event (CLI provides these)
              ...(ev.suggestions && ev.suggestions.length > 0
                ? { suggestions: ev.suggestions }
                : {}),
            },
          };
          if (ctx) {
            ctx.tl[tIdx] = updated;
          } else {
            const u = [...this.timeline];
            u[tIdx] = updated;
            this.timeline = u;
          }
          dbg("store", "permission_prompt: updated existing entry", {
            tIdx,
            tool_use_id: ev.tool_use_id,
            request_id: ev.request_id,
          });
        } else {
          // Tool not in main timeline — check ALL subTimelines (CLI sometimes omits parent_tool_use_id)
          const foundInSub = this._updateToolInAnySubTimeline(
            ev.tool_use_id,
            (t) => ({
              ...t,
              status: "permission_prompt" as const,
              permission_request_id: ev.request_id,
              ...(ev.suggestions && ev.suggestions.length > 0
                ? { suggestions: ev.suggestions }
                : {}),
            }),
            ctx,
          );
          if (!foundInSub) {
            // Truly new — create a synthetic tool entry in main timeline
            dbg("store", "permission_prompt: creating synthetic entry", {
              tool_use_id: ev.tool_use_id,
              request_id: ev.request_id,
              tool_name: ev.tool_name,
            });
            const tlEntry: TimelineEntry = {
              kind: "tool",
              id: ev.tool_use_id,
              anchorId: ev.tool_use_id,
              tool: {
                tool_use_id: ev.tool_use_id,
                tool_name: ev.tool_name,
                input: ev.tool_input as Record<string, unknown>,
                status: "permission_prompt",
                permission_request_id: ev.request_id,
                ...(ev.suggestions && ev.suggestions.length > 0
                  ? { suggestions: ev.suggestions }
                  : {}),
              },
              ts: eventTs(ev),
            };
            this._pushTimeline(ctx, tlEntry);
          } else {
            dbg("store", "permission_prompt: updated in subTimeline", {
              tool_use_id: ev.tool_use_id,
              request_id: ev.request_id,
            });
          }
        }
        break;
      }

      case "compact_boundary": {
        const isMicro = (ev.trigger ?? "").startsWith("micro");
        if (isMicro) {
          this.microcompactCount++;
        } else {
          this.compactCount++;
          // Full compaction: insert timeline separator
          const tokensInfo = ev.pre_tokens ? ` (${Math.round(ev.pre_tokens / 1000)}k tokens)` : "";
          const sepId = uuid();
          const entry: TimelineEntry = {
            kind: "separator",
            id: sepId,
            anchorId: sepId,
            content: `Context compacted${tokensInfo}`,
            ts: eventTs(ev),
          };
          this._pushTimeline(ctx, entry);
          // Reset per-turn token counts so contextUtilization reflects the
          // compacted state instead of showing stale pre-compact values.
          // The next usage_update event will supply accurate post-compact numbers.
          // Only reset on full compaction — micro-compaction keeps the existing
          // usage so the progress bar does not flash 90%→0%→85%.
          dbg("store", "compact: reset context usage", { preTokens: ev.pre_tokens });
          const prev = ctx ? ctx.usage : this.usage;
          const reset = { ...prev, inputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0 };
          if (ctx) ctx.usage = reset;
          else this.usage = reset;
          // Reset the context high-water too — the next usage_update re-establishes the
          // post-compaction peak. Keep the window (denominator unchanged by compaction). #135
          const hwTgt = ctx ?? this;
          hwTgt.contextHwTokens = 0;
          hwTgt.lastReqContextTokens = 0;
          this.contextBreakdown = null;
        }
        // Only set lastCompactedAt during live mode — during replay
        // the timestamp would be meaningless (Date.now() ≠ original event time).
        if (!replayOnly) {
          this.lastCompactedAt = Date.now();
        }
        break;
      }

      case "pi_context_usage": {
        const snapshot = ev.snapshot;
        if (
          snapshot &&
          snapshot.version === 1 &&
          Array.isArray(snapshot.categories) &&
          Number.isFinite(snapshot.usedTokens) &&
          Number.isFinite(snapshot.contextWindow)
        ) {
          this.contextBreakdown = snapshot;
        }
        break;
      }

      case "command_output": {
        if (
          isPiAbortCommandOutput(ev.content) ||
          isCodexModelMetadataNotice(ev.content) ||
          isPiRpcStderrCommandOutput(ev.content)
        ) {
          break;
        }
        dbg("store", "command_output received", {
          contentLen: ev.content.length,
          hasBatchCtx: !!ctx,
        });
        const cmdId = uuid();
        const cmdEntry: TimelineEntry = {
          kind: "command_output",
          id: cmdId,
          anchorId: cmdId,
          content: ev.content,
          ts: eventTs(ev),
        };
        this._pushTimeline(ctx, cmdEntry);
        break;
      }

      case "elicitation_prompt": {
        dbg("store", "elicitation_prompt received", {
          request_id: ev.request_id,
          server: ev.mcp_server_name,
          mode: ev.mode,
        });
        const updated = new Map(this.pendingElicitations);
        const hostRequest = this.piExtensionHostState.pendingRequests.find(
          (request) => request.id === ev.request_id,
        );
        updated.set(ev.request_id, {
          requestId: ev.request_id,
          mcpServerName: ev.mcp_server_name,
          message: ev.message,
          elicitationId: ev.elicitation_id,
          mode: ev.mode,
          url: ev.url,
          requestedSchema: ev.requested_schema,
          expiresAt: hostRequest?.expiresAt,
        });
        this.pendingElicitations = updated;
        break;
      }

      case "raw": {
        const rawText = typeof ev.data === "string" ? ev.data : JSON.stringify(ev.data);
        if (rawText && (ev.source === "claude_stdout_text" || ev.source === "claude_stderr")) {
          const rawId = uuid();
          const entry: TimelineEntry = {
            kind: "assistant",
            id: rawId,
            anchorId: rawId,
            content: `\`[${ev.source}]\` ${rawText}`,
            ts: new Date().toISOString(),
          };
          this._pushTimeline(ctx, entry);
        } else {
          this.rawFallbackCount++;
          dbgWarn("store", "raw fallback event:", ev.source, rawText?.slice(0, 100));
          if (this.strictMode) {
            throw new Error(`[STRICT] raw fallback event: source=${ev.source}`);
          }
        }
        break;
      }

      case "system_status":
        this.systemStatus = { status: ev.status };
        break;

      case "pi_extension_ui": {
        const data = (ev.data ?? {}) as Record<string, unknown>;
        const extId = String(data.extensionId ?? data.extensionName ?? data.name ?? "default");
        const prev = this.piExtensionUiMap[extId] ?? { extensionId: extId };
        const next = { ...prev };

        if (ev.method === "setStatus") {
          const status = data.statusText;
          if (isPiPermissionModeIndicator(status)) delete next.statusText;
          else if (typeof status === "string" && status.length > 0) next.statusText = status;
          else delete next.statusText;
        } else if (ev.method === "setWidget") {
          const lines = data.widgetLines;
          if (Array.isArray(lines)) {
            next.widgetLines = lines.filter((line): line is string => typeof line === "string");
          } else {
            delete next.widgetLines;
          }
        } else if (ev.method === "setTitle") {
          const title = data.title;
          if (typeof title === "string" && title.length > 0) next.title = title;
          else delete next.title;
        } else if (ev.method === "set_editor_text") {
          const text = data.text;
          if (typeof text === "string") {
            next.editorText = text;
            // Keep the one-shot editor action until PromptInput is mounted. This
            // legacy event shape follows the same delivery contract as the
            // Standard Host's pi_extension_editor_action event.
            this.pendingEditorText = text;
          } else delete next.editorText;
        } else if (ev.method === "notify") {
          const message = typeof data.message === "string" ? data.message : "Notification";
          if (isPiPermissionModeIndicator(message)) {
            const visibleNotifications = (next.notifications ?? []).filter(
              (notification) => !isPiPermissionModeIndicator(notification.message),
            );
            if (visibleNotifications.length > 0) next.notifications = visibleNotifications;
            else delete next.notifications;
          } else {
            const rawType =
              typeof data.notifyType === "string" ? data.notifyType.toLowerCase() : "info";
            const notifyType: "info" | "success" | "warning" | "error" =
              rawType === "success" || rawType === "warning" || rawType === "error"
                ? rawType
                : "info";
            const newNotif = {
              id: `notif-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
              type: notifyType,
              message,
            };
            next.notifications = [...(next.notifications ?? []).slice(-4), newNotif];
          }
        } else {
          dbgWarn("store", "unknown Pi extension UI method", ev.method);
          break;
        }

        const mapCopy = { ...this.piExtensionUiMap };
        const hasContent =
          next.title ||
          next.statusText ||
          (next.widgetLines && next.widgetLines.length > 0) ||
          next.editorText !== undefined ||
          (next.notifications && next.notifications.length > 0);
        if (hasContent) {
          mapCopy[extId] = next;
        } else {
          delete mapCopy[extId];
        }
        this.piExtensionUiMap = mapCopy;
        const firstKey = Object.keys(mapCopy)[0];
        this.piExtensionUi = firstKey ? mapCopy[firstKey] : null;
        break;
      }

      case "pi_extension_ui_request_created": {
        const req = (ev as { request?: import("$lib/types").PiExtensionUiRequest }).request;
        if (req) {
          const list = [
            ...this.piExtensionHostState.pendingRequests.filter((r) => r.id !== req.id),
            req,
          ];
          this.applyHostSnapshot({ ...this.piExtensionHostState, pendingRequests: list });
        }
        break;
      }

      case "pi_extension_ui_request_resolved": {
        const reqId = (ev as { request_id?: string }).request_id;
        if (reqId) {
          const list = this.piExtensionHostState.pendingRequests.filter((r) => r.id !== reqId);
          this.applyHostSnapshot({ ...this.piExtensionHostState, pendingRequests: list });
        }
        break;
      }

      case "pi_extension_notice": {
        const noticeType = (ev as { notice_type?: string }).notice_type || "info";
        const message = (ev as { message?: string }).message || "";
        if (message && !isPiContextPruneRoutineNotice(message, noticeType)) {
          this.latestNotice = { noticeType, message, timestamp: Date.now() };
        }
        break;
      }

      case "pi_extension_state_sync": {
        const snapshot = (ev as unknown as { snapshot?: Record<string, unknown> }).snapshot;
        if (snapshot && typeof snapshot === "object") {
          this.applyHostSnapshot(snapshot);
        }
        break;
      }

      case "pi_extension_editor_action": {
        const text = (ev as { text?: string }).text ?? "";
        this.pendingEditorText = text;
        break;
      }

      case "pi_feature_capabilities":
        this.piCapabilities = ev.capabilities;
        break;

      case "pi_plan_state":
        this.piPlanState = ev.state;
        break;

      case "pi_goal_state":
        this.piGoalState = ev.state;
        break;

      case "pi_todo_state":
        this.piTodoState = ev.state;
        if (
          ev.state.phases.some((phase) =>
            phase.tasks.some((task) => task.status === "pending" || task.status === "in_progress"),
          ) ||
          ev.state.phases.every((phase) => phase.tasks.length === 0)
        ) {
          this._showTodoPanel();
        }
        break;

      case "pi_permission_state":
        this.piPermissionState = ev.state;
        break;

      case "pi_queue_updated":
        this.piQueueState = ev.state;
        break;

      case "pi_session_entries": {
        const entries = ev.entries ?? [];
        const byId = new Map<string, import("$lib/types").PiSessionEntry>();
        const existing = this.piSessionTree.roots;
        const visit = (node: import("$lib/types").PiSessionTreeNode) => {
          byId.set(node.entry.id, node.entry);
          node.children.forEach(visit);
        };
        existing.forEach(visit);
        entries.forEach((entry) => byId.set(entry.id, entry));
        const nodes = [...byId.values()].map((entry) => ({
          entry,
          children: [] as import("$lib/types").PiSessionTreeNode[],
        }));
        const nodesById = new Map(nodes.map((node) => [node.entry.id, node]));
        const roots: import("$lib/types").PiSessionTreeNode[] = [];
        for (const node of nodes) {
          const parent = node.entry.parentId ? nodesById.get(node.entry.parentId) : undefined;
          if (parent) parent.children.push(node);
          else roots.push(node);
        }
        this.piSessionTree = {
          ...this.piSessionTree,
          roots,
          leafId: ev.leaf_id ?? this.piSessionTree.leafId,
          loading: false,
          lastEntryId: ev.last_entry_id ?? this.piSessionTree.lastEntryId,
          error: undefined,
        };
        break;
      }

      case "pi_session_tree":
        this.piSessionTree = {
          ...this.piSessionTree,
          roots: ev.roots,
          leafId: ev.leaf_id ?? null,
          loading: false,
          error: undefined,
        };
        break;

      case "auth_status":
        this.authStatus = { is_authenticating: ev.is_authenticating, output: ev.output };
        break;

      case "hook_started":
        this.hookEvents = [
          ...this.hookEvents,
          {
            type: ev.type,
            hook_id: ev.hook_id,
            data: ev,
            hook_name: ev.hook_name,
          },
        ];
        break;

      case "hook_progress":
        this.hookEvents = [
          ...this.hookEvents,
          {
            type: ev.type,
            hook_id: ev.hook_id,
            data: ev,
          },
        ];
        break;

      case "hook_response": {
        this.hookEvents = [
          ...this.hookEvents,
          {
            type: ev.type,
            hook_id: ev.hook_id,
            data: ev,
            hook_name: ev.hook_name,
            stdout: ev.stdout,
            stderr: ev.stderr,
            exit_code: ev.exit_code,
          },
        ];
        // SessionStart hooks can set the session title via hookSpecificOutput.sessionTitle.
        // Apply only when the run has no name yet — a user /rename always wins and must not be
        // clobbered when the same SessionStart hook fires again on resume.
        if (ev.hook_event === "SessionStart" && this.run && !this.run.name) {
          const title = extractSessionTitle(ev.stdout);
          if (title) {
            const runId = this.run.id;
            this.run = { ...this.run, name: title };
            void import("$lib/api")
              .then((m) => m.renameRun(runId, title))
              .catch((e) => dbgWarn("store", "renameRun from sessionTitle failed", e));
          }
        }
        break;
      }

      case "hook_callback":
        // Hook callback from CLI — PreToolUse hooks are actionable (allow/deny)
        this.hookEvents = [
          ...this.hookEvents,
          {
            type: ev.type,
            hook_id: ev.hook_id,
            data: ev,
            request_id: ev.request_id,
            status: ev.hook_event === "PreToolUse" ? "hook_pending" : "allowed",
          },
        ];
        break;

      case "task_notification": {
        const existing = this.taskNotifications.get(ev.task_id);
        const rawData = ev.data as Record<string, unknown> | undefined;
        const message =
          (rawData?.summary as string) ??
          (rawData?.message as string) ??
          (rawData?.task_description as string) ??
          ev.task_id;
        const updated = new Map(this.taskNotifications);
        updated.set(ev.task_id, {
          task_id: ev.task_id,
          status: ev.status,
          message,
          startedAt: existing?.startedAt ?? Date.now(),
          data: ev,
          output_file:
            ((rawData?.output_file ?? rawData?.outputFile) as string | undefined) ??
            existing?.output_file,
          task_type:
            ((rawData?.task_type ?? rawData?.taskType) as string | undefined) ??
            existing?.task_type,
          summary: (rawData?.summary as string | undefined) ?? existing?.summary,
          tool_use_id:
            ((rawData?.tool_use_id ?? rawData?.toolUseId) as string | undefined) ??
            existing?.tool_use_id,
        });
        this.taskNotifications = updated;
        break;
      }

      case "files_persisted":
        this.persistedFiles = [
          ...this.persistedFiles,
          ...(Array.isArray(ev.files) ? ev.files : []),
        ];
        break;

      case "tool_progress": {
        if (ev.parent_tool_use_id) {
          this._updateSubTimelineTool(
            ev.parent_tool_use_id,
            ev.tool_use_id,
            (t) => ({
              ...t,
              elapsed_time_seconds: ev.elapsed_time_seconds,
            }),
            ctx,
          );
          break;
        }
        const tl = getTl();
        const idx = this._findToolIdx(ctx, ev.tool_use_id);
        if (idx >= 0) {
          const old = tl[idx] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = {
            ...old,
            tool: { ...old.tool, elapsed_time_seconds: ev.elapsed_time_seconds },
          };
          if (ctx) ctx.tl[idx] = updated;
          else {
            const u = [...this.timeline];
            u[idx] = updated;
            this.timeline = u;
          }
        }
        break;
      }

      case "tool_output_delta": {
        // Append a streamed output chunk to the open tool card (Codex live command
        // output). extractOutputText reads the current {content} string ("" if absent),
        // so deltas accumulate; tool_end later overwrites with the authoritative output.
        if (ev.parent_tool_use_id) {
          this._updateSubTimelineTool(
            ev.parent_tool_use_id,
            ev.tool_use_id,
            (t) => ({ ...t, output: { content: extractOutputText(t.output) + ev.delta } }),
            ctx,
          );
          break;
        }
        const tl = getTl();
        const idx = this._findToolIdx(ctx, ev.tool_use_id);
        if (idx >= 0) {
          const old = tl[idx] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = {
            ...old,
            tool: {
              ...old.tool,
              output: { content: extractOutputText(old.tool.output) + ev.delta },
            },
          };
          if (ctx) ctx.tl[idx] = updated;
          else {
            const u = [...this.timeline];
            u[idx] = updated;
            this.timeline = u;
          }
        }
        break;
      }

      case "tool_use_summary": {
        if (ev.parent_tool_use_id) {
          this._updateSubTimelineTool(
            ev.parent_tool_use_id,
            ev.tool_use_id,
            (t) => ({
              ...t,
              summary: ev.summary,
            }),
            ctx,
          );
          break;
        }
        const tl2 = getTl();
        const idx2 = this._findToolIdx(ctx, ev.tool_use_id);
        if (idx2 >= 0) {
          const old = tl2[idx2] as Extract<TimelineEntry, { kind: "tool" }>;
          const updated: TimelineEntry = { ...old, tool: { ...old.tool, summary: ev.summary } };
          if (ctx) ctx.tl[idx2] = updated;
          else {
            const u = [...this.timeline];
            u[idx2] = updated;
            this.timeline = u;
          }
        }
        break;
      }

      case "control_cancelled": {
        // Resolve any permission_prompt or optimistic-running tool with matching request_id to "error"
        // Covers both main timeline and subTimelines.
        this._resolveStaleTools(
          (t) =>
            (t.status === "permission_prompt" || t.status === "running") &&
            t.permission_request_id === ev.request_id,
          ctx,
        );
        // Also resolve pending hook callbacks
        this.hookEvents = this.hookEvents.map((h) =>
          h.request_id === ev.request_id && h.status === "hook_pending"
            ? { ...h, status: "cancelled" as const }
            : h,
        );
        // Remove cancelled elicitation
        if (this.pendingElicitations.has(ev.request_id)) {
          const elicUpdated = new Map(this.pendingElicitations);
          elicUpdated.delete(ev.request_id);
          this.pendingElicitations = elicUpdated;
        }
        break;
      }

      // ── Ralph Loop events ──
      case "ralph_started": {
        this.ralphLoop = {
          active: true,
          prompt: ev.prompt,
          iteration: 0,
          maxIterations: ev.max_iterations,
          completionPromise: ev.completion_promise,
          startedAt: ev.started_at,
          reason: null,
        };
        dbg("store", "ralph_started", {
          maxIterations: ev.max_iterations,
          promise: ev.completion_promise,
        });
        break;
      }
      case "ralph_iteration": {
        if (this.ralphLoop) {
          this.ralphLoop = {
            ...this.ralphLoop,
            iteration: ev.iteration,
            maxIterations: ev.max_iterations,
          };
        }
        // Insert iteration separator in timeline
        const iterLabel =
          ev.max_iterations > 0
            ? `Ralph iteration ${ev.iteration}/${ev.max_iterations}`
            : `Ralph iteration ${ev.iteration}`;
        const iterSepId = uuid();
        this._pushTimeline(ctx, {
          kind: "separator",
          id: iterSepId,
          anchorId: iterSepId,
          content: `🔄 ${iterLabel}`,
          ts: eventTs(ev),
        });
        dbg("store", "ralph_iteration", { iteration: ev.iteration });
        break;
      }
      case "ralph_complete": {
        // Insert completion separator in timeline
        const reasonLabels: Record<string, string> = {
          max_iterations: "max iterations reached",
          completion_promise: "completion promise matched",
          cancelled: "cancelled",
          fail_stopped: "stopped after consecutive failures",
        };
        const reasonText = reasonLabels[ev.reason] ?? ev.reason;
        const completeIcon =
          ev.reason === "cancelled" || ev.reason === "fail_stopped" ? "❌" : "✅";
        const completeSepId = uuid();
        this._pushTimeline(ctx, {
          kind: "separator",
          id: completeSepId,
          anchorId: completeSepId,
          content: `${completeIcon} Ralph Loop completed · ${ev.iteration} iterations · ${reasonText}`,
          ts: eventTs(ev),
        });
        if (this.ralphLoop) {
          this.ralphLoop = {
            ...this.ralphLoop,
            active: false,
            iteration: ev.iteration,
            reason: ev.reason,
          };
        }
        dbg("store", "ralph_complete", { reason: ev.reason, iteration: ev.iteration });
        break;
      }
      case "goal_update": {
        // Codex `thread/goal/updated`: merge live progress into the goal field
        // so GoalPanel re-renders tokensUsed/timeUsedSeconds/status without
        // polling. Backend sends `goal: null` on `thread/goal/cleared` — treat
        // that as a reset, not a no-op merge.
        if (ev.goal == null) {
          this.goal = null;
          dbg("store", "goal_update (cleared)");
        } else {
          this.goal = { ...(this.goal ?? {}), ...ev.goal };
          dbg("store", "goal_update", {
            status: ev.goal?.status,
            tokensUsed: ev.goal?.tokensUsed,
          });
        }
        break;
      }

      case "codex_hook_run": {
        // Codex Wave-4 hook lifecycle. `hook/started` arrives with status="running",
        // then `hook/completed` with a terminal HookRunStatus. hook_id is stable across
        // the pair, so we UPSERT one timeline entry: update in place on completion rather
        // than stacking a second card. Distinct hook_ids stay distinct cards.
        dbg("store", "codex_hook_run", {
          hook_id: ev.hook_id,
          event_name: ev.event_name,
          status: ev.status,
          duration_ms: ev.duration_ms,
        });
        const tl = ctx ? ctx.tl : this.timeline;
        const idx = tl.findIndex((e) => e.kind === "hook" && e.hookId === ev.hook_id);
        if (idx >= 0) {
          const old = tl[idx] as Extract<TimelineEntry, { kind: "hook" }>;
          const updated: TimelineEntry = {
            ...old,
            status: ev.status,
            // Merge optional fields only when present — `hook/completed` carries
            // duration/message; `hook/started` may omit them, and we keep the prior value.
            ...(ev.status_message !== undefined ? { statusMessage: ev.status_message } : {}),
            ...(ev.duration_ms !== undefined ? { durationMs: ev.duration_ms } : {}),
          };
          if (ctx) {
            ctx.tl[idx] = updated;
          } else {
            const u = [...this.timeline];
            u[idx] = updated;
            this.timeline = u;
          }
        } else {
          const hookEntryId = uuid();
          const entry: TimelineEntry = {
            kind: "hook",
            id: hookEntryId,
            anchorId: hookEntryId,
            hookId: ev.hook_id,
            eventName: ev.event_name,
            status: ev.status,
            ...(ev.status_message !== undefined ? { statusMessage: ev.status_message } : {}),
            ...(ev.duration_ms !== undefined ? { durationMs: ev.duration_ms } : {}),
            ts: eventTs(ev),
          };
          this._pushTimeline(ctx, entry);
        }
        break;
      }

      case "codex_mcp_status": {
        // Codex Wave-4: live MCP server startup-state push. Map the raw Codex status to the
        // panel vocab and upsert store.mcpServers by name so McpStatusPanel updates without a
        // manual refresh. Unknown server name → append (a server can appear mid-session).
        const mapped =
          ev.status === "ready" ? "connected" : ev.status === "starting" ? "pending" : "failed"; // failed | cancelled → failed
        dbg("store", "codex_mcp_status", { name: ev.name, status: ev.status, mapped });
        const existing = this.mcpServers.find((s) => s.name === ev.name);
        const next: McpServerInfo[] = existing
          ? this.mcpServers.map((s) =>
              s.name === ev.name ? { ...s, status: mapped, error: ev.error } : s,
            )
          : [...this.mcpServers, { name: ev.name, status: mapped, error: ev.error }];
        this.updateMcpServers(next);
        break;
      }

      case "codex_turn_diff": {
        // Codex Wave-4: cumulative turn diff push. The diff is aggregated across the whole
        // turn server-side; later events supersede earlier ones, so keep the latest.
        // Cleared at the next turn start (run_state→running) and on reset.
        dbg("store", "codex_turn_diff", { len: ev.diff.length });
        this.turnDiff = ev.diff;
        break;
      }

      case "turn_file_summary": {
        // A durable timeline artifact: unlike `turnDiff` (the live latest-turn accumulator),
        // every completed turn keeps its own patch so later sends cannot erase old cards.
        const exists = (ctx?.tl ?? this.timeline).some(
          (entry) => entry.kind === "turn_summary" && entry.id === ev.summary_id,
        );
        if (!exists) {
          this._pushTimeline(ctx, {
            kind: "turn_summary",
            id: ev.summary_id,
            anchorId: ev.summary_id,
            cwd: ev.cwd,
            diff: ev.diff,
            ts: eventTs(ev),
          });
        }
        break;
      }

      case "structured_task_state": {
        // Full replacement snapshot. An empty array authoritatively clears prior tasks;
        // it must NOT be ignored as "no new info" (otherwise stale tasks survive deletes).
        this.structuredTaskState = ev.tasks;
        break;
      }

      case "work_task_state": {
        // Full, Run-scoped Work Harness snapshot. Resume replays the same state;
        // Continue receives an independent copy at the selected checkpoint.
        this.workTaskState = ev.state;
        break;
      }

      case "work_context_plan_updated": {
        // Authoritative Work Context Plan update for the active run / turn.
        this.workContextPlan = ev.plan;
        break;
      }

      default:
        this.unknownEventCount++;
        dbgWarn("store", "unknown bus event type:", (ev as Record<string, unknown>).type);
        if (this.strictMode) {
          throw new Error(`[STRICT] unknown event type: ${(ev as Record<string, unknown>).type}`);
        }
    }
  }
}
