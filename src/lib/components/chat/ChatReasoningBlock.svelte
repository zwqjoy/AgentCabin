<script lang="ts">
  import { extractLatestThinkingLine } from "$lib/utils/chat-presentation";
  import ConversationMarkdown from "$lib/components/ConversationMarkdown.svelte";

  let {
    content = "",
    isStreaming = false,
    isNarration = false,
  }: {
    content: string;
    isStreaming?: boolean;
    isNarration?: boolean;
  } = $props();

  let isExpanded = $state(false);

  // Extract single-line summary: latest line when streaming, first meaningful line when settled
  const summary = $derived.by(() => {
    if (!content) return "";
    if (isStreaming) {
      return extractLatestThinkingLine(content);
    }
    const lines = content
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    return lines[0] ?? "";
  });
</script>

{#if isNarration}
  <!-- DSH interim narration: normal assistant prose text between tool actions.
       Rendered through ConversationMarkdown (not plain MarkdownContent) so an
       ```html-preview fence inside narration is split out into the live preview
       card and builds up progressively while streaming, instead of showing the
       raw source as a code block until message_complete lands. `streaming` keeps
       rendering throttled during deltas; `lazy={false}` because these blocks are
       always on screen (turn is expanded while running). -->
  <div class="chat-message-body prose-chat py-1.5 select-text">
    <ConversationMarkdown text={content} streaming={isStreaming} lazy={false} />
  </div>
{:else}
  <!-- Reasoning row: clean flat chevron-only toggle, no atom icon, no blue pulse -->
  <div class="flex flex-col py-0.5 group/reason">
    <button
      type="button"
      class="flex items-center h-6 min-w-0 w-full text-left cursor-pointer rounded select-none text-[13px] text-muted-foreground/60 hover:text-foreground transition-colors"
      aria-expanded={isExpanded}
      onclick={() => (isExpanded = !isExpanded)}
    >
      <!-- Chevron only — no atom icon -->
      <span
        class="w-4 h-4 mr-1.5 flex items-center justify-center shrink-0 text-muted-foreground/45 group-hover/reason:text-muted-foreground transition-transform duration-150 {isExpanded
          ? ''
          : '-rotate-90'}"
      >
        <svg
          class="h-3 w-3"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </span>

      <!-- Title: 思考 -->
      <span
        class="shrink-0 font-normal text-muted-foreground/60 group-hover/reason:text-muted-foreground"
        >思考</span
      >

      <!-- Dot Separator -->
      <span class="mx-2 text-muted-foreground/35 font-mono text-[11px] select-none">·</span>

      <!-- Summary text (truncated) -->
      <span
        class="min-w-0 truncate text-[13px] text-muted-foreground/45 font-normal group-hover/reason:text-muted-foreground/60"
      >
        {summary}
      </span>

      {#if isStreaming}
        <!-- Subtle opacity-pulse indicator instead of bright blue dot -->
        <span
          class="ml-2 h-1 w-1 rounded-full bg-muted-foreground/40 shrink-0 animate-pulse"
          style="animation-duration: 1.5s"
        ></span>
      {/if}
    </button>

    <!-- Expanded Thought Body: indented text, very subtle left border, no card -->
    {#if isExpanded}
      <div
        class="pl-5 py-1.5 text-[var(--chat-secondary-size,13px)] leading-[var(--chat-secondary-line-height,1.5)] text-muted-foreground/75 whitespace-pre-wrap break-words select-text border-l border-border/25 ml-1.5 mt-0.5 mb-1 animate-fade-in"
      >
        {content}
      </div>
    {/if}
  </div>
{/if}
