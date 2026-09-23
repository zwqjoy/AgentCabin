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

<div class="context-injection-root" data-open={open ? "true" : undefined}>
  <button
    type="button"
    class="context-injection-header"
    onclick={() => (open = !open)}
    aria-expanded={open}
  >
    <span class="context-injection-chevron" class:context-injection-chevron-open={open}>
      <svg class="size-3" viewBox="0 0 16 16" fill="currentColor">
        <path
          d="M6.22 3.22a.75.75 0 0 1 1.06 0l4.25 4.25a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06-1.06L9.94 8 6.22 4.28a.75.75 0 0 1 0-1.06Z"
        />
      </svg>
    </span>

    <span class="context-injection-icon">
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

    <span class="context-injection-title">{displayTitle}</span>

    {#if source}
      <span class="context-injection-sep" aria-hidden="true"></span>
      <span class="context-injection-source">{source}</span>
    {/if}
  </button>

  {#if open && (content || summary)}
    <div class="context-injection-body">
      {#if summary && summary !== content}
        <div class="context-injection-summary-detail">{summary}</div>
      {/if}
      {#if content}
        <pre class="context-injection-pre">{content}</pre>
      {/if}
    </div>
  {/if}
</div>

<style>
  .context-injection-root {
    min-width: 0;
    padding: 2px 0;
  }

  .context-injection-root[data-open] {
    padding-bottom: 4px;
  }

  .context-injection-header {
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

  .context-injection-header:hover {
    background: hsl(var(--muted) / 0.5);
  }

  .context-injection-chevron {
    color: hsl(var(--muted-foreground));
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-right: 4px;
    transition: transform 0.15s;
  }

  .context-injection-chevron-open {
    transform: rotate(90deg);
  }

  .context-injection-icon {
    color: hsl(var(--muted-foreground));
    flex: none;
    display: inline-flex;
    align-items: center;
    margin-right: 6px;
  }

  .context-injection-title {
    font-size: 13px;
    line-height: 20px;
    color: hsl(var(--muted-foreground));
    font-weight: 400;
    flex: none;
  }

  .context-injection-sep {
    background: hsl(var(--muted-foreground) / 0.4);
    border-radius: 1px;
    flex: none;
    width: 2px;
    height: 2px;
    margin: 0 6px;
    opacity: 0.7;
  }

  .context-injection-source {
    min-width: 0;
    color: hsl(var(--muted-foreground));
    font-size: 13px;
    line-height: 20px;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: none;
    overflow: hidden;
  }

  .context-injection-summary-detail {
    margin-bottom: 6px;
    font-weight: 500;
    color: hsl(var(--foreground));
  }

  .context-injection-body {
    box-sizing: border-box;
    width: calc(100% - 22px);
    max-height: 141px;
    margin: 4px 0 0 22px;
    background: hsl(var(--muted) / 0.4);
    color: hsl(var(--muted-foreground));
    font:
      400 11px / 16px ui-monospace,
      SFMono-Regular,
      Menlo,
      Monaco,
      Consolas,
      monospace;
    border-radius: 8px;
    padding: 8px 12px;
    overflow: auto;
    border: 1px solid hsl(var(--border) / 0.5);
  }

  .context-injection-pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: inherit;
    color: inherit;
  }
</style>
