import { clearInboxItems, deleteInboxItem, listInboxItems, resolveInboxItem } from "$lib/api/work";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import type { BusEvent } from "$lib/types";
import type { InboxItem, InboxItemStatus } from "$lib/types/work";
import { isGlobalInboxInteraction } from "$lib/utils/work-interactions";

export class InboxStore {
  items = $state<InboxItem[]>([]);
  loading = $state(false);
  error = $state("");

  private lastFetchArgs: { onlyPending: boolean; taskId?: string } | null = null;
  private eventRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private eventRefreshQueued = false;
  private readonly unsubscribeEvents: () => void;

  constructor() {
    // The event middleware is shared by Code and Work. Inbox only reacts to
    // events that can create/resolve a user-facing Work interaction, so token
    // deltas never turn into a request storm.
    this.unsubscribeEvents = getEventMiddleware().subscribeEvents((event) => {
      if (!this.shouldRefreshForEvent(event) || !this.lastFetchArgs) return;
      this.queueEventRefresh();
    });
  }

  get pendingCount(): number {
    return this.pendingItems.length;
  }

  get pendingItems(): InboxItem[] {
    return this.items.filter((item) => item.status === "pending" && isGlobalInboxInteraction(item));
  }

  get actionItems(): InboxItem[] {
    return this.items.filter(isGlobalInboxInteraction);
  }

  async fetch(onlyPending = false, taskId?: string): Promise<void> {
    this.lastFetchArgs = { onlyPending, taskId };
    this.loading = true;
    this.error = "";
    try {
      this.items = await listInboxItems(onlyPending, taskId);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.loading = false;
      if (this.eventRefreshQueued) {
        this.eventRefreshQueued = false;
        this.queueEventRefresh();
      }
    }
  }

  /**
   * Refresh only after a state-changing event. This replaces the old Work-page
   * 3s interval while retaining a small debounce for bursts of bus events.
   */
  private queueEventRefresh(): void {
    if (this.loading) {
      this.eventRefreshQueued = true;
      return;
    }
    if (this.eventRefreshTimer || !this.lastFetchArgs) return;
    this.eventRefreshTimer = setTimeout(() => {
      this.eventRefreshTimer = null;
      const args = this.lastFetchArgs;
      if (!args) return;
      void this.fetch(args.onlyPending, args.taskId);
    }, 120);
  }

  private shouldRefreshForEvent(event: BusEvent): boolean {
    switch (event.type) {
      case "permission_prompt":
      case "permission_denied":
      case "elicitation_prompt":
      case "pi_extension_ui_request_created":
      case "pi_extension_ui_request_resolved":
      case "work_task_state":
      case "run_state":
      case "user_message":
        return true;
      case "tool_end":
        return event.tool_name === "AskUserQuestion" || event.status !== "success";
      default:
        return false;
    }
  }

  async resolve(id: string, status: InboxItemStatus, response?: unknown): Promise<InboxItem> {
    this.error = "";
    try {
      const updated = await resolveInboxItem(id, status, response);
      this.items = this.items.map((item) => (item.id === id ? updated : item));
      return updated;
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async deleteItem(id: string): Promise<void> {
    this.error = "";
    try {
      await deleteInboxItem(id);
      this.items = this.items.filter((item) => item.id !== id);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async clearAll(workspaceId?: string, onlyResolved = true): Promise<number> {
    this.error = "";
    try {
      const cleared = await clearInboxItems(workspaceId, onlyResolved);
      await this.fetch(false);
      return cleared;
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  dispose(): void {
    this.unsubscribeEvents();
    if (this.eventRefreshTimer) clearTimeout(this.eventRefreshTimer);
    this.eventRefreshTimer = null;
    this.eventRefreshQueued = false;
  }
}

export const inboxStore = new InboxStore();
