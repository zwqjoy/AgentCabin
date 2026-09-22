<script lang="ts">
  import type { RunStatus } from "$lib/types";

  type DisplayStatus = Exclude<RunStatus, "idle"> | "waiting" | "done";

  let {
    status,
    attention = false,
    class: className = "",
  }: {
    status: RunStatus;
    attention?: boolean;
    class?: string;
  } = $props();

  const displayStatus: DisplayStatus = $derived(
    (status === "running" || status === "idle") && attention
      ? "waiting"
      : status === "idle"
        ? "done"
        : status,
  );

  const dotColors: Record<DisplayStatus, string> = {
    pending: "bg-amber-500",
    running: "bg-blue-500 animate-pulse",
    done: "bg-foreground/30",
    waiting: "bg-amber-500 animate-pulse",
    completed: "bg-foreground/30",
    failed: "bg-red-500",
    cancelled: "bg-foreground/20",
    stopped: "bg-foreground/20",
  };
</script>

{#if displayStatus === "running"}
  <!-- Running: Spinning spinner -->
  <span
    class="inline-flex items-center justify-center rounded-full bg-blue-500/10 p-1 text-blue-500 dark:text-blue-400 {className}"
    title="running"
    aria-label="running"
  >
    <svg class="h-3 w-3 animate-spin" viewBox="0 0 24 24" fill="none">
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3"
      ></circle>
      <path
        class="opacity-90"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      ></path>
    </svg>
  </span>
{:else if displayStatus === "waiting"}
  <!-- Waiting: Pulsing bell/alert icon (requires user action) -->
  <span
    class="inline-flex items-center justify-center rounded-full bg-amber-500/15 p-1 text-amber-500 dark:text-amber-400 animate-pulse {className}"
    title="waiting for input"
    aria-label="waiting for input"
  >
    <svg
      class="h-3 w-3"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="12" x2="12" y1="8" y2="12" />
      <line x1="12" x2="12.01" y1="16" y2="16" />
    </svg>
  </span>
{:else if displayStatus === "pending"}
  <!-- Pending: Clock icon -->
  <span
    class="inline-flex items-center justify-center rounded-full bg-amber-500/10 p-1 text-amber-500 dark:text-amber-400 {className}"
    title="pending"
    aria-label="pending"
  >
    <svg
      class="h-3 w-3 animate-pulse"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  </span>
{:else if displayStatus === "failed"}
  <!-- Failed: Red X icon -->
  <span
    class="inline-flex items-center justify-center rounded-full bg-red-500/10 p-1 text-red-500 dark:text-red-400 {className}"
    title="failed"
    aria-label="failed"
  >
    <svg
      class="h-3 w-3"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="15" x2="9" y1="9" y2="15" />
      <line x1="9" x2="15" y1="9" y2="15" />
    </svg>
  </span>
{:else}
  <!-- Done / Completed / Stopped: Minimal dot -->
  <span class="inline-flex items-center {className}" title={displayStatus}>
    <span class="h-1.5 w-1.5 rounded-full {dotColors[displayStatus]}"></span>
  </span>
{/if}
