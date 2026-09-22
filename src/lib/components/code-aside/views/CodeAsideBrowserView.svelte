<script lang="ts">
  import BrowserInspector from "$lib/components/browser/BrowserInspector.svelte";

  interface Props {
    runId?: string;
    url?: string;
  }

  let { runId = "code-browser", url = "" }: Props = $props();
</script>

<div class="h-full w-full overflow-hidden bg-background">
  <BrowserInspector {runId} mode="code" />
</div>
