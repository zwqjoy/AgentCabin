<script lang="ts">
  import { goto } from "$app/navigation";
  import { WORK_STARTERS } from "$lib/data/work-starters";
  import WorkLibraryPicker from "./WorkLibraryPicker.svelte";
  import type { WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    workspaces: WorkWorkspaceSummary[];
    onSelectWorkspace: (id: string) => void;
    onCreateWorkspace: () => void;
    onOpenFolderWorkspace?: () => void;
  }

  let { workspaces, onSelectWorkspace, onCreateWorkspace, onOpenFolderWorkspace }: Props = $props();

  let quickTask = $state("");
  let showLibraryPicker = $state(false);

  function openTask(prompt = quickTask) {
    const text = prompt.trim();
    if (!text) return;
    void goto(`/chat/work?newSession=1&prompt=${encodeURIComponent(text)}`);
  }

  function handleQuickTaskSubmit(event: SubmitEvent) {
    event.preventDefault();
    openTask();
  }

  function insertLibraryContent(content: string, title: string) {
    const capped =
      content.length > 4000 ? `${content.slice(0, 4000)}\n…（内容过长，已截断）` : content;
    const block = content.trimStart().startsWith("<!-- 引用资料:")
      ? content.trim()
      : `【参考资料：${title}】\n${capped}`;
    quickTask = quickTask.trim() ? `${quickTask.trim()}\n\n${block}` : block;
  }

  function shortPath(path: string): string {
    const home = path.match(/^\/Users\/[^/]+/);
    return home ? `~${path.slice(home[0].length)}` : path;
  }

  function formatTimeAgo(isoString: string): string {
    if (!isoString) return "";
    const date = new Date(isoString);
    const diffMs = Date.now() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "刚刚";
    if (diffMins < 60) return `${diffMins} 分钟前`;
    if (diffHours < 24) return `${diffHours} 小时前`;
    if (diffDays === 1) return "昨天";
    if (diffDays < 30) return `${diffDays} 天前`;
    return date.toLocaleDateString();
  }
</script>

<div class="flex min-h-0 flex-1 flex-col space-y-5 overflow-y-auto pr-1 pb-8">
  <section
    class="relative shrink-0 overflow-hidden rounded-2xl border border-primary/20 bg-gradient-to-br from-primary/[0.12] via-card/80 to-card/40 p-4 shadow-sm sm:p-5"
  >
    <div
      class="pointer-events-none absolute -right-16 -top-20 h-48 w-48 rounded-full bg-primary/10 blur-3xl"
      aria-hidden="true"
    ></div>
    <div class="relative">
      <div class="flex flex-wrap items-center gap-2">
        <span class="rounded-full bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary"
          >Work · 对话</span
        >
        <span class="text-[11px] text-muted-foreground">本机处理，敏感操作按权限执行</span>
      </div>
      <h1 class="mt-2 text-xl font-bold tracking-tight text-foreground sm:text-2xl">
        今天先完成哪件事？
      </h1>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground sm:text-sm">
        用一句话描述目标，Work 会帮你规划、执行并交付结果。需要时再选择本地工作目录。
      </p>

      <form class="mt-4" onsubmit={handleQuickTaskSubmit}>
        <div
          class="flex flex-col gap-2 rounded-xl border border-border/80 bg-background/90 p-2 shadow-sm transition-colors focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-primary/10 sm:flex-row"
        >
          <input
            aria-label="描述你希望 Work 完成的任务"
            bind:value={quickTask}
            class="min-h-11 min-w-0 flex-1 bg-transparent px-3 text-sm text-foreground outline-none placeholder:text-muted-foreground/60"
            placeholder="例如：把这份销售数据整理成带图表的分析报告"
          />
          <button
            type="button"
            title="从资料库引用知识与模板"
            class="inline-flex min-h-11 items-center justify-center gap-1 rounded-lg border border-border/70 bg-card px-3 text-xs font-medium text-muted-foreground transition-colors hover:border-primary/50 hover:text-foreground"
            onclick={() => (showLibraryPicker = true)}
          >
            📚 引用资料
          </button>
          <button
            type="submit"
            disabled={!quickTask.trim()}
            class="inline-flex min-h-11 items-center justify-center gap-1.5 rounded-lg bg-primary px-5 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:-translate-y-0.5 hover:shadow-md disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:translate-y-0 disabled:hover:shadow-sm"
          >
            开始工作 <span aria-hidden="true">→</span>
          </button>
        </div>
        <div
          class="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 px-1 text-[11px] text-muted-foreground"
        >
          <span>本地处理</span>
          <span aria-hidden="true">·</span>
          <button
            type="button"
            class="font-medium text-foreground/75 underline-offset-2 hover:text-primary hover:underline"
            onclick={onOpenFolderWorkspace || onCreateWorkspace}
          >
            选择工作目录
          </button>
        </div>
      </form>

      <div class="mt-3 flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="inline-flex min-h-8 items-center gap-1.5 rounded-lg border border-border/70 bg-card/70 px-2.5 py-1.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={onCreateWorkspace}
        >
          <span aria-hidden="true">＋</span>新建工作空间
        </button>
        <a
          href="/chat/work?view=tasks"
          class="inline-flex min-h-8 items-center gap-1.5 rounded-lg border border-border/70 bg-card/70 px-2.5 py-1.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          查看任务 <span aria-hidden="true">→</span>
        </a>
      </div>
    </div>
  </section>

  <section class="shrink-0 space-y-2.5">
    <div>
      <h2 class="text-sm font-semibold text-foreground">常用任务</h2>
      <p class="mt-1 text-[11px] text-muted-foreground">选择一个场景，先编辑说明再开始对话。</p>
    </div>
    <div class="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
      {#each WORK_STARTERS.slice(0, 6) as starter (starter.id)}
        <button
          type="button"
          class="group flex min-h-[58px] items-center gap-3 rounded-xl border border-border/60 bg-card/60 p-3 text-left shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/40 hover:shadow-xs"
          onclick={() => openTask(starter.prompt)}
        >
          <div
            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-muted/70 text-foreground/75 group-hover:bg-accent group-hover:text-foreground transition-colors"
          >
            {#if starter.id === "document"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
            {:else if starter.id === "data-report"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <line x1="18" y1="20" x2="18" y2="10" />
                <line x1="12" y1="20" x2="12" y2="4" />
                <line x1="6" y1="20" x2="6" y2="14" />
              </svg>
            {:else if starter.id === "presentation"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <rect width="20" height="14" x="2" y="3" rx="2" />
                <line x1="8" y1="21" x2="16" y2="21" />
                <line x1="12" y1="17" x2="12" y2="21" />
              </svg>
            {:else if starter.id === "research"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </svg>
            {:else if starter.id === "file-organize"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                />
              </svg>
            {:else}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <polygon
                  points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"
                />
              </svg>
            {/if}
          </div>
          <div class="min-w-0 flex-1">
            <span class="block truncate text-xs font-medium text-foreground tracking-tight"
              >{starter.label}</span
            >
            <span class="mt-0.5 block truncate text-[11px] text-muted-foreground/75"
              >{starter.description}</span
            >
          </div>
        </button>
      {/each}
    </div>
  </section>

  {#if workspaces.length > 0}
    <section class="space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-foreground">继续工作</h2>
        <span class="text-xs text-muted-foreground">{workspaces.length} 个活跃空间</span>
      </div>

      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each workspaces as workspace (workspace.id)}
          <button
            type="button"
            class="group flex flex-col justify-between rounded-xl border border-border/70 bg-card/60 p-4 text-left shadow-2xs transition-all duration-150 hover:-translate-y-0.5 hover:border-primary/50 hover:bg-card hover:shadow-md"
            title={workspace.primaryWorkRoot || workspace.root}
            onclick={() => onSelectWorkspace(workspace.id)}
          >
            <div>
              <div class="flex items-start justify-between gap-2">
                <span
                  class="truncate text-sm font-semibold text-foreground group-hover:text-primary"
                  >{workspace.name}</span
                >
                <span class="shrink-0 text-[10px] text-muted-foreground/80"
                  >{formatTimeAgo(workspace.updatedAt || workspace.createdAt)}</span
                >
              </div>
              <p class="mt-1 truncate font-mono text-[11px] text-muted-foreground/70">
                {workspace.rootKind === "local_folder" && workspace.primaryWorkRoot
                  ? shortPath(workspace.primaryWorkRoot)
                  : "内置独立工作区"}
              </p>
            </div>

            <div
              class="mt-4 flex items-center justify-between border-t border-border/40 pt-3 text-[11px] text-muted-foreground"
            >
              <div class="flex flex-wrap items-center gap-2.5">
                {#if workspace.workingRootValid === false}
                  <span
                    class="rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-red-500"
                    >目录不可用</span
                  >
                {:else}
                  <span
                    >{workspace.artifactCount > 0
                      ? `${workspace.artifactCount} 个成果`
                      : "暂无成果"}</span
                  >
                {/if}
                {#if workspace.accessRoots?.length > 0}
                  <span>·</span>
                  <span>{workspace.accessRoots.length} 个授权目录</span>
                {/if}
              </div>
              <span
                class="text-xs font-medium text-primary opacity-0 transition-opacity group-hover:opacity-100"
              >
                进入 →
              </span>
            </div>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  <div
    class="flex items-center justify-between border-t border-border/50 pt-3 text-[11px] text-muted-foreground"
  >
    <span>需要找回旧对话？</span>
    <a href="/chat/work?view=archived" class="font-medium text-primary hover:underline"
      >查看已归档对话 →</a
    >
  </div>
</div>

{#if showLibraryPicker}
  <WorkLibraryPicker
    workspaceId=""
    onInsert={insertLibraryContent}
    onClose={() => (showLibraryPicker = false)}
  />
{/if}
