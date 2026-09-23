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

<div class="turn-error-row" role="status">
  <span class="state-dot state-dot-error" aria-hidden="true"></span>
  <div class="turn-error-copy">
    <span class="turn-error-title">{t("dsh_message_turnError")}</span>
    <span class="turn-error-message">{message || t("dsh_command_failed")}</span>
  </div>
  <div class="turn-error-actions">
    {#if code !== undefined && code !== null && String(code).trim() !== ""}
      <button
        type="button"
        class="turn-error-code"
        title={copied ? t("settings_debug_copied") : t("common_copy")}
        onclick={handleCopyCode}
      >
        <code>{code}</code>
        <span class="copy-indicator" aria-hidden="true">
          {#if copied}
            <svg class="size-3 text-emerald-500" viewBox="0 0 16 16" fill="currentColor">
              <path
                d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z"
              />
            </svg>
          {:else}
            <svg
              class="size-3 text-muted-foreground"
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
      <button type="button" class="turn-retry-btn" onclick={onRetry}>
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
  .turn-error-row {
    font-size: 13px;
    line-height: 20px;
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: start;
    gap: 8px;
    padding: 6px 0;
  }

  .state-dot {
    width: 7px;
    height: 7px;
    border-radius: 9999px;
    margin-top: 6px;
    flex-shrink: 0;
  }

  .state-dot-error {
    background-color: hsl(var(--destructive, 0 84.2% 60.2%));
    box-shadow: 0 0 0 2px hsl(var(--destructive, 0 84.2% 60.2%) / 0.2);
  }

  .turn-error-copy {
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .turn-error-title {
    color: hsl(var(--destructive, 0 84.2% 60.2%));
    margin-right: 6px;
    font-weight: 600;
  }

  .turn-error-message {
    color: hsl(var(--muted-foreground));
  }

  .turn-error-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .turn-error-code {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    background: hsl(var(--muted) / 0.5);
    border: 1px solid hsl(var(--border) / 0.5);
    border-radius: 4px;
    color: hsl(var(--muted-foreground));
    font-size: 11px;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
  }

  .turn-error-code:hover {
    background: hsl(var(--muted));
    border-color: hsl(var(--border));
  }

  .turn-error-code code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .turn-retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted-foreground));
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .turn-retry-btn:hover {
    background: hsl(var(--muted) / 0.5);
    color: hsl(var(--foreground));
  }
</style>
