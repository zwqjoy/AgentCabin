<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { listRuns } from "$lib/api";
  import type { TaskRun } from "$lib/types";
  import { getRunRoute, isWorkRun } from "$lib/utils/agent-target";
  import { cwdDisplayLabel, relativeTime } from "$lib/utils/format";
  import { buildProjectFolders } from "$lib/utils/sidebar-groups";
  import { RUNS_CHANGED_EVENT } from "$lib/utils/run-mutations";

  let runs = $state<TaskRun[]>([]);
  let loading = $state(true);
  let search = $state("");
  let selectedCwd = $state("");
  let filter = $state<"all" | "active" | "completed" | "needs_attention">("all");

  const codeRuns = $derived(runs.filter((run) => !isWorkRun(run) && !run.archived));
  const STANDALONE_SCOPE = "__standalone__";
  const projects = $derived(
    [
      ...new Set(
        codeRuns
          .filter((run) => !run.code_standalone_task)
          .map((run) => run.cwd)
          .filter((cwd) => Boolean(cwd && cwd !== "/")),
      ),
    ].sort(),
  );
  const tasks = $derived(
    buildProjectFolders(codeRuns, new Set(), [], [], false).flatMap((folder) =>
      folder.conversations.map((conversation) => ({
        ...conversation,
        projectLabel: conversation.latestRun.code_standalone_task
          ? "独立任务"
          : folder.isUncategorized
            ? "未关联项目"
            : cwdDisplayLabel(folder.cwd),
      })),
    ),
  );
  const visibleRuns = $derived(
    tasks
      .filter((task) => {
        const run = task.latestRun;
        const query = search.trim().toLocaleLowerCase();
        if (
          query &&
          ![task.title, run.prompt, task.projectLabel].some((value) =>
            value?.toLocaleLowerCase().includes(query),
          )
        ) {
          return false;
        }
        if (filter === "active") return run.status === "running" || run.status === "pending";
        if (filter === "completed") return run.status === "completed";
        if (filter === "needs_attention")
          return run.status === "failed" || run.status === "stopped" || run.status === "cancelled";
        return true;
      })
      .sort((a, b) =>
        (b.latestRun.last_activity_at ?? b.latestRun.started_at).localeCompare(
          a.latestRun.last_activity_at ?? a.latestRun.started_at,
        ),
      ),
  );

  async function refresh() {
    try {
      runs = await listRuns();
    } catch {
      // Keep the last successful list visible if a transient refresh fails.
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
    window.addEventListener(RUNS_CHANGED_EVENT, refresh);
    return () => window.removeEventListener(RUNS_CHANGED_EVENT, refresh);
  });

  function createTask() {
    const base = $page.url.pathname.includes("/pi") ? "/chat/pi" : "/chat";
    const codeStandaloneTask = selectedCwd === STANDALONE_SCOPE;
    const cwd = codeStandaloneTask ? "" : selectedCwd;
    void (async () => {
      await goto(cwd ? `${base}?folder=${encodeURIComponent(cwd)}` : base);
      window.dispatchEvent(
        new CustomEvent("agentcabin:new-chat", {
          detail: { cwd: cwd || undefined, codeStandaloneTask },
        }),
      );
    })();
  }

  function openTask(run: TaskRun) {
    void goto(getRunRoute(run, { runId: run.id }));
  }

  function statusLabel(run: TaskRun): string {
    switch (run.status) {
      case "pending":
        return "准备中";
      case "running":
        return "运行中";
      case "completed":
        return "已完成";
      case "failed":
        return "失败";
      case "stopped":
        return "已停止";
      case "cancelled":
        return "已取消";
      case "idle":
        return "待继续";
      default:
        return run.status;
    }
  }
</script>

<main class="mx-auto w-full max-w-5xl px-6 py-8 sm:px-8">
  <header class="mb-6 flex flex-wrap items-start justify-between gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight text-foreground">任务</h1>
      <p class="mt-1 text-sm text-muted-foreground">
        集中查看 Code 任务；新任务可以关联项目，也可以在应用隔离目录中独立运行。
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <select
        class="h-9 max-w-56 rounded-lg border border-border bg-background px-3 text-sm text-foreground"
        bind:value={selectedCwd}
        aria-label="新任务使用的项目"
      >
        <option value="">使用当前项目</option>
        <option value={STANDALONE_SCOPE}>不在项目中工作</option>
        {#each projects as cwd (cwd)}
          <option value={cwd}>{cwdDisplayLabel(cwd)}</option>
        {/each}
      </select>
      <button
        type="button"
        class="inline-flex min-h-9 items-center gap-2 rounded-lg bg-primary px-3.5 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90"
        onclick={createTask}
      >
        <span aria-hidden="true">＋</span> 新建任务
      </button>
    </div>
  </header>

  <section class="overflow-hidden rounded-xl border border-border bg-card">
    <div class="flex flex-wrap items-center gap-3 border-b border-border p-3">
      <input
        class="h-9 min-w-48 flex-1 rounded-lg border border-border bg-background px-3 text-sm outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-primary/30"
        type="search"
        placeholder="搜索任务或项目…"
        bind:value={search}
      />
      <select
        class="h-9 rounded-lg border border-border bg-background px-3 text-sm text-foreground"
        bind:value={filter}
        aria-label="按任务状态筛选"
      >
        <option value="all">全部</option>
        <option value="active">运行中</option>
        <option value="completed">已完成</option>
        <option value="needs_attention">失败或已停止</option>
      </select>
    </div>

    {#if loading}
      <div class="p-8 text-center text-sm text-muted-foreground">正在读取任务…</div>
    {:else if visibleRuns.length === 0}
      <div class="flex min-h-64 flex-col items-center justify-center px-6 text-center">
        <div class="text-sm font-medium text-foreground">
          {search || filter !== "all" ? "没有匹配的任务" : "还没有 Code 任务"}
        </div>
        <p class="mt-1 max-w-sm text-sm text-muted-foreground">
          {search || filter !== "all"
            ? "调整搜索词或筛选条件试试。"
            : "新建任务并发送第一条指令后，它会出现在这里。"}
        </p>
      </div>
    {:else}
      <ul class="divide-y divide-border">
        {#each visibleRuns as task (task.groupKey)}
          {@const run = task.latestRun}
          <li>
            <button
              type="button"
              class="flex w-full items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-accent/40"
              onclick={() => openTask(task.latestRun)}
            >
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm font-medium text-foreground"
                  >{task.title || "新任务"}</span
                >
                <span class="mt-1 block truncate text-xs text-muted-foreground"
                  >{task.projectLabel} · {run.agent}{task.runs.length > 1
                    ? ` · ${task.runs.length} 次运行`
                    : ""}</span
                >
              </span>
              <span class="shrink-0 rounded-full bg-muted px-2.5 py-1 text-xs text-muted-foreground"
                >{statusLabel(run)}</span
              >
              <time class="w-14 shrink-0 text-right text-xs text-muted-foreground"
                >{relativeTime(run.last_activity_at ?? run.started_at)}</time
              >
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</main>
