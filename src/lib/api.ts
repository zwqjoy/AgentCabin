import { getTransport } from "./transport";
import { dbg, dbgWarn, redactSensitive } from "./utils/debug";
import { perfMarkAsync } from "./utils/perf";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}
import type {
  TaskRun,
  RunEvent,
  RunArtifact,
  UserSettings,
  ProjectModelPreference,
  AgentSettings,
  DirListing,
  Attachment,
  CliCheckResult,
  ProjectInitStatus,
  CliDistTags,
  UsageOverview,
  BusEvent,
  CliInfo,
  CliModelInfo,
  CodexModelList,
  SessionMode,
  TeamSummary,
  TeamConfig,
  TeamTask,
  TeamInboxMessage,
  MarketplacePlugin,
  MarketplaceInfo,
  StandaloneSkill,
  InstalledPlugin,
  PluginOperationResult,
  PiProfileInfo,
  GitSummary,
  GitProjectInfo,
  GitWorktreeInfo,
  ConfiguredMcpServer,
  McpRegistrySearchResult,
  ProviderHealth,
  ChangelogEntry,
  RemoteTestResult,
  SshKeyInfo,
  PromptSearchResult,
  PromptFavorite,
  SyncResult,
  DiagnosticsReport,
  AgentDefinitionSummary,
  RunSearchFilters,
  RunSearchResponse,
  CodexAuthResult,
  SubscriptionRateLimits,
  PiAuthResult,
  ThreadGoal,
  GoalStatus,
} from "./types";
import type { WorkBrowserHealth, WorkBrowserSummary } from "$lib/types/work";

// Runs
export async function listRuns(): Promise<TaskRun[]> {
  dbg("api", "listRuns");
  try {
    const runs = await invoke<TaskRun[]>("list_runs");
    dbg("api", "listRuns →", runs.length);
    return runs;
  } catch (e) {
    dbgWarn("api", "listRuns error", e);
    throw e;
  }
}

export async function getRun(id: string): Promise<TaskRun> {
  dbg("api", "getRun", id);
  return invoke<TaskRun>("get_run", { id });
}

export async function startRun(
  prompt: string,
  cwd: string,
  agent: string,
  model?: string,
  remoteHostName?: string,
  platformId?: string,
  executionPath?: string,
  continuationContext?: string,
): Promise<TaskRun> {
  dbg("api", "startRun", {
    prompt: prompt.slice(0, 80),
    agent,
    cwd,
    remoteHostName,
    platformId,
    executionPath,
    continuationContext,
  });
  const result = await invoke<TaskRun>("start_run", {
    prompt,
    cwd,
    agent,
    model,
    remoteHostName: remoteHostName ?? null,
    platformId: platformId ?? null,
    executionPath: executionPath ?? null,
    continuationContext: continuationContext ?? null,
  });
  dbg("api", "startRun →", result.id);
  return result;
}

export async function stopRun(id: string): Promise<boolean> {
  dbg("api", "stopRun", id);
  return invoke<boolean>("stop_run", { id });
}

export async function renameRun(id: string, name: string): Promise<void> {
  dbg("api", "renameRun", { id, name });
  return invoke<void>("rename_run", { id, name });
}

/** Persist the run model. Live switching is a separate capability-gated session control. */
export async function updateRunModel(id: string, model: string): Promise<void> {
  dbg("api", "updateRunModel", { id, model });
  return invoke<void>("update_run_model", { id, model });
}

/** Persist the reasoning effort for this conversation. */
export async function updateRunEffort(id: string, effort: string): Promise<void> {
  dbg("api", "updateRunEffort", { id, effort });
  return invoke<void>("update_run_effort", { id, effort });
}

/** Persist the run permission mode. */
export async function updateRunPermissionMode(id: string, permissionMode: string): Promise<void> {
  dbg("api", "updateRunPermissionMode", { id, permissionMode });
  return invoke<void>("update_run_permission_mode", { id, permissionMode });
}

/** Permanently delete runs and their local conversation data. */
export async function deleteRuns(ids: string[]): Promise<number> {
  dbg("api", "deleteRuns", { ids });
  return invoke<number>("delete_runs", { ids });
}

/** Update run-level flags (pinned, archived, unread). Only provided fields are updated. */
export async function setRunFlags(
  id: string,
  flags: { pinned?: boolean; archived?: boolean; unread?: boolean },
): Promise<void> {
  dbg("api", "setRunFlags", { id, flags });
  return invoke<void>("set_run_flags", { id, flags });
}

/** Reveal a file/folder in the OS file manager (Finder on macOS, Explorer on Windows). */
export async function revealInFinder(path: string): Promise<void> {
  dbg("api", "revealInFinder", { path });
  return invoke<void>("reveal_in_finder", { path });
}

/** Check whether the VS Code desktop application is available locally. */
export async function checkVscodeAvailable(): Promise<boolean> {
  dbg("api", "checkVscodeAvailable");
  return invoke<boolean>("check_vscode_available");
}

/** Open a local project directory in VS Code. */
export async function openProjectInVscode(cwd: string): Promise<void> {
  dbg("api", "openProjectInVscode", { cwd });
  return invoke<void>("open_project_in_vscode", { cwd });
}

// Prompt search & favorites

export async function searchPrompts(query: string, limit?: number): Promise<PromptSearchResult[]> {
  dbg("api", "searchPrompts", { query, limit });
  return invoke<PromptSearchResult[]>("search_prompts", { query, limit });
}

export async function addPromptFavorite(
  runId: string,
  seq: number,
  text: string,
): Promise<PromptFavorite> {
  dbg("api", "addPromptFavorite", { runId, seq });
  const result = await invoke<PromptFavorite>("add_prompt_favorite", { runId, seq, text });
  window.dispatchEvent(new Event("agentcabin:favorites-changed"));
  return result;
}

export async function removePromptFavorite(runId: string, seq: number): Promise<void> {
  dbg("api", "removePromptFavorite", { runId, seq });
  await invoke<void>("remove_prompt_favorite", { runId, seq });
  window.dispatchEvent(new Event("agentcabin:favorites-changed"));
}

export async function updatePromptFavoriteTags(
  runId: string,
  seq: number,
  tags: string[],
): Promise<void> {
  dbg("api", "updatePromptFavoriteTags", { runId, seq, tags });
  await invoke<void>("update_prompt_favorite_tags", { runId, seq, tags });
  window.dispatchEvent(new Event("agentcabin:favorites-changed"));
}

export async function updatePromptFavoriteNote(
  runId: string,
  seq: number,
  note: string,
): Promise<void> {
  dbg("api", "updatePromptFavoriteNote", { runId, seq, note });
  await invoke<void>("update_prompt_favorite_note", { runId, seq, note });
  window.dispatchEvent(new Event("agentcabin:favorites-changed"));
}

export async function listPromptFavorites(): Promise<PromptFavorite[]> {
  dbg("api", "listPromptFavorites");
  return invoke<PromptFavorite[]>("list_prompt_favorites");
}

export async function listPromptTags(): Promise<string[]> {
  dbg("api", "listPromptTags");
  return invoke<string[]>("list_prompt_tags");
}

// Run search (History)

export async function searchRuns(filters: RunSearchFilters): Promise<RunSearchResponse> {
  dbg("api", "searchRuns", filters);
  return invoke<RunSearchResponse>("search_runs", { filters });
}

export async function getRunFiles(runId: string): Promise<string[]> {
  dbg("api", "getRunFiles", { runId });
  return invoke<string[]>("get_run_files", { runId });
}

// Chat
export async function sendChatMessage(
  runId: string,
  message: string,
  attachments?: Attachment[],
  model?: string,
  clientUuid?: string,
  permissionModeOverride?: string,
): Promise<void> {
  dbg("api", "sendChatMessage", {
    runId,
    msgLen: message.length,
    attachments: attachments?.length ?? 0,
    clientUuid,
    permissionModeOverride,
  });
  return invoke("send_chat_message", {
    runId,
    message,
    attachments,
    model,
    clientUuid: clientUuid ?? null,
    permissionModeOverride: permissionModeOverride ?? null,
  });
}

// CLI sync
export async function syncCliSession(runId: string): Promise<SyncResult> {
  dbg("api", "syncCliSession", { runId });
  return invoke<SyncResult>("sync_cli_session", { runId });
}

// Events
export async function getRunEvents(id: string, sinceSeq?: number): Promise<RunEvent[]> {
  dbg("api", "getRunEvents", { id, sinceSeq });
  return invoke<RunEvent[]>("get_run_events", { id, sinceSeq });
}

// Artifacts
export async function getRunArtifacts(id: string): Promise<RunArtifact> {
  dbg("api", "getRunArtifacts", id);
  return invoke<RunArtifact>("get_run_artifacts", { id });
}

// Settings
export async function getUserSettings(): Promise<UserSettings> {
  dbg("api", "getUserSettings");
  return invoke<UserSettings>("get_user_settings");
}

export async function updateUserSettings(patch: Partial<UserSettings>): Promise<UserSettings> {
  dbg("api", "updateUserSettings");
  const settings = await invoke<UserSettings>("update_user_settings", { patch });
  const changesModelCatalog =
    "global_providers" in patch ||
    "agent_provider_bindings" in patch ||
    "pi_provider" in patch ||
    "codex_provider" in patch;
  if (changesModelCatalog) {
    // The model catalog store is module-scoped so it survives route changes. Invalidate it after
    // the backend has materialized the canonical settings; otherwise a chat opened immediately
    // after saving a Provider can still display the removed model from the old cache.
    try {
      const { invalidateModelCatalogs } = await import("./stores/cli-info.svelte");
      invalidateModelCatalogs();
    } catch (error) {
      dbgWarn("api", "failed to invalidate provider model catalogs", error);
    }
    if (typeof window !== "undefined") {
      window.dispatchEvent(
        new CustomEvent<UserSettings>("agentcabin:user-settings-changed", {
          detail: settings,
        }),
      );
    }
  }
  return settings;
}

export function getProjectPreferences(
  cwd: string,
  remoteHostName: string,
  agent: string,
): Promise<ProjectModelPreference> {
  return invoke<ProjectModelPreference>("get_project_preferences", {
    cwd,
    remoteHostName: remoteHostName || null,
    agent,
  });
}

export function updateProjectPreferences(
  cwd: string,
  remoteHostName: string,
  agent: string,
  patch: Partial<ProjectModelPreference>,
): Promise<ProjectModelPreference> {
  return invoke<ProjectModelPreference>("update_project_preferences", {
    cwd,
    remoteHostName: remoteHostName || null,
    agent,
    patch,
  });
}

export async function getAgentSettings(agent: string): Promise<AgentSettings> {
  dbg("api", "getAgentSettings", agent);
  return invoke<AgentSettings>("get_agent_settings", { agent });
}

export async function updateAgentSettings(
  agent: string,
  patch: Partial<AgentSettings>,
): Promise<AgentSettings> {
  dbg("api", "updateAgentSettings", agent);
  const result = await invoke<AgentSettings>("update_agent_settings", { agent, patch });
  // Sync sidebar resume-gate cache with updated settings
  import("$lib/stores/agent-settings-cache.svelte").then((m) => m.refreshAgentSettingsCache(agent));
  return result;
}

// Filesystem
export async function listDirectory(path: string, showHidden?: boolean): Promise<DirListing> {
  dbg("api", "listDirectory", path, { showHidden });
  return invoke<DirListing>("list_directory", { path, showHidden });
}

export async function checkIsDirectory(path: string): Promise<boolean> {
  return invoke<boolean>("check_is_directory", { path });
}

// Remote filesystem (over SSH)
export async function listRemoteDirectory(
  hostName: string,
  path: string,
  showHidden?: boolean,
): Promise<DirListing> {
  dbg("api", "listRemoteDirectory", { hostName, path, showHidden });
  return invoke<DirListing>("list_remote_directory", {
    hostName,
    path,
    showHidden: showHidden ?? null,
  });
}

export async function resolveRemoteHome(hostName: string): Promise<string> {
  dbg("api", "resolveRemoteHome", { hostName });
  return invoke<string>("resolve_remote_home", { hostName });
}

export async function readFileBase64(path: string, cwd: string): Promise<[string, string]> {
  return perfMarkAsync(
    "ipc-readFileBase64",
    () => invoke<[string, string]>("read_file_base64", { path, cwd }),
    { path },
  );
}

// Git
export async function getGitSummary(cwd: string): Promise<GitSummary> {
  dbg("api", "getGitSummary", cwd);
  return invoke<GitSummary>("get_git_summary", { cwd });
}

export async function getGitBranch(cwd: string): Promise<string> {
  dbg("api", "getGitBranch", cwd);
  return invoke<string>("get_git_branch", { cwd });
}

export async function getGitProject(cwd: string): Promise<GitProjectInfo> {
  return invoke<GitProjectInfo>("get_git_project", { cwd });
}

export async function listGitWorktrees(cwd: string): Promise<GitWorktreeInfo[]> {
  return invoke<GitWorktreeInfo[]>("list_git_worktrees", { cwd });
}

export async function createGitWorktree(
  cwd: string,
  branch: string,
  path?: string,
): Promise<GitWorktreeInfo> {
  return invoke<GitWorktreeInfo>("create_git_worktree", { cwd, branch, path: path ?? null });
}

export async function removeGitWorktree(cwd: string, path: string, force = false): Promise<void> {
  return invoke("remove_git_worktree", { cwd, path, force });
}

export async function getContinuationContext(runId: string, anchorId?: string): Promise<string> {
  return invoke<string>("get_continuation_context", { runId, anchorId: anchorId ?? null });
}

export async function copyRunHistory(
  sourceRunId: string,
  targetRunId: string,
  anchorId?: string,
): Promise<void> {
  return invoke("copy_run_history", {
    sourceRunId,
    targetRunId,
    anchorId: anchorId ?? null,
  });
}

export type GitBranchInfo = {
  name: string;
  is_current: boolean;
  uncommitted_count: number;
  worktree_path?: string | null;
};

export async function listGitBranches(cwd: string): Promise<GitBranchInfo[]> {
  dbg("api", "listGitBranches", cwd);
  return invoke<GitBranchInfo[]>("list_git_branches", { cwd });
}

export async function checkoutGitBranch(cwd: string, branch: string): Promise<string> {
  dbg("api", "checkoutGitBranch", { cwd, branch });
  return invoke<string>("checkout_git_branch", { cwd, branch });
}

export async function createGitBranch(cwd: string, branch: string): Promise<string> {
  dbg("api", "createGitBranch", { cwd, branch });
  return invoke<string>("create_git_branch", { cwd, branch });
}

export async function runTerminalCommand(cwd: string, command: string): Promise<string> {
  dbg("api", "runTerminalCommand", { cwd, command });
  return invoke<string>("run_terminal_command", { cwd, command });
}

// ── PTY (interactive terminal) ──

export async function ptyCreate(
  sessionId: string,
  cwd: string,
  cols: number,
  rows: number,
): Promise<void> {
  dbg("api", "ptyCreate", { sessionId, cwd, cols, rows });
  return invoke<void>("pty_create", { sessionId, cwd, cols, rows });
}

export async function ptyWrite(sessionId: string, data: string): Promise<void> {
  return invoke<void>("pty_write", { sessionId, data });
}

export async function ptyResize(sessionId: string, cols: number, rows: number): Promise<void> {
  return invoke<void>("pty_resize", { sessionId, cols, rows });
}

export async function ptyKill(sessionId: string): Promise<void> {
  dbg("api", "ptyKill", { sessionId });
  return invoke<void>("pty_kill", { sessionId });
}

export interface PtyDataEvent {
  session_id: string;
  data: string;
}

export interface PtyExitEvent {
  session_id: string;
  exit_code: number | null;
}

export async function getGitDiff(cwd: string, staged: boolean, file?: string): Promise<string> {
  dbg("api", "getGitDiff", { cwd, staged, file });
  return perfMarkAsync(
    "ipc-getGitDiff",
    () => invoke<string>("get_git_diff", { cwd, staged, file: file ?? null }),
    { cwd, staged, file },
  );
}

export async function getGitStatus(cwd: string): Promise<string> {
  dbg("api", "getGitStatus", cwd);
  return invoke<string>("get_git_status", { cwd });
}

export async function createGitTurnSnapshot(cwd: string): Promise<string> {
  dbg("api", "createGitTurnSnapshot", cwd);
  return invoke<string>("create_git_turn_snapshot", { cwd });
}

export async function diffGitTurnSnapshot(
  cwd: string,
  beforeTree: string,
  paths?: string[],
): Promise<string> {
  dbg("api", "diffGitTurnSnapshot", { cwd, beforeTree, paths });
  return invoke<string>("diff_git_turn_snapshot", { cwd, beforeTree, paths });
}

export async function applyGitTurnPatch(cwd: string, patch: string, reverse = true): Promise<void> {
  dbg("api", "applyGitTurnPatch", { cwd, patchLen: patch.length, reverse });
  return invoke<void>("apply_git_turn_patch", { cwd, patch, reverse });
}

export async function persistTurnFileSummary(
  runId: string,
  summaryId: string,
  cwd: string,
  diff: string,
): Promise<void> {
  dbg("api", "persistTurnFileSummary", { runId, summaryId, cwd, diffLen: diff.length });
  return invoke<void>("persist_turn_file_summary", { runId, summaryId, cwd, diff });
}

// Export
export async function exportConversation(runId: string): Promise<string> {
  dbg("api", "exportConversation", runId);
  return invoke<string>("export_conversation", { runId });
}

export async function writeHtmlExport(path: string, content: string): Promise<void> {
  dbg("api", "writeHtmlExport", { path, contentLen: content.length });
  return invoke<void>("write_html_export", { path, content });
}

// Memory file candidates
export async function listMemoryFiles(
  cwd?: string,
): Promise<import("./types").MemoryFileCandidate[]> {
  dbg("api", "listMemoryFiles", { cwd });
  return invoke<import("./types").MemoryFileCandidate[]>("list_memory_files", { cwd: cwd ?? null });
}

export async function listGrokSkills(cwd?: string): Promise<import("./types").StandaloneSkill[]> {
  dbg("api", "listGrokSkills", { cwd });
  return invoke<import("./types").StandaloneSkill[]>("list_grok_skills", { cwd: cwd ?? null });
}

// Files

/** Check whether `{cwd}/AGENTS.md` exists.
 *
 * Narrow by design — only the AGENTS.md filename is checkable (hardcoded
 * backend-side). Used by the Codex `/init` flow to decide whether to skip
 * rather than overwrite. Backend rejects empty / relative / `..`-containing
 * cwds; this is not a general filesystem probe.
 */
export async function agentsMdExists(cwd: string): Promise<boolean> {
  dbg("api", "agentsMdExists", { cwd });
  return invoke<boolean>("agents_md_exists", { cwd });
}

export async function checkPathExists(path: string): Promise<boolean> {
  return invoke<boolean>("check_path_exists", { path });
}

export async function readTextFile(path: string, cwd?: string): Promise<string> {
  dbg("api", "readTextFile", path, { cwd });
  return perfMarkAsync(
    "ipc-readTextFile",
    async () => {
      const content = await invoke<string>("read_text_file", { path, cwd: cwd ?? null });
      return content;
    },
    { path, chars: 0 }, // chars not known until after; left for shape consistency
  );
}

/** Cheap file size lookup — used by FilePreviewPane to skip readTextFile for huge files. */
export async function statTextFile(path: string, cwd?: string): Promise<number> {
  dbg("api", "statTextFile", path, { cwd });
  return perfMarkAsync(
    "ipc-statTextFile",
    () => invoke<number>("stat_text_file", { path, cwd: cwd ?? null }),
    { path },
  );
}

export async function writeTextFile(path: string, content: string, cwd?: string): Promise<void> {
  dbg("api", "writeTextFile", path, { cwd });
  return invoke("write_text_file", { path, content, cwd: cwd ?? null });
}

// Task output
export async function readTaskOutput(path: string): Promise<string> {
  dbg("api", "readTaskOutput", path);
  return invoke<string>("read_task_output", { path });
}

// Stats
export async function getUsageOverview(days?: number): Promise<UsageOverview> {
  dbg("api", "getUsageOverview", { days });
  return invoke<UsageOverview>("get_usage_overview", { days: days ?? null });
}

export async function getGlobalUsageOverview(days?: number): Promise<UsageOverview> {
  dbg("api", "getGlobalUsageOverview", { days });
  return invoke<UsageOverview>("get_global_usage_overview", { days: days ?? null });
}

export async function clearUsageCache(): Promise<void> {
  dbg("api", "clearUsageCache");
  return invoke<void>("clear_usage_cache");
}

export async function getHeatmapDaily(
  scope: "app" | "global",
): Promise<import("./types").DailyAggregate[]> {
  dbg("api", "getHeatmapDaily", { scope });
  return invoke<import("./types").DailyAggregate[]>("get_heatmap_daily", { scope });
}

// Diagnostics
export async function checkCodexAuth(): Promise<CodexAuthResult> {
  return invoke<CodexAuthResult>("check_codex_auth");
}

export async function getChatGptSubscriptionRateLimits(): Promise<SubscriptionRateLimits | null> {
  return invoke<SubscriptionRateLimits | null>("get_chatgpt_subscription_rate_limits");
}

/** One `codex doctor` check (install/config/auth/runtime/app-server health). */
export interface CodexDoctorCheck {
  id: string;
  category: string;
  status: string; // "ok" | "warn" | "fail" | ...
  summary: string;
  details?: Record<string, unknown>;
  remediation?: string | null;
  durationMs?: number;
}

/** Structured `codex doctor --json` report. */
export interface CodexDoctorReport {
  schemaVersion: number;
  generatedAt: string;
  overallStatus: string; // "ok" | "warn" | "fail"
  codexVersion: string;
  checks: Record<string, CodexDoctorCheck>;
}

/** Run `codex doctor --json` — richer than checkCodexAuth (install/config/auth/runtime/app-server).
 *  Rejects when codex is absent / can't run / output isn't JSON. */
export async function runCodexDoctor(): Promise<CodexDoctorReport> {
  dbg("api", "runCodexDoctor");
  return invoke<CodexDoctorReport>("run_codex_doctor");
}

export async function checkAgentCli(agent: string): Promise<CliCheckResult> {
  dbg("api", "checkAgentCli", agent);
  return invoke<CliCheckResult>("check_agent_cli", { agent });
}

export async function resolveCapabilitiesDiagnostics(
  appMode: string,
  runtime: string,
  cwd?: string,
): Promise<any> {
  dbg("api", "resolveCapabilitiesDiagnostics", { appMode, runtime, cwd });
  return invoke("resolve_capabilities_diagnostics", { appMode, runtime, cwd });
}

export async function checkProjectInit(cwd: string): Promise<ProjectInitStatus> {
  dbg("api", "checkProjectInit", cwd);
  return invoke<ProjectInitStatus>("check_project_init", { cwd });
}

export async function getCliDistTags(): Promise<CliDistTags> {
  dbg("api", "getCliDistTags");
  return invoke<CliDistTags>("get_cli_dist_tags");
}

export async function checkSshKey(): Promise<SshKeyInfo> {
  dbg("api", "checkSshKey");
  return invoke<SshKeyInfo>("check_ssh_key");
}

export async function generateSshKey(): Promise<SshKeyInfo> {
  dbg("api", "generateSshKey");
  return invoke<SshKeyInfo>("generate_ssh_key");
}

export async function detectLocalProxy(
  proxyId: string,
  baseUrl: string,
): Promise<import("./types").LocalProxyStatus> {
  dbg("api", "detectLocalProxy", { proxyId, baseUrl });
  return invoke<import("./types").LocalProxyStatus>("detect_local_proxy", { proxyId, baseUrl });
}

export async function testApiConnectivity(
  apiKey: string,
  baseUrl: string,
  authEnvVar: string,
  model: string,
): Promise<import("./types").ApiTestResult> {
  dbg("api", "testApiConnectivity", { baseUrl, authEnvVar, model });
  return invoke<import("./types").ApiTestResult>("test_api_connectivity", {
    apiKey,
    baseUrl,
    authEnvVar,
    model,
  });
}

export async function testGlobalProvider(
  provider: import("./types").GlobalProviderCredential,
  model: string,
): Promise<import("./types").ApiTestResult> {
  dbg("api", "testGlobalProvider", { providerId: provider.id, model });
  return invoke<import("./types").ApiTestResult>("test_global_provider", { provider, model });
}

export async function listGlobalProviderModels(
  provider: import("./types").GlobalProviderCredential,
): Promise<import("./types").GlobalProviderModel[]> {
  dbg("api", "listGlobalProviderModels", { providerId: provider.id });
  return invoke<import("./types").GlobalProviderModel[]>("list_global_provider_models", {
    provider,
  });
}

export async function testPiProvider(
  provider: import("./types").PiProviderCredential,
): Promise<import("./types").ApiTestResult> {
  dbg("api", "testPiProvider", { id: provider.id, api: provider.api, model: provider.model });
  return invoke<import("./types").ApiTestResult>("test_pi_provider", { provider });
}

export async function runDiagnostics(cwd: string): Promise<DiagnosticsReport> {
  dbg("api", "runDiagnostics", { cwd });
  return invoke<DiagnosticsReport>("run_diagnostics", { cwd });
}

export async function testRemoteHost(
  host: string,
  user: string,
  port?: number,
  keyPath?: string,
  remoteClaudePath?: string,
): Promise<RemoteTestResult> {
  dbg("api", "testRemoteHost", { host, user, port });
  return invoke<RemoteTestResult>("test_remote_host", {
    host,
    user,
    port: port ?? null,
    keyPath: keyPath ?? null,
    remoteClaudePath: remoteClaudePath ?? null,
  });
}

// CLI Control Protocol
export async function getCliInfo(forceRefresh?: boolean): Promise<CliInfo> {
  dbg("api", "getCliInfo", { forceRefresh });
  try {
    const info = await invoke<CliInfo>("get_cli_info", { forceRefresh });
    dbg("api", "getCliInfo →", { models: info.models.length });
    return info;
  } catch (e) {
    dbgWarn("api", "getCliInfo error", e);
    throw e;
  }
}

export async function getCodexModels(forceRefresh?: boolean): Promise<CodexModelList> {
  dbg("api", "getCodexModels", { forceRefresh });
  // Backend already substitutes a minimal fallback on failure, so this rarely throws.
  const list = await invoke<CodexModelList>("get_codex_models", { forceRefresh });
  dbg("api", "getCodexModels →", { models: list.models.length });
  return list;
}

/** Fetch Pi's model catalog before a live session exists. */
export async function getPiModels(): Promise<CliModelInfo[]> {
  dbg("api", "getPiModels");
  const models = await invoke<CliModelInfo[]>("get_pi_models");
  dbg("api", "getPiModels →", { models: models.length });
  return models;
}

// Session (event bus)
export async function startSession(
  runId: string,
  mode?: SessionMode,
  sessionId?: string,
  initialMessage?: string,
  attachments?: Array<{ content_base64: string; media_type: string; filename: string }>,
  platformId?: string,
  permissionModeOverride?: string,
  skills?: Array<{ name: string; path: string }>,
): Promise<void> {
  dbg("api", "startSession", {
    runId,
    mode,
    sessionId,
    hasMessage: !!initialMessage,
    attachments: attachments?.length ?? 0,
    platformId,
    permissionModeOverride,
    skills: skills?.length ?? 0,
  });
  return invoke("start_session", {
    runId,
    mode,
    sessionId,
    initialMessage,
    attachments: attachments ?? null,
    platformId: platformId ?? null,
    permissionModeOverride: permissionModeOverride ?? null,
    skills: skills ?? null,
  });
}

export async function sendSessionMessage(
  runId: string,
  message: string,
  attachments?: Array<{ content_base64: string; media_type: string; filename: string }>,
  // Structured Codex skill refs — sent as {type:"skill", name, path} UserInput items so the
  // agent actually triggers the skill. `path` is required by the backend; sourcing name+path
  // from the runtime skills list (not from typed "/name" text) is what makes this valid.
  // Omitted/empty = unchanged behavior (Claude + Codex-without-skill).
  skills?: Array<{ name: string; path: string }>,
): Promise<void> {
  dbg("api", "sendSessionMessage", {
    runId,
    msgLen: message.length,
    attachments: attachments?.length ?? 0,
    skills: skills?.length ?? 0,
  });
  return invoke("send_session_message", {
    runId,
    message,
    attachments: attachments ?? null,
    skills: skills && skills.length > 0 ? skills : null,
  });
}

/** Interrupt the current Claude stream-json turn, then send guidance in the same session. */
export async function steerSessionMessage(
  runId: string,
  message: string,
  attachments?: Array<{ content_base64: string; media_type: string; filename: string }>,
): Promise<void> {
  dbg("api", "steerSessionMessage", {
    runId,
    msgLen: message.length,
    attachments: attachments?.length ?? 0,
  });
  return invoke("steer_session_message", {
    runId,
    message,
    attachments: attachments ?? null,
  });
}

/** Cancel only a DSH turn; the JSON-RPC session remains available for follow-up. */
export async function cancelSessionTurn(runId: string): Promise<void> {
  dbg("api", "cancelSessionTurn", { runId });
  return invoke("cancel_session_turn", { runId });
}

export async function sendSessionControl(
  runId: string,
  subtype: string,
  params?: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  dbg("api", "sendSessionControl", { runId, subtype, params });
  try {
    const result = await invoke<Record<string, unknown>>("send_session_control", {
      runId,
      subtype,
      params: params ?? null,
    });
    dbg("api", "sendSessionControl →", result);
    return result;
  } catch (e) {
    dbgWarn("api", "sendSessionControl error", e);
    throw e;
  }
}

export async function stopSession(runId: string): Promise<void> {
  dbg("api", "stopSession", runId);
  return invoke("stop_session", { runId });
}

export interface LoadRunDataResult {
  run: TaskRun;
  busEvents: BusEvent[];
}

export async function loadRunData(id: string, syncCli = false): Promise<LoadRunDataResult> {
  dbg("api", "loadRunData", { id, syncCli });
  return invoke<LoadRunDataResult>("load_run_data", { id, syncCli });
}

export async function getBusEvents(id: string, sinceSeq?: number): Promise<BusEvent[]> {
  dbg("api", "getBusEvents", { id, sinceSeq });
  return invoke<BusEvent[]>("get_bus_events", { id, sinceSeq });
}

export async function getToolResult(
  runId: string,
  toolUseId: string,
): Promise<Record<string, unknown> | null> {
  dbg("api", "getToolResult", { runId, toolUseId });
  return invoke<Record<string, unknown> | null>("get_tool_result", { runId, toolUseId });
}

export async function forkSession(runId: string): Promise<string> {
  dbg("api", "forkSession", { runId });
  return invoke<string>("fork_session", { runId });
}

export interface AgentSwitchResult {
  run_id: string;
  context: string;
}

/** Create a new branch that continues a run with a different first-party Agent. */
export async function switchRunAgent(
  runId: string,
  targetAgent: "claude" | "codex" | "pi" | "grok" | "dsh",
): Promise<AgentSwitchResult> {
  dbg("api", "switchRunAgent", { runId, targetAgent });
  return invoke<AgentSwitchResult>("switch_agent", {
    runId,
    targetAgent,
  });
}

export async function sideQuestion(runId: string, question: string): Promise<string> {
  dbg("api", "sideQuestion", { runId, question: question.slice(0, 50) });
  return invoke<string>("side_question", { runId, question });
}

export async function approveSessionTool(runId: string, toolName: string): Promise<void> {
  dbg("api", "approveSessionTool", { runId, toolName });
  return invoke("approve_session_tool", { runId, toolName });
}

export async function respondPermission(
  runId: string,
  requestId: string,
  behavior: string,
  updatedPermissions?: import("./types").PermissionSuggestion[],
  updatedInput?: Record<string, unknown>,
  denyMessage?: string,
  interrupt?: boolean,
): Promise<void> {
  dbg("api", "respondPermission", {
    runId,
    requestId,
    behavior,
    updatedPermissions,
    updatedInput,
    denyMessage,
    interrupt,
  });
  return invoke("respond_permission", {
    runId,
    requestId,
    behavior,
    updatedPermissions: updatedPermissions ?? null,
    updatedInput: updatedInput ?? null,
    denyMessage: denyMessage ?? null,
    interrupt: interrupt ?? null,
  });
}

export async function respondHookCallback(
  runId: string,
  requestId: string,
  decision: "allow" | "deny" | "defer",
  // PreToolUse hooks can rewrite tool input alongside `allow` (CLI v2.1.85+).
  // Only honored by the CLI when decision == "allow".
  updatedInput?: Record<string, unknown>,
): Promise<void> {
  dbg("api", "respondHookCallback", {
    runId,
    requestId,
    decision,
    hasUpdatedInput: updatedInput != null,
  });
  return invoke("respond_hook_callback", {
    runId,
    requestId,
    decision,
    updatedInput: updatedInput ?? null,
  });
}

// ── Typed control request wrappers ──

export async function setSessionModel(runId: string, model: string) {
  return sendSessionControl(runId, "set_model", { model });
}

export async function interruptSession(runId: string) {
  return sendSessionControl(runId, "interrupt");
}

export async function setPermissionMode(runId: string, mode: string) {
  return sendSessionControl(runId, "set_permission_mode", { mode });
}

/** Set an agent-native ACP session mode without conflating it with permission policy. */
export async function setSessionMode(runId: string, mode: string) {
  return sendSessionControl(runId, "set_session_mode", { mode });
}

/** Set reasoning effort live; the provider's response decides whether it was accepted. */
export async function setEffort(runId: string, effort: string) {
  const response = await sendSessionControl(runId, "set_effort", { effort });
  if (response.success === false || response.ok === false) {
    const error = response.error;
    const message =
      typeof error === "string"
        ? error
        : error && typeof error === "object" && "message" in error
          ? String(error.message)
          : "模型不支持当前思考档位";
    throw new Error(message);
  }
  return response;
}

/** Inject guidance into the currently-running turn without interrupting it
 *  (Codex app-server `turn/steer`). Routed by the store when a Codex turn is
 *  running and the user sends from the mid-turn send button. */
export async function steerSession(runId: string, text: string) {
  return sendSessionControl(runId, "steer", { text });
}

/** Pi RPC steer: deliver guidance during the current turn. */
export async function sendPiSteer(
  runId: string,
  message: string,
  attachments?: Array<{ content_base64: string; media_type: string; filename: string }>,
) {
  return sendSessionControl(runId, "pi_steer", {
    message,
    attachments: attachments ?? [],
  });
}

/** Pi RPC follow_up: run after the current turn settles. */
export async function sendPiFollowUp(
  runId: string,
  message: string,
  attachments?: Array<{ content_base64: string; media_type: string; filename: string }>,
) {
  return sendSessionControl(runId, "pi_follow_up", {
    message,
    attachments: attachments ?? [],
  });
}

/** Pi RPC clear_queue: clear queued steering and follow-up messages. */
export async function clearPiQueue(runId: string) {
  return sendSessionControl(runId, "pi_clear_queue");
}

export async function getPiSessionTree(runId: string) {
  return sendSessionControl(runId, "pi_get_tree");
}

export async function getPiSessionEntries(runId: string, since?: string) {
  return sendSessionControl(runId, "pi_get_entries", since ? { since } : undefined);
}

export async function forkPiSessionAt(runId: string, entryId: string): Promise<string> {
  return invoke<string>("fork_pi_session_at", { runId, entryId });
}

export async function clonePiSession(runId: string): Promise<string> {
  return invoke<string>("clone_pi_session", { runId });
}

export async function switchPiSession(runId: string, sessionPath: string) {
  return sendSessionControl(runId, "pi_switch_session", { session_path: sessionPath });
}

export async function setMaxThinkingTokens(runId: string, tokens: number) {
  return sendSessionControl(runId, "set_max_thinking_tokens", { max_thinking_tokens: tokens });
}

export async function getMcpStatus(runId: string) {
  return sendSessionControl(runId, "mcp_status");
}

/** A skill the Codex agent actually sees this session (vs the static file-scan list).
 *  Mirrors Codex 0.136 `SkillMetadata` (only the fields we render). `scope` is the
 *  load origin: user | repo | system | admin. */
export interface CodexRuntimeSkill {
  name: string;
  description: string;
  shortDescription?: string;
  path: string;
  scope: "user" | "repo" | "system" | "admin";
  enabled: boolean;
}

/** One cwd's resolved skill set, plus any per-skill load errors. Mirrors `SkillsListEntry`. */
export interface CodexRuntimeSkillsEntry {
  cwd: string;
  skills: CodexRuntimeSkill[];
  errors: { path: string; message: string }[];
}

/** Ask the live Codex app-server which skills the agent actually loaded this session
 *  (`skills/list`). Reply shape: `{data: SkillsListEntry[]}`. Caller MUST gate on a live
 *  Codex session — there is no static fallback here, it speaks to the running process. */
export async function listCodexSkillsRuntime(
  runId: string,
): Promise<{ data: CodexRuntimeSkillsEntry[] }> {
  const result = await sendSessionControl(runId, "skills_list");
  return result as unknown as { data: CodexRuntimeSkillsEntry[] };
}

/** A Codex feature flag + its current enablement, from `experimentalFeature/list`. `stage` is the
 *  lifecycle (ExperimentalFeatureStage): "beta" | "underDevelopment" | "stable" | "deprecated" |
 *  "removed". displayName/description/announcement are null for non-beta features. */
export interface CodexFeature {
  name: string;
  stage: string;
  displayName: string | null;
  description: string | null;
  announcement: string | null;
  enabled: boolean;
  defaultEnabled: boolean;
}

/** List Codex feature flags for the live session's config (incl. project-local). Needs a live
 *  Codex session (app-server request). */
export async function listCodexFeatures(runId: string): Promise<{ data: CodexFeature[] }> {
  const result = await sendSessionControl(runId, "experimental_feature_list");
  return result as unknown as { data: CodexFeature[] };
}

/** Durably toggle one `[features].<name>` flag (nested config write; preserves the rest of the
 *  table). `enabled=null` clears the override back to Codex's default. Effective next session. */
export async function setCodexFeature(name: string, enabled: boolean | null): Promise<unknown> {
  dbg("api", "setCodexFeature", { name, enabled });
  return invoke("set_codex_feature", { name, enabled });
}

/** One model from Codex's authoritative `model/list` catalog (only the fields we render). */
export interface CodexModel {
  id: string;
  model: string;
  displayName: string;
  description: string;
  hidden: boolean;
  supportedReasoningEfforts: { reasoningEffort: string; description: string }[];
  defaultReasoningEffort: string;
  supportsPersonality: boolean;
  isDefault: boolean;
}

/** Authoritative model catalog from the live Codex CLI (`model/list`). Needs a live session;
 *  callers cache the result so the (sessionless) picker stays accurate across CLI versions. */
export async function listCodexModels(runId: string): Promise<{ data: CodexModel[] }> {
  const result = await sendSessionControl(runId, "model_list");
  return result as unknown as { data: CodexModel[] };
}

/** One model from Pi's `get_available_models` RPC catalog. */
export interface PiModel {
  provider: string;
  id: string;
  name?: string;
  reasoning?: boolean;
  contextWindow?: number;
  maxTokens?: number;
  thinkingLevelMap?: Record<string, string | null>;
}

export async function listPiModels(runId: string): Promise<{ data: PiModel[] }> {
  const result = await sendSessionControl(runId, "model_list");
  const payload = result as { data?: unknown } | null | undefined;
  const rawData = payload?.data;
  const data = Array.isArray(rawData)
    ? rawData
    : rawData && typeof rawData === "object" && "models" in rawData
      ? (rawData as { models?: unknown }).models
      : [];
  return { data: (Array.isArray(data) ? data : []) as PiModel[] };
}

export async function setMcpServers(runId: string, servers: Record<string, unknown>) {
  return sendSessionControl(runId, "mcp_set_servers", { servers });
}

export async function reconnectMcpServer(runId: string, serverName: string) {
  return sendSessionControl(runId, "mcp_reconnect", { serverName });
}

export async function toggleMcpServer(runId: string, serverName: string, enabled: boolean) {
  return sendSessionControl(runId, "mcp_toggle", { serverName, enabled });
}

export async function broadcastMcpToggle(serverName: string, enabled: boolean): Promise<number> {
  return invoke<number>("broadcast_mcp_toggle", { serverName, enabled });
}

export async function getDisabledMcpServers(): Promise<string[]> {
  return invoke<string[]>("get_disabled_mcp_servers");
}

export async function toggleMcpServerConfig(
  serverName: string,
  enabled: boolean,
  scope: string,
  cwd?: string,
): Promise<{ success: boolean; message: string }> {
  dbg("api", "toggleMcpServerConfig", { serverName, enabled, scope, cwd });
  return invoke("toggle_mcp_server_config", {
    name: serverName,
    enabled,
    scope,
    cwd: cwd ?? null,
  });
}

export async function rewindFiles(
  runId: string,
  opts: { userMessageId: string; dryRun?: boolean; files?: string[] },
) {
  return sendSessionControl(runId, "rewind_files", {
    user_message_id: opts.userMessageId,
    ...(opts.dryRun ? { dry_run: true } : {}),
    ...(opts.files ? { files: opts.files } : {}),
  });
}

export async function cancelControlRequest(runId: string, requestId: string) {
  dbg("api", "cancelControlRequest", { runId, requestId });
  return invoke("cancel_control_request", { runId, requestId });
}

// ── Codex Wave-3: thread lifecycle (compact / rewind / goal) ──

/** Codex `thread/compact/start`: clears history but keeps a summary in context. */
export async function compactSession(runId: string) {
  return sendSessionControl(runId, "compact");
}

/**
 * Codex `thread/rollback`: drops the last N turns from conversation HISTORY.
 * ⚠️ Does NOT revert file changes (unlike Claude's snapshot rewind).
 */
export async function rollbackTurns(runId: string, numTurns: number) {
  return sendSessionControl(runId, "rollback", { num_turns: numTurns });
}

/** Codex `thread/goal/set`: set or update the session objective + budget. */
export async function setGoal(
  runId: string,
  goal: { objective?: string; status?: GoalStatus; tokenBudget?: number },
) {
  return sendSessionControl(runId, "goal_set", {
    ...(goal.objective !== undefined ? { objective: goal.objective } : {}),
    ...(goal.status !== undefined ? { status: goal.status } : {}),
    ...(goal.tokenBudget !== undefined ? { token_budget: goal.tokenBudget } : {}),
  });
}

/**
 * Codex `thread/goal/get`: read the current goal.
 * Backend forwards the JSON-RPC result verbatim — `{ goal: ThreadGoal | null }`
 * — so we unwrap the `goal` key here. Returns null when no objective is set.
 */
export async function getGoal(runId: string): Promise<ThreadGoal | null> {
  const res = await sendSessionControl(runId, "goal_get");
  const goal = (res as { goal?: ThreadGoal | null }).goal;
  return goal ?? null;
}

/** Codex `thread/goal/clear`: remove the current objective. */
export async function clearGoal(runId: string) {
  return sendSessionControl(runId, "goal_clear");
}

export async function respondElicitation(
  runId: string,
  requestId: string,
  action: "accept" | "decline" | "cancel",
  content?: Record<string, unknown>,
): Promise<void> {
  dbg("api", "respondElicitation", { runId, requestId, action });
  return invoke("respond_elicitation", {
    runId,
    requestId,
    action,
    content: content ?? null,
  });
}

export async function getPiExtensionUiState(
  runId: string,
): Promise<import("./types").PiExtensionHostState> {
  return invoke("get_pi_extension_ui_state", { runId });
}

export async function invokePiSlashCommand(
  runId: string,
  command: string,
  args?: string,
): Promise<unknown> {
  dbg("api", "invokePiSlashCommand", { runId, command, args });
  return sendSessionControl(runId, "execute_slash_command", {
    command,
    args: args ?? "",
  });
}

/** Answer a native multiple-choice prompt over the app-server/ACP transport.
 *  `answers` maps each question id to the selected option label(s). */
export async function respondUserInput(
  runId: string,
  requestId: string,
  answers: Record<string, string[]>,
): Promise<void> {
  dbg("api", "respondUserInput", { runId, requestId });
  return invoke("respond_user_input", { runId, requestId, answers });
}

// ── Teams ──

export async function listTeams(): Promise<TeamSummary[]> {
  dbg("api", "listTeams");
  return invoke<TeamSummary[]>("list_teams");
}

export async function getTeamConfig(name: string): Promise<TeamConfig> {
  dbg("api", "getTeamConfig", name);
  return invoke<TeamConfig>("get_team_config", { name });
}

export async function listTeamTasks(teamName: string): Promise<TeamTask[]> {
  dbg("api", "listTeamTasks", teamName);
  return invoke<TeamTask[]>("list_team_tasks", { teamName });
}

export async function getTeamTask(teamName: string, taskId: string): Promise<TeamTask> {
  dbg("api", "getTeamTask", { teamName, taskId });
  return invoke<TeamTask>("get_team_task", { teamName, taskId });
}

export async function getTeamInbox(
  teamName: string,
  agentName: string,
): Promise<TeamInboxMessage[]> {
  dbg("api", "getTeamInbox", { teamName, agentName });
  return invoke<TeamInboxMessage[]>("get_team_inbox", { teamName, agentName });
}

export async function getAllTeamInboxes(name: string): Promise<TeamInboxMessage[]> {
  dbg("api", "getAllTeamInboxes", name);
  return invoke<TeamInboxMessage[]>("get_all_team_inboxes", { name });
}

export async function deleteTeam(name: string): Promise<void> {
  dbg("api", "deleteTeam", name);
  return invoke<void>("delete_team", { name });
}

// ── Clipboard ──

export interface ClipboardFileInfo {
  path: string;
  name: string;
  size: number;
  mime_type: string;
}

export interface ClipboardFileContent {
  content_base64: string;
  content_text: string | null;
}

export async function getClipboardFiles(): Promise<ClipboardFileInfo[]> {
  dbg("api", "getClipboardFiles");
  return invoke<ClipboardFileInfo[]>("get_clipboard_files");
}

export async function readClipboardFile(
  path: string,
  asText: boolean,
): Promise<ClipboardFileContent> {
  dbg("api", "readClipboardFile", { path, asText });
  return invoke<ClipboardFileContent>("read_clipboard_file", { path, asText });
}

/** Save file to temp directory, return filesystem path. For >20MB PDFs from drag-drop/file picker. */
export async function saveTempAttachment(name: string, contentBase64: string): Promise<string> {
  dbg("api", "saveTempAttachment", { name, len: contentBase64.length });
  return invoke<string>("save_temp_attachment", { name, contentBase64 });
}

// ── Plugins ──

export async function listMarketplaces(): Promise<MarketplaceInfo[]> {
  dbg("api", "listMarketplaces");
  return invoke<MarketplaceInfo[]>("list_marketplaces");
}

export async function listMarketplacePlugins(): Promise<MarketplacePlugin[]> {
  dbg("api", "listMarketplacePlugins");
  return invoke<MarketplacePlugin[]>("list_marketplace_plugins");
}

export async function listClaudeInstalledPlugins(): Promise<InstalledPlugin[]> {
  dbg("api", "listClaudeInstalledPlugins");
  return invoke<InstalledPlugin[]>("list_claude_installed_plugins");
}

export async function listClaudeAvailablePlugins(): Promise<MarketplacePlugin[]> {
  dbg("api", "listClaudeAvailablePlugins");
  return invoke<MarketplacePlugin[]>("list_claude_available_plugins");
}

export async function listClaudeMarketplaces(): Promise<MarketplaceInfo[]> {
  dbg("api", "listClaudeMarketplaces");
  return invoke<MarketplaceInfo[]>("list_claude_marketplaces");
}

export async function syncClaudeOfficialMarketplace(): Promise<PluginOperationResult> {
  dbg("api", "syncClaudeOfficialMarketplace");
  return invoke<PluginOperationResult>("sync_claude_official_marketplace");
}

export async function ensureClaudeOfficialMarketplace(): Promise<PluginOperationResult> {
  dbg("api", "ensureClaudeOfficialMarketplace");
  return invoke<PluginOperationResult>("ensure_claude_official_marketplace");
}

export async function listProjectCommands(cwd?: string): Promise<import("./types").CliCommand[]> {
  dbg("api", "listProjectCommands", { cwd });
  return invoke<import("./types").CliCommand[]>("list_project_commands", { cwd: cwd ?? null });
}

export async function listStandaloneSkills(cwd?: string): Promise<StandaloneSkill[]> {
  dbg("api", "listStandaloneSkills", { cwd });
  return invoke<StandaloneSkill[]>("list_standalone_skills", { cwd: cwd ?? null });
}

export async function getSkillContent(path: string, cwd?: string): Promise<string> {
  dbg("api", "getSkillContent", path);
  return invoke<string>("get_skill_content", { path, cwd: cwd ?? "" });
}

export async function createSkill(
  name: string,
  description: string,
  content: string,
  scope: string,
  cwd?: string,
): Promise<StandaloneSkill> {
  dbg("api", "createSkill", { name, scope, cwd });
  return invoke<StandaloneSkill>("create_skill", {
    name,
    description,
    content,
    scope,
    cwd: cwd ?? null,
  });
}

export async function createGrokSkill(
  name: string,
  description: string,
  content: string,
  scope: string,
  cwd?: string,
): Promise<StandaloneSkill> {
  dbg("api", "createGrokSkill", { name, scope, cwd });
  return invoke<StandaloneSkill>("create_grok_skill", {
    name,
    description,
    content,
    scope,
    cwd: cwd ?? null,
  });
}

export async function updateSkill(path: string, content: string, cwd?: string): Promise<void> {
  dbg("api", "updateSkill", { path, cwd });
  return invoke<void>("update_skill", { path, content, cwd: cwd ?? null });
}

export async function deleteSkill(path: string, cwd?: string): Promise<void> {
  dbg("api", "deleteSkill", { path, cwd });
  return invoke<void>("delete_skill", { path, cwd: cwd ?? null });
}

// ── Codex Skills ──

export async function listCodexSkills(cwd?: string): Promise<StandaloneSkill[]> {
  dbg("api", "listCodexSkills", { cwd });
  return invoke<StandaloneSkill[]>("list_codex_skills", { cwd: cwd ?? null });
}

export async function createCodexSkill(
  name: string,
  description: string,
  content: string,
  scope: string,
  cwd?: string,
): Promise<StandaloneSkill> {
  dbg("api", "createCodexSkill", { name, scope, cwd });
  return invoke<StandaloneSkill>("create_codex_skill", {
    name,
    description,
    content,
    scope,
    cwd: cwd ?? null,
  });
}

export async function deleteCodexSkill(path: string, cwd?: string): Promise<void> {
  dbg("api", "deleteCodexSkill", { path, cwd });
  return invoke<void>("delete_codex_skill", { path, cwd: cwd ?? null });
}

export async function toggleCodexSkill(
  skillPath: string,
  enabled: boolean,
  cwd?: string,
): Promise<void> {
  dbg("api", "toggleCodexSkill", { skillPath, enabled, cwd });
  return invoke<void>("toggle_codex_skill", { skillPath, enabled, cwd: cwd ?? null });
}

// ── Universal Skills ──

export async function listSkills(cwd?: string): Promise<StandaloneSkill[]> {
  dbg("api", "listSkills", { cwd });
  return invoke<StandaloneSkill[]>("list_skills", { cwd: cwd ?? null });
}

export async function createSharedSkill(
  name: string,
  description: string,
  content: string,
  scope: string,
  cwd?: string,
): Promise<StandaloneSkill> {
  dbg("api", "createSharedSkill", { name, scope, cwd });
  return invoke<StandaloneSkill>("create_shared_skill", {
    name,
    description,
    content,
    scope,
    cwd: cwd ?? null,
  });
}

export async function deleteSharedSkill(path: string): Promise<PluginOperationResult> {
  dbg("api", "deleteSharedSkill", { path });
  return invoke<PluginOperationResult>("delete_shared_skill", { path });
}

export async function toggleSkillBinding(
  skillId: string,
  enabled: boolean,
): Promise<PluginOperationResult> {
  dbg("api", "toggleSkillBinding", { skillId, enabled });
  return invoke<PluginOperationResult>("toggle_skill_binding", {
    skillId,
    enabled,
  });
}

export async function getSkillBindings(): Promise<
  Array<{ skillId: string; enabled: boolean; disabledBy?: string }>
> {
  dbg("api", "getSkillBindings", { scope: "global" });
  return invoke<Array<{ skillId: string; enabled: boolean; disabledBy?: string }>>(
    "get_skill_bindings",
  );
}

// ── WorkBuddy experts ──

export async function listAgentPlugins(): Promise<import("./types").AgentPluginSummary[]> {
  return invoke<import("./types").AgentPluginSummary[]>("list_agent_plugins");
}

export async function installAgentPlugin(
  source: string,
): Promise<import("./types").AgentPluginSummary> {
  return invoke<import("./types").AgentPluginSummary>("install_agent_plugin", { source });
}

export async function updateAgentPlugin(
  pluginId: string,
): Promise<import("./types").AgentPluginSummary> {
  return invoke<import("./types").AgentPluginSummary>("update_agent_plugin", { pluginId });
}

export async function uninstallAgentPlugin(pluginId: string): Promise<void> {
  return invoke<void>("uninstall_agent_plugin", { pluginId });
}

export async function setAgentPluginTrust(
  pluginId: string,
  trusted: boolean,
): Promise<import("./types").AgentPluginSummary> {
  return invoke<import("./types").AgentPluginSummary>("set_agent_plugin_trust", {
    pluginId,
    trusted,
  });
}

export async function getAgentPluginBindings(): Promise<import("./types").AgentPluginBinding[]> {
  return invoke<import("./types").AgentPluginBinding[]>("get_agent_plugin_bindings");
}

export async function setAgentPluginBinding(pluginId: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_agent_plugin_binding", { pluginId, enabled });
}

// Backward-compatible aliases
export const listPiSkills = listSkills;
export const createPiSkill = createSharedSkill;
export const deletePiSkill = deleteSharedSkill;
export const togglePiSkillBinding = toggleSkillBinding;
export const getPiSkillBindings = getSkillBindings;

/** Statically discover Pi extension commands without starting a session. */
export async function listPiCommands(cwd?: string): Promise<import("./types").CliCommand[]> {
  dbg("api", "listPiCommands", { cwd });
  return invoke<import("./types").CliCommand[]>("list_pi_commands", { cwd: cwd ?? null });
}

export interface McpCatalogServer {
  id: string;
  name: string;
  description?: string;
  transport: string;
  command?: string;
  args: string[];
  url?: string;
  envSchema?: Record<string, string>;
  headersSchema?: Record<string, string>;
}

export interface McpBinding {
  serverId: string;
  enabled: boolean;
  secretRef?: string;
  args?: string[];
  env?: Record<string, string>;
}

export interface ConnectorCatalogItem {
  id: string;
  name: string;
  description?: string;
  version: string;
  author?: string;
  homepage?: string;
  authKind: string;
  entrypoint?: string;
  skills: string[];
  mcpServer?: string;
  cli?: string;
}

export interface ConnectorBinding {
  connectorId: string;
  connectionId?: string;
  enabled: boolean;
  permissionProfile: string;
}

export async function listConnectorCatalog(): Promise<ConnectorCatalogItem[]> {
  return invoke<ConnectorCatalogItem[]>("list_connector_catalog");
}

export async function saveConnectorCatalogItem(item: ConnectorCatalogItem): Promise<void> {
  return invoke<void>("save_connector_catalog_item", { item });
}

export async function deleteConnectorCatalogItem(connectorId: string): Promise<void> {
  return invoke<void>("delete_connector_catalog_item", { connectorId });
}

export async function listMcpCatalog(): Promise<McpCatalogServer[]> {
  return invoke<McpCatalogServer[]>("list_mcp_catalog");
}

export async function saveMcpCatalogServer(server: McpCatalogServer): Promise<void> {
  return invoke<void>("save_mcp_catalog_server", { server });
}

export async function deleteMcpCatalogServer(serverId: string): Promise<void> {
  return invoke<void>("delete_mcp_catalog_server", { serverId });
}

export async function getMcpBindings(): Promise<McpBinding[]> {
  return invoke<McpBinding[]>("get_mcp_bindings");
}

export async function getWebAccessBinding(): Promise<boolean> {
  return invoke<boolean>("get_web_access_binding");
}

export async function setWebAccessBinding(enabled: boolean): Promise<void> {
  return invoke<void>("set_web_access_binding", { enabled });
}

export function getBrowserConfig(): Promise<WorkBrowserSummary> {
  return invoke<WorkBrowserSummary>("get_browser_config");
}

export function saveBrowserConfig(input: {
  provider?: string;
  enabled: boolean;
  maxResults?: number;
  apiKey?: string | null;
  endpointUrl?: string | null;
  allowedHosts?: string[] | null;
}): Promise<WorkBrowserSummary> {
  return invoke<WorkBrowserSummary>("save_browser_config", {
    provider: input.provider ?? "tavily",
    enabled: input.enabled,
    maxResults: input.maxResults ?? 5,
    apiKey: input.apiKey ?? null,
    endpointUrl: input.endpointUrl ?? null,
    allowedHosts: input.allowedHosts ?? null,
  });
}

export function testBrowser(): Promise<WorkBrowserHealth> {
  return invoke<WorkBrowserHealth>("test_browser");
}

export async function getBrowserUseBinding(): Promise<boolean> {
  return invoke<boolean>("get_browser_use_binding");
}

export async function setBrowserUseBinding(enabled: boolean): Promise<void> {
  return invoke<void>("set_browser_use_binding", { enabled });
}

export async function getDesktopUseStatus(): Promise<import("$lib/types/work").DesktopUseStatus> {
  return invoke<import("$lib/types/work").DesktopUseStatus>("get_desktop_use_status");
}

export async function refreshDesktopUseStatus(): Promise<
  import("$lib/types/work").DesktopUseStatus
> {
  return invoke<import("$lib/types/work").DesktopUseStatus>("refresh_desktop_use_status");
}

export async function getDesktopUseBinding(): Promise<boolean> {
  return invoke<boolean>("get_desktop_use_binding");
}

export async function setDesktopUseBinding(enabled: boolean): Promise<void> {
  return invoke<void>("set_desktop_use_binding", { enabled });
}

export async function requestDesktopUsePermissions(): Promise<
  import("$lib/types/work").DesktopUseStatus
> {
  return invoke<import("$lib/types/work").DesktopUseStatus>("request_desktop_use_permissions");
}

export async function openDesktopPermissionPane(
  kind: "accessibility" | "screen-recording",
): Promise<void> {
  return invoke<void>("open_desktop_permission_pane", { kind });
}

export async function prepareBrowserRuntime(): Promise<WorkBrowserSummary> {
  return invoke<WorkBrowserSummary>("prepare_browser_runtime");
}

export async function setMcpBinding(
  serverId: string,
  enabled: boolean,
  secretRef?: string | null,
  clearSecretRef?: boolean,
): Promise<void> {
  return invoke<void>("set_mcp_binding", {
    serverId,
    enabled,
    secretRef: secretRef ?? null,
    clearSecretRef: clearSecretRef ?? false,
  });
}

export async function getConnectorBindings(): Promise<ConnectorBinding[]> {
  return invoke<ConnectorBinding[]>("get_connector_bindings");
}

export async function setConnectorBinding(
  connectorId: string,
  enabled: boolean,
  connectionId?: string | null,
  clearConnectionId?: boolean,
  permissionProfile?: string,
): Promise<void> {
  return invoke<void>("set_connector_binding", {
    connectorId,
    enabled,
    connectionId: connectionId ?? null,
    clearConnectionId: clearConnectionId ?? false,
    permissionProfile: permissionProfile ?? null,
  });
}

export async function listHostSecretRefs(): Promise<string[]> {
  return invoke<string[]>("list_host_secret_refs");
}

export async function setHostSecret(
  secretRef: string,
  secretMap: Record<string, string>,
): Promise<void> {
  return invoke<void>("set_host_secret", { secretRef, secretMap });
}

export async function deleteHostSecret(secretRef: string): Promise<void> {
  return invoke<void>("delete_host_secret", { secretRef });
}

// ── AgentCabin Prompt Templates ──

export async function listPromptTemplates(): Promise<import("./types").PromptTemplate[]> {
  dbg("api", "listPromptTemplates");
  return invoke<import("./types").PromptTemplate[]>("list_prompt_templates");
}

export async function createPromptTemplate(
  name: string,
  description: string,
  content: string,
): Promise<import("./types").PromptTemplate> {
  dbg("api", "createPromptTemplate", { name });
  return invoke<import("./types").PromptTemplate>("create_prompt_template", {
    name,
    description: description.trim() || null,
    content,
  });
}

export async function updatePromptTemplate(
  id: string,
  name: string,
  description: string,
  content: string,
): Promise<PluginOperationResult> {
  dbg("api", "updatePromptTemplate", { id, name });
  return invoke<PluginOperationResult>("update_prompt_template", {
    id,
    name,
    description: description.trim() || null,
    content,
  });
}

export async function deletePromptTemplate(id: string): Promise<PluginOperationResult> {
  dbg("api", "deletePromptTemplate", { id });
  return invoke<PluginOperationResult>("delete_prompt_template", { id });
}

// ── Codex Plugins ──

export async function listCodexInstalledPlugins(): Promise<InstalledPlugin[]> {
  dbg("api", "listCodexInstalledPlugins");
  return invoke<InstalledPlugin[]>("list_codex_installed_plugins");
}

export async function listCodexAvailablePlugins(): Promise<import("./types").CodexPluginInfo[]> {
  dbg("api", "listCodexAvailablePlugins");
  return invoke<import("./types").CodexPluginInfo[]>("list_codex_available_plugins");
}

export async function listCodexMarketplaces(): Promise<import("./types").CodexMarketplace[]> {
  dbg("api", "listCodexMarketplaces");
  return invoke<import("./types").CodexMarketplace[]>("list_codex_marketplaces");
}

export async function addCodexMarketplace(source: string): Promise<PluginOperationResult> {
  dbg("api", "addCodexMarketplace", { source });
  return invoke<PluginOperationResult>("add_codex_marketplace", { source });
}

export async function removeCodexMarketplace(name: string): Promise<PluginOperationResult> {
  dbg("api", "removeCodexMarketplace", { name });
  return invoke<PluginOperationResult>("remove_codex_marketplace", { name });
}

export async function upgradeCodexMarketplace(name?: string): Promise<PluginOperationResult> {
  dbg("api", "upgradeCodexMarketplace", { name });
  return invoke<PluginOperationResult>("upgrade_codex_marketplace", { name: name ?? null });
}

export async function installCodexPlugin(
  pluginId: string,
  marketplace?: string,
): Promise<PluginOperationResult> {
  dbg("api", "installCodexPlugin", { pluginId, marketplace });
  return invoke<PluginOperationResult>("install_codex_plugin", {
    pluginId,
    marketplace: marketplace ?? null,
  });
}

export async function uninstallCodexPlugin(
  pluginId: string,
  marketplace?: string,
): Promise<PluginOperationResult> {
  dbg("api", "uninstallCodexPlugin", { pluginId, marketplace });
  return invoke<PluginOperationResult>("uninstall_codex_plugin", {
    pluginId,
    marketplace: marketplace ?? null,
  });
}

export async function listPiInstalledPlugins(cwd?: string): Promise<InstalledPlugin[]> {
  dbg("api", "listPiInstalledPlugins", { cwd });
  return invoke<InstalledPlugin[]>("list_pi_installed_plugins", { cwd: cwd ?? null });
}

export async function getPiProfileInfo(mode: "code" | "work"): Promise<PiProfileInfo> {
  dbg("api", "getPiProfileInfo", { mode });
  return invoke<PiProfileInfo>("get_pi_profile_info", { mode });
}

export async function listPiProfileExtensions(
  mode: "code",
  cwd?: string,
): Promise<InstalledPlugin[]> {
  dbg("api", "listPiProfileExtensions", { mode, cwd });
  return invoke<InstalledPlugin[]>("list_pi_profile_extensions", {
    mode,
    cwd: cwd ?? null,
  });
}

/**
 * List Pi extensions from AgentCabin's shared package catalog with the
 * binding state for one runtime profile. Package files are shared; enabled
 * state is profile-local.
 */
export async function listPiSharedExtensions(mode: "code" | "work"): Promise<InstalledPlugin[]> {
  dbg("api", "listPiSharedExtensions", { mode });
  return invoke<InstalledPlugin[]>("list_pi_shared_extensions", { mode });
}

export async function installPiSharedExtension(source: string): Promise<PluginOperationResult> {
  dbg("api", "installPiSharedExtension", { source });
  return invoke<PluginOperationResult>("install_pi_shared_extension", { source });
}

export async function uninstallPiSharedExtension(
  extensionId: string,
): Promise<PluginOperationResult> {
  dbg("api", "uninstallPiSharedExtension", { extensionId });
  return invoke<PluginOperationResult>("uninstall_pi_shared_extension", { extensionId });
}

export async function updatePiSharedExtension(extensionId: string): Promise<PluginOperationResult> {
  dbg("api", "updatePiSharedExtension", { extensionId });
  return invoke<PluginOperationResult>("update_pi_shared_extension", { extensionId });
}

export async function togglePiSharedExtension(
  mode: "code" | "work",
  extensionId: string,
  enabled: boolean,
): Promise<void> {
  dbg("api", "togglePiSharedExtension", { mode, extensionId, enabled });
  return invoke<void>("toggle_pi_shared_extension", { mode, extensionId, enabled });
}

export async function installPiExtension(
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("install_pi_extension", {
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function uninstallPiExtension(
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("uninstall_pi_extension", {
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function updatePiExtension(
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("update_pi_extension", {
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function togglePiExtension(
  source: string,
  scope: "user" | "project",
  enabled: boolean,
  cwd?: string,
): Promise<void> {
  return invoke<void>("toggle_pi_extension", {
    source,
    scope,
    enabled,
    cwd: cwd ?? null,
  });
}

export async function installPiProfileExtension(
  mode: "code",
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("install_pi_profile_extension", {
    mode,
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function uninstallPiProfileExtension(
  mode: "code",
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("uninstall_pi_profile_extension", {
    mode,
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function updatePiProfileExtension(
  mode: "code",
  source: string,
  scope: "user" | "project",
  cwd?: string,
): Promise<PluginOperationResult> {
  return invoke<PluginOperationResult>("update_pi_profile_extension", {
    mode,
    source,
    scope,
    cwd: cwd ?? null,
  });
}

export async function togglePiProfileExtension(
  mode: "code",
  source: string,
  scope: "user" | "project",
  enabled: boolean,
  cwd?: string,
): Promise<void> {
  return invoke<void>("toggle_pi_profile_extension", {
    mode,
    source,
    scope,
    enabled,
    cwd: cwd ?? null,
  });
}

export async function toggleCodexPlugin(pluginId: string, enabled: boolean): Promise<void> {
  dbg("api", "toggleCodexPlugin", { pluginId, enabled });
  return invoke<void>("toggle_codex_plugin", { pluginId, enabled });
}

export async function listInstalledPlugins(): Promise<InstalledPlugin[]> {
  dbg("api", "listInstalledPlugins");
  return invoke<InstalledPlugin[]>("list_installed_plugins");
}

export async function installPlugin(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "installPlugin", { name, scope, cwd });
  return invoke<PluginOperationResult>("install_plugin", { name, scope, cwd });
}

export async function uninstallPlugin(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "uninstallPlugin", { name, scope, cwd });
  return invoke<PluginOperationResult>("uninstall_plugin", { name, scope, cwd });
}

export async function enablePlugin(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "enablePlugin", { name, scope, cwd });
  return invoke<PluginOperationResult>("enable_plugin", { name, scope, cwd });
}

export async function disablePlugin(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "disablePlugin", { name, scope, cwd });
  return invoke<PluginOperationResult>("disable_plugin", { name, scope, cwd });
}

export async function updatePlugin(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "updatePlugin", { name, scope, cwd });
  return invoke<PluginOperationResult>("update_plugin", { name, scope, cwd });
}

export async function addMarketplace(source: string): Promise<PluginOperationResult> {
  dbg("api", "addMarketplace", { source });
  return invoke<PluginOperationResult>("add_marketplace", { source });
}

export async function removeMarketplace(name: string): Promise<PluginOperationResult> {
  dbg("api", "removeMarketplace", { name });
  return invoke<PluginOperationResult>("remove_marketplace", { name });
}

export async function updateMarketplace(name?: string): Promise<PluginOperationResult> {
  dbg("api", "updateMarketplace", { name });
  return invoke<PluginOperationResult>("update_marketplace", { name: name ?? null });
}

// ── Community Skills ──

export async function checkCommunityHealth(): Promise<import("./types").ProviderHealth> {
  dbg("api", "checkCommunityHealth");
  return invoke<import("./types").ProviderHealth>("check_community_health");
}

export async function searchCommunitySkills(
  query: string,
  limit?: number,
): Promise<import("./types").CommunitySkillResult[]> {
  dbg("api", "searchCommunitySkills", { query, limit });
  return invoke<import("./types").CommunitySkillResult[]>("search_community_skills", {
    query,
    limit: limit ?? null,
  });
}

export async function getCommunitySkillDetail(
  source: string,
  skillId: string,
): Promise<import("./types").CommunitySkillDetail> {
  dbg("api", "getCommunitySkillDetail", { source, skillId });
  return invoke<import("./types").CommunitySkillDetail>("get_community_skill_detail", {
    source,
    skillId,
  });
}

export async function installCommunitySkill(
  source: string,
  skillId: string,
  scope: string,
  targetAgent?: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "installCommunitySkill", { source, skillId, scope, targetAgent });
  return invoke<PluginOperationResult>("install_community_skill", {
    source,
    skillId,
    scope,
    targetAgent: targetAgent ?? null,
    cwd: cwd ?? null,
  });
}

export async function importSkillZip(
  zipPath: string,
  slug: string,
  scope: string,
  targetAgent?: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "importSkillZip", { zipPath, slug, scope, targetAgent });
  return invoke<PluginOperationResult>("import_skill_zip", {
    zipPath,
    slug,
    scope,
    targetAgent: targetAgent ?? null,
    cwd: cwd ?? null,
  });
}

export async function fetchPiPackages(
  query?: string,
  sort?: string,
  packageType?: string,
): Promise<import("./types/work").PiPackageItem[]> {
  dbg("api", "fetchPiPackages", { query, sort, packageType });
  return invoke<import("./types/work").PiPackageItem[]>("fetch_pi_packages", {
    query: query || null,
    sort: sort || null,
    packageType: packageType || null,
  });
}

// ── MCP Registry ──

export async function listConfiguredMcpServers(cwd?: string): Promise<ConfiguredMcpServer[]> {
  dbg("api", "listConfiguredMcpServers", { cwd });
  return invoke<ConfiguredMcpServer[]>("list_configured_mcp_servers", { cwd: cwd ?? null });
}

export async function addMcpServer(
  name: string,
  transport: string,
  scope: string,
  cwd?: string,
  configJson?: string,
  url?: string,
  envVars?: Record<string, string>,
  headers?: Record<string, string>,
): Promise<PluginOperationResult> {
  dbg("api", "addMcpServer", { name, transport, scope });
  return invoke<PluginOperationResult>("add_mcp_server", {
    name,
    transport,
    scope,
    cwd: cwd ?? null,
    configJson: configJson ?? null,
    url: url ?? null,
    envVars: envVars ?? null,
    headers: headers ?? null,
  });
}

export async function removeMcpServer(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "removeMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("remove_mcp_server", {
    name,
    scope,
    cwd: cwd ?? null,
  });
}

// ── Codex MCP ──

export async function listCodexMcpServers(cwd?: string): Promise<ConfiguredMcpServer[]> {
  dbg("api", "listCodexMcpServers", { cwd });
  return invoke<ConfiguredMcpServer[]>("list_codex_mcp_servers", { cwd: cwd ?? null });
}

export async function addCodexMcpServer(
  name: string,
  config: Record<string, unknown>,
): Promise<PluginOperationResult> {
  dbg("api", "addCodexMcpServer", { name });
  return invoke<PluginOperationResult>("add_codex_mcp_server", { name, config });
}

export async function removeCodexMcpServer(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "removeCodexMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("remove_codex_mcp_server", {
    name,
    scope,
    cwd: cwd ?? null,
  });
}

// ── Grok MCP ──

export async function listGrokMcpServers(cwd?: string): Promise<ConfiguredMcpServer[]> {
  dbg("api", "listGrokMcpServers", { cwd });
  return invoke<ConfiguredMcpServer[]>("list_grok_mcp_servers", { cwd: cwd ?? null });
}

export async function addGrokMcpServer(
  name: string,
  config: Record<string, unknown>,
  scope = "user",
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "addGrokMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("add_grok_mcp_server", {
    name,
    config,
    scope,
    cwd: cwd ?? null,
  });
}

export async function removeGrokMcpServer(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "removeGrokMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("remove_grok_mcp_server", {
    name,
    scope,
    cwd: cwd ?? null,
  });
}

// ── Pi MCP ──

export async function listPiMcpServers(cwd?: string): Promise<ConfiguredMcpServer[]> {
  dbg("api", "listPiMcpServers", { cwd });
  return invoke<ConfiguredMcpServer[]>("list_pi_mcp_servers", { cwd: cwd ?? null });
}

export async function addPiMcpServer(
  name: string,
  config: Record<string, unknown>,
  scope = "user",
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "addPiMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("add_pi_mcp_server", {
    name,
    config,
    scope,
    cwd: cwd ?? null,
  });
}

export async function removePiMcpServer(
  name: string,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "removePiMcpServer", { name, scope, cwd });
  return invoke<PluginOperationResult>("remove_pi_mcp_server", {
    name,
    scope,
    cwd: cwd ?? null,
  });
}

export async function togglePiMcpServer(
  name: string,
  enabled: boolean,
  scope: string,
  cwd?: string,
): Promise<PluginOperationResult> {
  dbg("api", "togglePiMcpServer", { name, enabled, scope, cwd });
  return invoke<PluginOperationResult>("toggle_pi_mcp_server", {
    name,
    enabled,
    scope,
    cwd: cwd ?? null,
  });
}

export async function checkMcpRegistryHealth(): Promise<ProviderHealth> {
  dbg("api", "checkMcpRegistryHealth");
  return invoke<ProviderHealth>("check_mcp_registry_health");
}

export async function searchMcpRegistry(
  query: string,
  limit?: number,
  cursor?: string,
): Promise<McpRegistrySearchResult> {
  dbg("api", "searchMcpRegistry", { query, limit, cursor });
  return invoke<McpRegistrySearchResult>("search_mcp_registry", {
    query,
    limit: limit ?? null,
    cursor: cursor ?? null,
  });
}

// ── CLI Permissions ──

export interface CliPermissions {
  user: { allow: string[]; deny: string[] };
  project: { allow: string[]; deny: string[] };
  projectError?: string | null;
}

export async function getCliPermissions(cwd?: string): Promise<CliPermissions> {
  dbg("api", "getCliPermissions", { cwd });
  return invoke<CliPermissions>("get_cli_permissions", { cwd: cwd ?? null });
}

export async function updateCliPermissions(
  scope: "user" | "project",
  category: "allow" | "deny",
  rules: string[],
  cwd?: string,
): Promise<void> {
  dbg("api", "updateCliPermissions", { scope, category, count: rules.length });
  return invoke<void>("update_cli_permissions", {
    scope,
    category,
    rules,
    cwd: cwd ?? null,
  });
}

// ── CLI Config ──

export async function getCliConfig(): Promise<Record<string, unknown>> {
  dbg("api", "getCliConfig");
  return invoke<Record<string, unknown>>("get_cli_config");
}

export async function getProjectCliConfig(cwd: string): Promise<Record<string, unknown>> {
  dbg("api", "getProjectCliConfig", { cwd });
  return invoke<Record<string, unknown>>("get_project_cli_config", { cwd });
}

export async function updateCliConfig(
  patch: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  dbg("api", "updateCliConfig", { patch: redactSensitive(patch) });
  return invoke<Record<string, unknown>>("update_cli_config", { patch });
}

// ── Codex Hooks ──

export async function getCodexHooks(): Promise<{
  hooks: Record<string, unknown>;
  warning?: string;
}> {
  dbg("api", "getCodexHooks");
  return invoke("get_codex_hooks");
}

export async function updateCodexHooks(
  hooks: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  dbg("api", "updateCodexHooks");
  return invoke("update_codex_hooks", { hooks });
}

// ── Codex Config ──

export async function getCodexConfig(): Promise<import("./types").CodexConfigResult> {
  dbg("api", "getCodexConfig");
  return invoke<import("./types").CodexConfigResult>("get_codex_config");
}

export async function getProjectCodexConfig(cwd: string): Promise<Record<string, unknown>> {
  dbg("api", "getProjectCodexConfig", { cwd });
  return invoke<Record<string, unknown>>("get_project_codex_config", { cwd });
}

export async function updateCodexConfig(
  patch: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  dbg("api", "updateCodexConfig", { patch });
  return invoke<Record<string, unknown>>("update_codex_config", { patch });
}

// ── App Updates ──

export async function checkForUpdates(): Promise<import("./types").UpdateInfo> {
  dbg("api", "checkForUpdates");
  return invoke<import("./types").UpdateInfo>("check_for_updates");
}

// ── Changelog ──

export async function getChangelog(): Promise<ChangelogEntry[]> {
  dbg("api", "getChangelog");
  return invoke<ChangelogEntry[]>("get_changelog");
}

// ── Onboarding ──

export async function checkAuthStatus(): Promise<import("./types").AuthCheckResult> {
  dbg("api", "checkAuthStatus");
  return invoke<import("./types").AuthCheckResult>("check_auth_status");
}

export async function detectInstallMethods(
  agent: "claude" | "codex" | "pi" | "grok" = "claude",
): Promise<import("./types").InstallMethod[]> {
  dbg("api", "detectInstallMethods", { agent });
  return invoke<import("./types").InstallMethod[]>("detect_install_methods", { agent });
}

export async function runClaudeLogin(): Promise<boolean> {
  dbg("api", "runClaudeLogin");
  return invoke<boolean>("run_claude_login");
}

export async function runCodexLogin(): Promise<boolean> {
  dbg("api", "runCodexLogin");
  return invoke<boolean>("run_codex_login");
}

export async function runCodexSubscriptionLogin(): Promise<boolean> {
  dbg("api", "runCodexSubscriptionLogin");
  return invoke<boolean>("run_codex_subscription_login");
}

export async function reopenCodexSubscriptionLogin(): Promise<boolean> {
  dbg("api", "reopenCodexSubscriptionLogin");
  return invoke<boolean>("reopen_codex_subscription_login");
}

export async function runCodexLogout(): Promise<boolean> {
  dbg("api", "runCodexLogout");
  return invoke<boolean>("run_codex_logout");
}

export async function runCodexSubscriptionLogout(): Promise<boolean> {
  dbg("api", "runCodexSubscriptionLogout");
  return invoke<boolean>("run_codex_subscription_logout");
}

export async function checkPiAuth(): Promise<PiAuthResult> {
  dbg("api", "checkPiAuth");
  return invoke<PiAuthResult>("check_pi_auth");
}

export async function runPiLogout(): Promise<boolean> {
  dbg("api", "runPiLogout");
  return invoke<boolean>("run_pi_logout");
}

export async function getAuthOverview(): Promise<import("./types").AuthOverview> {
  dbg("api", "getAuthOverview");
  return invoke<import("./types").AuthOverview>("get_auth_overview");
}

export async function setCliApiKey(key: string): Promise<void> {
  dbg("api", "setCliApiKey");
  return invoke<void>("set_cli_api_key", { key });
}

export async function removeCliApiKey(): Promise<void> {
  dbg("api", "removeCliApiKey");
  return invoke<void>("remove_cli_api_key");
}

// ── Screenshot ──

export async function captureScreenshot(): Promise<void> {
  dbg("api", "captureScreenshot");
  return invoke<void>("capture_screenshot");
}

export async function updateScreenshotHotkey(hotkey: string | null): Promise<void> {
  dbg("api", "updateScreenshotHotkey", { hotkey });
  return invoke<void>("update_screenshot_hotkey", { hotkey });
}

// ── Web Server ──

export async function getWebServerToken(): Promise<string | null> {
  dbg("api", "getWebServerToken");
  return invoke<string | null>("get_web_server_token");
}

export async function getWebServerStatus(): Promise<{
  enabled: boolean;
  running: boolean;
  port: number;
  bind: string;
  warning?: string;
}> {
  dbg("api", "getWebServerStatus");
  return invoke<{
    enabled: boolean;
    running: boolean;
    port: number;
    bind: string;
    warning?: string;
  }>("get_web_server_status");
}

export async function regenerateWebServerToken(): Promise<string> {
  dbg("api", "regenerateWebServerToken");
  return invoke<string>("regenerate_web_server_token");
}

export interface WebServerConfig {
  enabled: boolean;
  port: number;
  bind: string;
  allowed_origins: string[] | null;
  tunnel_url: string | null;
}

export interface RestartResult {
  started: boolean;
  config_saved: boolean;
}

export async function restartWebServer(config: WebServerConfig): Promise<RestartResult> {
  dbg("api", "restartWebServer", { enabled: config.enabled, port: config.port });
  return invoke<RestartResult>("restart_web_server", { config });
}

export async function getLocalIp(preferV6: boolean): Promise<string | null> {
  dbg("api", "getLocalIp", { preferV6 });
  return invoke<string | null>("get_local_ip", { preferV6 });
}

// ── Agents ──

export async function listAgents(cwd?: string): Promise<AgentDefinitionSummary[]> {
  dbg("api", "listAgents", { cwd });
  return invoke<AgentDefinitionSummary[]>("list_agents", { cwd: cwd ?? null });
}

export async function listCodexAgents(): Promise<AgentDefinitionSummary[]> {
  dbg("api", "listCodexAgents");
  return invoke<AgentDefinitionSummary[]>("list_codex_agents");
}

export async function readAgentFile(
  scope: "user" | "project",
  fileName: string,
  cwd?: string,
): Promise<string> {
  dbg("api", "readAgentFile", { scope, fileName });
  return invoke<string>("read_agent_file", {
    scope,
    fileName,
    cwd: cwd ?? null,
  });
}

export async function createAgentFile(
  scope: "user" | "project",
  fileName: string,
  content: string,
  cwd?: string,
): Promise<void> {
  dbg("api", "createAgentFile", { scope, fileName });
  return invoke<void>("create_agent_file", {
    scope,
    fileName,
    content,
    cwd: cwd ?? null,
  });
}

export async function updateAgentFile(
  scope: "user" | "project",
  fileName: string,
  content: string,
  cwd?: string,
): Promise<void> {
  dbg("api", "updateAgentFile", { scope, fileName });
  return invoke<void>("update_agent_file", {
    scope,
    fileName,
    content,
    cwd: cwd ?? null,
  });
}

export async function deleteAgentFile(
  scope: "user" | "project",
  fileName: string,
  cwd?: string,
): Promise<void> {
  dbg("api", "deleteAgentFile", { scope, fileName });
  return invoke<void>("delete_agent_file", {
    scope,
    fileName,
    cwd: cwd ?? null,
  });
}

// ── Preview ──

export async function openPreviewWindow(url: string, instanceId: string): Promise<void> {
  dbg("api", "openPreviewWindow", { url, instanceId });
  return invoke("open_preview_window", { url, instanceId });
}

export async function closePreviewWindow(): Promise<void> {
  dbg("api", "closePreviewWindow");
  return invoke("close_preview_window");
}

// ── Ralph Loop ──

export async function startRalphLoop(
  runId: string,
  prompt: string,
  maxIterations: number,
  completionPromise: string | null,
): Promise<void> {
  dbg("api", "startRalphLoop", { runId, maxIterations, completionPromise });
  return invoke<void>("start_ralph_loop", {
    runId,
    prompt,
    maxIterations,
    completionPromise,
  });
}

export async function cancelRalphLoop(
  runId: string,
): Promise<{ iteration: number; immediate: boolean }> {
  dbg("api", "cancelRalphLoop", { runId });
  return invoke<{ iteration: number; immediate: boolean }>("cancel_ralph_loop", { runId });
}
