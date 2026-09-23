export interface MemoryFileCandidate {
  path: string;
  label: string;
  scope: "project" | "global" | "memory";
  exists: boolean;
}

export type RunStatus =
  | "pending"
  | "running"
  | "idle"
  | "completed"
  | "failed"
  | "stopped"
  | "cancelled";

export type RunEventType = "system" | "stdout" | "stderr" | "command" | "user" | "assistant";

/** App-internal execution path for a run (materialized in TaskRun, never undefined). */
export type ExecutionPath = "session_actor" | "pipe_exec";

/** Unified resume/fork identity across agents. */
export type ConversationRef =
  | { kind: "claude_session"; id: string }
  | { kind: "codex_thread"; id: string }
  | { kind: "pi_session"; id: string }
  | { kind: "grok_session"; id: string };

/**
 * Product/route target. Runtime identity lives in `agent`; `pi:work` remains
 * accepted only as a legacy wire value from pre-runtime-neutral runs.
 */
export type AgentTarget =
  | "native:codex"
  | "native:claude"
  | "native:grok"
  | "pi:code"
  | "work"
  | "pi:work";

export type SettingsScope =
  | "global"
  | "native_codex"
  | "native_claude"
  | "native_grok"
  | "pi_common"
  | "pi_code"
  | "pi_work";

export interface TaskRun {
  id: string;
  prompt: string;
  cwd: string;
  agent: string;
  /** Code run created without a project, using its own AgentCabin-managed cwd. */
  code_standalone_task?: boolean;
  /** Product-level mode that owns this run. Legacy runs default to Code. */
  app_mode?: "code" | "work";
  /** Explicit runtime target. */
  agent_target?: AgentTarget;
  /** Work Workspace that owns this run, when app_mode is Work. */
  workspace_id?: string;
  /** Work Task that owns this run, when app_mode is Work. */
  work_task_id?: string;
  /** Work Run that owns this session, when app_mode is Work. */
  work_run_id?: string;
  /** Work user-facing task preset. Legacy runs default to office. */
  work_preset?: "office" | "code" | "creative";
  auth_mode: string;
  status: RunStatus;
  started_at: string;
  ended_at?: string;
  exit_code?: number;
  error_message?: string;
  last_activity_at?: string;
  message_count?: number;
  last_message_preview?: string;
  session_id?: string;
  result_subtype?: string;
  /** Model used in this run (persisted on hot-switch). */
  model?: string;
  /** Reasoning effort selected for this conversation. */
  effort?: string;
  /** Permission mode configured for this run (e.g. plan/ask/auto for Pi/Work). */
  permission_mode?: string;
  /** The run_id this session was forked from. */
  parent_run_id?: string;
  /** User-assigned display name. */
  name?: string;
  /** Remote host name (if running on a remote machine). */
  remote_host_name?: string;
  /** Snapshot of remote working directory at run creation. */
  remote_cwd?: string;
  /** Snapshot of active_platform_id at run creation time. */
  platform_id?: string;
  /** Snapshot of anthropic_base_url at run creation time. */
  platform_base_url?: string;
  /** Run source (native or cli_import). */
  source?: "native" | "cli_import";
  /** CLI import watermark for incremental sync. */
  cli_import_watermark?: ImportWatermark;
  /** Absolute path to CLI session JSONL file. */
  cli_session_path?: string;
  /** True when CLI import couldn't reconstruct complete usage data. */
  cli_usage_incomplete?: boolean;
  /** Snapshot of no_session_persistence at run creation time. */
  no_session_persistence?: boolean;
  /** Resolved execution path (session_actor or pipe_exec). Always present in API output. */
  execution_path: ExecutionPath;
  /** Unified resume identity. Undefined = not resumable. */
  conversation_ref?: ConversationRef;
  /** Codex CLI import: rollout files imported into this run. Used by sync to detect new rollouts. */
  codex_imported_rollouts?: CodexImportedRollout[];
  /** Pinned to top of sidebar. */
  pinned?: boolean;
  /** Archived (hidden from default sidebar view). */
  archived?: boolean;
  /** Unread badge indicator. */
  unread?: boolean;
}

export interface ImportWatermark {
  offset: number;
  mtimeNs: number;
  fileSize: number;
  lastUuid?: string;
}

export interface CliSessionSummary {
  /** "claude" or "codex" */
  agent: string;
  /** Claude session UUID or Codex thread_id */
  sessionId: string;
  cwd: string;
  firstPrompt: string;
  startedAt: string;
  lastActivityAt: string;
  /** Claude: message count; Codex: completed turn count */
  messageCount: number;
  model?: string;
  cliVersion?: string;
  /** Claude: file size; Codex: sum of all rollout sizes */
  fileSize: number;
  /** Claude: JSONL path; Codex: latest rollout path */
  filePath: string;
  /** Codex-only: list of all rollout files (asc by mtime). Empty/absent for Claude. */
  rolloutPaths?: string[];
  hasSubagents: boolean;
  alreadyImported: boolean;
  existingRunId?: string;
}

export interface ImportResult {
  runId: string;
  sessionId: string;
  eventsImported: number;
  eventsSkipped: number;
  usageIncomplete: boolean;
  skippedSubtypes: Record<string, number>;
}

export interface DiscoverResult {
  sessions: CliSessionSummary[];
  total: number;
  truncated: boolean;
}

export interface SyncResult {
  newEvents: number;
  /** Claude-only: watermark for next incremental sync. Undefined for Codex. */
  newWatermark?: ImportWatermark;
  /** Codex-only: rollout files imported in this sync. Empty/absent for Claude. */
  newRollouts?: string[];
  usageIncomplete: boolean;
}

/** Codex rollout file imported into a run. Codex-only. */
export interface CodexImportedRollout {
  path: string;
  size: number;
  /** Stringified nanoseconds since epoch (u128 unsafe for JS number). */
  mtimeNs: string;
  lastEventTs?: string;
}

export interface RunEvent {
  id: string;
  task_id: string;
  seq: number;
  type: RunEventType;
  payload: Record<string, unknown>;
  timestamp: string;
}

export interface RunArtifact {
  task_id: string;
  files_changed: string[];
  diff_summary: string;
  commands: string[];
  cost_estimate?: number;
  updated_at: string;
}

export interface UserSettings {
  default_agent: string;
  default_model?: string;
  allowed_tools: string[];
  working_directory?: string;
  provider_mode: string;
  auth_mode: string;
  anthropic_api_key?: string;
  anthropic_base_url?: string;
  /** "ANTHROPIC_API_KEY" or "ANTHROPIC_AUTH_TOKEN" — set by platform preset */
  auth_env_var?: string;
  permission_mode: string;
  max_budget_usd?: number;
  fallback_model?: string;
  keybinding_overrides: KeyBindingOverride[];
  remote_hosts?: RemoteHost[];
  platform_credentials?: PlatformCredential[];
  active_platform_id?: string;
  /** Canonical cross-agent provider profiles. Legacy provider fields remain for migration/runtime compatibility. */
  global_providers?: GlobalProviderCredential[];
  /** Per-agent choice: native CLI auth or a compatible global provider. */
  agent_provider_bindings?: AgentProviderBindings | null;
  /** null clears it (serde → None); undefined when absent. */
  codex_provider?: CodexProviderCredential | null;
  /** null = use Pi CLI configuration; value = AgentCabin-managed Pi provider. */
  pi_provider?: PiProviderCredential | null;
  /** Codex session transport: "app_server" (interactive tools) | "exec" (legacy, default). */
  codex_transport?: string;
  ui_zoom?: number;
  onboarding_completed: boolean;
  /** Beta kill-switch for the complete Work product surface. */
  work_mode_enabled?: boolean;
  web_server_enabled?: boolean;
  web_server_port?: number;
  web_server_bind?: string;
  web_server_allowed_origins?: string[];
  web_server_tunnel_url?: string;
  /** Custom path/program to launch the Claude CLI (default: auto-detect). (#155) */
  claude_path?: string;
  /** Custom path/program to launch the Codex CLI (default: auto-detect). */
  codex_path?: string;
  /** Custom path/program to launch the Pi CLI (default: auto-detect). */
  pi_path?: string;
  /** Custom path/program to launch Grok Build (default: auto-detect). */
  grok_path?: string;
  /** Custom path/program to launch DeepSeek Harness (DSH) (default: auto-detect). */
  dsh_path?: string;
  /** Enabled agents list ("claude", "codex", "pi", "grok", "dsh"). Undefined = all enabled. */
  enabled_agents?: string[];
  code_default_runtime?: string;
  work_default_runtime?: string;
  pet_enabled?: boolean;
  pet_scale?: number;
  pet_always_on_top?: boolean;
  pet_id?: string;
  pet_patrol_enabled?: boolean;
  pet_patrol_pause_min?: number;
  pet_snap_to_edge?: boolean;
  pet_click_interaction_enabled?: boolean;
  pet_grokbot_color?: string;
  pet_grokbot_shape?: string;
  pet_grokbot_parts?: string[];
  pet_grokbot_accessories?: string[];
  /** Root directory used for automatically created Git worktrees. */
  worktree_root?: string;
  /** Prefix used for branches created by AgentCabin continuation flows. */
  worktree_branch_prefix: string;
  /** Remove old, clean worktrees registered as AgentCabin-managed. */
  worktree_auto_cleanup: boolean;
  /** Maximum number of registered worktrees retained per repository. */
  worktree_cleanup_limit: number;
  updated_at: string;
}

export interface ProjectModelPreference {
  model?: string;
  effort?: string;
  permission_mode?: string;
}

export type ProviderProtocol = "anthropic-messages" | "openai-responses" | "openai-completions";

export interface GlobalProviderModel {
  id: string;
  name?: string;
  context_window?: number;
  max_tokens?: number;
  supports_reasoning?: boolean;
  supports_xhigh?: boolean;
  supported_effort_levels?: string[];
  supports_images?: boolean;
}

export interface GlobalProviderCredential {
  id: string;
  name: string;
  protocol: ProviderProtocol;
  base_url: string;
  api_key?: string;
  auth_env_var?: string;
  env_key?: string;
  models?: GlobalProviderModel[];
  test_model?: string;
  supports_developer_role?: boolean;
  supports_reasoning_effort?: boolean;
  extra_env?: Record<string, string>;
  keyless?: boolean;
}

export interface AgentProviderBinding {
  mode: "cli" | "custom";
  provider_id?: string;
  /** Models enabled for this agent. Omitted for legacy single-model bindings. */
  models?: string[];
  /** Default model and current initial model for this agent. */
  model?: string;
}

export interface AgentProviderBindings {
  claude: AgentProviderBinding;
  codex: AgentProviderBinding;
  pi: AgentProviderBinding;
  grok?: AgentProviderBinding;
  /** Optional AgentCabin-managed DSH route. Older settings omit this key. */
  dsh?: AgentProviderBinding;
}

/** Current non-modal UI surface published by a Pi extension. */
export interface PiExtensionUiState {
  extensionId?: string;
  extensionName?: string;
  title?: string;
  statusText?: string;
  widgetLines?: string[];
  editorText?: string;
  notifications?: Array<{
    id: string;
    type: "info" | "success" | "warning" | "error";
    message: string;
  }>;
}

export type PiExtensionUiMethod = "select" | "confirm" | "input" | "editor";

export interface PiExtensionUiRequest {
  id: string;
  runId: string;
  method: PiExtensionUiMethod;
  title?: string;
  message?: string;
  options: string[];
  placeholder?: string;
  prefill?: string;
  createdAt: string;
  expiresAt?: string;
}

export interface PiExtensionWidget {
  key: string;
  lines: string[];
  placement?: string;
}

export interface PiExtensionHostState {
  title?: string;
  statuses: Record<string, string>;
  widgets: Record<string, PiExtensionWidget>;
  pendingRequests: PiExtensionUiRequest[];
}

export interface PiFeatureCapabilities {
  commandsReady: boolean;
  planAvailable: boolean;
  goalAvailable: boolean;
  permissionAvailable: boolean;
  sessionTreeAvailable: boolean;
  forkAvailable: boolean;
  cloneAvailable: boolean;
  steerAvailable: boolean;
  followUpAvailable: boolean;
  clearQueueAvailable: boolean;
  navigateTreeAvailable: boolean;
}

export type PiPlanPhase =
  | "unavailable"
  | "inactive"
  | "entering"
  | "active"
  | "exiting"
  | "unknown"
  | "error";

export interface PiPlanState {
  phase: PiPlanPhase;
  updatedAt?: string;
  detail?: string;
}

export type PiGoalPhase =
  | "unavailable"
  | "inactive"
  | "starting"
  | "active"
  | "paused"
  | "budget_limited"
  | "complete"
  | "clearing"
  | "unknown"
  | "error";

export interface PiGoalState {
  phase: PiGoalPhase;
  id?: string;
  objective?: string;
  iteration?: number;
  tokenBudget?: number;
  tokensUsed?: number;
  timeUsedSeconds?: number;
  updatedAt?: string;
}

export type PiTodoStatus = "pending" | "in_progress" | "completed" | "cancelled";

export interface PiTodoTask {
  name: string;
  description: string;
  status: PiTodoStatus;
}

export interface PiTodoPhase {
  name: string;
  tasks: PiTodoTask[];
}

export interface PiTodoState {
  phases: PiTodoPhase[];
  workingOn?: string;
}

export interface PiPermissionState {
  available: boolean;
  mode: "guarded" | "accept_edits" | "auto_approve" | "custom";
  pendingRequestCount: number;
  restartRequired: boolean;
}

export interface PiMessageQueueState {
  steering: string[];
  followUp: string[];
  pendingCount: number;
  detailsKnown: boolean;
}

export interface PiSessionEntry {
  id: string;
  parentId?: string | null;
  type: string;
  timestamp?: string;
  message?: unknown;
  customType?: string;
  data?: unknown;
}

export interface PiSessionTreeNode {
  entry: PiSessionEntry;
  children: PiSessionTreeNode[];
  label?: string;
  labelTimestamp?: string;
}

export interface PiSessionTreeState {
  roots: PiSessionTreeNode[];
  leafId: string | null;
  loading: boolean;
  error?: string;
  lastEntryId?: string;
}

// ── Remote SSH types ──

export interface RemoteHost {
  name: string;
  host: string;
  user: string;
  port: number;
  key_path?: string;
  remote_cwd?: string;
  remote_claude_path?: string;
  forward_api_key: boolean;
}

export interface RemoteTestResult {
  ssh_ok: boolean;
  cli_found: boolean;
  cli_version?: string;
  cli_path?: string;
  error?: string;
}

// ── Keybinding types ──

export interface KeyBinding {
  command: string;
  label: string;
  key: string;
  context: "global" | "chat" | "prompt" | "cli";
  editable: boolean;
  source: "app" | "cli" | "codex";
  /** If true, this binding is also registered as an OS-level global shortcut. */
  osGlobal?: boolean;
}

export interface ScreenshotPayload {
  contentBase64: string;
  mediaType: string;
  filename: string;
}

export interface KeyBindingOverride {
  command: string;
  key: string;
}

export interface HookEvent {
  run_id: string;
  hook_type: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  tool_output?: Record<string, unknown>;
  status?: string;
  usage?: TokenUsage;
  timestamp: string;
  source?: string;
  reason?: string;
  error?: string;
  message?: string;
  title?: string;
  notification_type?: string;
  agent_id?: string;
  agent_type?: string;
  trigger?: string;
  task_id?: string;
  task_subject?: string;
  model?: string;
  session_id?: string;
  worktree?: { name: string; path: string; branch: string; originalRepoDir: string };
  /** Allow index access for dynamic field lookup (e.g. tool_use_id from hooks). */
  [key: string]: unknown;
}

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  cost: number;
}

export interface AgentSettings {
  agent: string;
  model?: string;
  allowed_tools: string[];
  working_directory?: string;
  plan_mode?: boolean;
  disallowed_tools?: string[];
  append_system_prompt?: string;
  max_budget_usd?: number;
  fallback_model?: string;
  system_prompt?: string;
  tool_set?: string;
  add_dirs?: string[];
  json_schema?: unknown;
  include_partial_messages?: boolean;
  cli_debug?: string;
  no_session_persistence?: boolean;
  max_turns?: number;
  effort?: string;
  betas?: string[];
  agents_json?: string;
  permission_mode?: string;
  /** Claude plugin roots explicitly enabled for Grok Build sessions. */
  grok_plugin_dirs?: string[];
  /** Codex `--ephemeral` — disable on-disk session persistence. */
  ephemeral?: boolean;
  /** Codex `--profile <name>` — select profile from ~/.codex/config.toml. */
  profile?: string;
  /** Codex `--ignore-user-config` — skip ~/.codex/config.toml. */
  ignore_user_config?: boolean;
  /** Codex `--ignore-rules` — skip execpolicy .rules files. */
  ignore_rules?: boolean;
  /** Codex `--search` — enable the native web_search tool (new sessions only). */
  web_search?: boolean;
  /** Pi `-e npm:@narumitw/pi-plan-mode` — load Plan extension independently. */
  pi_plan_mode_enabled?: boolean;
  /** Pi `-e npm:@narumitw/pi-goal` — load Goal extension independently. */
  pi_goal_enabled?: boolean;
  /** Pi `-e npm:@pi9/todo` — load the phased todo extension. */
  pi_todo_enabled?: boolean;
  /** Pi `-e npm:pi-context-prune` — load optional context pruning. Disabled by default. */
  pi_context_prune_enabled?: boolean;
  /** Pi `-e npm:pi-subagents` — load the shared Code/Work delegation extension. */
  pi_subagents_enabled?: boolean;
  /** Legacy compatibility field; Pi Code always loads system-managed multi-edit. */
  pi_multi_edit_enabled?: boolean;
  /** Pi `-e npm:@narumitw/pi-lsp` — load Language Server Protocol (LSP) code intelligence. */
  pi_lsp_enabled?: boolean;
  updated_at: string;
}

export interface PiProviderCredential {
  id: string;
  name: string;
  base_url: string;
  api: "openai-completions" | "openai-responses" | "anthropic-messages";
  model: string;
  /** All models enabled for this provider, not only the current selection. */
  models?: GlobalProviderModel[];
  context_window?: number;
  api_key?: string;
}

export type SessionMode = "new" | "resume" | "continue" | "fork";

export interface DirEntry {
  name: string;
  is_dir: boolean;
  size: number;
}

/** Entry shown by the prompt's @-mention picker. */
export interface AtMentionEntry extends DirEntry {
  kind?: "file";
}

export interface DirListing {
  path: string;
  entries: DirEntry[];
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: string;
}

export interface Attachment {
  name: string;
  type: string;
  size: number;
  contentBase64: string;
}

/** A message held by the composer until the current turn settles. */
export interface QueuedMessage {
  id: string;
  text: string;
  attachments: Attachment[];
}

export interface CliCheckResult {
  agent: string;
  found: boolean;
  path?: string;
  version?: string;
  version_supported?: boolean;
  minimum_version?: string;
  version_error?: string;
  config_path?: string;
  current_provider?: string;
  current_model?: string;
}

export interface PiProfileInfo {
  mode: "code" | "work";
  profileDir: string;
  settingsPath: string;
  rulesPath: string;
  extensionsDir: string;
  npmDir: string;
}

export interface ProjectInitStatus {
  cwd: string;
  has_claude_md: boolean;
  has_agents_md?: boolean;
}

export interface CliDistTags {
  latest?: string;
  stable?: string;
}

export interface LocalProxyStatus {
  proxyId: string;
  running: boolean;
  needsAuth: boolean;
  baseUrl: string;
  error?: string;
}

export interface ApiTestResult {
  success: boolean;
  latencyMs: number;
  reply?: string;
  error?: string;
  /** True when auth+connectivity OK but probe model was rejected (no user model configured). */
  partial: boolean;
}

export interface ModelUsageSummary {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number;
}

export interface RunUsageSummary {
  runId: string;
  name: string;
  agent: string;
  model?: string;
  status: RunStatus;
  startedAt: string;
  endedAt?: string;
  totalCostUsd: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  durationMs: number;
  numTurns: number;
  modelUsage?: Record<string, ModelUsageSummary>;
  costEstimated?: boolean;
}

export interface ModelAggregate {
  model: string;
  runs: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number;
  pct: number;
}

export interface DailyAggregate {
  date: string;
  costUsd: number;
  runs: number;
  inputTokens: number;
  outputTokens: number;
  /** Message count (Global mode — from Claude Code stats-cache). */
  messageCount?: number;
  /** Session count (Global mode — from Claude Code stats-cache). */
  sessionCount?: number;
  /** Tool call count (Global mode — from Claude Code stats-cache). */
  toolCallCount?: number;
  /** Per-model token breakdown (populated for last 30 daily entries only). */
  modelBreakdown?: Record<string, ModelTokens>;
}

export interface ModelTokens {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface UsageOverview {
  totalCostUsd: number;
  totalTokens: number;
  totalRuns: number;
  avgCostPerRun: number;
  byModel: ModelAggregate[];
  daily: DailyAggregate[];
  runs: RunUsageSummary[];
  /** How the data was produced: "memory", "disk", "incremental", "full". */
  scanMode?: string;
  /** Number of days with activity. */
  activeDays: number;
  /** Current consecutive active days. */
  currentStreak: number;
  /** Longest consecutive active days ever. */
  longestStreak: number;
}

// ── Git types ──

export interface GitFileStat {
  path: string;
  status: string; // "M", "A", "D", "R", "?"
  insertions: number;
  deletions: number;
}

export interface GitSummary {
  branch: string;
  files: GitFileStat[];
  total_files: number;
  total_insertions: number;
  total_deletions: number;
}

export interface GitProjectInfo {
  projectRoot: string;
  checkoutRoot: string;
  branch?: string;
  isWorktree: boolean;
  isTopLevel: boolean;
}

export interface GitWorktreeInfo {
  path: string;
  branch?: string;
  isMain: boolean;
  isCurrent: boolean;
  isPrunable: boolean;
  isDirty: boolean;
  lockedReason?: string;
}

// ── CLI Control Protocol types ──

export interface CliModelInfo {
  value: string;
  displayName: string;
  description: string;
  supportsEffort?: boolean;
  supportedEffortLevels?: string[];
  supportsAdaptiveThinking?: boolean;
  contextWindow?: number;
  maxTokens?: number;
  providerId?: string;
  providerName?: string;
}

export interface CliCommand {
  name: string;
  description: string;
  aliases?: string[];
  [key: string]: unknown;
}

export interface CliAccount {
  tokenSource: string;
  [key: string]: unknown;
}

export interface CliInfo {
  models: CliModelInfo[];
  commands: CliCommand[];
  available_output_styles: string[];
  account?: CliAccount;
  /** The model currently selected in Claude Code (from ~/.claude/settings.json) */
  current_model?: string;
  fetched_at: string;
}

/** Codex model catalog fetched live from `codex app-server` (model/list). */
export interface CodexModelList {
  models: CliModelInfo[];
  /** Model marked `isDefault` in the catalog, when present. */
  defaultModel?: string;
}

// ── Per-model usage breakdown ──

export interface ModelUsageEntry {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  web_search_requests: number;
  cost_usd: number;
  context_window?: number;
  maxOutputTokens?: number;
}

// ── MCP server info ──

export interface McpServerInfo {
  name: string;
  status: string;
  server_type?: string;
  scope?: string;
  error?: string;
}

export interface SubscriptionRateLimitWindow {
  used_percent: number;
  window_duration_mins: number;
  resets_at?: number | null;
}

export interface SubscriptionRateLimits {
  primary?: SubscriptionRateLimitWindow | null;
  secondary?: SubscriptionRateLimitWindow | null;
  plan_type?: string | null;
  has_credits?: boolean | null;
  raw_json?: string | null;
}

export interface CodexAuthResult {
  installed: boolean;
  version?: string | null;
  logged_in: boolean;
  auth_method?: "chatgpt" | "api_key" | "unknown" | null;
  status_text?: string | null;
  /** True when AgentCabin's independent ChatGPT subscription OAuth is active. */
  subscription_logged_in?: boolean;
  subscription_account?: string | null;
  subscription_rate_limits?: SubscriptionRateLimits | null;
}

// ── Diagnostics report (run_diagnostics command) ──

export interface DiagnosticsReport {
  cli: CliDiagnostics;
  auth: AuthDiagnostics;
  project: ProjectDiagnostics;
  configs: ConfigDiagnostics;
  services: ServicesDiagnostics;
  system: SystemDiagnostics;
  codex?: CodexAuthResult;
}

export interface CliDiagnostics {
  found: boolean;
  version: string | null;
  path: string | null;
  latest: string | null;
  stable: string | null;
  auto_update_channel: string | null;
  ripgrep_available: boolean;
}

export interface AuthDiagnostics {
  has_oauth: boolean;
  oauth_account: string | null;
  has_api_key: boolean;
  api_key_hint: string | null;
  api_key_source: string | null;
  app_has_credentials: boolean;
  app_platform_name: string | null;
}

export interface ProjectDiagnostics {
  cwd: string;
  has_claude_md: boolean;
  claude_md_files: ClaudeMdInfo[];
  has_agents_md: boolean;
  agents_md_files: AgentsMdInfo[];
  skipped_project_scope: boolean;
}

export interface ClaudeMdInfo {
  path: string;
  size_chars: number;
}

export interface AgentsMdInfo {
  path: string;
  size_chars: number;
}

export interface ConfigDiagnostics {
  settings_issues: ConfigIssue[];
  keybinding_issues: ConfigIssue[];
  mcp_issues: ConfigIssue[];
  env_var_issues: ConfigIssue[];
}

export interface ConfigIssue {
  scope: string;
  file: string;
  severity: string;
  message: string;
}

export interface ServicesDiagnostics {
  community_registry: boolean | null;
  mcp_registry: boolean | null;
}

export interface SystemDiagnostics {
  sandbox_available: boolean | null;
  lock_files: string[];
}

// ── Permission suggestion ──

export interface PermissionSuggestion {
  type: string;
  rules?: string[];
  behavior?: string;
  mode?: string;
  directories?: string[];
  destination?: string;
  /** additionalContext hook data */
  message?: unknown;
}

// ── Event Bus types ──

export interface ChatDelta {
  text: string;
}

export interface ChatDone {
  ok: boolean;
  code: number;
  error?: string;
}

// ── Team types (mirror Rust models.rs) ──

export interface TeamConfig {
  name: string;
  description: string;
  /** camelCase from serde rename */
  createdAt: number;
  leadAgentId: string;
  leadSessionId: string;
  members: TeamMember[];
}

export interface TeamMember {
  agentId: string;
  name: string;
  agentType: string;
  model: string;
  color: string;
  planModeRequired: boolean;
  joinedAt: number;
  tmuxPaneId: string;
  cwd: string;
  subscriptions: string[];
  backendType: string;
  /** The prompt given to spawned teammates (empty for leader) */
  prompt: string;
  /** Runtime active status from Claude Code SDK */
  isActive: boolean;
}

export interface TeamInboxMessage {
  from: string;
  text: string;
  summary: string;
  timestamp: string;
  color: string;
  read: boolean;
}

export interface TeamTask {
  id: string;
  subject: string;
  description: string;
  activeForm?: string;
  owner: string;
  status: string;
  blocks: string[];
  blockedBy: string[];
  metadata?: unknown;
}

export interface TeamSummary {
  name: string;
  description: string;
  /** snake_case — internal type, not deserialized from Claude Code files */
  member_count: number;
  task_count: number;
  created_at: number;
}

/** Runtime sub-agent task normalized across Claude, Pi, and Codex. */
export type CollaborationTaskStatus = "running" | "completed" | "failed" | "stopped";

export interface CollaborationTask {
  id: string;
  run_id: string;
  agent: string;
  role: string;
  description: string;
  model: string;
  cwd: string;
  tool_name: string;
  status: CollaborationTaskStatus;
  started_at: number;
  ended_at?: number;
}

// ── Plugin types ──

export interface PluginAuthor {
  name: string;
  email?: string;
}

export interface PluginComponents {
  skills: string[];
  commands: string[];
  agents: string[];
  hooks: boolean;
  mcp_servers: string[];
  lsp_servers: string[];
}

export interface MarketplacePlugin {
  name: string;
  description: string;
  version?: string;
  author?: PluginAuthor;
  category?: string;
  homepage?: string;
  source?: unknown;
  tags: string[];
  strict?: boolean;
  lsp_servers?: unknown;
  marketplace_name?: string;
  install_count?: number;
  components: PluginComponents;
}

export interface MarketplaceInfo {
  name: string;
  source: unknown;
  install_location: string;
  last_updated?: string;
  plugin_count: number;
  builtin?: boolean;
  display_name?: string;
}

export type SkillSourceKind = "user" | "project-agents" | "project-codex" | "legacy" | "bundled";
export type SkillDisabledBy = "path" | "name" | "bundled";

export interface StandaloneSkill {
  name: string;
  description: string;
  path: string;
  scope?: string;
  agent?: "claude" | "codex" | "grok" | "pi" | "dsh" | "work";
  source_kind?: SkillSourceKind;
  enabled?: boolean;
  disabled_by?: SkillDisabledBy;
  can_edit?: boolean;
  can_delete?: boolean;
  can_toggle?: boolean;
}

export interface PromptTemplate {
  id: string;
  name: string;
  description?: string;
  content: string;
  builtin: boolean;
}

export interface InstalledPlugin {
  name: string;
  description: string;
  version?: string;
  scope?: string;
  enabled?: boolean;
  marketplace?: string;
  pluginId?: string;
  agent?: "claude" | "codex" | "grok" | "pi";
  /** Project directory this plugin was installed in (project/local scope only). */
  projectPath?: string;
  /** Claude CLI's installed plugin root, when reported by the CLI. */
  installLocation?: string;
  /** Alternate Claude CLI field names retained for compatibility. */
  installPath?: string;
  path?: string;
  root?: string;
  [key: string]: unknown;
}

export interface AgentPluginAuthor {
  name?: string;
  email?: string;
  url?: string;
}

export interface AgentPluginSource {
  kind: "github" | "local" | string;
  location: string;
  installedAt: string;
}

export interface AgentPluginSkillSummary {
  id: string;
  name: string;
  description: string;
  path: string;
  hasAssets: boolean;
  pluginId: string;
  readonly: boolean;
}

export interface AgentPluginMcpServerSummary {
  id: string;
  name: string;
  originalName: string;
  transport: string;
  command?: string;
  args: string[];
  cwd?: string;
  url?: string;
  envKeys: string[];
  headerKeys: string[];
  pluginId: string;
  readonly: boolean;
}

export interface WorkBuddyExpertMemberSummary {
  id: string;
  name: string;
  profession: string;
  role: "lead" | "member" | string;
}

export interface AgentPluginSummary {
  id: string;
  name: string;
  version: string;
  description: string;
  author?: AgentPluginAuthor;
  homepage?: string;
  repository?: string;
  license?: string;
  source?: AgentPluginSource;
  skills: AgentPluginSkillSummary[];
  mcpServers: AgentPluginMcpServerSummary[];
  packageFormat: "workbuddy";
  expertKind?: "expert" | "expert-team";
  displayName?: string;
  profession?: string;
  members: WorkBuddyExpertMemberSummary[];
  warnings: string[];
  error?: string;
  trusted: boolean;
  enabled: boolean;
  canUpdate: boolean;
  canUninstall: boolean;
}

export interface AgentPluginBinding {
  pluginId: string;
  enabled: boolean;
  disabledBy?: string;
}

export interface CodexPluginInfo {
  pluginId: string;
  name: string;
  description: string;
  version?: string;
  marketplaceName?: string;
  installed: boolean;
  enabled?: boolean;
  installPolicy?: string;
  authPolicy?: string;
  source?: unknown;
  marketplaceSource?: unknown;
}

export interface CodexMarketplace {
  name: string;
  root: string;
  marketplaceSource?: unknown;
}

export interface PluginOperationResult {
  success: boolean;
  message: string;
}

export interface CommunitySkillResult {
  id: string;
  name: string;
  skill_id: string;
  installs: number;
  source: string;
}

export interface CommunitySkillDetail {
  id: string;
  name: string;
  description: string;
  installs: number;
  source: string;
  content: string | null;
  raw_url: string | null;
  skills_sh_url: string | null;
  github_url: string | null;
}

export interface ProviderHealth {
  available: boolean;
  reason: string | null;
}

// ── Auto-context tracking ──

export interface ContextSnapshot {
  runId: string;
  turnIndex: number;
  ts: string;
  data: import("$lib/utils/context-parser").ContextData;
}

// ── MCP Registry types ──

export interface McpRegistrySearchResult {
  servers: McpRegistryServer[];
  nextCursor: string | null;
  count: number;
}

export interface McpRegistryServer {
  name: string;
  description: string;
  title?: string;
  version: string;
  packages: McpRegistryPackage[];
  remotes: McpRegistryRemote[];
  repository?: McpRegistryRepository;
}

export interface McpRegistryPackage {
  registryType: string;
  identifier: string;
  version?: string;
  environmentVariables: McpRegistryEnvVar[];
}

export interface McpRegistryRemote {
  type: string;
  url: string;
  headers: McpRegistryHeader[];
}

export interface McpRegistryEnvVar {
  name: string;
  description?: string;
  isRequired?: boolean;
  isSecret?: boolean;
}

export interface McpRegistryHeader {
  name: string;
  description?: string;
  value?: string;
  isRequired?: boolean;
  isSecret?: boolean;
}

export interface McpRegistryRepository {
  url?: string;
  source?: string;
}

export interface ConfiguredMcpServer {
  name: string;
  server_type: string;
  scope: string;
  command?: string;
  args: string[];
  url?: string;
  env_keys: string[];
  header_keys: string[];
  agent?: "claude" | "codex" | "grok" | "pi" | "work";
  source_path?: string;
}

// ── Sidebar panel types ──

export interface FileEntry {
  path: string;
  action: "read" | "write" | "edit" | "persisted";
  toolUseId?: string; // only top-level tools can be scrolled to
  status?: string;
}

export interface SessionInfoData {
  sessionId?: string;
  runId?: string;
  runName?: string;
  cwd: string;
  numTurns: number;
  status: RunStatus;
  startedAt: string | null;
  endedAt: string | null;
  lastTurnDurationMs: number;
  tokensEstimated: boolean;
  model: string;
  agent: string;
  cliVersion: string;
  permissionMode: string;
  fastModeState: string;
  cost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  contextWindow: number;
  contextWindowEstimated?: boolean;
  contextUtilization: number;
  contextTokens?: number;
  /** Pi's latest structured context composition, when the built-in classifier has reported it. */
  contextBreakdown?: ContextUsageBreakdown | null;
  compactCount: number;
  microcompactCount: number;
  mcpServers: McpServerInfo[];
  remoteHostName?: string | null;
  platformId?: string | null;
  cliUsageIncomplete?: boolean;
  runSource?: string;
  authSourceLabel?: string;
  platformName?: string;
  cliUpdateAvailable?: string;
}

export interface ContextUsageCategory {
  id: string;
  tokens: number;
  children?: ContextUsageCategory[];
}

export interface ContextUsageBreakdown {
  version: number;
  source: string;
  measuredAt: number;
  contextWindow: number;
  usedTokens: number;
  percent: number;
  estimated: boolean;
  reportedAggregate: boolean;
  categories: ContextUsageCategory[];
}

/** Agent-neutral capability contract announced by a live session. */
export interface AgentCapabilities {
  protocol: AgentProtocolCapabilities;
  runtime: AgentRuntimeCapabilities;
  ui: AgentUiCapabilities;
}

export interface AgentProtocolCapabilities {
  sessionLoad: boolean;
  sessionSetModel: boolean;
  /** Whether this session exposes a host-controlled native mode selector. */
  sessionModeControl: boolean;
  permissionRequest: boolean;
  /** Whether the host can change the active permission mode for a live session. */
  permissionModeControl: boolean;
  slashCommands: boolean;
  planMode: boolean;
  effortControl: boolean;
  /** Whether the agent owns an authoritative structured Goal state. */
  goalState: boolean;
  /** Whether the session owns an authoritative structured task/Todo snapshot. */
  structuredTaskState: boolean;
}

export interface AgentRuntimeCapabilities {
  attachments: boolean;
  remote: boolean;
  fork: boolean;
  steer: boolean;
  /** Distinct delayed-turn semantics; never inferred from steer support. */
  followUp: boolean;
}

export interface AgentUiCapabilities {
  slashCommandMenu: boolean;
  planModeToggle: boolean;
  /** Host-side Goal panel; it may be prompt-backed rather than agent-native. */
  goalPanel: boolean;
  effortSelector: boolean;
  permissionModeSwitch: boolean;
  addDirAction: boolean;
}

export interface AgentSessionMode {
  id: string;
  name: string;
  description?: string;
}

export type BusEvent =
  | {
      type: "session_init";
      run_id: string;
      session_id?: string;
      model?: string;
      /** Live model catalog advertised by the session protocol. */
      model_options?: CliModelInfo[];
      tools: string[];
      cwd: string;
      slash_commands?: CliCommand[];
      /** True only after Pi's get_commands response has completed. */
      commands_loaded?: boolean;
      mcp_servers?: McpServerInfo[];
      permissionMode?: string;
      apiKeySource?: string;
      claude_code_version?: string;
      output_style?: string;
      agents?: string[];
      skills?: string[];
      plugins?: unknown[];
      plugin_errors?: unknown[];
      fast_mode_state?: string;
      capabilities?: AgentCapabilities;
    }
  | {
      type: "agent_mode_update";
      run_id: string;
      current_mode_id: string;
      available_modes?: AgentSessionMode[];
    }
  | {
      type: "structured_task_state";
      run_id: string;
      /** Full replacement snapshot. An empty list clears previously known tasks. */
      tasks: StructuredTask[];
    }
  | {
      type: "work_task_state";
      run_id: string;
      state: import("$lib/types/work").WorkTaskState;
    }
  | {
      type: "work_projection_changed";
      run_id: string;
      projection: import("$lib/types/work").WorkProjection;
    }
  | {
      type: "work_context_plan_updated";
      run_id: string;
      plan: import("$lib/types/work").WorkContextPlan;
    }
  | {
      type: "rate_limit_event";
      run_id: string;
      /** Rate limit status: "allowed", "allowed_warning", "rejected" */
      status: string;
      /** When the rate limit window resets (epoch seconds). */
      resets_at?: number;
      /** Which limit: "five_hour", "seven_day", etc. */
      rate_limit_type?: string;
      /** Utilization percentage (0.0-1.0). */
      utilization?: number;
      data: Record<string, unknown>;
    }
  | { type: "message_delta"; run_id: string; text: string; parent_tool_use_id?: string }
  | {
      type: "message_complete";
      run_id: string;
      message_id: string;
      text: string;
      parent_tool_use_id?: string;
      model?: string;
      stop_reason?: string | null;
      message_usage?: Record<string, unknown>;
    }
  | {
      type: "tool_start";
      run_id: string;
      tool_use_id: string;
      tool_name: string;
      /** Streamed Claude tool calls may start with null before input deltas arrive. */
      input: Record<string, unknown> | null;
      parent_tool_use_id?: string;
    }
  | {
      type: "tool_end";
      run_id: string;
      tool_use_id: string;
      tool_name: string;
      output: Record<string, unknown>;
      status: string;
      duration_ms?: number;
      parent_tool_use_id?: string;
      /** Structured tool result metadata from CLI verbose mode */
      tool_use_result?: Record<string, unknown>;
    }
  | {
      type: "user_message";
      run_id: string;
      text: string;
      uuid?: string;
      client_uuid?: string;
      attachments?: Array<{ name: string; mime_type: string; size: number }>;
    }
  | {
      type: "agent_handoff";
      run_id: string;
      source_agent: string;
      target_agent: string;
      source_run_id: string;
    }
  | { type: "run_state"; run_id: string; state: string; exit_code?: number; error?: string }
  | {
      type: "usage_update";
      run_id: string;
      input_tokens: number;
      output_tokens: number;
      cache_read_tokens?: number;
      cache_write_tokens?: number;
      total_cost_usd: number;
      /** Backend-authoritative turn index (1-based). Present for user turns. */
      turn_index?: number;
      /** Current prompt context size, when the agent reports it separately from cumulative usage. */
      context_tokens?: number;
      /** Context window reported by the agent for this usage snapshot. */
      context_window?: number;
      model_usage?: Record<string, ModelUsageEntry>;
      duration_api_ms?: number;
      duration_ms?: number;
      num_turns?: number;
      stop_reason?: string | null;
      service_tier?: string;
      speed?: string;
      web_fetch_requests?: number;
      cache_creation_5m?: number;
      cache_creation_1h?: number;
    }
  | { type: "pi_context_usage"; run_id: string; snapshot: ContextUsageBreakdown }
  | { type: "raw"; run_id: string; source: string; data: Record<string, unknown> }
  | { type: "thinking_delta"; run_id: string; text: string; parent_tool_use_id?: string }
  | {
      type: "tool_input_delta";
      run_id: string;
      tool_use_id: string;
      partial_json: string;
      parent_tool_use_id?: string;
    }
  | {
      type: "permission_denied";
      run_id: string;
      tool_name: string;
      tool_use_id: string;
      tool_input: Record<string, unknown>;
    }
  | {
      type: "permission_prompt";
      run_id: string;
      request_id: string;
      tool_name: string;
      tool_use_id: string;
      tool_input: Record<string, unknown>;
      decision_reason: string;
      parent_tool_use_id?: string;
      suggestions?: PermissionSuggestion[];
    }
  | { type: "compact_boundary"; run_id: string; trigger: string; pre_tokens?: number }
  | { type: "system_status"; run_id: string; status?: string; data: Record<string, unknown> }
  | {
      type: "pi_extension_ui";
      run_id: string;
      method: "setStatus" | "setWidget" | "setTitle" | "set_editor_text" | string;
      data: Record<string, unknown>;
    }
  | {
      type: "hook_started";
      run_id: string;
      hook_event: string;
      hook_id: string;
      data: Record<string, unknown>;
      hook_name?: string;
    }
  | { type: "hook_progress"; run_id: string; hook_id: string; data: Record<string, unknown> }
  | {
      type: "hook_response";
      run_id: string;
      hook_id: string;
      hook_event: string;
      outcome: string;
      data: Record<string, unknown>;
      hook_name?: string;
      stdout?: string;
      stderr?: string;
      exit_code?: number;
    }
  | {
      type: "task_notification";
      run_id: string;
      task_id: string;
      status: string;
      data: Record<string, unknown>;
    }
  | { type: "files_persisted"; run_id: string; files: unknown; data: Record<string, unknown> }
  | {
      type: "tool_progress";
      run_id: string;
      tool_use_id: string;
      elapsed_time_seconds?: number;
      data: Record<string, unknown>;
      parent_tool_use_id?: string;
    }
  | {
      type: "tool_output_delta";
      run_id: string;
      tool_use_id: string;
      delta: string;
      parent_tool_use_id?: string;
    }
  | {
      type: "tool_use_summary";
      run_id: string;
      tool_use_id: string;
      summary: string;
      preceding_tool_use_ids: string[];
      data: Record<string, unknown>;
      parent_tool_use_id?: string;
    }
  | {
      type: "auth_status";
      run_id: string;
      is_authenticating: boolean;
      output: string[];
      data: Record<string, unknown>;
    }
  | {
      type: "hook_callback";
      run_id: string;
      request_id: string;
      hook_event: string;
      hook_id: string;
      hook_name?: string;
      data: Record<string, unknown>;
    }
  | { type: "control_cancelled"; run_id: string; request_id: string }
  | { type: "command_output"; run_id: string; content: string }
  | {
      type: "elicitation_prompt";
      run_id: string;
      request_id: string;
      mcp_server_name: string;
      message: string;
      elicitation_id?: string;
      mode?: string;
      url?: string;
      requested_schema?: ElicitationSchema;
    }
  | {
      type: "ralph_started";
      run_id: string;
      prompt: string;
      max_iterations: number;
      completion_promise: string | null;
      started_at: string;
    }
  | { type: "ralph_iteration"; run_id: string; iteration: number; max_iterations: number }
  | {
      type: "ralph_complete";
      run_id: string;
      reason: RalphCompleteReason;
      iteration: number;
    }
  // Codex Wave-3: live goal progress from `thread/goal/updated`; goal is null
  // when the objective was cleared (`thread/goal/cleared`).
  | { type: "goal_update"; run_id: string; goal: ThreadGoal | null }
  // Codex Wave-4: hook lifecycle. status is "running" on hook/started, then a terminal
  // HookRunStatus ("completed"|"failed"|"blocked"|"stopped") on hook/completed. hook_id is
  // stable across the pair so the reducer upserts one card. event_name is camelCase HookEventName.
  | {
      type: "codex_hook_run";
      run_id: string;
      hook_id: string;
      event_name: string;
      status: string;
      status_message?: string;
      duration_ms?: number;
    }
  // Codex Wave-4: live MCP server startup-state change (`mcpServer/startupStatus/updated`).
  // status is the raw Codex McpServerStartupState ("starting"|"ready"|"failed"|"cancelled");
  // the store reducer maps it to the panel vocab and upserts store.mcpServers by name.
  | { type: "codex_mcp_status"; run_id: string; name: string; status: string; error?: string }
  // Codex Wave-4: turn-level aggregated unified diff (`turn/diff/updated`). diff is cumulative
  // across the turn; the store keeps the latest (cleared at the next turn). Not replayed.
  | { type: "codex_turn_diff"; run_id: string; turn_id: string; diff: string }
  | {
      type: "turn_file_summary";
      run_id: string;
      summary_id: string;
      cwd: string;
      diff: string;
    }
  | { type: "pi_extension_ui"; run_id: string; method: string; data: Record<string, unknown> }
  | { type: "pi_extension_ui_request_created"; run_id: string; request: PiExtensionUiRequest }
  | { type: "pi_extension_ui_request_resolved"; run_id: string; request_id: string }
  | { type: "pi_extension_notice"; run_id: string; notice_type: string; message: string }
  | { type: "pi_extension_state_sync"; run_id: string; snapshot: PiExtensionHostState }
  | { type: "pi_extension_editor_action"; run_id: string; text: string }
  | { type: "pi_feature_capabilities"; run_id: string; capabilities: PiFeatureCapabilities }
  | { type: "pi_plan_state"; run_id: string; state: PiPlanState }
  | { type: "pi_goal_state"; run_id: string; state: PiGoalState }
  | { type: "pi_todo_state"; run_id: string; state: PiTodoState }
  | { type: "pi_permission_state"; run_id: string; state: PiPermissionState }
  | { type: "pi_queue_updated"; run_id: string; state: PiMessageQueueState }
  | {
      type: "pi_session_entries";
      run_id: string;
      entries: PiSessionEntry[];
      leaf_id?: string;
      last_entry_id?: string;
    }
  | { type: "pi_session_tree"; run_id: string; roots: PiSessionTreeNode[]; leaf_id?: string };

export type RalphCompleteReason =
  | "max_iterations"
  | "completion_promise"
  | "cancelled"
  | "fail_stopped";

// ── Codex Wave-3: thread goal ──

/** Goal status values from Codex `thread/goal/*`. */
export type GoalStatus =
  | "active"
  | "paused"
  | "blocked"
  | "usageLimited"
  | "budgetLimited"
  | "complete";

/** Snapshot of a Codex session objective + live progress. */
export interface ThreadGoal {
  objective?: string;
  status?: GoalStatus;
  tokenBudget?: number;
  tokensUsed?: number;
  timeUsedSeconds?: number;
  createdAt?: string;
  updatedAt?: string;
}

// ── MCP Elicitation types ──

export interface ElicitationFieldSchema {
  type: "string" | "number" | "boolean" | "enum" | "array";
  title?: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  required?: boolean;
}

export interface ElicitationSchema {
  type?: string;
  properties?: Record<string, ElicitationFieldSchema>;
  required?: string[];
  [key: string]: unknown;
}

export interface BusToolItem {
  tool_use_id: string;
  tool_name: string;
  input: Record<string, unknown>;
  output?: Record<string, unknown>;
  status:
    | "running"
    | "success"
    | "error"
    | "denied"
    | "ask_pending"
    | "permission_denied"
    | "permission_prompt";
  /** For permission_prompt status: the control_request ID needed to respond. */
  permission_request_id?: string;
  duration_ms?: number;
  /** Real-time elapsed time from tool_progress (seconds, float). */
  elapsed_time_seconds?: number;
  /** Summary text from tool_use_summary. */
  summary?: string;
  /** Permission update suggestions from CLI. */
  suggestions?: PermissionSuggestion[];
  /** Structured tool result metadata from CLI verbose mode (e.g. file info for Read). */
  tool_use_result?: Record<string, unknown>;
  /** Allow index access for dynamic field lookup (e.g. _inputJsonAccum, _seq). */
  [key: string]: unknown;
}

export type TimelineEntry =
  | {
      kind: "user";
      id: string;
      /** Stable anchor for DOM id and search scroll-to. */
      anchorId: string;
      content: string;
      ts: string;
      attachments?: Attachment[];
      cliUuid?: string;
    }
  | {
      kind: "assistant";
      id: string;
      anchorId: string;
      content: string;
      ts: string;
      thinkingText?: string;
      model?: string;
    }
  | {
      kind: "tool";
      id: string;
      anchorId: string;
      tool: BusToolItem;
      ts: string;
      subTimeline?: TimelineEntry[];
    }
  | { kind: "separator"; id: string; anchorId: string; content: string; ts: string }
  | { kind: "command_output"; id: string; anchorId: string; content: string; ts: string }
  | {
      kind: "turn_summary";
      id: string;
      anchorId: string;
      cwd: string;
      diff: string;
      ts: string;
    }
  // Codex Wave-4: a hook execution timeline entry. Upserted by the `codex_hook_run`
  // reducer — keyed by `hookId` (= Codex run.id), stable across the started→completed
  // pair so the running card updates in place instead of stacking a second entry.
  | {
      kind: "hook";
      id: string;
      anchorId: string;
      hookId: string;
      eventName: string;
      status: string;
      statusMessage?: string;
      durationMs?: number;
      ts: string;
    };

/** One row of a TodoWrite checklist (lives in a tool's `tool_use_result.newTodos`). */
export interface TodoItem {
  content: string;
  status: "pending" | "in_progress" | "completed";
  activeForm: string;
}

/**
 * Unified task row for the TodoPanel, normalized from either source:
 * the Tasks system (TaskCreate/TaskUpdate, aggregated) or legacy TodoWrite snapshots.
 */
export interface PanelTask {
  id: string;
  text: string;
  status: "pending" | "in_progress" | "completed";
}

/** Authoritative task snapshot emitted by a session protocol (for example Grok ACP `plan`). */
export interface StructuredTask {
  id: string;
  text: string;
  status: "pending" | "in_progress" | "completed";
}

// ── App Updates ──

export interface UpdateInfo {
  hasUpdate: boolean;
  latestVersion: string;
  currentVersion: string;
  downloadUrl: string;
}

// ── Changelog ──

export interface ChangelogEntry {
  version: string;
  date: string;
  changes: string[];
}

// ── Hook config types (mirrors ~/.claude/settings.json hooks) ──

export type HookEventType =
  | "PreToolUse"
  | "PostToolUse"
  | "Notification"
  | "Stop"
  | "SubagentStop"
  | "SubagentTool"
  | "SubagentStart"
  | "SessionStart"
  | "SessionEnd"
  | "PermissionRequest"
  | "Setup"
  | "ConfigChange"
  | "TeammateIdle"
  | "TaskCompleted"
  | "WorktreeCreate"
  | "WorktreeRemove"
  | "InstructionsLoaded"
  | "Elicitation"
  | "ElicitationResult"
  | "PreCompact"
  | "PostCompact"
  | "StopFailure"
  | "TaskCreated"
  | "CwdChanged"
  | "FileChanged"
  | "PermissionDenied"
  | "MessageDisplay";

export interface HookHandler {
  type: "command" | "prompt" | "http" | "mcp_tool";
  command?: string;
  prompt?: string;
  timeout?: number;
  async?: boolean;
  statusMessage?: string;
  model?: string;
  once?: boolean;
  /** Conditional filter using permission rule syntax (e.g., `Bash(git *)`) — CLI 2.1.85+ */
  if?: string;
  /** mcp_tool handler: name of an already-configured MCP server to invoke — CLI 2.1.118+ */
  server?: string;
  /** mcp_tool handler: name of the tool on that server to call — CLI 2.1.118+ */
  tool?: string;
  /** mcp_tool handler: arguments passed to the MCP tool. String values support
   *  `${path}` interpolation from the hook input JSON (e.g. `${tool_input.file_path}`). */
  input?: Record<string, unknown>;
}

export interface HookMatcherGroup {
  matcher?: string;
  hooks: HookHandler[];
  [key: string]: unknown;
}

export type HooksConfig = Record<string, HookMatcherGroup[]>;

// ── CLI Config types ──

export interface CliConfigSettingDef {
  key: string;
  label: string;
  description: string;
  group: "behavior" | "appearance" | "advanced";
  type: "boolean" | "enum" | "string";
  default: unknown;
  options?: { value: string; label: string }[];
}

export interface CodexConfigResult {
  config: Record<string, unknown>;
  warning?: string;
}

// ── Onboarding types ──

export interface SshKeyInfo {
  key_path: string;
  key_path_expanded: string;
  pub_key_path: string;
  key_type: string;
  exists: boolean;
  pub_exists: boolean;
  ssh_copy_id_available: boolean;
}

export interface AuthCheckResult {
  has_oauth: boolean;
  has_api_key: boolean;
  oauth_account?: string;
}

/** Overview of all authentication sources (configuration state, no runtime inference). */
export interface AuthOverview {
  auth_mode: string;
  cli_login_available: boolean;
  cli_login_account?: string;
  cli_has_api_key: boolean;
  cli_api_key_hint?: string;
  /** Source of CLI API key: "settings" | "env" | "shell_config" */
  cli_api_key_source?: string;
  app_has_credentials: boolean;
  app_platform_id?: string;
  app_platform_name?: string;
}

/** Metadata-only status for Pi's native OAuth credential store. */
export interface PiAuthResult {
  logged_in: boolean;
  provider?: string;
  auth_type?: string;
  status_text?: string;
}

export interface InstallMethod {
  id: string;
  name: string;
  command: string;
  available: boolean;
  unavailable_reason?: string;
  note?: string;
}

// ── Prompt search & favorites ──

export interface PromptSearchResult {
  runId: string;
  runName?: string;
  runPrompt: string;
  agent: string;
  agentTarget?: AgentTarget;
  model?: string;
  status: RunStatus;
  startedAt: string;
  matchedText: string;
  matchedSeq: number;
  matchedTs: string;
  /** Stable event ID for scroll-to anchor (uuid or message_id). */
  matchedEventId?: string;
  isFavorite: boolean;
}

export interface PromptFavorite {
  runId: string;
  seq: number;
  text: string;
  tags: string[];
  note: string;
  createdAt: string;
}

// ── Run search (History page) ──

export interface RunSearchFilters {
  query?: string;
  projects?: string[];
  tools?: string[];
  dateFrom?: string;
  dateTo?: string;
  costMin?: number;
  costMax?: number;
  statuses?: RunStatus[];
  hasErrors?: boolean;
  agents?: string[];
  agentTarget?: string;
  realm?: string;
  sortBy?: "date" | "cost" | "tokens" | "turns";
  sortAsc?: boolean;
  limit?: number;
  offset?: number;
}

export interface RunSearchResult {
  runId: string;
  cwd: string;
  agent: string;
  agentTarget?: AgentTarget;
  model?: string;
  status: RunStatus;
  startedAt: string;
  endedAt?: string;
  name?: string;
  promptPreview: string;
  toolsUsed: string[];
  toolCallCount: number;
  filesTouchedCount: number;
  totalCostUsd: number;
  inputTokens: number;
  outputTokens: number;
  durationMs: number;
  numTurns: number;
  hasErrors: boolean;
  errorSummary?: string;
}

export interface FacetCount {
  value: string;
  count: number;
}

export interface RunSearchFacets {
  projects: FacetCount[];
  tools: FacetCount[];
  agents: FacetCount[];
  costRange: [number, number];
  dateRange: [string, string];
  totalRuns: number;
  totalCost: number;
}

export interface RunSearchResponse {
  results: RunSearchResult[];
  facets: RunSearchFacets;
  totalMatching: number;
}

export interface PlatformPreset {
  id: string;
  name: string;
  base_url: string;
  auth_env_var: "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN";
  description: string;
  key_placeholder: string;
  category: "provider" | "proxy" | "local" | "custom";
  models?: string[];
  extra_env?: Record<string, string>;
  docs_url?: string;
  setup_hint?: string;
}

/** Snapshot of PromptInput state for stash/restore. */
export interface PromptInputSnapshot {
  text: string;
  attachments: Array<{
    id: string;
    name: string;
    type: string;
    size: number;
    contentBase64?: string;
    filePath?: string;
  }>;
  pastedBlocks: Array<{
    id: string;
    text: string;
    lineCount: number;
    charCount: number;
    preview: string;
  }>;
  pathRefs?: Array<{ id: string; name: string; path: string; isDir: boolean }>;
  /** Global AgentCabin prompt templates inserted into the draft, kept position-aware by token. */
  promptTemplateTokens?: Array<{ token: string; name: string; content: string }>;
}

export interface PlatformCredential {
  platform_id: string;
  api_key?: string;
  base_url?: string;
  auth_env_var?: string;
  name?: string;
  models?: string[];
  extra_env?: Record<string, string>;
}

/** Codex third-party provider. `wire_api="chat"` is converted by AgentCabin's local bridge
 *  before the provider is injected into Codex. */
export interface CodexProviderCredential {
  id: string;
  name: string;
  base_url: string;
  env_key: string;
  wire_api: string;
  model: string;
  api_key?: string;
  supports_reasoning_effort?: boolean;
  supports_websockets?: boolean;
}

/** BTW side question streaming events (from Tauri) */
export interface BtwDelta {
  btw_id: string;
  text: string;
}
export interface BtwComplete {
  btw_id: string;
}
export interface BtwError {
  btw_id: string;
  error: string;
}

export interface AgentDefinitionSummary {
  file_name: string;
  name: string;
  description: string;
  model?: string;
  source: string;
  scope: "user" | "project" | "plugin";
  tools?: string[];
  disallowed_tools?: string[];
  permission_mode?: string;
  max_turns?: number;
  background?: boolean;
  isolation?: string;
  readonly: boolean;
  raw_content?: string;
  agent?: "claude" | "codex";
}

// ── Preview / Element Picker ──

export interface ElementSelection {
  url: string;
  viewport: { width: number; height: number };
  domPath: string;
  tagName: string;
  textContent: string;
  attributes: {
    id: string | null;
    class: string | null;
    role: string | null;
    name: string | null;
    ariaLabel: string | null;
  };
  outerHtmlSnippet: string;
  styleSummary: Record<string, string>;
}

/** Runtime guard for ElementSelection payloads from preview bridge. */
export function isElementSelection(v: unknown): v is ElementSelection {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  if (
    typeof o.url !== "string" ||
    typeof o.domPath !== "string" ||
    typeof o.tagName !== "string" ||
    typeof o.textContent !== "string" ||
    typeof o.outerHtmlSnippet !== "string"
  )
    return false;
  const vp = o.viewport;
  if (!vp || typeof vp !== "object") return false;
  const vpc = vp as Record<string, unknown>;
  if (typeof vpc.width !== "number" || typeof vpc.height !== "number") return false;
  const attrs = o.attributes;
  if (!attrs || typeof attrs !== "object") return false;
  const a = attrs as Record<string, unknown>;
  for (const key of ["id", "class", "role", "name", "ariaLabel"]) {
    if (a[key] !== null && typeof a[key] !== "string") return false;
  }
  const styles = o.styleSummary;
  if (!styles || typeof styles !== "object") return false;
  if (!Object.values(styles as Record<string, unknown>).every((v) => typeof v === "string"))
    return false;
  return true;
}
