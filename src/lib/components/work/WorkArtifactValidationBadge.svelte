<script lang="ts">
  import { formatArtifactStatus } from "$lib/utils/work-result";
  import type { WorkArtifactSummary } from "$lib/types/work";

  interface Props {
    artifact: WorkArtifactSummary;
  }

  let { artifact }: Props = $props();

  const statusInfo = $derived(formatArtifactStatus(artifact.status));
  const passed = $derived(artifact.status === "validated" || artifact.status === "delivered");
  const failed = $derived(artifact.status === "invalid" || artifact.status === "failed");
</script>

<span
  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold {passed
    ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
    : failed
      ? 'bg-red-500/10 text-red-600 dark:text-red-400'
      : 'bg-amber-500/10 text-amber-700 dark:text-amber-300'}"
  title={artifact.validationSummary ?? statusInfo.tooltip}
>
  {#if passed}✓{/if}
  {statusInfo.label}
</span>
