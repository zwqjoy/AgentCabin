<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { ChatMessage } from "$lib/types";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";

  let {
    message,
    thinkingText,
    agent = "pi",
  }: {
    message: ChatMessage;
    thinkingText?: string;
    agent?: string;
  } = $props();

  const isCodex = $derived(agent === "codex");
  let thinkingCollapsed = $state(true);

  function copyContent() {
    if (!message.content) return;
    navigator.clipboard.writeText(message.content).catch(() => {});
  }
</script>

<div class="w-full">
  <div class="chat-content-width py-1">
    <div class="group flex items-start gap-2">
      <div
        class="mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm {isCodex
          ? 'bg-emerald-500/10 text-emerald-500/70'
          : 'bg-orange-500/10 text-orange-500/70'}"
        aria-hidden="true"
      >
        <svg
          class="h-2.5 w-2.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          {#if isCodex}
            <polyline points="4 17 10 11 4 5" /><line x1="12" x2="20" y1="19" y2="19" />
          {:else}
            <path
              d="M12 3l1.912 5.813a2 2 0 0 0 1.275 1.275L21 12l-5.813 1.912a2 2 0 0 0-1.275 1.275L12 21l-1.912-5.813a2 2 0 0 0-1.275-1.275L3 12l5.813-1.912a2 2 0 0 0 1.275-1.275L12 3z"
            />
          {/if}
        </svg>
      </div>

      <div class="min-w-0 flex-1">
        <div class="flex min-h-5 items-start gap-2">
          {#if message.content}
            <div class="min-w-0 flex-1 text-sm leading-relaxed text-foreground/75 prose-chat">
              <MarkdownContent text={message.content} />
            </div>
          {/if}

          <div class="flex shrink-0 items-center gap-1">
            {#if thinkingText}
              <button
                class="flex items-center gap-1 text-[11px] text-muted-foreground/70 transition-colors hover:text-foreground"
                aria-expanded={!thinkingCollapsed}
                onclick={() => (thinkingCollapsed = !thinkingCollapsed)}
              >
                <svg
                  class="h-3 w-3 transition-transform {thinkingCollapsed ? '' : 'rotate-90'}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
                >
                {t("chat_thoughtProcess")}
              </button>
            {/if}
            {#if message.content}
              <button
                class="ml-auto rounded p-1 text-muted-foreground/50 opacity-0 transition-all hover:bg-muted hover:text-foreground group-hover:opacity-100"
                onclick={copyContent}
                title={t("chat_copyMessage")}
                aria-label={t("chat_copyMessage")}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><rect width="14" height="14" x="8" y="8" rx="2" /><path
                    d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
                  /></svg
                >
              </button>
            {/if}
          </div>
        </div>

        {#if thinkingText && !thinkingCollapsed}
          <div
            class="mt-1 border-l-2 border-muted-foreground/15 pl-2.5 text-xs leading-relaxed text-muted-foreground/65 whitespace-pre-line"
          >
            {thinkingText.trimEnd()}
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
