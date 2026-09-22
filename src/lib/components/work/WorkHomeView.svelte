<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { listRecentWorkSessions } from "$lib/api/work";
  import { WORK_STARTERS } from "$lib/data/work-starters";
  import type { TaskRun } from "$lib/types";
  import type { WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    workspaces: WorkWorkspaceSummary[];
    onSelectWorkspace: (id: string) => void;
    onCreateWorkspace: () => void;
    onOpenFolderWorkspace?: () => void;
  }

  let { workspaces, onSelectWorkspace, onCreateWorkspace, onOpenFolderWorkspace }: Props = $props();
  let quickTask = $state("");
  let recentSessions = $state<TaskRun[]>([]);

  onMount(() => {
    void listRecentWorkSessions(8)
      .then((sessions) => (recentSessions = sessions))
      .catch(() => {
        recentSessions = [];
      });
  });

  function openTask(prompt = quickTask) {
    const text = prompt.trim();
    if (text) void goto(`/chat/work?newSession=1&prompt=${encodeURIComponent(text)}`);
  }

  function formatTime(isoString: string): string {
    const date = new Date(isoString);
    const diffMinutes = Math.floor((Date.now() - date.getTime()) / 60000);
    if (diffMinutes < 1) return "刚刚";
    if (diffMinutes < 60) return `${diffMinutes} 分钟前`;
    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) return `${diffHours} 小时前`;
    if (diffHours < 48) return "昨天";
    return date.toLocaleDateString();
  }

  function sessionTitle(session: TaskRun): string {
    return (session.name?.trim() || session.prompt?.trim() || "新对话").split("\n")[0].slice(0, 72);
  }

  function openSession(session: TaskRun) {
    const params = new URLSearchParams({ run: session.id });
    if (session.workspace_id) params.set("workspace", session.workspace_id);
    void goto(`/chat/work?${params.toString()}`);
  }
</script>

<div
  class="mx-auto flex min-h-0 w-full max-w-3xl flex-1 flex-col overflow-y-auto px-1 pb-10 pt-10 sm:pt-16"
>
  <section aria-labelledby="work-home-title">
    <p class="text-xs font-medium text-muted-foreground">AgentCabin Work</p>
    <h1
      id="work-home-title"
      class="mt-2 text-2xl font-semibold tracking-tight text-foreground sm:text-3xl"
    >
      今天要处理什么？
    </h1>

    <form
      class="mt-6"
      onsubmit={(event) => {
        event.preventDefault();
        openTask();
      }}
    >
      <div
        class="flex items-center gap-2 rounded-2xl border border-border bg-card px-3 py-2 shadow-sm transition-colors focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-primary/10"
      >
        <input
          aria-label="描述你希望完成的事情"
          bind:value={quickTask}
          class="min-h-12 min-w-0 flex-1 bg-transparent px-2 text-sm text-foreground outline-none placeholder:text-muted-foreground/60"
          placeholder="描述你希望完成的事情…"
        />
        <button
          type="submit"
          aria-label="开始工作"
          disabled={!quickTask.trim()}
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary text-lg text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
          >→</button
        >
      </div>
      <div class="mt-3 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
        <button
          type="button"
          class="hover:text-foreground"
          onclick={onOpenFolderWorkspace || onCreateWorkspace}>选择工作区</button
        >
        <span aria-hidden="true">·</span>
        <button type="button" class="hover:text-foreground" onclick={onCreateWorkspace}
          >添加文件</button
        >
        <span aria-hidden="true">·</span>
        {#each WORK_STARTERS.slice(0, 4) as starter (starter.id)}
          <button
            type="button"
            class="rounded-full border border-border/70 px-2 py-1 transition-colors hover:border-primary/40 hover:text-foreground"
            onclick={() => openTask(starter.prompt)}>{starter.label}</button
          >
        {/each}
      </div>
    </form>
  </section>

  {#if recentSessions.length > 0}
    <section class="mt-12" aria-labelledby="recent-title">
      <div class="flex items-center justify-between">
        <h2 id="recent-title" class="text-sm font-semibold text-foreground">最近</h2>
        <span class="text-[11px] text-muted-foreground">{recentSessions.length}</span>
      </div>
      <div class="mt-2 divide-y divide-border/60 border-y border-border/60">
        {#each recentSessions as session (session.id)}
          <button
            type="button"
            class="flex w-full items-center gap-3 py-3 text-left transition-colors hover:bg-accent/40"
            onclick={() => openSession(session)}
          >
            <span class="min-w-0 flex-1 truncate text-sm text-foreground"
              >{sessionTitle(session)}</span
            >
            <span class="shrink-0 text-[11px] text-muted-foreground"
              >{formatTime(session.last_activity_at || session.started_at)}</span
            >
          </button>
        {/each}
      </div>
    </section>
  {/if}

  <section class="mt-10" aria-labelledby="workspace-title">
    <div class="flex items-center justify-between">
      <h2 id="workspace-title" class="text-sm font-semibold text-foreground">工作区</h2>
      <button
        type="button"
        class="text-[11px] text-muted-foreground hover:text-foreground"
        onclick={onCreateWorkspace}>新建</button
      >
    </div>
    {#if workspaces.length > 0}
      <div class="mt-2 divide-y divide-border/60 border-y border-border/60">
        {#each workspaces as workspace (workspace.id)}
          <button
            type="button"
            class="flex w-full items-center gap-3 py-3 text-left transition-colors hover:bg-accent/40"
            title={workspace.primaryWorkRoot || workspace.root}
            onclick={() => onSelectWorkspace(workspace.id)}
            ><span class="min-w-0 flex-1 truncate text-sm text-foreground">{workspace.name}</span
            ><span class="shrink-0 text-[11px] text-muted-foreground">进入 →</span></button
          >
        {/each}
      </div>
    {:else}
      <button
        type="button"
        class="mt-2 w-full rounded-xl border border-dashed border-border px-3 py-4 text-left text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
        onclick={onCreateWorkspace}>还没有工作区，创建一个来关联本地文件夹。</button
      >
    {/if}
  </section>
</div>
