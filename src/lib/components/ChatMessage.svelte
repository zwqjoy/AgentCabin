<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { fmtTime, fmtDateTime } from "$lib/i18n/format";
  import MarkdownContent from "./MarkdownContent.svelte";
  import FileAttachment from "./FileAttachment.svelte";
  import { getAssistantDisplayName } from "$lib/utils/agent-metadata";
  import type { ChatMessage, Attachment } from "$lib/types";

  let {
    message,
    attachments,
    thinkingText,
    onRewind,
    onContinueFromMessage,
    onExport,
    agent = "claude",
    isRunning = false,
  }: {
    message: ChatMessage;
    attachments?: Attachment[];
    thinkingText?: string;
    onRewind?: () => void;
    onContinueFromMessage?: () => void;
    onExport?: () => void;
    agent?: string;
    isRunning?: boolean;
  } = $props();

  const assistantLabel = $derived(getAssistantDisplayName(agent, t("chat_roleClaude")));
  const isCodex = $derived(agent === "codex");

  const isUser = $derived(message.role === "user");

  let hovered = $state(false);
  let copied = $state(false);
  let collapsed = $state(true);
  let thinkingCollapsed = $state(true);

  const lineCount = $derived(message.content.split("\n").length);
  const isLong = $derived(isUser && lineCount > 10);

  function formatTime(ts: string): string {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return "";
    const now = new Date();
    const isToday =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    return isToday ? fmtTime(d) : fmtDateTime(d);
  }

  function formatFullTime(ts: string): string {
    return fmtDateTime(ts);
  }

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Silently fail
    }
  }
</script>

<div
  class="w-full {isUser ? 'bg-muted/50' : ''}"
  role="group"
  onmouseenter={() => (hovered = true)}
  onmouseleave={() => (hovered = false)}
>
  <div class="chat-content-width py-4">
    <!-- Header: icon + name + secondary actions + timestamp -->
    <div class="mb-1.5 flex items-center gap-2">
      {#if isUser}
        <div class="flex h-5 w-5 items-center justify-center rounded-sm bg-primary/10 text-primary">
          <svg
            class="h-3 w-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
        </div>
        <span class="text-sm font-semibold text-foreground">{t("chat_roleYou")}</span>
      {:else}
        <div
          class="flex h-5 w-5 items-center justify-center rounded-sm {isCodex
            ? 'bg-emerald-500/10 text-emerald-500'
            : 'bg-orange-500/10 text-orange-500'}"
        >
          <svg
            class="h-3 w-3"
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
        <span class="text-sm font-semibold text-foreground">{assistantLabel}</span>
      {/if}
      {#if onRewind}
        <button
          class="ml-auto p-1 rounded-md text-muted-foreground/50 hover:bg-muted hover:text-foreground transition-all duration-150 {hovered
            ? 'opacity-100'
            : 'opacity-0'}"
          onclick={onRewind}
          title={t("rewind_toHere")}
          data-export-exclude
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
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
          </svg>
        </button>
      {/if}
      {#if isUser}
        <button
          class="{onRewind
            ? ''
            : 'ml-auto'} rounded-md p-1 text-muted-foreground/50 transition-all duration-150 hover:bg-muted hover:text-foreground {hovered ||
          copied
            ? 'opacity-100'
            : 'opacity-0'}"
          onclick={copyContent}
          title={t("chat_copyMessage")}
          aria-label={t("chat_copyMessage")}
          data-export-exclude
        >
          {#if copied}
            <svg
              class="h-3.5 w-3.5 text-emerald-500"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
          {:else}
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
          {/if}
        </button>
      {/if}
      <span class="text-[10px] text-muted-foreground" title={formatFullTime(message.timestamp)}>
        {formatTime(message.timestamp)}
      </span>
    </div>
    <!-- Content: indented to align with text after icon -->
    <div class="pl-7 text-sm text-foreground leading-relaxed">
      {#if isUser}
        {#if attachments && attachments.length > 0}
          <div class="flex flex-wrap gap-2 mb-2">
            {#each attachments as att}
              <FileAttachment
                name={att.name}
                size={att.size}
                mimeType={att.type}
                contentBase64={att.contentBase64}
              />
            {/each}
          </div>
        {/if}
        {#if isLong}
          <p
            class="whitespace-pre-wrap {collapsed ? 'max-h-24 overflow-hidden' : ''}"
            style={collapsed
              ? "mask-image: linear-gradient(to bottom, black 70%, transparent);"
              : ""}
          >
            {message.content}
          </p>
          <button
            class="mt-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
            onclick={() => (collapsed = !collapsed)}
          >
            {collapsed
              ? t("common_showAllLines", { count: String(lineCount) })
              : t("common_collapse")}
          </button>
        {:else}
          <p class="whitespace-pre-wrap">{message.content}</p>
        {/if}
      {:else}
        {#if thinkingText}
          <button
            class="mb-2 flex items-center gap-1.5 text-xs text-muted-foreground/70 hover:text-foreground transition-colors"
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
          {#if !thinkingCollapsed}
            <div
              class="mb-3 border-l-2 border-muted-foreground/15 pl-3 text-xs text-muted-foreground/70 whitespace-pre-line leading-relaxed"
            >
              {thinkingText.trimEnd()}
            </div>
          {/if}
        {/if}
        <div class="prose-chat">
          <MarkdownContent text={message.content} />
        </div>
        {#if !isRunning}
          <div class="mt-2 flex items-center gap-0.5" data-export-exclude>
            <button
              class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground {copied
                ? 'text-emerald-500'
                : ''}"
              onclick={copyContent}
              title={t("chat_copyMessage")}
              aria-label={t("chat_copyMessage")}
            >
              {#if copied}
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
                >
              {:else}
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
              {/if}
            </button>
            {#if onExport}
              <button
                class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground"
                onclick={onExport}
                title={t("chat_exportConversation")}
                aria-label={t("chat_exportConversation")}
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
                  <path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" />
                  <polyline points="16 6 12 2 8 6" />
                  <line x1="12" x2="12" y1="2" y2="15" />
                </svg>
              </button>
            {/if}
            {#if onContinueFromMessage}
              <button
                class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground"
                onclick={onContinueFromMessage}
                title={t("chat_continueFromMessage")}
                aria-label={t("chat_continueFromMessage")}
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
                  <path d="M6 3v12" /><path d="M6 9h8a4 4 0 0 1 4 4v8" /><circle
                    cx="6"
                    cy="3"
                    r="2"
                  />
                  <circle cx="6" cy="15" r="2" /><circle cx="18" cy="21" r="2" />
                </svg>
              </button>
            {/if}
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>
