<script lang="ts">
  import type { PanelTask, PiTodoState } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import type { UnifiedDiffSummary } from "$lib/utils/diff-stats";
  import { dbg } from "$lib/utils/debug";
  import TaskChecklistCard from "$lib/components/TaskChecklistCard.svelte";

  type Props = {
    /** Current task list (Tasks system or legacy TodoWrite). Empty hides the task hotspot. */
    tasks: PanelTask[];
    /** Pi9 phased todo state. This is rendered natively instead of as a raw extension widget. */
    piTodoState?: PiTodoState | null;
    /** Current-turn file changes shown from the trailing hotspot. */
    changeSummary?: UnifiedDiffSummary | null;
    onViewDiff?: () => void;
  };

  let { tasks, piTodoState = null, changeSummary = null, onViewDiff }: Props = $props();

  let todoPinned = $state(false);
  let changesPinned = $state(false);

  let phases = $derived(piTodoState?.phases.filter((phase) => phase.tasks.length > 0) ?? []);
  let piTasks = $derived(phases.flatMap((phase) => phase.tasks));
  let todoItems = $derived(
    piTasks.length > 0
      ? piTasks.map((task) => ({
          text: task.name,
          description: task.description,
          status: task.status,
        }))
      : tasks.map((task) => ({ text: task.text, description: "", status: task.status })),
  );
  let todoSections = $derived(
    phases.length > 0
      ? phases.map((phase) => ({
          id: phase.name,
          title: phase.name,
          tasks: phase.tasks.map((task, index) => ({
            id: `${phase.name}-${index}`,
            text: task.name,
            description: task.description,
            status: task.status,
          })),
        }))
      : tasks.length > 0
        ? [
            {
              id: "tasks",
              title: "",
              tasks: tasks.map((task) => ({ ...task, description: "" })),
            },
          ]
        : [],
  );
  let hasTodo = $derived(todoItems.length > 0);
  let doneCount = $derived(
    todoItems.filter((task) => task.status === "completed" || task.status === "cancelled").length,
  );
  let totalCount = $derived(todoItems.length);
  let currentStep = $derived.by(() => {
    const activeIndex = todoItems.findIndex((task) => task.status === "in_progress");
    if (activeIndex >= 0) return activeIndex + 1;
    return totalCount > 0 ? Math.min(doneCount + 1, totalCount) : 0;
  });
  let progressLabel = $derived(
    hasTodo ? t("todos_progress", { current: String(currentStep), total: String(totalCount) }) : "",
  );

  let changeFiles = $derived(changeSummary?.files ?? []);
  let hasChanges = $derived(changeFiles.length > 0);
  let filesLabel = $derived(
    changeFiles.length === 1
      ? t("sidebar_changedFile", { count: "1" })
      : t("sidebar_changedFiles", { count: String(changeFiles.length) }),
  );

  function viewDiff() {
    changesPinned = false;
    onViewDiff?.();
  }

  $effect(() => {
    if (tasks.length > 0) dbg("todo-panel", "render", { total: tasks.length, done: doneCount });
    if (!hasTodo && !hasChanges) {
      todoPinned = false;
      changesPinned = false;
    }
  });
</script>

{#if hasTodo || hasChanges}
  <div class="mx-auto w-full max-w-4xl px-4 pb-2">
    <div
      class="mx-auto flex w-fit max-w-full items-center rounded-full border border-border bg-background/95 p-0.5 text-xs text-muted-foreground shadow-sm backdrop-blur"
    >
      {#if hasTodo}
        <div class="trigger-group relative" class:pinned={todoPinned}>
          <button
            type="button"
            class="progress-trigger flex items-center gap-1.5 rounded-full px-2.5 py-1 transition-colors hover:bg-muted/70 hover:text-foreground"
            aria-haspopup="dialog"
            aria-expanded={todoPinned}
            onclick={() => (todoPinned = !todoPinned)}
          >
            <span
              class="h-3 w-3 shrink-0 rounded-full border border-primary/40 {todoItems.some(
                (task) => task.status === 'in_progress',
              )
                ? 'border-blue-500/70'
                : ''}"
              aria-hidden="true"
            ></span>
            <span class="whitespace-nowrap">{progressLabel}</span>
          </button>

          <div
            class="hover-popover absolute bottom-full left-0 z-50 w-[min(24rem,calc(100vw-2rem))] pb-2"
          >
            <div
              class="overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl backdrop-blur-md"
              role="dialog"
              aria-label={t("todos_panelHeader")}
            >
              <div class="flex items-center gap-2 border-b border-border/70 px-3 py-2.5">
                <span class="text-xs font-semibold">{t("todos_panelHeader")}</span>
                <span class="ml-auto text-[11px] text-muted-foreground/70"
                  >{doneCount}/{totalCount}</span
                >
              </div>

              <TaskChecklistCard tasks={[]} sections={todoSections} embedded />
            </div>
          </div>
        </div>
      {/if}

      {#if hasTodo && hasChanges}
        <span class="px-0.5 text-muted-foreground/45" aria-hidden="true">·</span>
      {/if}

      {#if hasChanges}
        <div class="trigger-group changes-group relative" class:pinned={changesPinned}>
          <button
            type="button"
            class="changes-trigger flex items-center gap-1.5 rounded-full px-2.5 py-1 transition-colors hover:bg-muted/70 hover:text-foreground"
            aria-haspopup="dialog"
            aria-expanded={changesPinned}
            onclick={() => (changesPinned = !changesPinned)}
          >
            <span class="whitespace-nowrap">{filesLabel}</span>
            <span class="font-mono text-[11px] tabular-nums text-emerald-600 dark:text-emerald-400"
              >+{changeSummary?.totalInsertions ?? 0}</span
            >
            <span class="font-mono text-[11px] tabular-nums text-red-500 dark:text-red-400"
              >-{changeSummary?.totalDeletions ?? 0}</span
            >
          </button>

          <div
            class="hover-popover absolute bottom-full right-0 z-50 w-[min(24rem,calc(100vw-2rem))] pb-2"
          >
            <div
              class="overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl backdrop-blur-md"
              role="dialog"
              aria-label={t("diff_turnDiff")}
            >
              <div class="flex items-center gap-2 border-b border-border/70 px-3 py-2.5">
                <span class="text-xs font-semibold">{t("diff_turnDiff")}</span>
                <span class="ml-auto flex items-center gap-2 font-mono text-[11px] tabular-nums">
                  <span class="text-emerald-600 dark:text-emerald-400"
                    >+{changeSummary?.totalInsertions ?? 0}</span
                  >
                  <span class="text-red-500 dark:text-red-400"
                    >-{changeSummary?.totalDeletions ?? 0}</span
                  >
                </span>
              </div>

              <div class="max-h-[min(22rem,60vh)] overflow-y-auto py-1">
                {#each changeFiles as file (file.path)}
                  <div
                    class="flex items-center gap-2 px-3 py-2 text-xs transition-colors hover:bg-accent/50"
                    title={file.path}
                  >
                    <svg
                      class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.7"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                      <path d="M14 2v6h6M8 13h8M8 17h6" />
                    </svg>
                    <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{file.path}</span>
                    <span
                      class="shrink-0 font-mono text-[11px] tabular-nums text-emerald-600 dark:text-emerald-400"
                      >+{file.insertions}</span
                    >
                    <span
                      class="shrink-0 font-mono text-[11px] tabular-nums text-red-500 dark:text-red-400"
                      >-{file.deletions}</span
                    >
                  </div>
                {/each}
              </div>

              <div
                class="flex items-center justify-between border-t border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
              >
                <span>{filesLabel}</span>
                {#if onViewDiff}
                  <button
                    type="button"
                    class="rounded-md px-1.5 py-1 text-[11px] transition-colors hover:bg-accent hover:text-foreground"
                    onclick={viewDiff}
                  >
                    {t("diff_viewFull")}
                  </button>
                {:else}
                  <span class="font-mono tabular-nums"
                    >+{changeSummary?.totalInsertions ?? 0} -{changeSummary?.totalDeletions ??
                      0}</span
                  >
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .hover-popover {
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transform: translateY(4px) scale(0.97);
    transform-origin: bottom left;
    transition:
      opacity 160ms cubic-bezier(0.23, 1, 0.32, 1),
      transform 160ms cubic-bezier(0.23, 1, 0.32, 1),
      visibility 0s linear 160ms;
  }

  .changes-group .hover-popover {
    transform-origin: bottom right;
  }

  .trigger-group:hover .hover-popover,
  .trigger-group:focus-within .hover-popover,
  .trigger-group.pinned .hover-popover {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transform: translateY(0) scale(1);
    transition-delay: 0s;
  }

  .progress-trigger:active,
  .changes-trigger:active {
    transform: scale(0.97);
  }

  @media (prefers-reduced-motion: reduce) {
    .hover-popover {
      transition: none;
    }
  }
</style>
