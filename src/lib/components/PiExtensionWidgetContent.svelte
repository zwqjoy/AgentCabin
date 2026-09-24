<script lang="ts">
  import type { PiExtensionWidget } from "$lib/types";

  let { widget }: { widget: PiExtensionWidget } = $props();
</script>

<pre
  class="max-h-32 overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-5 text-muted-foreground">{widget.lines.join(
    "\n",
  )}</pre>
