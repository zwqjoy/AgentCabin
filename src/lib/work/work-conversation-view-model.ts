import type { Attachment } from "$lib/types";
import type {
  InboxItem,
  WorkArtifactSummary,
  WorkResultPresentation,
  WorkRunRecovery,
} from "$lib/types/work";

export type WorkConversationStatus =
  | "idle"
  | "running"
  | "waiting"
  | "completed"
  | "failed"
  | "stopped";

export interface WorkConversationViewModel {
  id: string | null;
  workspaceId: string | null;
  workspaceName?: string;
  status: WorkConversationStatus;
  title: string;
  readOnly: boolean;
  hasPendingAttention: boolean;
  pendingInteractions: InboxItem[];
  artifacts: WorkArtifactSummary[];
  resultPresentation?: WorkResultPresentation | null;
  recovery?: WorkRunRecovery | null;
}

export function computeConversationStatus(opts: {
  isRunning: boolean;
  sessionAlive: boolean;
  pendingCount: number;
  runStatus?: string;
}): WorkConversationStatus {
  if (opts.pendingCount > 0) return "waiting";
  if (opts.isRunning || opts.sessionAlive) return "running";
  if (opts.runStatus === "failed") return "failed";
  if (opts.runStatus === "stopped") return "stopped";
  if (opts.runStatus === "completed") return "completed";
  return "idle";
}
