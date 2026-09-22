<script lang="ts">
  import { formatWorkSchedule } from "$lib/utils/work-schedule";
  import type { WorkTask, WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    task: WorkTask;
    workspaces: WorkWorkspaceSummary[];
    triggering?: boolean;
    onToggleSchedule: (task: WorkTask) => void;
    onTriggerNow: (task: WorkTask) => void;
    onViewHistory: (task: WorkTask) => void;
    onEdit: (task: WorkTask) => void;
    onDelete: (task: WorkTask) => void;
    onDuplicate: (task: WorkTask) => void;
  }

  let {
    task,
    workspaces,
    triggering = false,
    onToggleSchedule,
    onTriggerNow,
    onViewHistory,
    onEdit,
    onDelete,
    onDuplicate,
  }: Props = $props();

  const isEnabled = $derived(Boolean(task.schedule?.enabled));
  const isRunning = $derived(task.status === "in_run");
  const isNeedsAttention = $derived(task.status === "needs_attention");
  const scheduleLabel = $derived(
    task.schedule ? formatWorkSchedule(task.schedule) : "未设置定时调度",
  );
  const workspaceName = $derived(
    workspaces.find((w) => w.id === task.workspaceId)?.name || task.workspaceId,
  );
</script>

<div
  class="group flex flex-col justify-between rounded-xl border border-border/70 bg-card/60 p-4 transition-all hover:border-border hover:bg-card/90 hover:shadow-sm"
>
  <!-- Card Top -->
  <div>
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0 flex-1">
        <div class="flex flex-wrap items-center gap-2">
          {#if isRunning}
            <span
              class="flex items-center gap-1 rounded-full bg-amber-500/10 border border-amber-500/20 px-2 py-0.5 text-[10px] font-semibold text-amber-600 dark:text-amber-400"
            >
              <span class="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse"></span>
              执行中
            </span>
          {:else if isNeedsAttention}
            <span
              class="flex items-center gap-1 rounded-full bg-orange-500/10 border border-orange-500/20 px-2 py-0.5 text-[10px] font-semibold text-orange-600 dark:text-orange-400"
            >
              <span class="h-1.5 w-1.5 rounded-full bg-orange-500"></span>
              需人工确认
            </span>
          {:else if isEnabled}
            <span
              class="flex items-center gap-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 px-2 py-0.5 text-[10px] font-semibold text-emerald-600 dark:text-emerald-400"
            >
              <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
              定时已激活
            </span>
          {:else}
            <span
              class="rounded-full bg-muted border border-border px-2 py-0.5 text-[10px] font-medium text-muted-foreground"
            >
              已暂停
            </span>
          {/if}

          <span
            class="rounded bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground truncate max-w-[140px]"
            title={workspaceName}
          >
            {workspaceName}
          </span>
        </div>

        <h3
          class="mt-2 text-sm font-semibold text-foreground leading-snug truncate"
          title={task.title}
        >
          {task.title}
        </h3>

        {#if task.instructions}
          <p class="mt-1 line-clamp-2 text-xs text-muted-foreground leading-relaxed">
            {task.instructions}
          </p>
        {/if}
      </div>

      <!-- Switch -->
      <div class="shrink-0 flex items-center gap-1.5">
        <label
          class="relative inline-flex cursor-pointer items-center"
          title={isEnabled ? "点击暂停调度" : "点击开启定时调度"}
        >
          <input
            type="checkbox"
            class="peer sr-only"
            checked={isEnabled}
            onchange={() => onToggleSchedule(task)}
          />
          <div
            class="h-5 w-9 rounded-full bg-muted transition-colors peer-checked:bg-primary peer-focus:outline-none after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:after:translate-x-full"
          ></div>
        </label>
      </div>
    </div>

    <!-- Schedule Info & Policy Badges -->
    <div
      class="mt-3.5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground border-t border-border/40 pt-2.5"
    >
      <div class="flex items-center gap-1.5 font-medium text-foreground/80">
        <span class="text-xs">⏱</span>
        <span>{scheduleLabel}</span>
      </div>

      {#if task.schedule?.nextRunAt && isEnabled}
        <span class="text-[10px] text-muted-foreground">
          (下次：{task.schedule.nextRunAt.slice(0, 16).replace("T", " ")})
        </span>
      {/if}

      {#if task.artifactRequirements && task.artifactRequirements.length > 0}
        <span class="rounded bg-muted/70 px-1.5 py-0.5 text-[9px] text-muted-foreground">
          {task.artifactRequirements.length} 项必需产物
        </span>
      {/if}
    </div>
  </div>

  <!-- Card Bottom Actions -->
  <div class="mt-4 flex items-center justify-between border-t border-border/50 pt-3">
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-lg border border-primary/40 bg-primary/10 px-2.5 py-1 text-xs font-semibold text-primary hover:bg-primary/20 disabled:opacity-50 transition-colors"
        disabled={triggering || isRunning}
        onclick={() => onTriggerNow(task)}
      >
        <span>{isRunning ? "执行中…" : triggering ? "启动中…" : "立即执行"}</span>
        {#if !isRunning && !triggering}
          <span class="text-[10px]">▶</span>
        {/if}
      </button>

      <button
        type="button"
        class="rounded-lg border border-border/70 px-2.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        onclick={() => onViewHistory(task)}
      >
        历史流水 ({task.runCount || 0})
      </button>
    </div>

    <div class="flex items-center gap-1">
      <button
        type="button"
        class="rounded p-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        title="编辑配置"
        onclick={() => onEdit(task)}
      >
        编辑
      </button>
      <button
        type="button"
        class="rounded p-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        title="复制任务配置"
        onclick={() => onDuplicate(task)}
      >
        复制
      </button>
      <button
        type="button"
        class="rounded p-1 text-xs text-red-500 hover:bg-red-500/10 transition-colors"
        title="删除任务"
        onclick={() => onDelete(task)}
      >
        删除
      </button>
    </div>
  </div>
</div>
