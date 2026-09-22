<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    type = "injection", // "system_prompt" | "injection" | "recall"
    title,
    source,
    summary,
    content = "",
  }: {
    type?: "system_prompt" | "injection" | "recall";
    title?: string;
    source?: string;
    summary?: string;
    content?: string;
  } = $props();

  let open = $state(false);

  let displayTitle = $derived(
    title ??
      (type === "system_prompt"
        ? t("dsh_message_systemPrompt") || "系统提示词"
        : type === "recall"
          ? t("dsh_message_contextRecall") || "上下文召回"
          : "上下文"),
  );
</script>

<div class="dsh-context-root" data-open={open ? "true" : undefined}>
  <button
    type="button"
    class="dsh-context-header"
    onclick={() => (open = !open)}
    aria-expanded={open}
  >
    <span class="dsh-context-chevron" class:dsh-context-chevron-open={open}>
      <svg class="size-3" viewBox="0 0 16 16" fill="currentColor">
        <path
          d="M6.22 3.22a.75.75 0 0 1 1.06 0l4.25 4.25a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06-1.06L9.94 8 6.22 4.28a.75.75 0 0 1 0-1.06Z"
        />
      </svg>
    </span>

    <span class="dsh-context-icon">
      {#if type === "system_prompt"}
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <circle cx="8" cy="8" r="6" />
          <path d="M8 5v3.5l2 1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      {:else if type === "recall"}
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path
            d="M2.5 5.5A2.5 2.5 0 0 1 5 3h6a2.5 2.5 0 0 1 2.5 2.5v5A2.5 2.5 0 0 1 11 13H5a2.5 2.5 0 0 1-2.5-2.5v-5Z"
          />
          <path d="M5.5 6.5h5M5.5 9.5h3" stroke-linecap="round" />
        </svg>
      {:else}
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M8 2.5v11M2.5 8h11" stroke-linecap="round" />
          <circle cx="8" cy="8" r="6" />
        </svg>
      {/if}
    </span>

    <span class="dsh-context-title">{displayTitle}</span>

    {#if source}
      <span class="dsh-context-sep" aria-hidden="true"></span>
      <span class="dsh-context-source">{source}</span>
    {/if}
  </button>

  {#if open && (content || summary)}
    <div class="dsh-context-body">
      {#if summary && summary !== content}
        <div class="dsh-context-summary-detail">{summary}</div>
      {/if}
      {#if content}
        <pre class="dsh-context-pre">{content}</pre>
      {/if}
    </div>
  {/if}
</div>

<style>
  .dsh-context-root {
    min-width: 0;
    padding: 2px 0;
  }

  .dsh-context-root[data-open] {
    padding-bottom: 4px;
  }

  .dsh-context-header {
    width: 100%;
    min-width: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 6px;
    align-items: center;
    padding: 2px 4px;
    display: flex;
    cursor: pointer;
    transition: background 0.15s;
  }

  .dsh-context-header:hover {
    background: var(--dsw-alias-interactive-bg-hover, rgba(0, 0, 0, 0.05));
  }

  .dsh-context-chevron {
    color: var(--dsw-alias-label-tertiary, #8b909a);
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-right: 4px;
    transition: transform 0.15s;
  }

  .dsh-context-chevron-open {
    transform: rotate(90deg);
  }

  .dsh-context-icon {
    color: var(--dsw-alias-label-tertiary, #8b909a);
    flex: none;
    display: inline-flex;
    align-items: center;
    margin-right: 6px;
  }

  .dsh-context-title {
    font-size: 13px;
    line-height: calc(20px + var(--dsh-content-font-delta, 0px));
    color: var(--dsw-alias-label-tertiary, #8b909a);
    font-weight: 400;
    flex: none;
  }

  .dsh-context-sep {
    background: var(--dsw-alias-label-caption, #a0a5af);
    border-radius: 1px;
    flex: none;
    width: 2px;
    height: 2px;
    margin: 0 6px;
    opacity: 0.7;
  }

  .dsh-context-source {
    min-width: 0;
    color: var(--dsw-alias-label-tertiary, #8b909a);
    font-size: 13px;
    line-height: calc(20px + var(--dsh-content-font-delta, 0px));
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: none;
    overflow: hidden;
  }

  .dsh-context-summary-detail {
    margin-bottom: 6px;
    font-weight: 500;
    color: var(--dsw-alias-label-secondary, #5c626d);
  }

  .dsh-context-body {
    box-sizing: border-box;
    width: calc(100% - 22px);
    max-height: 141px;
    margin: 4px 0 0 22px;
    background: var(--dsw-alias-markdown-code-block, #f5f6f8);
    color: var(--dsw-alias-label-tertiary, #8b909a);
    font: 400 11px / 16px var(--ds-font-family-code, monospace);
    border-radius: 8px;
    padding: 8px 12px;
    overflow: auto;
    border: 0.5px solid var(--dsw-alias-border-l1, rgba(0, 0, 0, 0.06));
  }

  .dsh-context-pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: inherit;
    color: inherit;
  }
</style>
