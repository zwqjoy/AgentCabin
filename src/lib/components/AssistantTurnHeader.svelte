<script lang="ts">
  import { fmtTime, fmtDateTime } from "$lib/i18n/format";
  import type { ActiveExpertContext } from "$lib/utils/expert-context";

  let {
    timestamp,
    agent = "claude",
    displayName,
    expert = null,
  }: {
    timestamp?: string;
    agent?: string;
    displayName?: string;
    expert?: ActiveExpertContext | null;
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

<!--
  AssistantTurnHeader — only rendered for Expert context.
  For ordinary single-agent sessions the header is not displayed;
  Final Answer starts directly without an avatar or agent name label.
  This matches the DeepSeek Harness "content-first" visual principle.
-->
{#if expert}
  <div class="w-full py-1.5">
    <div class="chat-content-width">
      <div class="flex items-center gap-2">
        <div
          class="inline-flex items-center gap-2 rounded-lg border border-border/40 bg-muted/30 px-2.5 py-1"
        >
          <div
            class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full {expert.avatarBg ||
              'bg-muted-foreground/40'} text-[9px] font-bold text-white"
          >
            {expert.avatarChar || "专"}
          </div>
          <span class="text-xs font-medium text-foreground/80">{expert.title}</span>
          {#if expert.isTeam}
            <span
              class="rounded bg-muted/60 px-1.5 py-0.5 text-[9px] font-medium text-muted-foreground"
              >专家团队</span
            >
          {/if}
        </div>
        {#if timestamp}
          <span class="text-[11px] text-muted-foreground/50">{formatTime(timestamp)}</span>
        {/if}
      </div>
    </div>
  </div>
{/if}
<!-- Non-expert ordinary turns: no header rendered — Final Answer begins directly -->
