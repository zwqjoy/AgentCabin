<script lang="ts">
  import type { PiExtensionHostState } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import { stripAnsi } from "$lib/utils/ansi";
  import PiExtensionWidgetContent from "$lib/components/PiExtensionWidgetContent.svelte";

  interface ExtensionNotice {
    noticeType: string;
    message: string;
    timestamp: number;
  }

  let {
    hostState,
    notice,
  }: {
    hostState?: PiExtensionHostState | null;
    notice?: ExtensionNotice | null;
  } = $props();

  // Work already owns dedicated surfaces for plan/todo/context/MCP state. Keep
  // those extension implementation details out of the generic host card.
  const hiddenSurfaceKeys = new Set([
    "plan-mode",
    "plan-mode-plan",
    "todo",
    "pi-deck-todo",
    "ask_user_question",
  ]);

  const activeStatuses = $derived.by(() => {
    if (!hostState?.statuses) return [];
    return Object.entries(hostState.statuses)
      .filter(([key]) => !hiddenSurfaceKeys.has(key) && key !== "context-prune" && key !== "mcp")
      .map(([key, text]) => ({ key, text: stripAnsi(text) }))
      .filter(({ text }) => text.length > 0);
  });

  const activeWidgets = $derived.by(() => {
    if (!hostState?.widgets) return [];
    return Object.values(hostState.widgets)
      .filter((widget) => !hiddenSurfaceKeys.has(widget.key))
      .filter((widget) => widget.lines.length > 0);
  });

  const hasSurface = $derived(
    Boolean(hostState?.title) || activeStatuses.length > 0 || activeWidgets.length > 0,
  );

  let visibleNotice = $state<ExtensionNotice | null>(null);

  // Match the ambient toast behavior used by the reference Extension UI host:
  // notifications are transient and must not become chat messages.
  $effect(() => {
    const nextNotice = notice;
    if (!nextNotice?.message) {
      visibleNotice = null;
      return;
    }

    visibleNotice = nextNotice;
    const timeout = window.setTimeout(() => {
      if (visibleNotice?.timestamp === nextNotice.timestamp) visibleNotice = null;
    }, 6000);

    return () => window.clearTimeout(timeout);
  });

  function noticeClasses(noticeType: string): string {
    switch (noticeType.toLowerCase()) {
      case "error":
        return "border-red-400/40 bg-red-500/10 text-red-700 dark:text-red-300";
      case "warning":
        return "border-amber-400/40 bg-amber-500/10 text-amber-700 dark:text-amber-300";
      case "success":
        return "border-emerald-400/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
      default:
        return "border-border/70 bg-background/95 text-foreground";
    }
  }
</script>

{#if visibleNotice}
  <div
    class="pointer-events-auto fixed right-4 top-4 z-[60] flex max-w-[min(28rem,calc(100vw-2rem))] items-start gap-3 rounded-xl border px-3.5 py-3 text-xs shadow-lg backdrop-blur-md animate-in fade-in slide-in-from-top-2 duration-200 {noticeClasses(
      visibleNotice.noticeType,
    )}"
    role={visibleNotice.noticeType === "error" ? "alert" : "status"}
    aria-live={visibleNotice.noticeType === "error" ? "assertive" : "polite"}
  >
    <span class="mt-0.5 shrink-0 text-sm" aria-hidden="true">
      {visibleNotice.noticeType === "error"
        ? "!"
        : visibleNotice.noticeType === "warning"
          ? "⚠"
          : visibleNotice.noticeType === "success"
            ? "✓"
            : "✦"}
    </span>
    <span class="min-w-0 flex-1 whitespace-pre-wrap break-words leading-5">
      {visibleNotice.message}
    </span>
    <button
      type="button"
      class="shrink-0 rounded-md px-1 text-current/60 transition-colors hover:bg-black/5 hover:text-current dark:hover:bg-white/10"
      aria-label={t("common_dismiss")}
      onclick={() => (visibleNotice = null)}
    >
      ×
    </button>
  </div>
{/if}

{#if hasSurface}
  <div class="shrink-0 px-4 pt-2 sm:px-5" data-pi-extension-surfaces>
    <div
      class="mx-auto w-full max-w-6xl rounded-2xl border border-primary/25 bg-card/80 p-3 text-xs shadow-sm"
    >
      {#if hostState?.title}
        <div class="mb-2 flex items-center gap-2 text-orange-500 dark:text-orange-300">
          <span
            class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-orange-500/15 font-bold"
            aria-hidden="true">✦</span
          >
          <span class="min-w-0 truncate font-medium">{hostState.title}</span>
        </div>
      {/if}

      {#if activeStatuses.length > 0}
        <div class="mb-2 flex flex-wrap gap-1.5" aria-label={t("infoPanel_status")}>
          {#each activeStatuses as status (status.key)}
            <div
              class="flex min-w-0 items-center gap-1.5 rounded-lg border border-border/60 bg-background/55 px-2 py-1 text-muted-foreground"
            >
              <span class="shrink-0 font-medium text-foreground/80">{status.key}</span>
              <span class="truncate">{status.text}</span>
            </div>
          {/each}
        </div>
      {/if}

      {#if activeWidgets.length > 0}
        <div class="grid gap-2 md:grid-cols-2">
          {#each activeWidgets as widget (widget.key)}
            <div class="min-w-0 rounded-lg border border-border/60 bg-background/55 p-2">
              {#if widget.key !== "default"}
                <div class="mb-1 truncate text-[11px] font-medium text-muted-foreground">
                  {widget.key}
                </div>
              {/if}
              <PiExtensionWidgetContent {widget} />
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}
