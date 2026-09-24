import type { RunStatus, StructuredTask } from "$lib/types";

export type AppMode = "code" | "work";

export type WorkPreset = "office" | "code" | "creative";

export const WORK_PRESETS: Array<{
  id: WorkPreset;
  label: string;
  description: string;
}> = [
  { id: "office", label: "Office", description: "文档、表格、演示与资料整理" },
  { id: "code", label: "Code", description: "代码阅读、修改、测试与验证" },
  { id: "creative", label: "Creative", description: "文案、视觉、演示与创意产出" },
];

export type WorkRuntime = "pi" | "dsh" | "claude" | "codex" | "grok" | "claude-code";

export type WorkResourceKind =
  | "skill"
  | "capability"
  | "connector"
  | "pi_extension"
  | "artifact_tool";

export interface WorkResourceDiscovery {
  aliases: string[];
  domains: string[];
  verbs: string[];
  nouns: string[];
  keywords: string[];
  guidance: string[];
  examples: string[];
}

export type WorkExecutionRuntime = "python" | "node";

export type ExecutionNetworkPolicy = "none" | "connector_only" | "internet";

export interface WorkExecutionManifest {
  trusted: boolean;
  runtime: WorkExecutionRuntime;
  entry: string;
  actions: string[];
  network: ExecutionNetworkPolicy;
  timeoutSeconds: number;
  readableAreas: string[];
  writableAreas: string[];
}

export interface WorkAutomationStats {
  totalTasks: number;
  enabledSchedules: number;
  runningTasks: number;
  needsAttentionTasks: number;
  completedRunsToday: number;
}

export interface WorkTaskRunSummary {
  runId: string;
  taskId: string;
  workspaceId: string;
  trigger: WorkRunTrigger;
  status: WorkRunStatus;
  durationMs?: number | null;
  deliveredArtifactsCount: number;
  goalPassed: boolean;
  goalStatus?: GoalStatus | null;
  errorMessage?: string | null;
  createdAt: string;
  startedAt?: string | null;
  finishedAt?: string | null;
}

export type LibraryCategory = "doc" | "template" | "rule" | "dataset" | "link";

export interface LibraryCitation {
  sourceType: string;
  sourceId?: string | null;
  label?: string | null;
  url?: string | null;
  artifactId?: string | null;
  runId?: string | null;
  toolCallId?: string | null;
  createdAt: string;
}

export interface LibraryItem {
  id: string;
  workspaceId?: string | null;
  title: string;
  description: string;
  category: LibraryCategory;
  content: string;
  tags: string[];
  sourcePath?: string | null;
  collection?: string | null;
  metadata: Record<string, string>;
  citations: LibraryCitation[];
  sourceArtifactId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface LibraryItemSummary {
  id: string;
  workspaceId?: string | null;
  title: string;
  description: string;
  category: LibraryCategory;
  tags: string[];
  sourcePath?: string | null;
  collection?: string | null;
  metadata: Record<string, string>;
  citationCount: number;
  contentPreview: string;
  sourceArtifactId?: string | null;
  updatedAt: string;
}

export type BrowserActionType =
  | "navigate"
  | "snapshot"
  | "screenshot"
  | "click"
  | "type"
  | "select_option"
  | "scroll"
  | "wait_for"
  | "tabs"
  | "close"
  | "takeover"
  | "custom";

export type BrowserSessionStatus =
  | "idle"
  | "running"
  | "waiting_approval"
  | "paused"
  | "taking_over"
  | "completed"
  | "failed"
  | "closed";

export interface BrowserTraceEntry {
  stepIndex: number;
  actionType: BrowserActionType;
  description: string;
  targetUrl?: string | null;
  selector?: string | null;
  status: string;
  screenshotData?: string | null;
  durationMs: number;
  error?: string | null;
  timestamp: string;
}

export interface BrowserSession {
  sessionId: string;
  runId: string;
  mode: string;
  surface?: "embedded";
  status: BrowserSessionStatus;
  currentUrl?: string | null;
  pageTitle?: string | null;
  currentAction?: string | null;
  lastScreenshot?: string | null;
  traces: BrowserTraceEntry[];
  lastError?: string | null;
  isTakingOver: boolean;
  createdAt: string;
  updatedAt: string;
}

export type BrowserEventType =
  | "session_started"
  | "navigation_started"
  | "navigation_finished"
  | "read_started"
  | "read_finished"
  | "action_requested"
  | "approval_required"
  | "action_started"
  | "action_succeeded"
  | "action_failed"
  | "screenshot_created"
  | "takeover_started"
  | "takeover_finished"
  | "session_paused"
  | "session_resumed"
  | "session_stopped"
  | "session_closed";

export interface BrowserEvent {
  eventType: BrowserEventType;
  sessionId: string;
  runId: string;
  mode: string;
  payload: Record<string, unknown>;
  timestamp: string;
}

export interface WorkResourceSummary {
  id: string;
  name: string;
  description: string;
  kind: WorkResourceKind;
  origin?: "builtin" | "user" | "community";
  entry: string;
  permissions: string[];
  enabled: boolean;
  active: boolean;
  runtimeAvailable: boolean;
  discovery: WorkResourceDiscovery;
  execution?: WorkExecutionManifest;
}

export interface WorkCapabilityMatch {
  resource: WorkResourceSummary;
  score: number;
  matchedFields: string[];
}

export interface PiPackageItem {
  name: string;
  description: string;
  author: string;
  downloads: number;
  downloadsFormatted: string;
  timeAgo: string;
  date: number;
  npmUrl: string;
  repoUrl: string;
  packagePath: string;
  installSource: string;
  types: string[];
  isRecent: boolean;
}

export interface WorkConnectorSummary {
  name: string;
  transport: string;
  enabled: boolean;
  command: string | null;
  args: string[];
  url: string | null;
  envKeys: string[];
  headerKeys: string[];
  runtimeAvailable: boolean;
  piRuntimeAvailable?: boolean;
  dshRuntimeAvailable?: boolean;
  adapterInstalled: boolean;
}

export type ConnectorRuntimeKind = "mcp" | "cli" | "skill";
export type ConnectorPackageOrigin = "builtin" | "community" | "local";
export type ConnectorAuthKind = "none" | "oauth2" | "api_key" | "cli";
export type ConnectorAuthStatus =
  | "not_required"
  | "not_authenticated"
  | "pending"
  | "authenticated"
  | "expired"
  | "error";
export type ConnectorRuntimeStatus = "not_ready" | "ready" | "failed";

export interface ConnectorPackagePermissions {
  network: string[];
  commands: string[];
  readableAreas: string[];
  writableAreas: string[];
  homePaths: string[];
}

export interface ConnectorPackageManifest {
  id: string;
  version: string;
  displayName: string;
  description: string;
  icon?: string | null;
  origin: ConnectorPackageOrigin;
  runtimes: ConnectorRuntimeKind[];
  auth: {
    kind: ConnectorAuthKind;
    provider?: string | null;
  };
  authFields: Array<{
    key: string;
    label: string;
    fieldType: "text" | "password" | string;
    required: boolean;
    placeholder: string;
    description: string;
  }>;
  mcpFile?: string | null;
  cliFile?: string | null;
  skillDirs: string[];
  permissions: ConnectorPackagePermissions;
}

export interface ConnectorPackageState {
  packageId: string;
  version: string;
  installed: boolean;
  trusted: boolean;
  enabled: boolean;
  authStatus: ConnectorAuthStatus;
  runtimeStatus: ConnectorRuntimeStatus;
  lastError?: string | null;
}

export interface ConnectorPackageSummary {
  manifest: ConnectorPackageManifest;
  state: ConnectorPackageState;
}

/** Built-in Connector catalog item shown in the Work Marketplace. */
export interface ConnectorCatalogItem {
  packageId: string;
  displayName: string;
  description: string;
  icon: string;
  categories: string[];
  capabilities: string[];
  runtimes: ConnectorRuntimeKind[];
  auth: {
    kind: ConnectorAuthKind;
    provider?: string | null;
  };
  origin: ConnectorPackageOrigin;
  builtin: boolean;
  installed: boolean;
  trusted: boolean;
  enabled: boolean;
  authStatus: ConnectorAuthStatus;
  connectionStatus: ConnectionStatus;
  accountCount: number;
  documentationUrl?: string | null;
}

export type WorkConnectorHealthStatus = "disabled" | "healthy" | "failed";

export interface WorkConnectorHealth {
  name: string;
  status: WorkConnectorHealthStatus;
  message: string;
  checkedAt: string;
  latencyMs: number;
  toolCount: number;
  toolNames: string[];
}

export type AppAuthType = "oauth2" | "api_key";
export type ConnectionStatus = "connected" | "disconnected" | "expired" | "error" | "pending";

export interface AppCatalogItem {
  appId: string;
  displayName: string;
  icon: string;
  description: string;
  categories: string[];
  capabilities: string[];
  authType: AppAuthType;
  documentationUrl?: string | null;
}

export interface AppAccount {
  accountId: string;
  alias?: string | null;
  displayName?: string | null;
  email?: string | null;
  status: ConnectionStatus;
  createdAt: string;
  lastUsedAt?: string | null;
}

export interface AppConnection {
  connectionId: string;
  appId: string;
  provider: string;
  status: ConnectionStatus;
  accounts: AppAccount[];
  lastCheckedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AuthorizeResponse {
  authorizationUrl: string;
  connectionId: string;
  expiresAt?: string | null;
}

export interface ComposioProviderConfig {
  hasApiKey: boolean;
  baseUrl: string;
}

export interface WorkBrowserSummary {
  enabled: boolean;
  provider: string;
  configured: boolean;
  adapterInstalled: boolean;
  piRuntimeAvailable?: boolean;
  dshRuntimeAvailable?: boolean;
  runtimeAvailable: boolean;
  browserRuntimeNodeAvailable: boolean;
  browserRuntimeAvailable: boolean;
  playwrightInstalled: boolean;
  chromiumInstalled: boolean;
  browserRuntimeManaged: boolean;
  browserRuntimeMessage: string;
  maxResults: number;
  endpointUrl?: string;
  authKind?: string;
  allowedHosts?: string[];
}

export interface DesktopUseStatus {
  enabled: boolean;
  ready: boolean;
  ownerRunId?: string | null;
  command: string;
  message: string;
  accessibility?: boolean | null;
  screenRecording?: boolean | null;
}

export type BrowserRuntimePreparationPhase = "initializing" | "verifying" | "completed" | "failed";

export type BrowserRuntimePreparationStatus = "running" | "completed" | "failed";

export interface BrowserRuntimePreparationProgress {
  phase: BrowserRuntimePreparationPhase;
  status: BrowserRuntimePreparationStatus;
  progress: number;
  message: string;
  log?: string;
  elapsedMs: number;
  error?: string;
  failedPhase?: BrowserRuntimePreparationPhase;
}

export type WorkBrowserHealthStatus = "disabled" | "unconfigured" | "healthy" | "failed";

export interface WorkBrowserHealth {
  status: WorkBrowserHealthStatus;
  provider: string;
  message: string;
  checkedAt: string;
  latencyMs: number;
}

export interface WorkFileSummary {
  path: string;
  name: string;
  area: string;
  size: number;
  modifiedAt: string;
}

export type WorkArtifactStatus =
  | "creating"
  | "ready"
  | "invalid"
  | "failed"
  | "validated"
  | "delivered";

export type WorkArtifactCategory =
  | "document"
  | "spreadsheet"
  | "presentation"
  | "pdf"
  | "image"
  | "html"
  | "code"
  | "archive"
  | "other";

export type WorkArtifactPreviewKind =
  | "pdf"
  | "image"
  | "markdown"
  | "html"
  | "csv"
  | "office"
  | "mermaid"
  | "code"
  | "binary";

export type WorkFileChangeKind = "created" | "modified" | "deleted";

export interface ArtifactProducer {
  runId: string;
  producerToolCallId?: string | null;
  executionId?: string | null;
}

export interface ArtifactVerificationEvidence {
  verifiedAt: string;
  validatorVersion: string;
  sha256: string;
  size: number;
  checks: string[];
  verificationStatus: WorkArtifactStatus;
}

export interface ArtifactSourceRef {
  sourceToolCallId: string;
  sourceType: string;
  url?: string | null;
  resultDigest: string;
  capturedAt: string;
  isClaimed: boolean;
}

export interface WorkArtifactSummary {
  id: string;
  workspaceId: string;
  runId?: string | null;
  artifactType: string;
  category: WorkArtifactCategory;
  mimeType: string;
  previewKind: WorkArtifactPreviewKind;
  canPreview: boolean;
  version: number;
  validationSummary?: string | null;
  title: string;
  path: string;
  status: WorkArtifactStatus;
  size: number;
  sha256?: string | null;
  producer?: ArtifactProducer | null;
  evidence?: ArtifactVerificationEvidence | null;
  sources?: ArtifactSourceRef[];
  createdAt: string;
  updatedAt: string;
}

export interface WorkArtifactRequirement {
  path: string;
  title?: string | null;
  artifactType?: string | null;
  required: boolean;
}

export type WorkArtifactCheckStatus = "satisfied" | "missing" | "invalid";

export interface WorkArtifactCheck {
  requirement: WorkArtifactRequirement;
  status: WorkArtifactCheckStatus;
  artifactId?: string | null;
  resolvedPath?: string | null;
  message: string;
  size?: number | null;
}

export interface WorkArtifactAcceptance {
  runId: string;
  checks: WorkArtifactCheck[];
  requiredCount: number;
  satisfiedCount: number;
  missingCount: number;
  invalidCount: number;
  satisfied: boolean;
}

/** User-facing lifecycle phase for a Work task. */
export type WorkProgressPhase =
  | "planning"
  | "running"
  | "waiting_approval"
  | "waiting_input"
  | "validating"
  | "awaiting_delivery"
  | "recoverable"
  | "completed"
  | "failed"
  | "cancelled"
  | "stopped"
  | "idle";

export interface WorkTaskCheckpoint {
  summary: string;
  currentStepId: string | null;
  createdAt: string;
}

export interface WorkPendingApproval {
  id: string;
  kind: "plan";
  summary: string;
  steps: StructuredTask[];
  createdAt: string;
}

export type GoalStatus =
  | "pending"
  | "checking"
  | "passed"
  | "failed"
  | "insufficient_evidence"
  | "not_applicable";

export type CriterionStatus =
  | "pending"
  | "checking"
  | "passed"
  | "failed"
  | "insufficient_evidence";

export type VerifierType = "artifact" | "file" | "structured" | "machine" | "llm" | "composite";

export interface AcceptanceCriterion {
  id: string;
  description: string;
  verifierType: VerifierType;
  targetRef?: string | null;
  status: CriterionStatus;
  evidenceRefs: string[];
  failureReason?: string | null;
}

export interface GoalSpec {
  goalId: string;
  workspaceId: string;
  runId: string;
  statement: string;
  criteria: AcceptanceCriterion[];
  status: GoalStatus;
  repairRound: number;
  maxRepairRounds: number;
  repairInstruction?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Authoritative task state owned by one Work Run. */
export interface WorkTaskState {
  version: number;
  revision: number;
  goal: string | null;
  goalSpec?: GoalSpec | null;
  plan: StructuredTask[];
  checkpoint: WorkTaskCheckpoint | null;
  /** Older persisted snapshots omit this field until a plan needs approval. */
  pendingApproval?: WorkPendingApproval | null;
  updatedAt: string;
}

export interface WorkSubagentSummary {
  id: string;
  agentId: string;
  role: string;
  task: string;
  resultSummary?: string;
  error?: string;
  status: "running" | "completed" | "failed" | "stopped" | "interrupted";
  statusText: string;
}

/** Wire shape of Rust WorkSubagentRecord (serde rename_all = "camelCase"). */
export interface WorkSubagentRecord {
  agentId: string;
  providerRunId: string;
  childIndex: number;
  role: string;
  parentScope: string;
  workspaceId?: string | null;
  taskId?: string | null;
  launchContractDigest: string;
  status: "running" | "completed" | "failed" | "stopped" | "interrupted" | string;
  error?: string | null;
  resultSummary?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Runtime snapshot shared by the Work chat and the Progress inspector. */
export interface WorkProgressSnapshot {
  phase: WorkProgressPhase;
  sessionPhase: string;
  runStatus: RunStatus | null;
  tasks: StructuredTask[];
  taskState: WorkTaskState | null;
  activeToolName: string;
  pendingApprovalCount: number;
  pendingAccessRoot?: boolean;
  pendingElicitation: boolean;
  error: string;
  toolCallCount?: number;
  subagents?: WorkSubagentSummary[];
}

export interface WorkProfile {
  id: string;
  name: string;
  enabled: boolean;
  runtime: WorkRuntime;
}

export type WorkRunProgressPhase =
  | "queued"
  | "planning"
  | "running"
  | "researching"
  | "implementing"
  | "reviewing"
  | "waiting_user"
  | "recoverable"
  | "awaiting_delivery"
  | "blocked"
  | "completed"
  | "failed"
  | "cancelled"
  | "idle";

export interface WorkCurrentActivity {
  kind: string;
  stepId?: string | null;
  toolName?: string | null;
  agentRole?: string | null;
  detail?: string | null;
}

export interface WorkProgressAttention {
  kind: string;
  count: number;
}

export interface WorkProgressAgent {
  agentId: string;
  childIndex: number;
  role: string;
  status: "running" | "completed" | "failed" | "stopped" | "interrupted" | string;
  resultSummary?: string | null;
  error?: string | null;
}

export interface WorkProgressToolSummary {
  proposed: number;
  started: number;
  completed: number;
  failed: number;
  running: number;
}

export interface WorkRunProgressView {
  taskId: string;
  workRunId: string;
  sessionId?: string | null;
  runStatus: WorkRunStatus;
  phase: WorkRunProgressPhase;
  goal?: string | null;
  goalSpec?: GoalSpec | null;
  steps: StructuredTask[];
  checkpoint?: WorkTaskCheckpoint | null;
  currentActivity?: WorkCurrentActivity | null;
  attention?: WorkProgressAttention | null;
  health?: WorkRunHealthView | null;
  agents: WorkProgressAgent[];
  toolSummary: WorkProgressToolSummary;
  updatedAt: string;
}

export interface WorkAvailableActions {
  canSend: boolean;
  canStop: boolean;
  canResume: boolean;
  canRecover: boolean;
}

export type PendingInteractionKind =
  | "permission"
  | "user_input"
  | "plan_approval"
  | "artifact_validation"
  | "access_root_request"
  | "app_connection_request"
  | "connector_auth_request"
  | { custom: string };

export type PendingInteractionState = "pending" | "delivering" | "resolved" | "cancelled";

export interface PendingInteraction {
  interactionId: string;
  taskId: string;
  workRunId: string;
  sessionId?: string | null;
  runtimeRequestId?: string | null;
  toolCallId?: string | null;
  kind: PendingInteractionKind;
  state: PendingInteractionState;
  title: string;
  description: string;
  payload: Record<string, unknown>;
  preparedTargetState?: PendingInteractionState | null;
  preparedInboxStatus?: InboxItemStatus | null;
  resolution?: Record<string, unknown> | null;
  createdAt: string;
  resolvedAt?: string | null;
}

export interface WorkProjection {
  runId: string;
  workspaceId?: string | null;
  automationTaskId?: string | null;
  taskId: string;
  workRunId: string;
  sessionId?: string | null;
  runtimeStatus: RunStatus;
  automationStatus?: WorkRunStatus | null;
  runStatus: WorkRunStatus;
  status: WorkRunStatus;
  phase: WorkRunProgressPhase;
  goal?: string | null;
  goalSpec?: GoalSpec | null;
  plan: StructuredTask[];
  steps: StructuredTask[];
  checkpoint?: WorkTaskCheckpoint | null;
  currentActivity?: WorkCurrentActivity | null;
  attention?: WorkProgressAttention | null;
  health?: WorkRunHealthView | null;
  agents: WorkProgressAgent[];
  toolSummary: WorkProgressToolSummary;
  artifacts: WorkArtifactSummary[];
  pendingInteractions: PendingInteraction[];
  availableActions: WorkAvailableActions;
  updatedAt: string;
}

export type WorkRecoveryAction = "continue" | "retry" | "verify" | "from_scratch" | "cancel";

export interface WorkRunRecovery {
  taskId: string;
  workRunId: string;
  sessionId?: string | null;
  status: WorkRunStatus;
  detectedAt: string;
  stepId?: string | null;
  stepTitle?: string | null;
  toolCallId?: string | null;
  toolName?: string | null;
  action?: string | null;
  sideEffectClass?: string | null;
  reusedExistingOutput: boolean;
  reason: string;
  recommendation: string;
  pendingInteractionId?: string | null;
  expectedOutputs: string[];
  acceptance?: WorkArtifactAcceptance | null;
  pendingInteractionCount: number;
  activeSubagents: number;
  interruptedSubagents: number;
  interruptedSubagentIds: string[];
  availableActions: WorkRecoveryAction[];
}

// ============================================================================
// Work Run Receipt（Ledger 投影的任务回执：成果 / 来源 / 输入 / 变更）
// ============================================================================

export type WorkReceiptSourceKind = "web_search" | "web_page" | "browser" | "library" | "other";

export interface WorkReceiptSource {
  url: string;
  title?: string | null;
  provider?: string | null;
  /** 出现在成功搜索结果中（未证明读取）。 */
  surfacedBySearch: boolean;
  /** 有真实成功的抓取/打开事件证明读取过。 */
  accessed: boolean;
  accessCount: number;
  firstSeenAt?: string | null;
  lastAccessedAt?: string | null;
  kinds: WorkReceiptSourceKind[];
}

export interface WorkReceiptSearchQuery {
  query: string;
  provider?: string | null;
  resultCount: number;
  timestamp: string;
}

export interface WorkReceiptFileChange {
  path: string;
  changeKind: WorkFileChangeKind;
}

export interface WorkRunReceipt {
  taskId: string;
  workRunId: string;
  workspaceId: string;
  sessionId?: string | null;
  standalone: boolean;
  status: WorkRunStatus;
  goal?: string | null;
  runtime?: string | null;
  model?: string | null;
  startedAt: string;
  finishedAt?: string | null;
  durationMs?: number | null;
  errorMessage?: string | null;
  artifacts: WorkArtifactSummary[];
  sources: WorkReceiptSource[];
  searchQueries: WorkReceiptSearchQuery[];
  inputFiles: string[];
  changedFiles: WorkReceiptFileChange[];
  toolSummary: {
    proposed: number;
    started: number;
    completed: number;
    failed: number;
    running: number;
  };
  ledgerAvailable: boolean;
  generatedAt: string;
}

export interface WorkRulesInfo {
  path: string;
  content: string;
  exists: boolean;
}

export interface WorkAccessRoot {
  path: string;
  writable: boolean;
}

export type WorkRootKind = "managed" | "local_folder";

export type WorkArtifactStorageMode = "managed" | "primary_work_root";

export interface WorkWorkspaceSummary {
  id: string;
  name: string;
  root: string;
  inputDir: string;
  scratchDir: string;
  outputDir: string;
  contextDir: string;
  createdAt: string;
  updatedAt: string;
  artifactCount: number;
  archived: boolean;
  accessRoots: WorkAccessRoot[];
  /** Workspace-level default policy; tasks inherit this and cannot exceed its autonomy. */
  defaultPolicy: WorkPolicy;
  defaultModel?: string;
  defaultEffort?: string;
  rootKind?: WorkRootKind;
  primaryWorkRoot?: string;
  workingRootValid?: boolean;
  artifactStorageMode?: WorkArtifactStorageMode;
}

export interface WorkCapabilitySummary {
  id: string;
  name: string;
  description: string;
  domain: string;
  active: boolean;
}

/* ============================================================================
 * Work Mode V2 Entities: WorkTask, WorkRun, InboxItem, WorkPolicy
 * ============================================================================ */

export type WorkExecutionMode = "direct" | "plan_first" | "auto" | "full_access";

export type ToolRiskClass = "read" | "write_local" | "exec" | "external";

export interface TaskStandingRule {
  id: string;
  toolName: string;
  targetPattern: string;
  riskClass: ToolRiskClass;
  grantedAt: string;
}

export type RunHealth = "healthy" | "warning" | "stalled" | "degraded" | "needs_attention";

export type RuntimeLiveness = "alive" | "dead" | "disconnected" | "alive_but_stalled";

export interface WorkRunBudgetView {
  toolCallsUsed?: number | null;
  toolCallsLimit?: number | null;
  isAnyWarning?: boolean;
  isAnyExceeded?: boolean;
}

export interface WorkRunHealthView {
  status: RunHealth;
  reason?: string | null;
  warningCount: number;
  stalledSince?: string | null;
  budget?: WorkRunBudgetView | null;
  lastMeaningfulProgressAt?: string | null;
  liveness?: RuntimeLiveness | null;
}

export type GuardianAnomalyKind =
  | "stall"
  | "duplicate_tool"
  | "tool_failure_streak"
  | "budget_exceeded"
  | "provider_degraded";

export interface GuardianConfig {
  stallAfterMs: number;
  stallWarnAfterMs?: number;
  maxDuplicateCalls: number;
  maxFailureStreak: number;
  maxToolCalls?: number;
  maxSteeringNudges?: number;
  maxAutomatedSteps?: number | null;
}

export interface WorkPolicy {
  executionMode: WorkExecutionMode;
  maxAutomatedSteps: number;
  allowExternalConnectors: boolean;
  standingRules: TaskStandingRule[];
  guardianConfig?: GuardianConfig | null;
}

export type WorkTaskStatus =
  | "draft"
  | "active"
  | "scheduled"
  | "in_run"
  | "needs_attention"
  | "completed"
  | "archived";

export type WorkScheduleKind = "cron" | "once" | "daily" | "weekly";

export interface WorkScheduleConfig {
  enabled: boolean;
  kind?: WorkScheduleKind;
  timezone?: string;
  fireAt?: string | null;
  fire_at?: string | null;
  timeOfDay?: string | null;
  time_of_day?: string | null;
  dayOfWeek?: number | null;
  day_of_week?: number | null;
  cronExpression?: string | null;
  cron_expression?: string | null;
  runOnStartup?: boolean;
  run_on_startup?: boolean;
  maxRuns?: number | null;
  max_runs?: number | null;
  nextRunAt?: string | null;
  next_run_at?: string | null;
  lastRunAt?: string | null;
  last_run_at?: string | null;
}

export type WorkTaskSource = "manual" | "dialog";

export interface WorkTask {
  id: string;
  workspaceId: string;
  title: string;
  instructions: string;
  status: WorkTaskStatus;
  source: WorkTaskSource;
  policy: WorkPolicy;
  schedule: WorkScheduleConfig | null;
  requiredArtifacts?: string[];
  artifactRequirements?: WorkArtifactRequirement[];
  runCount: number;
  lastRunId: string | null;
  lastRunAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export type WorkRunTrigger = "manual" | "scheduled" | "event_triggered";

export type ExecutionContext = "attended" | "unattended";

export type WorkRunStatus =
  | "queued"
  | "running"
  | "waiting_approval"
  | "waiting_input"
  | "recoverable"
  | "waiting_delivery"
  | "completed"
  | "failed"
  | "cancelled"
  | "skipped";

export interface WorkRun {
  id: string;
  taskId: string;
  workspaceId: string;
  sessionId: string | null;
  trigger: WorkRunTrigger;
  executionContext?: ExecutionContext;
  execution_context?: ExecutionContext;
  scheduledFor?: string | null;
  scheduled_for?: string | null;
  skippedReason?: string | null;
  skipped_reason?: string | null;
  status: WorkRunStatus;
  taskState: WorkTaskState;
  errorMessage: string | null;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  guardianConfig?: GuardianConfig | null;
  guardian_config?: GuardianConfig | null;
}

export type InboxItemType =
  | "permission_request"
  | "question_elicitation"
  | "plan_approval"
  | "artifact_validation"
  | "access_root_request"
  | "app_connection_request"
  | "connector_auth_request";

export type InboxItemStatus =
  | "pending"
  | "approved"
  | "rejected"
  | "answered"
  | "cancelled"
  | "expired";

export interface StandingRuleProposal {
  toolName: string;
  targetPattern: string;
  scope: "task" | "workspace";
  /** Backend-resolved effective risk class (action-level for work_execute). */
  riskClass?: ToolRiskClass;
}

export interface InboxItemPayload {
  requestId?: string;
  interactionKind?: string;
  toolUseId?: string;
  toolCallId?: string;
  toolName?: string;
  path?: string;
  writable?: boolean;
  purpose?: string;
  parameters?: Record<string, unknown>;
  question?: string;
  proposedPlan?: StructuredTask[];
  artifactId?: string;
  standingRuleProposal?: StandingRuleProposal;
  appId?: string;
  connectorId?: string;
  runtimeKind?: "mcp" | "cli" | "skill" | string;
  provider?: string;
  accountId?: string;
  requestedScopes?: string[];
  /** Legacy only; new auth interactions never persist an authorization URL. */
  authUrl?: string;
  recoveryKey?: string;
  recoveryAction?: string;
  originalToolCallId?: string;
  sideEffectClass?: string;
  expectedOutputs?: string[];
  failureKind?: string;
  failureReason?: string;
  availableActions?: string[];
  executionLane?: string;
  fallbackReason?: string;
  sandboxStderr?: string;
  packageManager?: string;
  packages?: string[];
  source?: string;
  destructiveReason?: string;
}

export interface InboxItem {
  id: string;
  taskId: string;
  runId: string;
  workspaceId: string;
  itemType: InboxItemType;
  status: InboxItemStatus;
  title: string;
  description: string;
  payload: InboxItemPayload;
  response?: unknown;
  createdAt: string;
  resolvedAt?: string;
}

export type WorkResultOutcome = "completed" | "failed" | "cancelled" | "unknown";

export interface WorkResultPresentation {
  outcome: WorkResultOutcome;
  title: string;
  subtitle?: string;
  goal?: string;
  completedSteps: number;
  totalSteps: number;
  artifactCount: number;
  deliveredCount: number;
  problematicCount: number;
  primaryArtifact?: WorkArtifactSummary;
  hasFileResults: boolean;
  isTerminal: boolean;
  collaborationSummary?: string;
  /** 失败/中断原因摘要（来自进度快照或子代理错误）。 */
  error?: string;
  /** 失败时最后未完成（in_progress）的步骤文本。 */
  failedStep?: string;
  /** 任务运行时长（毫秒），从 Session/Ledger/Run 获取。 */
  durationMs?: number;
  /** 格式化后的任务运行时长（例如 48s、1m 14s）。若无可靠时间则为 undefined。 */
  durationFormatted?: string;
}

export type WorkContextKind =
  | "base_policy"
  | "runtime_instructions"
  | "work_preset"
  | "workspace_rules"
  | "workspace_context"
  | "current_goal"
  | "current_task"
  | "conversation"
  | "skill"
  | "mcp"
  | "connector"
  | "agent_plugin"
  | "web_access"
  | "browser_use"
  | "library"
  | "artifact"
  | "memory"
  | "other";

export type WorkContextSource =
  | "system"
  | "workspace"
  | "capability_center"
  | "task_state"
  | "artifact_store"
  | "library_store"
  | "user_prompt"
  | "runtime"
  | { custom: string };

/** Harness relevance / policy observation; not runtime capability loading state. */
export type WorkContextSelection = "selected" | "deferred" | "rejected" | "unavailable";

export interface WorkContextSegment {
  id: string;
  kind: WorkContextKind;
  source: WorkContextSource;
  title: string;
  required: boolean;
  selection: WorkContextSelection;
  reason?: string | null;
  renderPriority: number;
  budgetPriority: number;
  estimatedTokens: number;
  refId?: string | null;
  content?: string | null;
  metadata?: Record<string, string>;
}

export interface WorkContextPlan {
  version: number;
  workspaceId?: string | null;
  taskId?: string | null;
  runId: string;
  sessionId?: string | null;
  runtime: WorkRuntime;
  preset?: WorkPreset | null;
  segments: WorkContextSegment[];
  estimatedTokens: number;
  createdAt: string;
}

// ── Capability Center 2.0 & Run Effective Capabilities Types ──

export type CapabilityReadiness =
  | "ready"
  | "needs_auth"
  | "missing_dependency"
  | "disabled"
  | "unhealthy"
  | "incompatible"
  | "not_installed";

export interface RuntimeAvailabilityScope {
  provider: string;
  available: boolean;
  /** Mode-specific availability; older projection responses may omit these. */
  workAvailable?: boolean;
  codeAvailable?: boolean;
  reason?: string;
}

export interface CapabilityAuthInfo {
  status: string;
  accountCount: number;
  accounts?: string[];
}

export interface CapabilityHealthInfo {
  status: string;
  latencyMs?: number;
  message?: string;
}

export interface CapabilityAction {
  actionType: string;
  label: string;
}

export interface CapabilityCenterItem {
  id: string;
  name: string;
  description: string;
  category: "skill" | "mcp" | "connector" | "app" | "browser" | string;
  origin: "builtin" | "user" | "community" | string;
  installed: boolean;
  enabled: boolean;
  readiness: CapabilityReadiness;
  readinessReason: string;
  scopes: string[];
  runtimeAvailability: RuntimeAvailabilityScope[];
  permissions: string[];
  auth?: CapabilityAuthInfo;
  health?: CapabilityHealthInfo;
  diagnostics?: string[];
  capabilities?: string[];
  actions: CapabilityAction[];
}

export interface CapabilityCenterOverview {
  total: number;
  readyCount: number;
  needsSetupCount: number;
  needsAuthCount: number;
  unavailableCount: number;
}

export interface CapabilityCenterProjection {
  overview: CapabilityCenterOverview;
  items: CapabilityCenterItem[];
}

export interface SkillCapabilityItemView {
  id: string;
  name: string;
  description?: string;
  owner?: { kind: string; id: string } | null;
}

export interface McpServerCapabilityItemView {
  id: string;
  transport: string;
  envKeys: string[];
  headerKeys: string[];
}

export interface ConnectorCapabilityItemView {
  id: string;
  name: string;
  entryPoint?: string;
}

export interface RunEffectiveCapabilitiesView {
  runId: string;
  runtime: string;
  appMode: string;
  enabledSkills: SkillCapabilityItemView[];
  mcpServers: McpServerCapabilityItemView[];
  connectors: ConnectorCapabilityItemView[];
  browserEnabled: boolean;
  browserUseEnabled: boolean;
  browserStatus: string;
  allowedTools: string[];
  disallowedTools: string[];
  diagnostics: string[];
  strictMode: boolean;
}
