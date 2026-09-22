import { getTransport } from "$lib/transport";
import type { TaskRun } from "$lib/types";
import type { Attachment } from "$lib/types";
import type {
  InboxItem,
  InboxItemPayload,
  InboxItemStatus,
  InboxItemType,
  TaskStandingRule,
  ToolRiskClass,
  AppCatalogItem,
  AppConnection,
  AuthorizeResponse,
  ConnectorPackageManifest,
  ConnectorPackageSummary,
  ConnectorCatalogItem,
  ComposioProviderConfig,
  ConnectionStatus,
  WorkArtifactSummary,
  WorkRunRecovery,
  WorkRecoveryAction,
  WorkAccessRoot,
  WorkCapabilityMatch,
  WorkConnectorHealth,
  WorkConnectorSummary,
  WorkBrowserHealth,
  WorkBrowserSummary,
  WorkFileSummary,
  WorkExecutionMode,
  WorkPolicy,
  WorkProfile,
  WorkPreset,
  WorkResourceSummary,
  PiPackageItem,
  WorkRulesInfo,
  WorkRun,
  WorkRunProgressView,
  WorkProjection,
  WorkRunReceipt,
  WorkRunStatus,
  WorkRunTrigger,
  WorkScheduleConfig,
  WorkSubagentRecord,
  WorkTask,
  GoalSpec,
  WorkAutomationStats,
  WorkTaskRunSummary,
  LibraryCategory,
  LibraryItem,
  LibraryItemSummary,
  BrowserSession,
  BrowserTraceEntry,
  WorkArtifactRequirement,
  WorkArtifactStorageMode,
  WorkTaskState,
  WorkWorkspaceSummary,
  WorkContextPlan,
  CapabilityCenterItem,
  CapabilityCenterProjection,
  RunEffectiveCapabilitiesView,
  WorkArtifactAcceptance,
} from "$lib/types/work";

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(command, args);
}

export function getWorkProfile(): Promise<WorkProfile> {
  return invoke<WorkProfile>("work_get_profile");
}

export function getWorkRulesInfo(): Promise<WorkRulesInfo> {
  return invoke<WorkRulesInfo>("work_get_rules_info");
}

export function getWorkContextPlan(runId: string): Promise<WorkContextPlan | null> {
  return invoke<WorkContextPlan | null>("work_get_context_plan", { runId });
}

export function saveWorkRules(content: string): Promise<void> {
  return invoke<void>("work_save_rules", { content });
}

export function listWorkspaces(): Promise<WorkWorkspaceSummary[]> {
  return invoke<WorkWorkspaceSummary[]>("work_list_workspaces");
}

export function listArchivedWorkspaces(): Promise<WorkWorkspaceSummary[]> {
  return invoke<WorkWorkspaceSummary[]>("work_list_archived_workspaces");
}

export function createWorkspace(name: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_create_workspace", { name });
}

export function createWorkspaceFromFolder(
  folderPath: string,
  name?: string,
): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_create_workspace_from_folder", {
    folderPath,
    name,
  });
}

export function relinkWorkspaceFolder(
  id: string,
  folderPath: string,
): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_relink_workspace_folder", {
    id,
    folderPath,
  });
}

export function getWorkspace(id: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_get_workspace", { id });
}

export function renameWorkspace(id: string, name: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_rename_workspace", { id, name });
}

/** Set the workspace-level default policy. New tasks inherit this; existing tasks keep their own. */
export function setWorkspaceDefaultPolicy(
  workspaceId: string,
  policy: WorkPolicy,
): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_set_workspace_default_policy", {
    workspaceId,
    policy,
  });
}

export function setWorkspaceModelPreferences(
  workspaceId: string,
  model?: string,
  effort?: string,
): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_set_workspace_model_preferences", {
    workspaceId,
    model: model || null,
    effort: effort || null,
  });
}

export function archiveWorkspace(id: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_archive_workspace", { id });
}

export function restoreWorkspace(id: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_restore_workspace", { id });
}

/** Permanently delete a Workspace and its data directory (tasks, files, artifacts). Irreversible. */
export function deleteWorkspace(id: string): Promise<void> {
  return invoke<void>("work_delete_workspace", { id });
}

export function listWorkAccessRoots(workspaceId: string): Promise<WorkAccessRoot[]> {
  return invoke<WorkAccessRoot[]>("work_list_access_roots", { workspaceId });
}

export function addWorkAccessRoot(
  workspaceId: string,
  path: string,
  writable = false,
): Promise<WorkAccessRoot[]> {
  return invoke<WorkAccessRoot[]>("work_add_access_root", { workspaceId, path, writable });
}

export function setWorkAccessRootWritable(
  workspaceId: string,
  path: string,
  writable: boolean,
): Promise<WorkAccessRoot[]> {
  return invoke<WorkAccessRoot[]>("work_set_access_root_writable", {
    workspaceId,
    path,
    writable,
  });
}

export function removeWorkAccessRoot(workspaceId: string, path: string): Promise<WorkAccessRoot[]> {
  return invoke<WorkAccessRoot[]>("work_remove_access_root", { workspaceId, path });
}

export function listWorkResources(runtime?: string): Promise<WorkResourceSummary[]> {
  return invoke<WorkResourceSummary[]>("work_list_resources", runtime ? { runtime } : undefined);
}

export function setWorkResourceEnabled(id: string, enabled: boolean): Promise<WorkResourceSummary> {
  return invoke<WorkResourceSummary>("work_set_resource_enabled", { id, enabled });
}

export function installWorkCommunitySkill(
  source: string,
  skillId: string,
): Promise<WorkResourceSummary> {
  return invoke<WorkResourceSummary>("work_install_community_skill", { source, skillId });
}

export function importWorkSkillZip(zipPath: string, slug: string): Promise<WorkResourceSummary> {
  return invoke<WorkResourceSummary>("work_import_skill_zip", { zipPath, slug });
}

export function discoverWorkCapabilities(
  query: string,
  limit = 20,
): Promise<WorkCapabilityMatch[]> {
  return invoke<WorkCapabilityMatch[]>("work_discover_capabilities", { query, limit });
}

export function listWorkConnectors(): Promise<WorkConnectorSummary[]> {
  return invoke<WorkConnectorSummary[]>("work_list_connectors");
}

export function saveWorkConnector(input: {
  name: string;
  transport: string;
  command?: string | null;
  args: string[];
  url?: string | null;
  envVars?: Record<string, string>;
  headers?: Record<string, string>;
}): Promise<WorkConnectorSummary> {
  return invoke<WorkConnectorSummary>("work_save_connector", {
    name: input.name,
    transport: input.transport,
    command: input.command ?? null,
    args: input.args,
    url: input.url ?? null,
    envVars: input.envVars ?? {},
    headers: input.headers ?? {},
  });
}

export function toggleWorkConnector(name: string, enabled: boolean): Promise<WorkConnectorSummary> {
  return invoke<WorkConnectorSummary>("work_toggle_connector", { name, enabled });
}

export function removeWorkConnector(name: string): Promise<void> {
  return invoke<void>("work_remove_connector", { name });
}

export function listWorkConnectorPackages(): Promise<ConnectorPackageSummary[]> {
  return invoke<ConnectorPackageSummary[]>("work_list_connector_packages");
}

export function listWorkConnectorCatalog(): Promise<ConnectorCatalogItem[]> {
  return invoke<ConnectorCatalogItem[]>("work_list_connector_catalog");
}

export function validateWorkConnectorPackage(source: string): Promise<ConnectorPackageManifest> {
  return invoke<ConnectorPackageManifest>("work_validate_connector_package", { source });
}

export function installWorkConnectorPackage(source: string): Promise<ConnectorPackageSummary> {
  return invoke<ConnectorPackageSummary>("work_install_connector_package", { source });
}

export function trustWorkConnectorPackage(
  packageId: string,
  trusted: boolean,
): Promise<ConnectorPackageSummary> {
  return invoke<ConnectorPackageSummary>("work_trust_connector_package", {
    packageId,
    trusted,
  });
}

export function enableWorkConnectorPackage(
  packageId: string,
  enabled: boolean,
): Promise<ConnectorPackageSummary> {
  return invoke<ConnectorPackageSummary>("work_enable_connector_package", {
    packageId,
    enabled,
  });
}

export function uninstallWorkConnectorPackage(packageId: string): Promise<void> {
  return invoke<void>("work_uninstall_connector_package", { packageId });
}

export function installWorkMcpAdapter(): Promise<string> {
  return invoke<string>("work_install_mcp_adapter");
}

export function isWorkMcpAdapterInstalled(): Promise<boolean> {
  return invoke<boolean>("work_is_mcp_adapter_installed");
}

export function listWorkAppsCatalog(): Promise<AppCatalogItem[]> {
  return invoke<AppCatalogItem[]>("work_apps_catalog");
}

export function listWorkAppsConnections(): Promise<AppConnection[]> {
  return invoke<AppConnection[]>("work_apps_connections");
}

export function getWorkAppsProviderConfig(): Promise<ComposioProviderConfig> {
  return invoke<ComposioProviderConfig>("work_apps_get_provider_config");
}

export function saveWorkAppsProviderConfig(apiKey: string): Promise<void> {
  return invoke<void>("work_apps_save_provider_config", { apiKey });
}

export interface LarkCliInfo {
  installed: boolean;
  command: string;
  version: string | null;
  auth_url: string;
  privateInstall: boolean;
  configScope: string;
}

export function checkLarkCli(): Promise<LarkCliInfo> {
  return invoke<LarkCliInfo>("work_lark_cli_check");
}

export function installLarkCli(): Promise<string> {
  return invoke<string>("work_lark_cli_install");
}

export function mountLarkSkills(): Promise<string[]> {
  return invoke<string[]>("work_lark_skills_mount");
}

export interface LarkAuthStatus {
  taskId: string | null;
  status: "idle" | "pending" | "authenticated" | "error";
  stage: string;
  message: string;
  url: string | null;
  identity: {
    identity: string;
    displayName: string | null;
    email: string | null;
    openId: string | null;
  } | null;
  connection: AppConnection | null;
}

export function startLarkAuth(alias?: string, email?: string): Promise<LarkAuthStatus> {
  return invoke<LarkAuthStatus>("work_lark_auth_start", {
    alias: alias ?? null,
    email: email ?? null,
  });
}

export function getLarkAuthStatus(taskId?: string): Promise<LarkAuthStatus> {
  return invoke<LarkAuthStatus>("work_lark_auth_status", {
    taskId: taskId ?? null,
  });
}

export function connectWorkAppNative(
  appId: string,
  token: string,
  alias?: string,
  email?: string,
): Promise<AppConnection> {
  return invoke<AppConnection>("work_app_connect_native", {
    appId,
    token,
    alias: alias ?? null,
    email: email ?? null,
  });
}

export function authorizeWorkApp(
  appId: string,
  redirectUrl?: string,
  userId?: string,
): Promise<AuthorizeResponse> {
  return invoke<AuthorizeResponse>("work_app_authorize", {
    appId,
    redirectUrl: redirectUrl ?? null,
    userId: userId ?? null,
  });
}

export function startWorkConnectorAuth(
  packageId: string,
  accountId?: string,
): Promise<AuthorizeResponse> {
  return invoke<AuthorizeResponse>("work_start_connector_auth", {
    packageId,
    accountId: accountId ?? null,
  });
}

export function configureWorkConnectorToken(
  packageId: string,
  values: Record<string, string>,
): Promise<ConnectorPackageSummary> {
  return invoke<ConnectorPackageSummary>("work_configure_connector_token", {
    packageId,
    values,
  });
}

export function getWorkAppStatus(appId: string, connectionId?: string): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("work_app_status", {
    appId,
    connectionId: connectionId ?? null,
  });
}

export function disconnectWorkApp(appId: string, accountId?: string): Promise<void> {
  return invoke<void>("work_app_disconnect", {
    appId,
    accountId: accountId ?? null,
  });
}

export function setWorkAppDefaultAccount(
  workspaceId: string,
  appId: string,
  accountId: string,
): Promise<void> {
  return invoke<void>("work_app_set_default_account", {
    workspaceId,
    appId,
    accountId,
  });
}

export function fetchPiPackages(
  query?: string,
  sort?: string,
  packageType?: string,
): Promise<PiPackageItem[]> {
  return invoke<PiPackageItem[]>("fetch_pi_packages", {
    query: query || null,
    sort: sort || null,
    packageType: packageType || null,
  });
}

export function installWorkPiExtension(
  packageName: string,
  displayName?: string,
  description?: string,
): Promise<WorkResourceSummary> {
  return invoke<WorkResourceSummary>("install_work_pi_extension", {
    packageName,
    displayName: displayName || null,
    description: description || null,
  });
}

export function uninstallWorkPiExtension(id: string): Promise<void> {
  return invoke<void>("uninstall_work_pi_extension", { id });
}

export function testWorkConnector(name: string): Promise<WorkConnectorHealth> {
  return invoke<WorkConnectorHealth>("work_test_connector", { name });
}

export function getWorkBrowserConfig(): Promise<WorkBrowserSummary> {
  return invoke<WorkBrowserSummary>("work_get_browser_config");
}

export function saveWorkBrowserConfig(input: {
  provider?: string;
  enabled: boolean;
  maxResults?: number;
  apiKey?: string | null;
  endpointUrl?: string | null;
  allowedHosts?: string[] | null;
}): Promise<WorkBrowserSummary> {
  return invoke<WorkBrowserSummary>("work_save_browser_config", {
    provider: input.provider ?? "tavily",
    enabled: input.enabled,
    maxResults: input.maxResults ?? 5,
    apiKey: input.apiKey ?? null,
    endpointUrl: input.endpointUrl ?? null,
    allowedHosts: input.allowedHosts ?? null,
  });
}

export function testWorkBrowser(): Promise<WorkBrowserHealth> {
  return invoke<WorkBrowserHealth>("work_test_browser");
}

export function installWorkBrowserAdapter(): Promise<string> {
  return invoke<string>("work_install_browser_adapter");
}

export function isWorkBrowserAdapterInstalled(): Promise<boolean> {
  return invoke<boolean>("work_is_browser_adapter_installed");
}

export function listWorkFiles(workspaceId: string, area?: string): Promise<WorkFileSummary[]> {
  return invoke<WorkFileSummary[]>("work_list_files", {
    workspaceId,
    area: area ?? null,
  });
}

export function importWorkFile(workspaceId: string, sourcePath: string): Promise<WorkFileSummary> {
  return invoke<WorkFileSummary>("work_import_file", { workspaceId, sourcePath });
}

export function openWorkFile(workspaceId: string, path: string): Promise<void> {
  return invoke<void>("work_open_file", { workspaceId, path });
}

export function openWorkDirectory(workspaceId: string, path: string): Promise<void> {
  return invoke<void>("work_open_directory", { workspaceId, path });
}

export function removeWorkFile(workspaceId: string, path: string): Promise<void> {
  return invoke<void>("work_remove_file", { workspaceId, path });
}

export function openWorkspace(id: string): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_open_workspace", { id });
}

export function listWorkArtifacts(
  workspaceId: string,
  runId?: string | null,
): Promise<WorkArtifactSummary[]> {
  return invoke<WorkArtifactSummary[]>("work_list_artifacts", {
    workspaceId,
    runId: runId ?? null,
  });
}

export function registerWorkArtifact(
  workspaceId: string,
  path: string,
  title: string,
  artifactType?: string,
  runId?: string | null,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_register_artifact", {
    workspaceId,
    path,
    title,
    artifactType: artifactType ?? null,
    runId: runId ?? null,
  });
}

export function updateWorkOfficeArtifact(
  workspaceId: string,
  artifactId: string,
  contentBase64: string,
  runId?: string | null,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_update_office_artifact", {
    workspaceId,
    artifactId,
    contentBase64,
    runId: runId ?? null,
  });
}

export function createWorkOfficeArtifact(
  workspaceId: string,
  path: string,
  title: string,
  artifactType: string,
  contentBase64: string,
  runId?: string | null,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_create_office_artifact", {
    workspaceId,
    path,
    title,
    artifactType,
    contentBase64,
    runId: runId ?? null,
  });
}

export function validateWorkArtifact(
  workspaceId: string,
  artifactId: string,
  runId?: string | null,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_validate_artifact", {
    workspaceId,
    artifactId,
    runId: runId ?? null,
  });
}

export function deliverWorkArtifact(
  workspaceId: string,
  artifactId: string,
  runId?: string | null,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_deliver", {
    workspaceId,
    artifactId,
    runId: runId ?? null,
  });
}

export function deleteWorkArtifact(workspaceId: string, artifactId: string): Promise<void> {
  return invoke<void>("work_delete_artifact", { workspaceId, artifactId });
}

export function exportWorkArtifact(
  workspaceId: string,
  artifactId: string,
  destinationPath: string,
  runId?: string | null,
): Promise<string> {
  return invoke<string>("work_export_artifact", {
    workspaceId,
    artifactId,
    destinationPath,
    runId: runId ?? null,
  });
}

export function copyWorkArtifactToPrimary(
  workspaceId: string,
  artifactId: string,
  runId?: string | null,
): Promise<string> {
  return invoke<string>("work_copy_artifact_to_primary", {
    workspaceId,
    artifactId,
    runId: runId ?? null,
  });
}

export function setWorkArtifactStorageMode(
  workspaceId: string,
  mode: WorkArtifactStorageMode,
): Promise<WorkWorkspaceSummary> {
  return invoke<WorkWorkspaceSummary>("work_set_artifact_storage_mode", {
    workspaceId,
    mode,
  });
}

export function getWorkSession(workspaceId: string): Promise<TaskRun | null> {
  return invoke<TaskRun | null>("work_get_session", { workspaceId });
}

export function listWorkSessions(workspaceId: string): Promise<TaskRun[]> {
  return invoke<TaskRun[]>("work_list_sessions", { workspaceId });
}

export function startWorkSession(
  workspaceId: string,
  message: string,
  model?: string,
  attachments?: Attachment[],
  preset: WorkPreset = "office",
  runtime?: string,
): Promise<TaskRun> {
  return invoke<TaskRun>("work_start_session", {
    workspaceId,
    message,
    model: model ?? null,
    attachments: attachments?.map(toWorkAttachment) ?? [],
    preset,
    runtime: runtime ?? null,
  });
}

export function resumeWorkSession(
  runId: string,
  message?: string,
  attachments?: Attachment[],
): Promise<TaskRun> {
  return invoke<TaskRun>("work_resume_session", {
    runId,
    message: message?.trim() ? message.trim() : null,
    attachments: attachments?.map(toWorkAttachment) ?? [],
  });
}

export function sendWorkMessage(
  runId: string,
  message: string,
  attachments?: Attachment[],
): Promise<void> {
  return invoke<void>("work_send_message", {
    runId,
    message,
    attachments: attachments?.map(toWorkAttachment) ?? [],
  });
}

function toWorkAttachment(attachment: Attachment): {
  content_base64: string;
  media_type: string;
  filename: string;
} {
  return {
    content_base64: attachment.contentBase64,
    media_type: attachment.type || "application/octet-stream",
    filename: attachment.name,
  };
}

export function stopWorkSession(runId: string): Promise<TaskRun> {
  return invoke<TaskRun>("work_stop_session", { runId });
}

/** Cancel only the active DSH Work turn; keep the Work session alive. */
export function cancelWorkTurn(runId: string): Promise<void> {
  return invoke<void>("cancel_session_turn", { runId });
}

/* ============================================================================
 * Standalone (workspace-less) Work Task API
 * ============================================================================ */

export function startStandaloneWorkSession(
  message: string,
  model?: string,
  attachments?: Attachment[],
  permissionMode?: WorkExecutionMode,
  preset: WorkPreset = "office",
  runtime?: string,
): Promise<TaskRun> {
  return invoke<TaskRun>("work_start_standalone_session", {
    message,
    model: model ?? null,
    attachments: attachments?.map(toWorkAttachment) ?? [],
    permissionMode: permissionMode ?? null,
    preset,
    runtime: runtime ?? null,
  });
}

export function listStandaloneWorkSessions(): Promise<TaskRun[]> {
  return invoke<TaskRun[]>("work_list_standalone_sessions");
}

export function listRecentWorkSessions(limit = 20): Promise<TaskRun[]> {
  return invoke<TaskRun[]>("work_list_recent_sessions", { limit });
}

export function listArchivedWorkSessions(limit = 50): Promise<TaskRun[]> {
  return invoke<TaskRun[]>("work_list_archived_sessions", { limit });
}

export function listStandaloneWorkArtifacts(runId: string): Promise<WorkArtifactSummary[]> {
  return invoke<WorkArtifactSummary[]>("work_list_standalone_artifacts", { runId });
}

export function registerStandaloneWorkArtifact(
  runId: string,
  path: string,
  title: string,
  artifactType?: string,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_register_standalone_artifact", {
    runId,
    path,
    title,
    artifactType: artifactType ?? null,
  });
}

export function updateStandaloneWorkOfficeArtifact(
  runId: string,
  artifactId: string,
  contentBase64: string,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_update_standalone_office_artifact", {
    runId,
    artifactId,
    contentBase64,
  });
}

export function createStandaloneWorkOfficeArtifact(
  runId: string,
  path: string,
  title: string,
  artifactType: string,
  contentBase64: string,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_create_standalone_office_artifact", {
    runId,
    path,
    title,
    artifactType,
    contentBase64,
  });
}

export function validateStandaloneWorkArtifact(
  runId: string,
  artifactId: string,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_validate_standalone_artifact", {
    runId,
    artifactId,
  });
}

export function deliverStandaloneWorkArtifact(
  runId: string,
  artifactId: string,
): Promise<WorkArtifactSummary> {
  return invoke<WorkArtifactSummary>("work_deliver_standalone_artifact", {
    runId,
    artifactId,
  });
}

export function deleteStandaloneWorkArtifact(runId: string, artifactId: string): Promise<void> {
  return invoke<void>("work_delete_standalone_artifact", { runId, artifactId });
}

export function exportStandaloneWorkArtifact(
  runId: string,
  artifactId: string,
  destinationPath: string,
): Promise<string> {
  return invoke<string>("work_export_standalone_artifact", {
    runId,
    artifactId,
    destinationPath,
  });
}

export function continueWorkSession(
  workspaceId: string,
  runId: string,
  anchorId?: string,
  model?: string,
): Promise<TaskRun> {
  return invoke<TaskRun>("work_continue_session", {
    workspaceId,
    runId,
    anchorId: anchorId ?? null,
    model: model ?? null,
  });
}

/* ============================================================================
 * Work Mode V2 API Functions: WorkTask, InboxItem, WorkPolicy
 * ============================================================================ */

export function createWorkTask(
  workspaceId: string,
  title: string,
  instructions: string,
  policy?: WorkPolicy,
  schedule?: WorkScheduleConfig | null,
  requiredArtifacts?: string[],
  artifactRequirements?: WorkArtifactRequirement[],
): Promise<WorkTask> {
  return invoke<WorkTask>("work_create_task", {
    workspaceId,
    title,
    instructions,
    policy: policy ?? null,
    schedule: schedule ?? null,
    requiredArtifacts: requiredArtifacts ?? null,
    artifactRequirements: artifactRequirements ?? null,
  });
}

export function deleteWorkTask(id: string): Promise<void> {
  return invoke<void>("work_delete_task", { id });
}

export function duplicateWorkTask(id: string): Promise<WorkTask> {
  return invoke<WorkTask>("work_duplicate_task", { id });
}

export function getWorkTask(id: string): Promise<WorkTask> {
  return invoke<WorkTask>("work_get_task", { id });
}

export function listWorkTasks(workspaceId?: string): Promise<WorkTask[]> {
  return invoke<WorkTask[]>("work_list_tasks", { workspaceId: workspaceId ?? null });
}

export function updateWorkTask(task: WorkTask): Promise<void> {
  return invoke<void>("work_update_task", { task });
}

export function addWorkStandingRule(taskId: string, rule: TaskStandingRule): Promise<WorkTask> {
  return invoke<WorkTask>("work_add_standing_rule", { taskId, rule });
}

export function startWorkRun(
  taskId: string,
  sessionId?: string,
  trigger?: WorkRunTrigger,
): Promise<WorkRun> {
  return invoke<WorkRun>("work_start_run", {
    taskId,
    sessionId: sessionId ?? null,
    trigger: trigger ?? null,
  });
}

export function retryWorkTaskRun(taskId: string, runId: string): Promise<WorkRun> {
  return invoke<WorkRun>("work_retry_task_run", { taskId, runId });
}

export function finishWorkRun(
  taskId: string,
  runId: string,
  status: WorkRunStatus,
  errorMessage?: string,
  taskState?: WorkTaskState,
): Promise<WorkRun> {
  return invoke<WorkRun>("work_finish_run", {
    taskId,
    runId,
    status,
    errorMessage: errorMessage ?? null,
    taskState: taskState ?? null,
  });
}

export function resolvePendingWorkApproval(
  runId: string,
  decision: "approve" | "revise" | "cancel",
): Promise<WorkTaskState> {
  return invoke<WorkTaskState>("work_resolve_pending_approval", { runId, decision });
}

export function listWorkRuns(taskId: string): Promise<WorkRun[]> {
  return invoke<WorkRun[]>("work_list_runs", { taskId });
}

export function getWorkRun(taskId: string, runId: string): Promise<WorkRun> {
  return invoke<WorkRun>("work_get_run", { taskId, runId });
}

export function getWorkProjection(runId: string): Promise<WorkProjection> {
  return invoke<WorkProjection>("work_get_projection", { runId });
}

export function getWorkRunProgress(taskId: string, runId: string): Promise<WorkRunProgressView> {
  return invoke<WorkRunProgressView>("work_get_run_progress", { taskId, runId });
}

/** 任务回执：成果 + 真实来源 + 输入 + 文件变更 + 运行信息（Ledger 投影）。 */
export function getWorkRunReceipt(taskId: string, runId: string): Promise<WorkRunReceipt> {
  return invoke<WorkRunReceipt>("work_get_run_receipt", { taskId, runId });
}

/** 独立（无 Workspace）Work 会话的任务回执。 */
export function getStandaloneWorkRunReceipt(runId: string): Promise<WorkRunReceipt> {
  return invoke<WorkRunReceipt>("work_get_standalone_run_receipt", { runId });
}

/** 获取 WorkRun 的 GoalSpec 目标规范契约。 */
export function getWorkGoalSpec(taskId: string, runId: string): Promise<GoalSpec | null> {
  return invoke<GoalSpec | null>("work_get_goal_spec", { taskId, runId });
}

/** 强制触发 WorkRun 的目标机器验收。 */
export function verifyWorkGoal(taskId: string, runId: string): Promise<GoalSpec> {
  return invoke<GoalSpec>("work_verify_goal", { taskId, runId });
}

/** 推进或手动触发 WorkRun 目标自动修复。 */
export function triggerWorkGoalRepair(taskId: string, runId: string): Promise<GoalSpec> {
  return invoke<GoalSpec>("work_trigger_goal_repair", { taskId, runId });
}

/** 获取全局自动化概览与统计数据。 */
export function getWorkAutomationStats(): Promise<WorkAutomationStats> {
  return invoke<WorkAutomationStats>("work_get_automation_stats");
}

/** 获取指定任务的历史执行记录摘要。 */
export function getWorkTaskRuns(taskId: string): Promise<WorkTaskRunSummary[]> {
  return invoke<WorkTaskRunSummary[]>("work_list_task_runs", { taskId });
}

/** 获取资料库条目列表（支持按工作空间、分类及关键字检索）。 */
export function listWorkLibraryItems(
  workspaceId?: string,
  category?: LibraryCategory,
  query?: string,
): Promise<LibraryItemSummary[]> {
  return invoke<LibraryItemSummary[]>("work_list_library_items", {
    workspaceId: workspaceId || null,
    category: category || null,
    query: query || null,
  });
}

/** 获取单条资料详情与全文。 */
export function getWorkLibraryItem(id: string, workspaceId?: string): Promise<LibraryItem> {
  return invoke<LibraryItem>("work_get_library_item", {
    id,
    workspaceId: workspaceId || null,
  });
}

/** 保存或新建资料条目。 */
export function saveWorkLibraryItem(item: LibraryItem): Promise<LibraryItem> {
  return invoke<LibraryItem>("work_save_library_item", { item });
}

/** 删除指定资料条目。 */
export function deleteWorkLibraryItem(id: string, workspaceId?: string): Promise<void> {
  return invoke<void>("work_delete_library_item", {
    id,
    workspaceId: workspaceId || null,
  });
}

export function addWorkLibraryFile(
  workspaceId: string | undefined,
  sourcePath: string,
  category: LibraryCategory,
  options?: { title?: string; collection?: string; tags?: string[] },
): Promise<LibraryItem> {
  return invoke<LibraryItem>("work_add_library_file", {
    workspaceId: workspaceId || null,
    sourcePath,
    title: options?.title || null,
    category,
    collection: options?.collection || null,
    tags: options?.tags || [],
  });
}

export function addWorkLibraryDirectory(
  workspaceId: string | undefined,
  sourcePath: string,
  category: LibraryCategory,
  options?: { collection?: string; tags?: string[] },
): Promise<LibraryItem[]> {
  return invoke<LibraryItem[]>("work_add_library_directory", {
    workspaceId: workspaceId || null,
    sourcePath,
    category,
    collection: options?.collection || null,
    tags: options?.tags || [],
  });
}

export function renameWorkLibraryItem(
  id: string,
  title: string,
  workspaceId?: string,
): Promise<LibraryItem> {
  return invoke<LibraryItem>("work_rename_library_item", {
    id,
    title,
    workspaceId: workspaceId || null,
  });
}

/** 从已验证的成果物沉淀为资料库条目。 */
export function createWorkLibraryItemFromArtifact(
  workspaceId: string,
  artifactId: string,
  title: string,
  category: LibraryCategory,
): Promise<LibraryItem> {
  return invoke<LibraryItem>("work_create_library_item_from_artifact", {
    workspaceId,
    artifactId,
    title,
    category,
  });
}

export function getWorkRunRecovery(
  workspaceId: string,
  taskId: string,
  runId: string,
): Promise<WorkRunRecovery | null> {
  return invoke<WorkRunRecovery | null>("work_get_run_recovery", {
    workspaceId,
    taskId,
    runId,
  });
}

export function recoverWorkRun(
  workspaceId: string,
  taskId: string,
  runId: string,
  action: WorkRecoveryAction,
  subagentId?: string,
): Promise<WorkRun> {
  return invoke<WorkRun>("work_recover_run", {
    workspaceId,
    taskId,
    runId,
    action,
    subagentId: subagentId ?? null,
  });
}

export function verifyRecoveredWorkOutput(
  workspaceId: string,
  taskId: string,
  runId: string,
): Promise<WorkRun> {
  return invoke<WorkRun>("work_verify_recovered_output", {
    workspaceId,
    taskId,
    runId,
  });
}

export function retryWorkSubagent(
  workspaceId: string,
  taskId: string,
  runId: string,
  subagentId: string,
): Promise<WorkRun> {
  return invoke<WorkRun>("work_retry_subagent", {
    workspaceId,
    taskId,
    runId,
    subagentId,
  });
}

export async function listWorkSubagents(
  parentScope: string,
  taskId?: string | null,
): Promise<WorkSubagentRecord[]> {
  const records = await invoke<WorkSubagentRecord[]>("work_list_subagents", {
    parentScope,
    taskId: taskId ?? null,
  });
  // These IDs become Svelte keyed-list identities. Reject a broken host
  // contract here so a refresh error cannot abort the whole Work render.
  const ids = new Set<string>();
  if (!Array.isArray(records)) throw new Error("Invalid Work subagent response");
  for (const record of records) {
    if (!record || typeof record.agentId !== "string" || !record.agentId.trim()) {
      throw new Error("Work subagent response is missing agentId");
    }
    if (ids.has(record.agentId)) throw new Error("Duplicate Work subagent agentId");
    ids.add(record.agentId);
  }
  return records;
}

export function createInboxItem(
  taskId: string,
  runId: string,
  workspaceId: string,
  itemType: InboxItemType,
  title: string,
  description: string,
  payload: InboxItemPayload,
): Promise<InboxItem> {
  return invoke<InboxItem>("work_create_inbox_item", {
    taskId,
    runId,
    workspaceId,
    itemType,
    title,
    description,
    payload,
  });
}

export function getInboxItem(id: string): Promise<InboxItem> {
  return invoke<InboxItem>("work_get_inbox_item", { id });
}

/** Return the session run id behind a durable Inbox WorkRun. */
export function getInboxSessionRun(id: string): Promise<string | null> {
  return invoke<string | null>("work_get_inbox_session_run", { id });
}

export function listInboxItems(onlyPending?: boolean, taskId?: string): Promise<InboxItem[]> {
  return invoke<InboxItem[]>("work_list_inbox_items", {
    onlyPending: onlyPending ?? null,
    taskId: taskId ?? null,
  });
}

export function resolveInboxItem(
  id: string,
  status: InboxItemStatus,
  response?: unknown,
): Promise<InboxItem> {
  return invoke<InboxItem>("work_resolve_inbox_item", { id, status, response: response ?? null });
}

export function deleteInboxItem(id: string): Promise<void> {
  return invoke<void>("work_delete_inbox_item", { id });
}

export function clearInboxItems(workspaceId?: string, onlyResolved?: boolean): Promise<number> {
  return invoke<number>("work_clear_inbox_items", {
    workspaceId: workspaceId ?? null,
    onlyResolved: onlyResolved ?? null,
  });
}

export function evaluateToolRisk(toolName: string): Promise<ToolRiskClass> {
  return invoke<ToolRiskClass>("work_evaluate_tool_risk", { toolName });
}

export function checkConfirmationRequired(
  policy: WorkPolicy,
  toolName: string,
  target: string,
): Promise<boolean> {
  return invoke<boolean>("work_check_confirmation_required", { policy, toolName, target });
}

export function assignSessionWorkspace(runId: string, workspaceId: string): Promise<TaskRun> {
  return invoke<TaskRun>("work_assign_session_workspace", { runId, workspaceId });
}

export function getBrowserSession(runId: string): Promise<BrowserSession | null> {
  return invoke<BrowserSession | null>("get_browser_session", { runId });
}

export function listBrowserSessions(): Promise<BrowserSession[]> {
  return invoke<BrowserSession[]>("list_browser_sessions");
}

export function controlBrowserSession(runId: string, action: string): Promise<BrowserSession> {
  return invoke<BrowserSession>("control_browser_session", { runId, action });
}

export function browserUserInteract(
  runId: string,
  action: string,
  params: Record<string, unknown> = {},
): Promise<BrowserSession> {
  return invoke<BrowserSession>("browser_user_interact", { runId, action, params });
}

export function getBrowserTraces(runId: string): Promise<BrowserTraceEntry[]> {
  return invoke<BrowserTraceEntry[]>("get_browser_traces", { runId });
}

export function getCapabilityCenterProjection(
  runtime?: string,
): Promise<CapabilityCenterProjection> {
  return invoke<CapabilityCenterProjection>("work_get_capability_center_projection", {
    runtime: runtime ?? null,
  });
}

export function searchCapabilities(query: string, limit?: number): Promise<CapabilityCenterItem[]> {
  return invoke<CapabilityCenterItem[]>("work_search_capabilities", {
    query,
    limit: limit ?? null,
  });
}

export function getRunEffectiveCapabilities(runId: string): Promise<RunEffectiveCapabilitiesView> {
  return invoke<RunEffectiveCapabilitiesView>("work_get_run_effective_capabilities", {
    runId,
  });
}

export function getWorkArtifactAcceptance(runId: string): Promise<WorkArtifactAcceptance> {
  return invoke<WorkArtifactAcceptance>("work_get_artifact_acceptance", {
    runId,
  });
}
