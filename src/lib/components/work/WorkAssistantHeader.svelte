<script lang="ts">
  import { fmtTime, fmtDateTime } from "$lib/i18n/format";
  import { getAgentDisplayName } from "$lib/utils/agent-metadata";

  let {
    timestamp,
    agent = "unknown",
  }: {
    timestamp?: string;
    agent?: string;
  } = $props();

  function formatTime(ts?: string): string {
    if (!ts) return "";
    const date = new Date(ts);
    if (Number.isNaN(date.getTime())) return "";
    const now = new Date();
    const sameDay =
      date.getFullYear() === now.getFullYear() &&
      date.getMonth() === now.getMonth() &&
      date.getDate() === now.getDate();
    return sameDay ? fmtTime(date) : fmtDateTime(date);
  }
</script>

<div class="w-full py-2">
  <div class="chat-content-width">
    <div class="flex items-center gap-2">
      <div
        class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full {agent === 'codex'
          ? 'bg-emerald-500/10 text-emerald-500'
          : 'bg-orange-500/10 text-orange-500'}"
        aria-hidden="true"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          {#if agent === "codex"}
            <polyline points="4 17 10 11 4 5" /><line x1="12" x2="20" y1="19" y2="19" />
          {:else}
            <path
              d="M12 3l1.912 5.813a2 2 0 0 0 1.275 1.275L21 12l-5.813 1.912a2 2 0 0 0-1.275 1.275L12 21l-1.912-5.813a2 2 0 0 0-1.275-1.275L3 12l5.813-1.912a2 2 0 0 0 1.275-1.275L12 3z"
            />
          {/if}
        </svg>
      </div>
      <span class="text-sm font-semibold text-foreground">{getAgentDisplayName(agent)}</span>
      {#if timestamp}
        <span class="text-[10px] text-muted-foreground">{formatTime(timestamp)}</span>
      {/if}
    </div>
  </div>
</div>
