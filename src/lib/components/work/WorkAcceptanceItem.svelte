<script lang="ts">
  import { formatCriterionStatus, formatVerifierType } from "$lib/utils/work-goal";
  import type { AcceptanceCriterion } from "$lib/types/work";

  interface Props {
    criterion: AcceptanceCriterion;
    index: number;
  }

  let { criterion, index }: Props = $props();

  const statusMeta = $derived(formatCriterionStatus(criterion.status));
  const verifierMeta = $derived(formatVerifierType(criterion.verifierType));
</script>

<div
  class="group flex flex-col gap-1 rounded-lg border border-border/50 bg-card/30 p-2.5 transition-colors hover:border-border"
>
  <div class="flex items-start gap-2.5">
    <span
      class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-xs font-bold {statusMeta.iconClass} bg-muted/40"
      aria-label={statusMeta.label}
    >
      {statusMeta.icon}
    </span>

    <div class="min-w-0 flex-1">
      <div class="flex flex-wrap items-center gap-1.5">
        <p class="text-xs font-medium text-foreground leading-snug">
          {criterion.description}
        </p>
        <span class="rounded px-1.5 py-0.5 text-[9px] font-medium {verifierMeta.badgeClass}">
          {verifierMeta.label}
        </span>
      </div>

      {#if criterion.targetRef}
        <p class="mt-0.5 font-mono text-[10px] text-muted-foreground/70 truncate">
          目标：{criterion.targetRef}
        </p>
      {/if}

      {#if criterion.failureReason}
        <p class="mt-1 text-[11px] text-red-500/90 dark:text-red-400/90">
          ✕ {criterion.failureReason}
        </p>
      {/if}

      {#if criterion.evidenceRefs.length > 0}
        <div class="mt-1 flex flex-wrap gap-1">
          {#each criterion.evidenceRefs as ref (ref)}
            <span
              class="rounded bg-muted/70 px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground"
              title="核验证据"
            >
              ✓ {ref}
            </span>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
