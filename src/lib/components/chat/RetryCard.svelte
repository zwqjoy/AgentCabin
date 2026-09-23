<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    retry = 1,
    maximum = 3,
    delayMs = 2000,
    seconds = 2,
    failure = "",
    active = true,
  }: {
    retry?: number;
    maximum?: number;
    delayMs?: number;
    seconds?: number;
    failure?: string;
    active?: boolean;
  } = $props();

  let open = $state(false);

  let statusLabel = $derived(
    active ? t("dsh_message_retry_active") : t("dsh_message_retry_started"),
  );
</script>

<details class="retry-row" data-active={active ? "true" : undefined} bind:open>
  <summary class="retry-summary">
    <span class="retry-text">
      {statusLabel}（{retry}/{maximum}） · {seconds}s
    </span>
  </summary>
  <div class="retry-details">
    <div class="retry-item">
      <span class="retry-label">{t("dsh_message_retry_delay")}</span>
      <span>{Math.round(delayMs)}毫秒</span>
    </div>
    {#if failure}
      <div class="retry-item">
        <span class="retry-label">{t("dsh_message_retry_failure")}</span>
        <span class="text-rose-500/90">{failure}</span>
      </div>
    {/if}
  </div>
</details>

<style>
  .retry-row {
    color: hsl(var(--muted-foreground));
    font-size: 13px;
    line-height: 20px;
    padding: 2px 0;
  }

  .retry-summary {
    width: fit-content;
    color: inherit;
    cursor: pointer;
    user-select: none;
    border-radius: 4px;
    align-items: center;
    gap: 7px;
    padding: 2px 4px;
    list-style: none;
    display: inline-flex;
    transition: color 0.15s;
  }

  .retry-summary::-webkit-details-marker {
    display: none;
  }

  .retry-summary:after {
    content: "";
    opacity: 0.8;
    border-bottom: 1.5px solid;
    border-right: 1.5px solid;
    width: 6px;
    height: 6px;
    transition: transform 0.12s;
    transform: rotate(-45deg);
    margin-left: 2px;
  }

  .retry-row[open] .retry-summary:after {
    transform: rotate(45deg);
  }

  .retry-summary:hover {
    color: hsl(var(--foreground));
  }

  .retry-text {
    color: inherit;
  }

  .retry-row[data-active] .retry-text {
    color: hsl(var(--primary));
    animation: pulse 1.6s ease-in-out infinite;
  }

  .retry-details {
    overflow-wrap: anywhere;
    font-size: 13px;
    line-height: 18px;
    gap: 4px;
    margin-top: 4px;
    padding-left: 14px;
    display: grid;
  }

  .retry-item {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .retry-label {
    color: hsl(var(--muted-foreground));
    font-weight: 500;
  }
</style>
