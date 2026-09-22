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
      使用 Electron 内置的 Chromium 页面，通过受保护的原生 CDP 中继支持可视化页面操控与自动化测试。
    </p>
  </div>

  <BrowserUsePanel {runtime} {enabled} {onToggle} {onPrepare} />
</div>
