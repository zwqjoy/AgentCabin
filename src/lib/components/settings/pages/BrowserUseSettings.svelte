<script lang="ts">
  import BrowserUsePanel from "$lib/components/BrowserUsePanel.svelte";
  import type { WorkBrowserSummary } from "$lib/types/work";

  interface Props {
    runtime: WorkBrowserSummary | null;
    enabled: boolean;
    onToggle: (enabled: boolean) => Promise<WorkBrowserSummary | void>;
    onPrepare: () => Promise<WorkBrowserSummary>;
  }

  let { runtime, enabled, onToggle, onPrepare }: Props = $props();
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      浏览器自动化 (Browser Use)
    </h2>
    <p class="mt-1 text-xs text-muted-foreground">
      由 Pi Browser Extension 负责浏览器自动化执行，AgentCabin 负责 capability、Host 集成和 UX
      展示。
    </p>
  </div>

  <BrowserUsePanel {runtime} {enabled} {onToggle} {onPrepare} />
</div>
