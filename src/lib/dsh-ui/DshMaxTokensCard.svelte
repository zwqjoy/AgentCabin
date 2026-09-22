<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    onContinue,
  }: {
    onContinue?: () => void;
  } = $props();
</script>

<div class="dsh-turn-error-row" role="status">
  <span class="dsh-state-dot dsh-state-dot-warning" aria-hidden="true"></span>
  <div class="dsh-turn-error-copy">
    <span class="dsh-max-tokens-title">{t("dsh_message_maxTokens")}</span>
    <span class="dsh-turn-error-message">{t("dsh_message_maxTokens_hint")}</span>
  </div>
  {#if onContinue}
    <div class="dsh-max-tokens-actions">
      <button type="button" class="dsh-continue-btn" onclick={onContinue}>
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
  .dsh-turn-error-row {
    font-size: var(--dsh-content-font-size-secondary, 13px);
    line-height: calc(20px + var(--dsh-content-font-delta-secondary, 0px));
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }

  .dsh-state-dot {
    width: 7px;
    height: 7px;
    border-radius: 9999px;
    margin-top: 2px;
    flex-shrink: 0;
  }

  .dsh-state-dot-warning {
    background-color: var(--dsw-alias-state-warn-primary, #f59e0b);
    box-shadow: 0 0 0 2px
      color-mix(in srgb, var(--dsw-alias-state-warn-primary, #f59e0b) 20%, transparent);
  }

  .dsh-turn-error-copy {
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .dsh-max-tokens-title {
    color: var(--dsw-alias-state-warn-primary, #f59e0b);
    margin-right: 6px;
    font-weight: 600;
  }

  .dsh-turn-error-message {
    color: var(--dsw-alias-label-secondary, #5c626d);
  }

  .dsh-max-tokens-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dsh-continue-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--dsw-alias-brand-primary, #4d6bfe) 10%, transparent);
    border: 0.5px solid color-mix(in srgb, var(--dsw-alias-brand-primary, #4d6bfe) 30%, transparent);
    color: var(--dsw-alias-brand-primary, #4d6bfe);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .dsh-continue-btn:hover {
    background: color-mix(in srgb, var(--dsw-alias-brand-primary, #4d6bfe) 18%, transparent);
    border-color: var(--dsw-alias-brand-primary, #4d6bfe);
  }
</style>
