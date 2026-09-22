<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import ConversationMarkdown from "$lib/components/ConversationMarkdown.svelte";

  type ProgressEntry = {
    content: string;
    thinkingText?: string;
  };

  let {
    entries,
    agent = "pi",
    workMode = false,
  }: {
    entries: ProgressEntry[];
    agent?: string;
    workMode?: boolean;
  } = $props();

  const isCodex = $derived(agent === "codex");
  const thinkingText = $derived(
    entries
      .map((entry) => entry.thinkingText)
      .filter((text): text is string => Boolean(text?.trim()))
      .join("\n\n"),
  );
  let thinkingExpanded = $state(false);
</script>

<div class="w-full py-1">
  <div class="chat-content-width space-y-1.5">
    {#if thinkingText}
      <div>
        <button
          type="button"
          class="group inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-xs text-blue-500/80 transition-colors hover:bg-muted/40 hover:text-blue-400"
          aria-expanded={thinkingExpanded}
          onclick={() => (thinkingExpanded = !thinkingExpanded)}
        >
          <svg
            class="h-3 w-3 shrink-0 transition-transform {thinkingExpanded ? 'rotate-90' : ''}"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
          >
          <span>{workMode ? "模型思考过程" : t("chat_thoughtProcess")}</span>
        </button>

        {#if thinkingExpanded}
          <div
            class="mt-1 ml-2 rounded-lg border border-blue-500/15 bg-blue-500/5 px-3 py-2 text-xs leading-relaxed text-blue-300/75"
          >
            <pre
              class="m-0 whitespace-pre-wrap break-words font-mono text-[11px]">{thinkingText.trimEnd()}</pre>
          </div>
        {/if}
      </div>
    {/if}

    {#each entries as entry, index (index)}
      {#if entry.content?.trim()}
        <div class="prose-chat text-sm leading-relaxed text-foreground/90">
          <ConversationMarkdown text={entry.content.trim()} />
        </div>
      {/if}
    {/each}
  </div>
</div>
