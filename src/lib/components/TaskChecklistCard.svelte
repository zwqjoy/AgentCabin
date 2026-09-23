<script lang="ts">
  type TaskStatus = "pending" | "in_progress" | "completed" | "cancelled";
  type ChecklistTask = {
    id: string;
    text: string;
    status: TaskStatus;
    description?: string;
  };
  type ChecklistSection = { id: string; title: string; tasks: ChecklistTask[] };

  interface Props {
    tasks: ChecklistTask[];
    sections?: ChecklistSection[];
    title?: string;
    initiallyExpanded?: boolean;
    embedded?: boolean;
  }

  let {
    tasks,
    sections = [],
    title = "任务",
    initiallyExpanded = true,
    embedded = false,
  }: Props = $props();

  let allTasks = $derived(
    sections.length > 0 ? sections.flatMap((section) => section.tasks) : tasks,
  );

  let expanded = $state(true);
  let initialized = $state(false);
  $effect(() => {
    if (initialized) return;
    initialized = true;
    expanded = initiallyExpanded;
  });
  let completedCount = $derived(
    allTasks.filter((task) => task.status === "completed" || task.status === "cancelled").length,
  );
  let pendingCount = $derived(allTasks.length - completedCount);
  let statusLabel = $derived(pendingCount > 0 ? `${pendingCount} 待处理` : "全部完成");

  function statusClass(status: TaskStatus): string {
    if (status === "completed") return "border-emerald-400 bg-emerald-500 text-white";
    if (status === "in_progress") return "border-primary/60 bg-primary/10 text-primary";
    if (status === "cancelled") return "border-border bg-muted text-muted-foreground";
    return "border-border bg-background text-muted-foreground";
  }
</script>

{#if allTasks.length > 0}
  <section
    class={embedded
      ? "space-y-3"
      : "overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm"}
  >
    {#if !embedded}
      <button
        type="button"
        class="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/40"
        aria-expanded={expanded}
        onclick={() => (expanded = !expanded)}
      >
        <svg
          viewBox="0 0 24 24"
          class="h-4 w-4 shrink-0 text-muted-foreground"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M8 6h13M8 12h13M8 18h13" />
          <path d="M3 6h.01M3 12h.01M3 18h.01" stroke-width="2.5" />
        </svg>
        <span class="text-sm font-semibold text-foreground">{title}</span>
        <span class="text-sm text-muted-foreground">{allTasks.length}</span>
        <span class="text-sm text-muted-foreground">{statusLabel}</span>
        <span
          class="ml-auto text-muted-foreground transition-transform {expanded ? 'rotate-180' : ''}"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path d="m6 9 6 6 6-6" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </span>
      </button>
    {/if}

    {#if embedded || expanded}
      <div
        class={embedded
          ? "max-h-[min(22rem,60vh)] space-y-3 overflow-y-auto"
          : "space-y-3 border-t border-border/60 bg-muted/20 px-4 py-3"}
      >
        {#each sections.length > 0 ? sections : [{ id: "tasks", title: "", tasks }] as section (section.id)}
          <div class="space-y-2">
            {#if section.title}<h3 class="text-xs font-semibold text-foreground/80">
                {section.title}
              </h3>{/if}
            {#each section.tasks as task, index (task.id)}
              <div class="flex items-start gap-3 text-sm leading-5">
                <span
                  class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[11px] {statusClass(
                    task.status,
                  )}"
                  aria-label={task.status === "completed" || task.status === "cancelled"
                    ? "已完成"
                    : task.status === "in_progress"
                      ? "进行中"
                      : "待处理"}
                >
                  {#if task.status === "completed"}
                    ✓
                  {:else if task.status === "cancelled"}
                    ×
                  {:else if task.status === "in_progress"}
                    <span class="h-2 w-2 rounded-full bg-primary animate-pulse"></span>
                  {:else}
                    {index + 1}
                  {/if}
                </span>
                <span
                  class="min-w-0 flex-1 {task.status === 'completed' || task.status === 'cancelled'
                    ? 'text-muted-foreground line-through'
                    : 'text-foreground'}">{task.text}</span
                >
                {#if task.description}
                  <span class="min-w-0 flex-1 text-xs text-muted-foreground"
                    >{task.description}</span
                  >
                {/if}
                {#if task.status === "in_progress"}
                  <span class="shrink-0 text-xs font-medium text-primary">进行中</span>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}
