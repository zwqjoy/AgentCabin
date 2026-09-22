<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { copyToClipboard } from "$lib/utils/tool-rendering";

  let {
    content = "",
    onExport,
    onContinueFromMessage,
  }: {
    content?: string;
    onExport?: () => void;
    onContinueFromMessage?: () => void;
  } = $props();

  let copied = $state(false);

  async function handleCopy() {
    try {
      await copyToClipboard(content);
      copied = true;
      setTimeout(() => {
        copied = false;
      }, 1500);
    } catch {
      // Clipboard access can be unavailable in a WebView; the message stays usable.
    }
  }

  const actionButtonClass =
    "inline-flex h-6 w-6 items-center justify-center rounded text-[var(--chat-text-tertiary,#9298a1)] transition-colors hover:bg-muted/50 hover:text-foreground cursor-pointer";
</script>

<div class="flex items-center gap-0.5">
  <!-- Copy -->
  <button
    type="button"
    class="{actionButtonClass} {copied ? 'text-emerald-500 dark:text-emerald-400' : ''}"
    onclick={handleCopy}
    title={copied ? t("statusbar_copied") : t("chat_copyMessage")}
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
          d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 2 2 2"
        /></svg
      >
    {/if}
  </button>

  <!-- Branch: continue the conversation from this message -->
  {#if onContinueFromMessage}
    <button
      type="button"
      class={actionButtonClass}
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
        ><line x1="6" x2="6" y1="3" y2="15" /><circle cx="18" cy="6" r="3" /><circle
          cx="6"
          cy="18"
          r="3"
        /><path d="M18 9a9 9 0 0 1-9 9" /></svg
      >
    </button>
  {/if}

  <!-- Export: download the conversation -->
  {#if onExport}
    <button
      type="button"
      class={actionButtonClass}
      onclick={onExport}
      title={t("chat_exportConversation")}
      aria-label={t("chat_exportConversation")}
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
        ><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><path d="m7 10 5 5 5-5" /><path
          d="M12 15V3"
        /></svg
      >
    </button>
  {/if}
</div>
