import * as api from "$lib/api";
import type { SelectedExpert } from "$lib/components/WorkBuddyCascadingMenu.svelte";

type ExpertApi = Pick<typeof api, "getSessionExpert" | "setSessionExpert" | "resolveSessionExpert">;

/** Conversation configuration survives composer remounts; the host persists established runs. */
export class SessionExpertStore {
  selected = $state<SelectedExpert | null>(null);
  loading = $state(false);
  changing = $state(false);
  loadFailed = $state(false);
  error = $state("");
  private runId: string | undefined;
  private generation = 0;
  private newChatExpert: SelectedExpert | null = null;

  constructor(private readonly host: ExpertApi = api) {}

  async load(runId: string): Promise<void> {
    if (this.runId === runId) return;
    const generation = ++this.generation;
    this.runId = runId;
    this.error = "";
    this.loadFailed = false;
    this.changing = false;
    this.selected = runId ? null : this.newChatExpert;
    this.loading = !!runId;
    if (!runId) return;
    try {
      const expert = await this.host.getSessionExpert(runId);
      if (generation === this.generation) this.selected = expert;
    } catch (error) {
      if (generation === this.generation) {
        this.error = String(error);
        this.loadFailed = true;
      }
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  async select(expert: SelectedExpert | null): Promise<void> {
    if (this.loading || this.changing) throw new Error("专家配置正在加载，请稍后重试");
    const runId = this.runId ?? "";
    const generation = this.generation;
    this.changing = true;
    this.error = "";
    try {
      const selected = runId
        ? await this.host.setSessionExpert(runId, expert?.id ?? null)
        : expert
          ? await this.host.resolveSessionExpert(expert.id)
          : null;
      if (generation !== this.generation) return;
      this.selected = selected;
      this.loadFailed = false;
      if (!runId) this.newChatExpert = selected;
    } catch (error) {
      if (generation === this.generation) this.error = String(error);
      throw error;
    } finally {
      if (generation === this.generation) this.changing = false;
    }
  }

  adopt(runId: string, expert: SelectedExpert | null): void {
    ++this.generation;
    this.runId = runId;
    this.selected = expert;
    this.loadFailed = false;
    this.newChatExpert = null;
    this.loading = false;
    this.changing = false;
    this.error = "";
  }
}
