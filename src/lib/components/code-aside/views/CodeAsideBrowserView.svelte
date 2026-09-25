<script lang="ts">
  import BrowserInspector from "$lib/components/browser/BrowserInspector.svelte";

  interface Props {
    runId?: string;
    url?: string;
    surfaceVisible?: boolean;
  }

  let { runId = "code-browser", url = "", surfaceVisible = true }: Props = $props();
</script>

<div class="h-full w-full overflow-hidden bg-background">
  <BrowserInspector {runId} mode="code" {surfaceVisible} />
</div>
