<script lang="ts">
  import type { ActivityItem } from "$lib/utils/tool-activity-adapter";
  import ChatActivityItem from "./ChatActivityItem.svelte";

  let {
    activities = [],
    summaryLabel = "",
    iconKind = "terminal",
    hasRunning = false,
    hasFailed = false,
    runId = "",
    fetchToolResult,
    onPreviewFile,
    defaultExpanded = false,
    renderCustomActivity,
  }: {
    activities: ActivityItem[];
    summaryLabel?: string;
    iconKind?: ActivityItem["iconKind"];
    hasRunning?: boolean;
    hasFailed?: boolean;
    runId?: string;
    fetchToolResult?: (runId: string, toolUseId: string) => Promise<Record<string, unknown> | null>;
    onPreviewFile?: (path: string) => void;
    defaultExpanded?: boolean;
    renderCustomActivity?: import("svelte").Snippet<[activity: ActivityItem]>;
  } = $props();

  let userExpanded = $state<boolean | null>(null);

  const autoExpanded = $derived(Boolean(defaultExpanded || hasRunning));
  const isExpanded = $derived(userExpanded !== null ? userExpanded : autoExpanded);
</script>

<div class="flex flex-col py-1">
  <!-- Group header toggle button -->
  <button
    type="button"
    class="group/group-btn inline-flex items-center gap-2 rounded-md px-1.5 py-1 text-left text-[var(--chat-secondary-size,13px)] leading-normal text-muted-foreground/70 hover:bg-muted/30 hover:text-foreground transition-colors cursor-pointer select-none"
    aria-expanded={isExpanded}
    onclick={() => (userExpanded = !isExpanded)}
  >
    <!-- Group Icon: spinner when running, icon when done -->
    <span
      class="flex h-4 w-4 shrink-0 items-center justify-center {hasFailed
        ? 'text-rose-500'
        : 'text-muted-foreground/60 group-hover/group-btn:text-foreground'}"
    >
      {#if hasRunning}
        <!-- Subtle neutral spinner -->
        <span
          class="h-3 w-3 animate-spin rounded-full border-[1.5px] border-muted-foreground/40 border-t-transparent"
        ></span>
      {:else if iconKind === "terminal"}
        <span class="font-mono text-[10.5px] font-bold select-none">&gt;_</span>
      {:else if iconKind === "pencil"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
          <path d="m15 5 4 4" />
        </svg>
      {:else if iconKind === "book"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5Z" />
          <path d="M6 6h10" /><path d="M6 10h10" />
        </svg>
      {:else if iconKind === "search"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
        </svg>
      {:else if iconKind === "globe"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" />
          <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
          <path d="M2 12h20" />
        </svg>
      {:else if iconKind === "bot"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect width="18" height="14" x="3" y="6" rx="2" />
          <circle cx="9" cy="13" r="1" /><circle cx="15" cy="13" r="1" />
          <path d="M12 2v4" />
        </svg>
      {:else if iconKind === "wrench"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"
          />
        </svg>
      {:else if iconKind === "box"}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"
          />
          <path d="m3.3 7 8.7 5 8.7-5" />
          <path d="M12 22V12" />
        </svg>
      {:else}
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
        >
          <circle cx="12" cy="12" r="10" />
        </svg>
      {/if}
    </span>

    <!-- Summary Label -->
    <span class="font-normal text-[var(--chat-secondary-size,13px)]">
      {summaryLabel}
    </span>

    <!-- Running indicator: simple muted text instead of blue pulse -->
    {#if hasRunning}
      <span class="text-[11px] text-muted-foreground/50 font-normal">…</span>
    {/if}

    <!-- Chevron -->
    <svg
      class="h-3.5 w-3.5 text-muted-foreground/50 transition-transform duration-150 {isExpanded
        ? 'rotate-180'
        : ''}"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>

  <!-- Expanded list of activities -->
  {#if isExpanded}
    <div class="ml-2.5 pl-3 border-l border-border/40 space-y-0.5 my-1 animate-fade-in">
      {#each activities as activity (activity.id)}
        {#if renderCustomActivity}
          {@render renderCustomActivity(activity)}
        {:else}
          <ChatActivityItem {activity} {runId} {fetchToolResult} {onPreviewFile} />
        {/if}
      {/each}
    </div>
  {/if}
</div>
