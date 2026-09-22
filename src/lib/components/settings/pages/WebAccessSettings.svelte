<script lang="ts">
  import WebAccessPanel from "$lib/components/WebAccessPanel.svelte";
  import type { WorkBrowserHealth, WorkBrowserSummary } from "$lib/types/work";

  interface Props {
    config: WorkBrowserSummary | null;
    enabled: boolean;
    onSave: (input: {
      provider?: string;
      enabled: boolean;
      maxResults: number;
      apiKey: string | null;
      endpointUrl?: string | null;
      allowedHosts?: string[] | null;
    }) => Promise<void>;
    onToggle: (enabled: boolean) => Promise<void>;
    onTest: () => Promise<WorkBrowserHealth>;
  }

  let { config, enabled, onSave, onToggle, onTest }: Props = $props();
</script>

<div class="space-y-6">
  <WebAccessPanel
    {config}
    {enabled}
    showSharedConfiguration={true}
    canToggle={true}
    {onSave}
    {onToggle}
    {onTest}
  />
</div>
