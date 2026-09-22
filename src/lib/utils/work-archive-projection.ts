import type { TaskRun } from "$lib/types";
import { applyRunMutation, type RunMutation } from "$lib/utils/run-mutations";

export interface WorkArchiveProjection {
  recentSessions: TaskRun[];
  archivedSessions: TaskRun[];
}

/** Keep the two sidebar projections mutually exclusive and archive-aware. */
export function enforceWorkArchiveProjectionInvariant(
  projection: WorkArchiveProjection,
): WorkArchiveProjection {
  const archivedSessions = projection.archivedSessions.filter(
    (session) => session.archived === true,
  );
  const archivedIds = new Set(archivedSessions.map((session) => session.id));
  return {
    recentSessions: projection.recentSessions.filter(
      (session) => session.archived !== true && !archivedIds.has(session.id),
    ),
    archivedSessions,
  };
}

function upsertSession(sessions: TaskRun[], session: TaskRun): TaskRun[] {
  const index = sessions.findIndex((candidate) => candidate.id === session.id);
  if (index < 0) return [session, ...sessions];
  const next = [...sessions];
  next[index] = session;
  return next;
}

function removeSession(sessions: TaskRun[], runId: string): TaskRun[] {
  return sessions.filter((session) => session.id !== runId);
}

/** Apply a local run mutation to the recent and archived Work projections. */
export function applyWorkArchiveProjectionMutation(
  projection: WorkArchiveProjection,
  mutation: RunMutation,
  knownRun?: TaskRun,
): WorkArchiveProjection {
  if (mutation.kind === "delete") {
    const deletedIds = new Set(mutation.runIds);
    return enforceWorkArchiveProjectionInvariant({
      recentSessions: projection.recentSessions.filter((session) => !deletedIds.has(session.id)),
      archivedSessions: projection.archivedSessions.filter(
        (session) => !deletedIds.has(session.id),
      ),
    });
  }

  if (mutation.kind === "create") {
    if (mutation.run.archived === true) {
      return enforceWorkArchiveProjectionInvariant({
        recentSessions: removeSession(projection.recentSessions, mutation.run.id),
        archivedSessions: upsertSession(projection.archivedSessions, mutation.run),
      });
    }
    return enforceWorkArchiveProjectionInvariant({
      recentSessions: upsertSession(projection.recentSessions, mutation.run),
      archivedSessions: removeSession(projection.archivedSessions, mutation.run.id),
    });
  }

  if (mutation.patch.archived !== undefined) {
    const existing =
      projection.recentSessions.find((session) => session.id === mutation.runId) ??
      projection.archivedSessions.find((session) => session.id === mutation.runId) ??
      knownRun;
    const updated = existing ? { ...existing, ...mutation.patch } : null;
    if (mutation.patch.archived === true) {
      return enforceWorkArchiveProjectionInvariant({
        recentSessions: removeSession(projection.recentSessions, mutation.runId),
        archivedSessions: updated
          ? upsertSession(projection.archivedSessions, updated)
          : removeSession(projection.archivedSessions, mutation.runId),
      });
    }
    return enforceWorkArchiveProjectionInvariant({
      recentSessions: updated
        ? upsertSession(projection.recentSessions, updated)
        : removeSession(projection.recentSessions, mutation.runId),
      archivedSessions: removeSession(projection.archivedSessions, mutation.runId),
    });
  }

  return enforceWorkArchiveProjectionInvariant({
    recentSessions: applyRunMutation(projection.recentSessions, mutation),
    archivedSessions: applyRunMutation(projection.archivedSessions, mutation),
  });
}

export function filterRecentWorkSessions(
  sessions: TaskRun[],
  matches: (session: TaskRun) => boolean,
): TaskRun[] {
  return sessions.filter((session) => session.archived !== true && matches(session));
}

export function filterArchivedWorkSessions(
  sessions: TaskRun[],
  matches: (session: TaskRun) => boolean,
): TaskRun[] {
  return sessions.filter((session) => session.archived === true && matches(session));
}
