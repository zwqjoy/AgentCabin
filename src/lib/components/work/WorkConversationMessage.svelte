<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { fmtTime, fmtDateTime } from "$lib/i18n/format";
  import ConversationMarkdown from "$lib/components/ConversationMarkdown.svelte";
  import MessageActionBar from "$lib/components/chat/MessageActionBar.svelte";
  import FileAttachment from "$lib/components/FileAttachment.svelte";
  import { stripExpertTag } from "$lib/utils/expert-context";
  import type { Attachment, ChatMessage } from "$lib/types";

  let {
    message,
    attachments,
    thinkingText,
    onRewind,
    onContinueFromMessage,
    onExport,
    onEdit,
    agent = "unknown",
    isRunning = false,
    showHeader = true,
    workspaceId = "",
    basePath = "",
  }: {
    message: ChatMessage;
    attachments?: Attachment[];
    thinkingText?: string;
    onRewind?: () => void;
    onContinueFromMessage?: () => void;
    onExport?: () => void;
    onEdit?: (newContent: string) => void | Promise<void>;
    agent?: string;
    isRunning?: boolean;
    showHeader?: boolean;
    workspaceId?: string;
    basePath?: string;
  } = $props();

  const isUser = $derived(message.role === "user");
  let hovered = $state(false);
  let copied = $state(false);
  let thinkingExpanded = $state(false);

  let isEditing = $state(false);
  let editContent = $state("");
  let editTextareaEl = $state<HTMLTextAreaElement | null>(null);
  let isSubmittingEdit = $state(false);

  function startEditing() {
    if (isRunning) return;
    editContent = stripExpertTag(message.content);
    isEditing = true;
  }

  function cancelEditing() {
    isEditing = false;
    editContent = "";
  }

  async function submitEditing() {
    const trimmed = editContent.trim();
    if (!trimmed || isSubmittingEdit || isRunning) return;
    isSubmittingEdit = true;
    try {
      if (onEdit) {
        await onEdit(trimmed);
      }
      isEditing = false;
    } catch {
      // Keep editing state if failed
    } finally {
      isSubmittingEdit = false;
    }
  }

  function handleEditKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancelEditing();
      return;
    }
    if ((e.key === "Enter" && !e.shiftKey) || ((e.metaKey || e.ctrlKey) && e.key === "Enter")) {
      e.preventDefault();
      void submitEditing();
    }
  }

  function adjustTextareaHeight() {
    if (editTextareaEl) {
      editTextareaEl.style.height = "auto";
      editTextareaEl.style.height = `${Math.min(Math.max(editTextareaEl.scrollHeight, 68), 400)}px`;
    }
  }

  $effect(() => {
    if (isEditing && editTextareaEl) {
      editTextareaEl.focus();
      editTextareaEl.selectionStart = editTextareaEl.selectionEnd = editTextareaEl.value.length;
      adjustTextareaHeight();
    }
  });

  function formatTime(ts: string): string {
    const date = new Date(ts);
    if (Number.isNaN(date.getTime())) return "";
    const now = new Date();
    const sameDay =
      date.getFullYear() === now.getFullYear() &&
      date.getMonth() === now.getMonth() &&
      date.getDate() === now.getDate();
    return sameDay ? fmtTime(date) : fmtDateTime(date);
  }

  function formatFullTime(ts: string): string {
    return fmtDateTime(new Date(ts));
  }

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard access can be unavailable in a WebView; the message remains usable.
    }
  }
</script>

<div
  class="w-full py-3"
  role="group"
  onmouseenter={() => (hovered = true)}
  onmouseleave={() => (hovered = false)}
>
  <div class="chat-content-width">
    {#if isUser}
      <div class="flex justify-end">
        <div
          class="max-w-[min(78%,48rem)]"
          class:w-full={isEditing}
          class:min-w-[min(100%,360px)]={isEditing}
        >
          <!-- Hover-only timestamp metadata -->
          <div
            class="mb-1 flex items-center justify-end gap-2 text-[11px] text-muted-foreground/50 transition-opacity {hovered
              ? 'opacity-100'
              : 'opacity-0'}"
          >
            <span title={formatFullTime(message.timestamp)}>{formatTime(message.timestamp)}</span>
          </div>
          <div
            class="chat-message-user-body rounded-[var(--chat-radius-user,12px)] bg-[var(--chat-surface-user,#f5f6f7)] dark:bg-[var(--chat-surface-user,#242629)] px-4 py-3 text-[var(--chat-font-size,15px)] leading-[var(--chat-line-height,1.62)] text-foreground"
          >
            {#if attachments && attachments.length > 0}
              <div class="mb-2 flex flex-wrap justify-end gap-2">
                {#each attachments as attachment}
                  <FileAttachment
                    name={attachment.name}
                    size={attachment.size}
                    mimeType={attachment.type}
                    contentBase64={attachment.contentBase64}
                  />
                {/each}
              </div>
            {/if}
            {#if isEditing}
              <div class="flex flex-col gap-2.5">
                <textarea
                  bind:this={editTextareaEl}
                  bind:value={editContent}
                  oninput={adjustTextareaHeight}
                  onkeydown={handleEditKeydown}
                  placeholder={t("chat_editMessage")}
                  class="w-full resize-none rounded-xl border border-input bg-background/90 px-3 py-2 text-sm text-foreground shadow-inner outline-none transition-colors focus:border-ring focus:ring-1 focus:ring-ring"
                ></textarea>
                <div class="flex items-center justify-end gap-2">
                  <button
                    type="button"
                    class="rounded-lg px-3 py-1.5 text-xs font-medium text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
                    onclick={cancelEditing}
                  >
                    {t("chat_editCancel")}
                  </button>
                  <button
                    type="button"
                    class="rounded-lg bg-foreground text-background dark:bg-primary dark:text-primary-foreground px-3.5 py-1.5 text-xs font-medium shadow-sm hover:opacity-90 transition-opacity disabled:opacity-50"
                    disabled={!editContent.trim() || isSubmittingEdit || isRunning}
                    onclick={submitEditing}
                  >
                    {isSubmittingEdit ? "..." : t("chat_editSend")}
                  </button>
                </div>
              </div>
            {:else}
              <p class="whitespace-pre-wrap break-words">{stripExpertTag(message.content)}</p>
            {/if}
          </div>
          {#if !isEditing}
            <div
              class="mt-1 flex items-center justify-end gap-0.5 transition-opacity focus-within:opacity-100 {hovered ||
              copied
                ? 'opacity-100'
                : 'opacity-0'}"
            >
              {#if onRewind}
                <button
                  type="button"
                  class="rounded-md p-1 text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground"
                  onclick={onRewind}
                  title={t("rewind_toHere")}
                  aria-label={t("rewind_toHere")}
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
                    ><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                      d="M3 3v5h5"
                    /></svg
                  >
                </button>
              {/if}
              <button
                type="button"
                class="rounded-md p-1 text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground"
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
                      d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 2 2 2"
                    /></svg
                  >
                {/if}
              </button>
              {#if onEdit && !isRunning}
                <button
                  type="button"
                  class="rounded-md p-1 text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground"
                  onclick={startEditing}
                  title={t("chat_editMessage")}
                  aria-label={t("chat_editMessage")}
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
                    <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    <path d="m15 5 4 4" />
                  </svg>
                </button>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <!-- Assistant message: no avatar icon, no name label — content starts directly -->
      <div class="flex items-start">
        <div class="min-w-0 flex-1">
          {#if thinkingText}
            <div data-exclude-search="true" class="chat-thought-process mb-2">
              <button
                type="button"
                class="inline-flex items-center gap-1.5 text-[13px] text-muted-foreground/70 transition-colors hover:text-foreground"
                aria-expanded={thinkingExpanded}
                onclick={() => (thinkingExpanded = !thinkingExpanded)}
              >
                <svg
                  class="h-3 w-3 shrink-0 transition-transform {thinkingExpanded
                    ? 'rotate-90'
                    : ''}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
                >
                <span
                  class="h-1.5 w-1.5 shrink-0 rounded-full bg-muted-foreground/40"
                  aria-hidden="true"
                ></span>
                {t("chat_thoughtProcess")}
              </button>
              {#if thinkingExpanded}
                <div
                  class="chat-thought-process-panel mt-1.5 mb-2 rounded-lg px-3 py-2 text-[13px] leading-relaxed"
                >
                  <pre
                    class="m-0 whitespace-pre-wrap break-words font-mono text-foreground/75">{thinkingText.trimEnd()}</pre>
                </div>
              {/if}
            </div>
          {/if}
          <div class="chat-message-body prose-chat text-foreground">
            <ConversationMarkdown text={message.content} {workspaceId} {basePath} />
          </div>
          {#if !isRunning}
            <div
              class="mt-2 flex items-center gap-1.5 transition-opacity focus-within:opacity-100 {hovered
                ? 'opacity-100'
                : 'opacity-0'}"
            >
              <MessageActionBar content={message.content} {onExport} {onContinueFromMessage} />
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>
