<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    onContinue,
  }: {
    onContinue?: () => void;
  } = $props();
</script>

<div class="turn-error-row" role="status">
  <span class="state-dot state-dot-warning" aria-hidden="true"></span>
  <div class="turn-error-copy">
    <span class="max-tokens-title">{t("dsh_message_maxTokens")}</span>
    <span class="turn-error-message">{t("dsh_message_maxTokens_hint")}</span>
  </div>
  {#if onContinue}
    <div class="max-tokens-actions">
      <button type="button" class="continue-btn" onclick={onContinue}>
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M4 3l8 5-8 5V3z" fill="currentColor" stroke="none" />
        </svg>
        <span>{t("dsh_message_maxTokens_continue")}</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .turn-error-row {
    font-size: 13px;
    line-height: 20px;
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }

  .state-dot {
    width: 7px;
    height: 7px;
    border-radius: 9999px;
    margin-top: 2px;
    flex-shrink: 0;
  }

  .state-dot-warning {
    background-color: #f59e0b;
    box-shadow: 0 0 0 2px rgba(245, 158, 11, 0.2);
  }

  .turn-error-copy {
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .max-tokens-title {
    color: #f59e0b;
    margin-right: 6px;
    font-weight: 600;
  }

  .turn-error-message {
    color: hsl(var(--muted-foreground));
  }

  .max-tokens-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .continue-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 6px;
    background: hsl(var(--primary) / 0.1);
    border: 1px solid hsl(var(--primary) / 0.3);
    color: hsl(var(--primary));
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .continue-btn:hover {
    background: hsl(var(--primary) / 0.18);
    border-color: hsl(var(--primary));
  }
</style>
