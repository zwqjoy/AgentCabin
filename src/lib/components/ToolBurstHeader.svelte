<script lang="ts">
  import type { MessageKey } from "$lib/i18n/types";
  import { t } from "$lib/i18n/index.svelte";
  import {
    getToolActivityKind,
    isToolTerminal,
    type ToolActivityKind,
    type ToolBurst,
  } from "$lib/utils/tool-rendering";
  import { getToolColor } from "$lib/utils/tool-colors";

  const ACTION_LABELS = {
    read: { active: "toolBurst_reading", done: "toolBurst_read" },
    search: { active: "toolBurst_searching", done: "toolBurst_searched" },
    command: { active: "toolBurst_runningCommand", done: "toolBurst_ranCommand" },
    edit: { active: "toolBurst_editing", done: "toolBurst_edited" },
    web: { active: "toolBurst_browsing", done: "toolBurst_browsed" },
    skill: { active: "toolBurst_loading", done: "toolBurst_loaded" },
    other: { active: "toolBurst_working", done: "toolBurst_worked" },
  } as const satisfies Record<ToolActivityKind, { active: MessageKey; done: MessageKey }>;

  let {
    burst,
    collapsed,
    onToggle,
  }: {
    burst: ToolBurst;
    collapsed: boolean;
    onToggle: () => void;
  } = $props();

  let hasActiveTool = $derived(burst.tools.some((tool) => !isToolTerminal(tool.status)));

  let actionKinds = $derived.by(() => {
    const kinds: ToolActivityKind[] = [];
    for (const tool of burst.tools) {
      const kind = getToolActivityKind(tool.tool_name);
      if (!kinds.includes(kind)) kinds.push(kind);
    }
    return kinds;
  });

  let actionSummary = $derived.by(() =>
    actionKinds
      .map((kind) => {
        const categoryHasActiveTool = burst.tools.some(
          (tool) => getToolActivityKind(tool.tool_name) === kind && !isToolTerminal(tool.status),
        );
        return t(ACTION_LABELS[kind][categoryHasActiveTool ? "active" : "done"]);
      })
      .join(" · "),
  );

  // Prefer the active/latest action so the icon describes what the agent is doing now.
  let primaryToolName = $derived(
    [...burst.tools].reverse().find((tool) => !isToolTerminal(tool.status))?.tool_name ??
      burst.tools[burst.tools.length - 1]?.tool_name ??
      "",
  );
  let iconStyle = $derived(getToolColor(primaryToolName));
</script>

<button
  class="w-full rounded-md px-2 py-1 text-left transition-colors hover:bg-muted/40"
  aria-expanded={!collapsed}
  aria-label={actionSummary}
  onclick={onToggle}
>
  <div class="flex min-h-5 items-center gap-1.5">
    <span class="inline-block w-3 shrink-0 text-center text-[10px] text-muted-foreground/60">
      {collapsed ? "\u25b8" : "\u25be"}
    </span>

    <span
      class="flex h-4 w-4 shrink-0 items-center justify-center {iconStyle.text}"
      aria-hidden="true"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"><path d={iconStyle.icon} /></svg
      >
    </span>

    <span
      class="min-w-0 truncate text-xs {hasActiveTool
        ? 'text-foreground/80'
        : 'text-muted-foreground'}"
    >
      {actionSummary}
    </span>

    {#if hasActiveTool}
      <span
        class="ml-0.5 h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-blue-400"
        aria-hidden="true"
      ></span>
    {/if}
  </div>
</button>
