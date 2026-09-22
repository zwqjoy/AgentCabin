<script lang="ts">
  import ConversationMarkdown from "$lib/components/ConversationMarkdown.svelte";
  import TurnUsagePill from "$lib/components/chat/TurnUsagePill.svelte";
  import MessageActionBar from "$lib/components/chat/MessageActionBar.svelte";
  import type { TurnUsage } from "$lib/stores/types";

  let {
    content = "",
    isStreaming = false,
    model,
    durationFormatted,
    durationMs = 0,
    timestamp = "",
    usage = null,
    isLatest = false,
    onExport,
    onContinueFromMessage,
    workspaceId = "",
    basePath = "",
  }: {
    content: string;
    isStreaming?: boolean;
    model?: string;
    durationFormatted?: string;
    durationMs?: number;
    timestamp?: string;
    usage?: TurnUsage | null;
    isLatest?: boolean;
    onExport?: () => void;
    onContinueFromMessage?: () => void;
    workspaceId?: string;
    basePath?: string;
  } = $props();

  let hovered = $state(false);
</script>

<div
  class="w-full py-2.5 transition-colors"
  role="region"
  aria-label="Assistant response"
  onmouseenter={() => (hovered = true)}
  onmouseleave={() => (hovered = false)}
>
  <div class="chat-message-body prose-chat text-foreground">
    <ConversationMarkdown text={content} streaming={isStreaming} {workspaceId} {basePath} />
  </div>

  {#if !isStreaming && content.trim().length > 0}
    <div
      class="mt-2 flex flex-wrap items-center gap-1.5 transition-opacity duration-150 focus-within:opacity-100 {isLatest ||
      hovered
        ? 'opacity-100'
        : 'opacity-0'}"
    >
      <!-- Icon actions: copy / branch / export -->
      <MessageActionBar {content} {onExport} {onContinueFromMessage} />

      <!-- DSH-style Stats & Usage Pills (TurnUsage, TurnTime, Timestamp) right alongside actions -->
      <TurnUsagePill {usage} {model} {durationMs} {timestamp} />
    </div>
  {/if}
</div>
