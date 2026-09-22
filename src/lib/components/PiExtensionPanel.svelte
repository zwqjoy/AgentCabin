<script lang="ts">
  import type { PiExtensionHostState } from "$lib/types";
  import { stripAnsi } from "$lib/utils/ansi";

  let {
    hostState,
  }: {
    hostState?: PiExtensionHostState | null;
  } = $props();

  // Plan mode is already represented by the composer control and plan panel.
  // Do not duplicate the extension's generic status/widget surfaces here.
  const hiddenSurfaceKeys = new Set(["plan-mode", "plan-mode-plan", "todo", "pi-deck-todo"]);

  const activeStatuses = $derived.by(() => {
    if (!hostState?.statuses) return [];
    return Object.entries(hostState.statuses)
      .filter(([key]) => !hiddenSurfaceKeys.has(key) && key !== "context-prune" && key !== "mcp")
      .map(([key, text]) => ({ key, text: stripAnsi(text) }));
  });

  const activeWidgets = $derived.by(() => {
    if (!hostState?.widgets) return [];
    return Object.values(hostState.widgets).filter((widget) => !hiddenSurfaceKeys.has(widget.key));
  });
</script>

{#if hostState?.title || activeStatuses.length > 0 || activeWidgets.length > 0}
  <div class="space-y-2 w-full chat-content-width mx-auto py-1">
    <div class="rounded-xl border border-primary/30 bg-card p-3 text-xs shadow-md transition-all">
      <!-- Title -->
      {#if hostState?.title}
        <div class="mb-2 flex items-center justify-between">
          <div class="flex items-center gap-2 text-orange-400">
            <span
              class="flex h-5 w-5 items-center justify-center rounded-full bg-orange-500/15 font-bold"
              >✦</span
            >
            <span class="font-medium">{hostState.title}</span>
          </div>
        </div>
      {/if}

      <!-- Statuses -->
      {#if activeStatuses.length > 0}
        <div class="mb-2 flex flex-wrap gap-1.5">
          {#each activeStatuses as status (status.key)}
            <div
              class="rounded border border-border/60 bg-background/50 px-2 py-1 text-muted-foreground flex items-center gap-1.5"
            >
              <span class="font-medium text-xs text-foreground/80">{status.key}:</span>
              <span>{status.text}</span>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Widgets -->
      {#if activeWidgets.length > 0}
        <div class="space-y-2">
          {#each activeWidgets as widget (widget.key)}
            <div class="rounded border border-border/60 bg-background/50 p-2">
              {#if widget.key !== "default"}
                <div class="font-medium text-[11px] text-muted-foreground mb-1">{widget.key}</div>
              {/if}
              <pre
                class="max-h-32 overflow-auto whitespace-pre-wrap font-mono text-muted-foreground text-xs">{widget.lines.join(
                  "\n",
                )}</pre>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}
