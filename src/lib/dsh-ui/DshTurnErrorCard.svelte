<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    message = "",
    code,
    onRetry,
  }: {
    message?: string;
    code?: string | number;
    onRetry?: () => void;
  } = $props();

  let copied = $state(false);

  async function handleCopyCode() {
    if (!code) return;
    try {
      await navigator.clipboard.writeText(String(code));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // ignore
    }
  }
</script>

<div class="dsh-turn-error-row" role="status">
  <span class="dsh-state-dot dsh-state-dot-error" aria-hidden="true"></span>
  <div class="dsh-turn-error-copy">
    <span class="dsh-turn-error-title">{t("dsh_message_turnError")}</span>
    <span class="dsh-turn-error-message">{message || t("dsh_command_failed")}</span>
  </div>
  <div class="dsh-turn-error-actions">
    {#if code !== undefined && code !== null && String(code).trim() !== ""}
      <button
        type="button"
        class="dsh-turn-error-code"
        title={copied ? t("settings_debug_copied") : t("common_copy")}
        onclick={handleCopyCode}
      >
        <code>{code}</code>
        <span class="dsh-copy-indicator" aria-hidden="true">
          {#if copied}
            <svg class="size-3 text-emerald-500" viewBox="0 0 16 16" fill="currentColor">
              <path
                d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z"
              />
            </svg>
          {:else}
            <svg
              class="size-3 text-[var(--dsw-alias-label-tertiary)]"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
            >
              <rect x="5" y="5" width="8" height="8" rx="1.5" />
              <path d="M3 11V3.5A.5.5 0 0 1 3.5 3H11" stroke-linecap="round" />
            </svg>
          {/if}
        </span>
      </button>
    {/if}
    {#if onRetry}
      <button type="button" class="dsh-turn-retry-btn" onclick={onRetry}>
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path
            d="M2.5 8a5.5 5.5 0 1 0 1.2-3.4L2 6.5M2 2.5v4h4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <span>{t("common_retry")}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  .dsh-turn-error-row {
    font-size: var(--dsh-content-font-size-secondary, 13px);
    line-height: calc(20px + var(--dsh-content-font-delta-secondary, 0px));
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: start;
    gap: 8px;
    padding: 6px 0;
  }

  .dsh-state-dot {
    width: 7px;
    height: 7px;
    border-radius: 9999px;
    margin-top: 6px;
    flex-shrink: 0;
  }

  .dsh-state-dot-error {
    background-color: var(--dsw-alias-state-error-primary, #ef4444);
    box-shadow: 0 0 0 2px
      color-mix(in srgb, var(--dsw-alias-state-error-primary, #ef4444) 20%, transparent);
  }

  .dsh-turn-error-copy {
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .dsh-turn-error-title {
    color: var(--dsw-alias-state-error-primary, #ef4444);
    margin-right: 6px;
    font-weight: 600;
  }

  .dsh-turn-error-message {
    color: var(--dsw-alias-label-secondary, #5c626d);
  }

  .dsh-turn-error-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dsh-turn-error-code {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    background: var(--dsw-alias-markdown-inline-code, rgba(0, 0, 0, 0.05));
    border: 0.5px solid var(--dsw-alias-border-l2, rgba(0, 0, 0, 0.1));
    border-radius: 4px;
    color: var(--dsw-alias-label-tertiary, #8b909a);
    font-size: 11px;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
  }

  .dsh-turn-error-code:hover {
    background: var(--dsw-alias-interactive-bg-hover, rgba(0, 0, 0, 0.08));
    border-color: var(--dsw-alias-border-l3, rgba(0, 0, 0, 0.2));
  }

  .dsh-turn-error-code code {
    font-family: var(--ds-font-family-code, monospace);
  }

  .dsh-turn-retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 6px;
    background: 0 0;
    border: 0.5px solid var(--dsw-alias-border-l2, rgba(0, 0, 0, 0.1));
    color: var(--dsw-alias-label-secondary, #5c626d);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .dsh-turn-retry-btn:hover {
    background: var(--dsw-alias-interactive-bg-hover, rgba(0, 0, 0, 0.05));
    color: var(--dsw-alias-label-primary, #0f1115);
  }
</style>
