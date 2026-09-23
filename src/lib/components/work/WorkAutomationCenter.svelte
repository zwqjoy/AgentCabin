<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { workTaskStore } from "$lib/stores/work-task-store.svelte";
  import {
    duplicateWorkTask,
    getWorkAutomationStats,
    getWorkTaskRuns,
    retryWorkTaskRun,
    startWorkRun,
  } from "$lib/api/work";
  import {
    buildWorkSchedule,
    parseWorkSchedule,
    WORK_WEEKDAY_OPTIONS,
    type WorkScheduleEditorKind,
  } from "$lib/utils/work-schedule";
  import {
    computeAutomationQuickStats,
    type AutomationQuickStats,
  } from "$lib/utils/work-automation";
  import WorkAutomationCard from "./WorkAutomationCard.svelte";
  import WorkAutomationTemplates from "./WorkAutomationTemplates.svelte";
  import WorkAutomationHistory from "./WorkAutomationHistory.svelte";
  import WorkLibraryPicker from "./WorkLibraryPicker.svelte";
  import type { WorkAutomationTemplate } from "$lib/data/work-starters";
  import type {
    WorkAutomationStats,
    WorkTask,
    WorkTaskRunSummary,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  interface Props {
    workspaceId?: string;
    workspaces?: WorkWorkspaceSummary[];
    mode?: "work" | "code";
    activeProjectId?: string;
  }

  let { workspaceId = "", workspaces = [], mode = "work", activeProjectId = "" }: Props = $props();

  const localTimezone = (() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    } catch {
      return "UTC";
    }
  })();

  let isGlobalView = $derived(workspaceId === "");
  let searchQuery = $state("");
  let filter = $state<"all" | "scheduled" | "in_run" | "needs_attention" | "paused">("all");
  let triggeringTaskId = $state("");

  let stats = $state<WorkAutomationStats | null>(null);

  // History Drawer State
  let historyTask = $state<WorkTask | null>(null);
  let historyRuns = $state<WorkTaskRunSummary[]>([]);
  let loadingHistory = $state(false);
  let actionError = $state("");

  // Create / Edit Modal State
  let showModal = $state(false);
  let showLibraryPicker = $state(false);
  let editingTask = $state<WorkTask | null>(null);
  let newTitle = $state("");
  let newInstructions = $state("");
  let newScheduled = $state(false);
  let newScheduleKind = $state<WorkScheduleEditorKind>("daily");
  let newScheduleTime = $state("09:00");
  let newScheduleTimezone = $state(localTimezone);
  let newWeeklyDays = $state<number[]>([1]);
  let newCustomCron = $state("");
  let newRequiredArtifacts = $state("");
  let newSelectedWorkspaceId = $state("");
  let saving = $state(false);
  let modalError = $state("");

  // Delete Modal State
  let taskToDelete = $state<WorkTask | null>(null);
  let deletingTask = $state(false);

  let rawTasks = $derived(
    workspaceId
      ? workTaskStore.tasks.filter((t) => t.workspaceId === workspaceId && t.status !== "archived")
      : mode === "code"
        ? workTaskStore.tasks.filter(
            (t) => t.status !== "archived" && workspaces.some((ws) => ws.id === t.workspaceId),
          )
        : workTaskStore.tasks.filter((t) => t.status !== "archived"),
  );

  let quickStats = $derived<AutomationQuickStats>(computeAutomationQuickStats(rawTasks));
  let pageTitle = $derived(isGlobalView ? "任务" : mode === "code" ? "项目任务" : "工作空间任务");
  let pageDescription = $derived(
    isGlobalView
      ? mode === "code"
        ? "管理 Code 任务；需要在定时运行时在任务中开启调度"
        : "管理任务；需要定时运行时在任务中开启调度"
      : mode === "code"
        ? "管理当前项目的 Code 任务，需要时开启定时调度"
        : "管理当前工作空间的任务，需要时开启定时调度",
  );

  let filteredTasks = $derived(
    rawTasks
      .filter((t) => {
        if (searchQuery.trim()) {
          const q = searchQuery.trim().toLowerCase();
          if (!t.title.toLowerCase().includes(q) && !t.instructions.toLowerCase().includes(q)) {
            return false;
          }
        }
        if (filter === "scheduled") return Boolean(t.schedule?.enabled);
        if (filter === "in_run") return t.status === "in_run";
        if (filter === "needs_attention") return t.status === "needs_attention";
        if (filter === "paused") return !t.schedule?.enabled && t.status !== "in_run";
        return true;
      })
      .sort((a, b) => (b.updatedAt || b.createdAt).localeCompare(a.updatedAt || a.createdAt)),
  );

  let taskSections = $derived(
    [
      { title: "运行中", tasks: filteredTasks.filter((task) => task.status === "in_run") },
      {
        title: "待处理",
        tasks: filteredTasks.filter((task) => task.status === "needs_attention"),
      },
      {
        title: "定时任务",
        tasks: filteredTasks.filter(
          (task) =>
            task.status !== "in_run" &&
            task.status !== "needs_attention" &&
            Boolean(task.schedule?.enabled),
        ),
      },
      {
        title: "其他任务",
        tasks: filteredTasks.filter(
          (task) =>
            task.status !== "in_run" &&
            task.status !== "needs_attention" &&
            !task.schedule?.enabled,
        ),
      },
    ].filter((section) => section.tasks.length > 0),
  );

  async function loadStats() {
    if (!isGlobalView) {
      stats = null;
      return;
    }
    try {
      stats = await getWorkAutomationStats();
    } catch {
      // ignore
    }
  }

  onMount(() => {
    if (workspaceId) {
      void workTaskStore.fetchTasks(workspaceId);
    } else {
      void workTaskStore.fetchTasks();
    }
    void loadStats();
  });

  async function handleToggleSchedule(task: WorkTask) {
    if (!task.schedule) {
      // Open modal to set schedule if none exists
      openEditModal(task);
      return;
    }
    const updated = {
      ...task,
      schedule: {
        ...task.schedule,
        enabled: !task.schedule.enabled,
      },
    };
    try {
      await workTaskStore.updateTask(updated);
      void loadStats();
    } catch {
      // ignore
    }
  }

  async function handleTriggerNow(task: WorkTask) {
    if (triggeringTaskId) return;
    triggeringTaskId = task.id;
    try {
      const run = await startWorkRun(task.id, undefined, "manual");
      if (run) {
        await workTaskStore.fetchTasks(workspaceId || undefined);
        void loadStats();
        // Optionally jump to the active run
        void goto(
          `/chat/work?workspace=${encodeURIComponent(task.workspaceId)}&run=${encodeURIComponent(run.id)}`,
        );
      }
    } catch {
      // ignore
    } finally {
      triggeringTaskId = "";
    }
  }

  function insertLibraryToInstructions(content: string, title: string) {
    const capped =
      content.length > 4000 ? `${content.slice(0, 4000)}\n…（内容过长，已截断）` : content;
    const block = content.trimStart().startsWith("<!-- 引用资料:")
      ? content.trim()
      : `【参考资料：${title}】\n${capped}`;
    newInstructions = newInstructions.trim() ? `${newInstructions.trim()}\n\n${block}` : block;
  }

  async function handleViewHistory(task: WorkTask) {
    historyTask = task;
    loadingHistory = true;
    historyRuns = [];
    try {
      historyRuns = await getWorkTaskRuns(task.id);
    } catch {
      historyRuns = [];
    } finally {
      loadingHistory = false;
    }
  }

  async function handleDuplicate(task: WorkTask) {
    actionError = "";
    try {
      await duplicateWorkTask(task.id);
      if (isGlobalView) {
        await workTaskStore.fetchTasks();
      } else {
        await workTaskStore.fetchTasks(workspaceId);
      }
      void loadStats();
    } catch (cause) {
      actionError = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function handleRetryRun(run: WorkTaskRunSummary) {
    if (!historyTask) return;
    actionError = "";
    try {
      const newRun = await retryWorkTaskRun(historyTask.id, run.runId);
      historyRuns = await getWorkTaskRuns(historyTask.id);
      void loadStats();
      void goto(
        `/chat/work?workspace=${encodeURIComponent(newRun.workspaceId)}&run=${encodeURIComponent(newRun.id)}`,
      );
      historyTask = null;
    } catch (cause) {
      actionError = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function handleOpenRunFromHistory(wsId: string, runId: string) {
    historyTask = null;
    void goto(`/chat/work?workspace=${encodeURIComponent(wsId)}&run=${encodeURIComponent(runId)}`);
  }

  function openCreateModal(template?: WorkAutomationTemplate) {
    modalError = "";
    editingTask = null;
    newTitle = template?.title ?? "";
    newInstructions = template?.instructions ?? "";
    newScheduled = Boolean(template);
    newScheduleKind = template?.scheduleKind ?? "daily";
    newScheduleTime = template?.time ?? "09:00";
    newScheduleTimezone = localTimezone;
    newWeeklyDays = template?.days ?? [1];
    newCustomCron = "";
    newRequiredArtifacts = "";
    newSelectedWorkspaceId =
      workspaceId ||
      (activeProjectId && workspaces.some((w) => w.id === activeProjectId)
        ? activeProjectId
        : (workspaces[0]?.id ?? ""));
    showModal = true;
  }

  function openEditModal(task: WorkTask) {
    modalError = "";
    editingTask = task;
    newTitle = task.title;
    newInstructions = task.instructions;
    newSelectedWorkspaceId = task.workspaceId;
    if (task.schedule) {
      newScheduled = true;
      const parsed = parseWorkSchedule(task.schedule);
      newScheduleKind = parsed.kind;
      newScheduleTime = parsed.time;
      newWeeklyDays = parsed.days;
      newScheduleTimezone = task.schedule.timezone || localTimezone;
      newCustomCron = task.schedule.cronExpression ?? "";
    } else {
      newScheduled = false;
      newScheduleKind = "daily";
      newScheduleTime = "09:00";
      newScheduleTimezone = localTimezone;
      newWeeklyDays = [1];
      newCustomCron = "";
    }
    newRequiredArtifacts = (task.requiredArtifacts || []).join(", ");
    showModal = true;
  }

  function closeModal() {
    showModal = false;
    editingTask = null;
    modalError = "";
  }

  async function handleSaveTask() {
    const title = newTitle.trim();
    const targetWsId = isGlobalView ? newSelectedWorkspaceId : workspaceId;
    if (!title || !targetWsId) return;

    saving = true;
    modalError = "";
    try {
      const schedule = buildWorkSchedule(
        newScheduled,
        newScheduleKind,
        newScheduleTime,
        newWeeklyDays,
        newCustomCron,
        newScheduleTimezone,
      );

      const reqs = [
        ...new Set(
          newRequiredArtifacts
            .split(/\r?\n|,/u)
            .map((item) => item.trim())
            .filter(Boolean),
        ),
      ];

      if (editingTask) {
        await workTaskStore.updateTask({
          ...editingTask,
          title,
          instructions: newInstructions.trim(),
          schedule,
          requiredArtifacts: reqs,
        });
      } else {
        await workTaskStore.createTask(
          targetWsId,
          title,
          newInstructions.trim(),
          undefined,
          schedule,
          reqs,
        );
      }

      if (isGlobalView) {
        await workTaskStore.fetchTasks();
      } else {
        await workTaskStore.fetchTasks(workspaceId);
      }
      void loadStats();
      closeModal();
    } catch (e) {
      modalError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function confirmDeleteTask() {
    if (!taskToDelete) return;
    deletingTask = true;
    try {
      await workTaskStore.deleteTask(taskToDelete.id);
      taskToDelete = null;
      if (isGlobalView) {
        await workTaskStore.fetchTasks();
      } else {
        await workTaskStore.fetchTasks(workspaceId);
      }
      void loadStats();
    } catch {
      // ignore
    } finally {
      deletingTask = false;
    }
  }
</script>

<div class="flex h-full w-full flex-col overflow-y-auto p-4 sm:p-6 space-y-6">
  <!-- Top Header & Metrics Banner -->
  <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
    <div>
      <div class="flex items-center gap-2">
        <h1 class="text-lg font-bold text-foreground">{pageTitle}</h1>
      </div>
      <p class="mt-1 text-xs text-muted-foreground">
        {pageDescription}
      </p>
    </div>

    <button
      type="button"
      class="inline-flex items-center gap-1.5 rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground shadow-xs transition-all hover:bg-primary/90"
      onclick={() => openCreateModal()}
    >
      <span class="text-sm font-bold">+</span>
      <span>新建任务</span>
    </button>
  </div>

  <!-- Metric Badges -->
  <div class="grid grid-cols-2 gap-3 {isGlobalView ? 'sm:grid-cols-4' : 'sm:grid-cols-3'}">
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">定时已激活</span>
      <div class="mt-1 flex items-baseline gap-1.5">
        <span class="text-xl font-bold text-emerald-600 dark:text-emerald-400">
          {quickStats.scheduled}
        </span>
        <span class="text-[10px] text-muted-foreground">/ {quickStats.total} 总计</span>
      </div>
    </div>

    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">正在执行中</span>
      <div class="mt-1 flex items-baseline gap-1.5">
        <span class="text-xl font-bold text-amber-600 dark:text-amber-400">
          {quickStats.inRun}
        </span>
        <span class="text-[10px] text-muted-foreground">个任务</span>
      </div>
    </div>

    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">需人工确认</span>
      <div class="mt-1 flex items-baseline gap-1.5">
        <span class="text-xl font-bold text-orange-600 dark:text-orange-400">
          {quickStats.needsAttention}
        </span>
        <span class="text-[10px] text-muted-foreground">项待办</span>
      </div>
    </div>

    {#if isGlobalView}
      <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
        <span class="text-[11px] text-muted-foreground font-medium block">今日已完成</span>
        <div class="mt-1 flex items-baseline gap-1.5">
          <span class="text-xl font-bold text-foreground">
            {stats?.completedRunsToday ?? 0}
          </span>
          <span class="text-[10px] text-muted-foreground">次运行</span>
        </div>
      </div>
    {/if}
  </div>

  {#if rawTasks.some((task) => task.schedule?.enabled)}
    <p
      class="rounded-lg border border-indigo-500/25 bg-indigo-500/[0.06] px-3 py-2 text-[11px] text-indigo-700 dark:text-indigo-300"
    >
      定时任务由本机执行；应用退出后不会继续运行。
    </p>
  {/if}

  <!-- Templates -->
  <WorkAutomationTemplates {mode} onSelectTemplate={(t) => openCreateModal(t)} />

  <!-- Filter & Search Toolbar -->
  {#if actionError}
    <div
      class="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-400"
      role="alert"
    >
      {actionError}
    </div>
  {/if}

  <div
    class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between border-t border-border/60 pt-4"
  >
    <div class="flex flex-wrap items-center gap-1.5">
      <button
        type="button"
        class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {filter === 'all'
          ? 'bg-secondary text-secondary-foreground font-semibold'
          : 'text-muted-foreground hover:bg-accent'}"
        onclick={() => (filter = "all")}
      >
        全部 ({quickStats.total})
      </button>
      <button
        type="button"
        class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {filter === 'scheduled'
          ? 'bg-secondary text-secondary-foreground font-semibold'
          : 'text-muted-foreground hover:bg-accent'}"
        onclick={() => (filter = "scheduled")}
      >
        已激活 ({quickStats.scheduled})
      </button>
      <button
        type="button"
        class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {filter === 'in_run'
          ? 'bg-secondary text-secondary-foreground font-semibold'
          : 'text-muted-foreground hover:bg-accent'}"
        onclick={() => (filter = "in_run")}
      >
        执行中 ({quickStats.inRun})
      </button>
      <button
        type="button"
        class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {filter ===
        'needs_attention'
          ? 'bg-secondary text-secondary-foreground font-semibold'
          : 'text-muted-foreground hover:bg-accent'}"
        onclick={() => (filter = "needs_attention")}
      >
        需确认 ({quickStats.needsAttention})
      </button>
      <button
        type="button"
        class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {filter === 'paused'
          ? 'bg-secondary text-secondary-foreground font-semibold'
          : 'text-muted-foreground hover:bg-accent'}"
        onclick={() => (filter = "paused")}
      >
        已暂停
      </button>
    </div>

    <div class="relative w-full sm:w-64">
      <input
        type="text"
        placeholder="搜索任务名称或指令…"
        class="w-full rounded-xl border border-border bg-background px-3 py-1.5 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:ring-1 focus:ring-primary/20"
        bind:value={searchQuery}
      />
    </div>
  </div>

  <!-- Tasks are grouped by the next useful action; editing remains a secondary action. -->
  {#if filteredTasks.length === 0}
    <div
      class="flex h-56 flex-col items-center justify-center rounded-2xl border border-dashed border-border/80 p-8 text-center text-xs text-muted-foreground"
    >
      <span class="text-2xl">🤖</span>
      <span class="mt-2 font-medium">
        {mode === "code" ? "没有匹配的 Code 任务" : "没有匹配的任务"}
      </span>
      <p class="mt-1 text-[11px] opacity-75">
        {mode === "code"
          ? "点击上方「新建任务」或从推荐场景中直接套用模板创建 Code 任务。"
          : "点击上方「新建任务」或从推荐场景中直接套用模板创建。"}
      </p>
    </div>
  {:else}
    <div class="space-y-7">
      {#each taskSections as section (section.title)}
        <section aria-labelledby={`task-section-${section.title}`}>
          <h2
            id={`task-section-${section.title}`}
            class="mb-2 text-sm font-semibold text-foreground"
          >
            {section.title}
            <span class="font-normal text-muted-foreground">{section.tasks.length}</span>
          </h2>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            {#each section.tasks as task (task.id)}
              <WorkAutomationCard
                {task}
                {workspaces}
                triggering={triggeringTaskId === task.id}
                onToggleSchedule={handleToggleSchedule}
                onTriggerNow={handleTriggerNow}
                onViewHistory={handleViewHistory}
                onEdit={openEditModal}
                onDelete={(t) => (taskToDelete = t)}
                onDuplicate={handleDuplicate}
              />
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<!-- History Drawer -->
{#if historyTask}
  <WorkAutomationHistory
    taskTitle={historyTask.title}
    runs={historyRuns}
    loading={loadingHistory}
    onClose={() => (historyTask = null)}
    onOpenRun={handleOpenRunFromHistory}
    onRetry={handleRetryRun}
  />
{/if}

<!-- Create / Edit Modal -->
{#if showModal}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 backdrop-blur-[2px]"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.target === e.currentTarget && closeModal()}
  >
    <div class="w-full max-w-lg rounded-2xl border border-border bg-card p-6 shadow-2xl space-y-5">
      <div class="flex items-start justify-between gap-4">
        <div>
          <h2 class="text-base font-semibold text-foreground">
            {editingTask
              ? mode === "code"
                ? "编辑 Code 任务"
                : "编辑任务"
              : mode === "code"
                ? "新建 Code 任务"
                : "新建任务"}
          </h2>
          <p class="mt-0.5 text-xs text-muted-foreground">
            {mode === "code"
              ? "配置 Code 任务的目标、执行指令、定时调度与产物验收标准"
              : "配置任务的目标、执行指令、定时调度与产物验收标准"}
          </p>
        </div>
        <button
          type="button"
          class="rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={closeModal}
        >
          ✕
        </button>
      </div>

      <form
        class="space-y-4"
        onsubmit={(e) => {
          e.preventDefault();
          void handleSaveTask();
        }}
      >
        {#if isGlobalView && !editingTask}
          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1" for="ws-select">
              {mode === "code" ? "所属项目" : "所属工作空间"}
            </label>
            <select
              id="ws-select"
              class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
              bind:value={newSelectedWorkspaceId}
            >
              {#each workspaces as ws (ws.id)}
                <option value={ws.id}>{ws.name}</option>
              {/each}
            </select>
          </div>
        {/if}

        <div>
          <label class="block text-xs font-medium text-muted-foreground mb-1" for="task-title">
            任务标题 *
          </label>
          <input
            id="task-title"
            class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
            placeholder={mode === "code"
              ? "例如：每日定时运行自动化测试套件"
              : "例如：每日全网竞品价格监控并生成周报"}
            bind:value={newTitle}
            required
          />
        </div>

        <div>
          <label
            class="block text-xs font-medium text-muted-foreground mb-1"
            for="task-instructions"
          >
            执行指令与目标细节
            <button
              type="button"
              class="float-right text-[11px] font-medium text-primary hover:underline"
              onclick={() => (showLibraryPicker = true)}
            >
              📚 从资料库引用
            </button>
          </label>
          <textarea
            id="task-instructions"
            rows="3"
            class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
            placeholder={mode === "code"
              ? "描述 Agent 每次运行时在项目中要执行的操作和关注重点…"
              : "描述 Agent 每次运行时要执行的操作和关注重点…"}
            bind:value={newInstructions}
          ></textarea>
        </div>

        <!-- Schedule Section -->
        <div class="rounded-xl border border-border/60 bg-muted/20 p-3.5 space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-foreground">开启定时调度</span>
            <label class="relative inline-flex cursor-pointer items-center">
              <input type="checkbox" class="peer sr-only" bind:checked={newScheduled} />
              <div
                class="h-5 w-9 rounded-full bg-muted transition-colors peer-checked:bg-primary peer-focus:outline-none after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:after:translate-x-full"
              ></div>
            </label>
          </div>

          {#if newScheduled}
            <div class="space-y-3 border-t border-border/40 pt-3">
              <div class="grid grid-cols-3 gap-2">
                <button
                  type="button"
                  class="rounded-lg border py-1.5 text-xs font-medium transition-colors {newScheduleKind ===
                  'daily'
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border bg-card text-muted-foreground'}"
                  onclick={() => (newScheduleKind = "daily")}
                >
                  每天
                </button>
                <button
                  type="button"
                  class="rounded-lg border py-1.5 text-xs font-medium transition-colors {newScheduleKind ===
                  'custom_weekly'
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border bg-card text-muted-foreground'}"
                  onclick={() => (newScheduleKind = "custom_weekly")}
                >
                  每周
                </button>
                <button
                  type="button"
                  class="rounded-lg border py-1.5 text-xs font-medium transition-colors {newScheduleKind ===
                  'custom_cron'
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border bg-card text-muted-foreground'}"
                  onclick={() => (newScheduleKind = "custom_cron")}
                >
                  Cron 表达式
                </button>
              </div>

              {#if newScheduleKind === "daily" || newScheduleKind === "custom_weekly" || newScheduleKind === "weekdays"}
                <div class="flex items-center gap-3">
                  <div class="flex-1">
                    <label class="block text-[10px] text-muted-foreground mb-1" for="time-picker">
                      触发时间 (24小时制)
                    </label>
                    <input
                      id="time-picker"
                      type="time"
                      class="w-full rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs text-foreground"
                      bind:value={newScheduleTime}
                    />
                  </div>
                </div>
              {/if}

              {#if newScheduleKind === "custom_weekly"}
                <div>
                  <label class="block text-[10px] text-muted-foreground mb-1.5">
                    每周哪几天执行
                  </label>
                  <div class="flex flex-wrap gap-1.5">
                    {#each WORK_WEEKDAY_OPTIONS as opt (opt.value)}
                      <button
                        type="button"
                        class="rounded-md border px-2 py-1 text-[11px] font-medium transition-colors {newWeeklyDays.includes(
                          opt.value,
                        )
                          ? 'border-primary bg-primary text-primary-foreground'
                          : 'border-border bg-card text-muted-foreground'}"
                        onclick={() => {
                          if (newWeeklyDays.includes(opt.value)) {
                            if (newWeeklyDays.length > 1) {
                              newWeeklyDays = newWeeklyDays.filter((d) => d !== opt.value);
                            }
                          } else {
                            newWeeklyDays = [...newWeeklyDays, opt.value];
                          }
                        }}
                      >
                        {opt.label}
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}

              {#if newScheduleKind === "custom_cron"}
                <div>
                  <label class="block text-[10px] text-muted-foreground mb-1" for="cron-input">
                    Cron 表达式 (分 时 日 月 周)
                  </label>
                  <input
                    id="cron-input"
                    class="w-full rounded-lg border border-border bg-background px-2.5 py-1.5 font-mono text-xs text-foreground"
                    placeholder="0 9 * * 1-5"
                    bind:value={newCustomCron}
                  />
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <div>
          <label class="block text-xs font-medium text-muted-foreground mb-1" for="req-artifacts">
            必需交付成果物（每行一个，也支持逗号分隔）
          </label>
          <textarea
            id="req-artifacts"
            rows="2"
            class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
            placeholder={mode === "code"
              ? "例如：output/weekly-dev-report.md, output/test-summary.md"
              : "例如：output/weekly-report.pdf\\noutput/data.xlsx"}
            bind:value={newRequiredArtifacts}
          ></textarea>
        </div>

        {#if modalError}
          <div
            class="rounded-lg border border-red-500/30 bg-red-500/10 p-2 text-xs text-red-600 dark:text-red-400"
          >
            {modalError}
          </div>
        {/if}

        <div class="flex items-center justify-end gap-2 border-t border-border/60 pt-4">
          <button
            type="button"
            class="rounded-xl border border-border px-4 py-2 text-xs text-foreground hover:bg-accent"
            onclick={closeModal}
          >
            取消
          </button>
          <button
            type="submit"
            class="rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            disabled={saving || !newTitle.trim()}
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<!-- Delete Confirm Modal -->
{#if taskToDelete}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 backdrop-blur-[2px]"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="w-full max-w-sm rounded-2xl border border-border bg-card p-5 shadow-2xl space-y-4">
      <h3 class="text-sm font-semibold text-foreground">确认删除任务？</h3>
      <p class="text-xs text-muted-foreground">
        任务「{taskToDelete.title}」将被永久删除，其历史运行记录将保留在日志中。
      </p>
      <div class="flex items-center justify-end gap-2 pt-2">
        <button
          type="button"
          class="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground hover:bg-accent"
          onclick={() => (taskToDelete = null)}
        >
          取消
        </button>
        <button
          type="button"
          class="rounded-lg bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-700 disabled:opacity-50"
          disabled={deletingTask}
          onclick={confirmDeleteTask}
        >
          {deletingTask ? "删除中…" : "确认删除"}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showLibraryPicker}
  <WorkLibraryPicker
    workspaceId={newSelectedWorkspaceId}
    onInsert={insertLibraryToInstructions}
    onClose={() => (showLibraryPicker = false)}
  />
{/if}
