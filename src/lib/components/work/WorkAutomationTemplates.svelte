<script lang="ts">
  import {
    WORK_AUTOMATION_TEMPLATES,
    CODE_AUTOMATION_TEMPLATES,
    type WorkAutomationTemplate,
  } from "$lib/data/work-starters";

  interface Props {
    mode?: "work" | "code";
    onSelectTemplate: (template: WorkAutomationTemplate) => void;
  }

  let { mode = "work", onSelectTemplate }: Props = $props();

  const templates = $derived(
    mode === "code" ? CODE_AUTOMATION_TEMPLATES : WORK_AUTOMATION_TEMPLATES,
  );
  const subtitle = $derived(
    mode === "code"
      ? "从高频开发场景模板一键创建定时或周期性任务"
      : "从高频业务场景模板一键创建定时或周期性任务",
  );
</script>

<div class="rounded-xl border border-border/70 bg-card/40 p-4">
  <div class="flex items-center justify-between">
    <div>
      <h3 class="text-xs font-semibold text-foreground">快速新建任务</h3>
      <p class="mt-0.5 text-[11px] text-muted-foreground">
        {subtitle}
      </p>
    </div>
  </div>

  <div class="mt-3 grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-4">
    {#each templates as template (template.id)}
      <button
        type="button"
        class="group flex flex-col justify-between rounded-lg border border-border/60 bg-card/80 p-3 text-left transition-all hover:border-primary/60 hover:bg-primary/5 hover:shadow-xs"
        onclick={() => onSelectTemplate(template)}
      >
        <div>
          <div class="flex items-center justify-between gap-1.5">
            <span class="text-xs font-semibold text-foreground group-hover:text-primary">
              {template.title}
            </span>
            <span
              class="rounded bg-muted px-1.5 py-0.5 text-[9px] font-medium text-muted-foreground uppercase"
            >
              {template.scheduleKind}
            </span>
          </div>
          <p class="mt-1.5 line-clamp-2 text-[11px] leading-relaxed text-muted-foreground">
            {template.instructions}
          </p>
        </div>

        <div class="mt-2.5 flex items-center justify-between text-[10px] text-muted-foreground/80">
          <span>{template.time} 自动执行</span>
          <span
            class="font-semibold text-primary opacity-0 group-hover:opacity-100 transition-opacity"
          >
            套用模版 &rarr;
          </span>
        </div>
      </button>
    {/each}
  </div>
</div>
