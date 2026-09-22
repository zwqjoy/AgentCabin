import {
  addWorkAccessRoot,
  copyWorkArtifactToPrimary,
  deleteStandaloneWorkArtifact,
  deleteWorkArtifact,
  deliverStandaloneWorkArtifact,
  deliverWorkArtifact,
  exportStandaloneWorkArtifact,
  exportWorkArtifact,
  getStandaloneWorkRunReceipt,
  getWorkRunReceipt,
  getWorkRunRecovery,
  importWorkFile,
  listStandaloneWorkArtifacts,
  listWorkAccessRoots,
  listWorkArtifacts,
  listWorkFiles,
  openWorkDirectory,
  openWorkFile,
  recoverWorkRun,
  registerStandaloneWorkArtifact,
  registerWorkArtifact,
  removeWorkAccessRoot,
  removeWorkFile,
  setWorkAccessRootWritable,
  updateStandaloneWorkOfficeArtifact,
  updateWorkOfficeArtifact,
  validateStandaloneWorkArtifact,
  validateWorkArtifact,
} from "$lib/api/work";
import {
  normalizeWorkIdentity,
  type WorkIdentitySourceProgress,
  type WorkIdentitySourceRun,
} from "$lib/utils/work-identity";
import { scopeStorageId, type WorkScope } from "./work-scope";
import type {
  WorkAccessRoot,
  WorkArtifactSummary,
  WorkFileSummary,
  WorkRecoveryAction,
  WorkRun,
  WorkRunReceipt,
  WorkRunRecovery,
} from "$lib/types/work";

/**
 * Unified Work resource facade.
 *
 * Every artifact / file / receipt / recovery operation the Work UI needs is
 * expressed against a WorkScope. The standalone-vs-workspace backend split is
 * an implementation detail resolved inside this module — UI components and
 * stores call one function and never branch on the API family again.
 */

export interface WorkRunIdentityQuery {
  scope: WorkScope;
  /** Session run id of the current conversation ("" when not started yet). */
  conversationRunId: string;
  run?: WorkIdentitySourceRun | null;
  progressView?: WorkIdentitySourceProgress | null;
}

export interface WorkArtifactMutation {
  scope: WorkScope;
  /** Session run id backing the conversation; standalone scope requires it. */
  runId: string;
  artifactId: string;
  /**
   * Workspace artifacts can be registered against a specific run; callers pass
   * `artifact.runId` here so validation/delivery targets the producing run.
   */
  artifactRunId?: string | null;
}

function missingRunError(action: string): Error {
  return new Error(`当前对话尚未建立可${action}的任务记录。`);
}

function isMissingArtifactError(cause: unknown): boolean {
  const message = cause instanceof Error ? cause.message : String(cause);
  return /work artifact\b.*\bnot found\b/i.test(message);
}

// ── Artifacts ───────────────────────────────────────────────────────────────

export async function listArtifacts(
  scope: WorkScope,
  runId: string | null,
): Promise<WorkArtifactSummary[]> {
  if (scope.workspaceId === null) {
    return runId ? listStandaloneWorkArtifacts(runId) : [];
  }
  return listWorkArtifacts(scope.workspaceId, runId);
}

export async function registerArtifact(
  scope: WorkScope,
  runId: string | null,
  path: string,
  title: string,
  artifactType?: string,
): Promise<WorkArtifactSummary> {
  if (scope.workspaceId === null) {
    if (!runId) throw missingRunError("注册");
    return registerStandaloneWorkArtifact(runId, path, title, artifactType);
  }
  return registerWorkArtifact(scope.workspaceId, path, title, artifactType, runId);
}

export async function deleteArtifact(
  mutation: WorkArtifactMutation,
  artifact?: WorkArtifactSummary,
): Promise<void> {
  const { scope, runId, artifactId } = mutation;
  if (scope.workspaceId === null) {
    // Standalone artifacts are registered against the task that produced
    // them. The conversation may already have moved to a newer task while an
    // older card is still on screen, so prefer the artifact's own run id.
    const targetRunId = artifact?.runId || artifact?.workspaceId || runId;
    if (!targetRunId) {
      return;
    }
    try {
      await deleteStandaloneWorkArtifact(targetRunId, artifactId);
    } catch (cause) {
      // The registry may already have reconciled this card away. Removing the
      // stale UI entry is safe; we never delete an unknown file here.
      if (!isMissingArtifactError(cause)) throw cause;
    }
    return;
  }
  try {
    await deleteWorkArtifact(scope.workspaceId, artifactId);
  } catch (cause) {
    // Treat an already-reconciled registry entry as an idempotent delete so
    // an orphaned card cannot remain permanently undeletable.
    if (!isMissingArtifactError(cause)) throw cause;
  }
}

export async function validateArtifact(
  mutation: WorkArtifactMutation,
): Promise<WorkArtifactSummary> {
  const { scope, runId, artifactId, artifactRunId } = mutation;
  if (scope.workspaceId === null) {
    if (!runId) throw missingRunError("验证");
    return validateStandaloneWorkArtifact(runId, artifactId);
  }
  return validateWorkArtifact(scope.workspaceId, artifactId, artifactRunId ?? null);
}

export async function deliverArtifact(
  mutation: WorkArtifactMutation,
): Promise<WorkArtifactSummary> {
  const { scope, runId, artifactId, artifactRunId } = mutation;
  if (scope.workspaceId === null) {
    if (!runId) throw missingRunError("交付");
    return deliverStandaloneWorkArtifact(runId, artifactId);
  }
  return deliverWorkArtifact(scope.workspaceId, artifactId, artifactRunId ?? null);
}

export async function exportArtifact(
  scope: WorkScope,
  runId: string,
  artifactId: string,
  destinationPath: string,
  artifactRunId?: string | null,
): Promise<string> {
  if (scope.workspaceId === null) {
    if (!runId) throw missingRunError("导出");
    return exportStandaloneWorkArtifact(runId, artifactId, destinationPath);
  }
  return exportWorkArtifact(scope.workspaceId, artifactId, destinationPath, artifactRunId ?? null);
}

export async function copyArtifactToPrimary(
  scope: WorkScope,
  artifactId: string,
  artifactRunId?: string | null,
): Promise<string> {
  if (scope.workspaceId === null) throw new Error("独立对话的成果请先导出或移入工作区。");
  return copyWorkArtifactToPrimary(scope.workspaceId, artifactId, artifactRunId ?? null);
}

export async function saveOfficeArtifact(
  scope: WorkScope,
  runId: string,
  artifactId: string,
  contentBase64: string,
): Promise<WorkArtifactSummary> {
  if (scope.workspaceId === null) {
    if (!runId) throw missingRunError("保存");
    return updateStandaloneWorkOfficeArtifact(runId, artifactId, contentBase64);
  }
  return updateWorkOfficeArtifact(scope.workspaceId, artifactId, contentBase64, runId || null);
}

// ── Run receipt / recovery ──────────────────────────────────────────────────

export async function getReceipt(query: WorkRunIdentityQuery): Promise<WorkRunReceipt | null> {
  if (query.scope.workspaceId === null) {
    return query.conversationRunId ? getStandaloneWorkRunReceipt(query.conversationRunId) : null;
  }
  const identity = normalizeWorkIdentity({
    run: query.run,
    progressView: query.progressView,
    workspaceId: query.scope.workspaceId,
    conversationRunId: query.conversationRunId,
  });
  if (!identity.canLoadWorkspaceReceipt || !identity.taskId || !identity.workRunId) return null;
  return getWorkRunReceipt(identity.taskId, identity.workRunId);
}

export type WorkRecoveryQuery = WorkRunIdentityQuery;

function resolveRecoveryIdentity(query: WorkRecoveryQuery) {
  return normalizeWorkIdentity({
    run: query.run,
    progressView: query.progressView,
    workspaceId: query.scope.workspaceId,
    conversationRunId: query.conversationRunId,
  });
}

export async function getRecovery(query: WorkRecoveryQuery): Promise<WorkRunRecovery | null> {
  if (query.scope.workspaceId === null) {
    return null;
  }
  const identity = resolveRecoveryIdentity(query);
  if (!identity.canLoadWorkspaceReceipt || !identity.taskId || !identity.workRunId) return null;
  return getWorkRunRecovery(query.scope.workspaceId, identity.taskId, identity.workRunId);
}

export async function recoverRun(
  query: WorkRecoveryQuery,
  action: WorkRecoveryAction,
  subagentId?: string,
): Promise<WorkRun | null> {
  if (query.scope.workspaceId === null) return null;
  const identity = resolveRecoveryIdentity(query);
  if (!identity.canLoadWorkspaceReceipt || !identity.taskId || !identity.workRunId) return null;
  return recoverWorkRun(
    query.scope.workspaceId,
    identity.taskId,
    identity.workRunId,
    action,
    subagentId,
  );
}

// ── Workspace files (input area) ────────────────────────────────────────────

export async function listInputFiles(scope: WorkScope): Promise<WorkFileSummary[]> {
  if (scope.workspaceId === null) return [];
  return listWorkFiles(scope.workspaceId, "input");
}

export async function importInputFile(
  scope: WorkScope,
  sourcePath: string,
): Promise<WorkFileSummary | null> {
  if (scope.workspaceId === null) return null;
  return importWorkFile(scope.workspaceId, sourcePath);
}

export async function removeInputFile(scope: WorkScope, path: string): Promise<void> {
  if (scope.workspaceId === null) return;
  await removeWorkFile(scope.workspaceId, path);
}

/** Open a file with the OS default application. Standalone sessions reuse the run id namespace. */
export async function openFile(
  scope: WorkScope,
  standaloneRunId: string,
  path: string,
): Promise<void> {
  await openWorkFile(scopeStorageId(scope, standaloneRunId), path);
}

export async function openDirectory(
  scope: WorkScope,
  standaloneRunId: string,
  path: string,
): Promise<void> {
  await openWorkDirectory(scopeStorageId(scope, standaloneRunId), path);
}

// ── Access roots (workspace-only capability) ────────────────────────────────

export async function listAccessRoots(scope: WorkScope): Promise<WorkAccessRoot[]> {
  if (scope.workspaceId === null) return [];
  return listWorkAccessRoots(scope.workspaceId);
}

export async function addAccessRoot(
  scope: WorkScope,
  path: string,
  writable = false,
): Promise<WorkAccessRoot[]> {
  if (scope.workspaceId === null) return [];
  return addWorkAccessRoot(scope.workspaceId, path, writable);
}

export async function setAccessRootWritable(
  scope: WorkScope,
  path: string,
  writable: boolean,
): Promise<WorkAccessRoot[]> {
  if (scope.workspaceId === null) return [];
  return setWorkAccessRootWritable(scope.workspaceId, path, writable);
}

export async function removeAccessRoot(scope: WorkScope, path: string): Promise<WorkAccessRoot[]> {
  if (scope.workspaceId === null) return [];
  return removeWorkAccessRoot(scope.workspaceId, path);
}
