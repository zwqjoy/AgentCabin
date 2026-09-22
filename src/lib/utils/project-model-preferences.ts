import type { ProjectModelPreference } from "$lib/types";

/**
 * Return the stable settings key for a project + Agent selected in the left sidebar.
 * Remote projects include the host so identical paths on two hosts never collide.
 */
export function getProjectModelPreferenceKey(
  cwd: string,
  remoteHostName = "",
  agent = "claude",
): string {
  const normalizedCwd = cwd.trim().replace(/[\\/]+$/, "") || "/";
  const host = remoteHostName.trim() || "local";
  return `${host}::${normalizedCwd}::${agent.trim() || "claude"}`;
}

/** Whether the project preference should hydrate the current chat state automatically. */
export function shouldApplyProjectModelPreference(args: {
  preference: ProjectModelPreference | undefined;
  currentModel: string | undefined;
  availableModelIds: readonly string[];
  hasConversationMessages?: boolean;
  conversationStartPending?: boolean;
}): boolean {
  const preferredModel = args.preference?.model?.trim();
  return (
    !args.conversationStartPending &&
    !args.hasConversationMessages &&
    !!preferredModel &&
    args.availableModelIds.includes(preferredModel) &&
    args.currentModel !== preferredModel
  );
}
