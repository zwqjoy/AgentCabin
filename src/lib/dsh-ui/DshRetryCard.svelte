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

<details class="dsh-retry-row" data-active={active ? "true" : undefined} bind:open>
  <summary class="dsh-retry-summary">
    <span class="dsh-retry-text">
      {statusLabel}（{retry}/{maximum}） · {seconds}s
    </span>
  </summary>
  <div class="dsh-retry-details">
    <div class="dsh-retry-item">
      <span class="dsh-retry-label">{t("dsh_message_retry_delay")}</span>
      <span>{Math.round(delayMs)}毫秒</span>
    </div>
    {#if failure}
      <div class="dsh-retry-item">
        <span class="dsh-retry-label">{t("dsh_message_retry_failure")}</span>
        <span class="text-rose-500/90">{failure}</span>
      </div>
    {/if}
  </div>
</details>

<style>
  .dsh-retry-row {
    color: var(--dsw-alias-label-tertiary, #8b909a);
    font-size: var(--dsh-content-font-size-secondary, 13px);
    line-height: calc(20px + var(--dsh-content-font-delta-secondary, 0px));
    padding: 2px 0;
  }

  .dsh-retry-summary {
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

  .dsh-retry-summary::-webkit-details-marker {
    display: none;
  }

  .dsh-retry-summary:after {
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

  .dsh-retry-row[open] .dsh-retry-summary:after {
    transform: rotate(45deg);
  }

  .dsh-retry-summary:hover {
    color: var(--dsw-alias-label-secondary, #5c626d);
  }

  .dsh-retry-text {
    color: inherit;
  }

  .dsh-retry-row[data-active] .dsh-retry-text {
    background: linear-gradient(
      90deg,
      var(--dsw-alias-label-tertiary, #8b909a) 0%,
      var(--dsw-alias-label-tertiary, #8b909a) 40%,
      var(--dsw-alias-label-secondary, #5c626d) 50%,
      var(--dsw-alias-label-tertiary, #8b909a) 60%,
      var(--dsw-alias-label-tertiary, #8b909a) 100%
    );
    color: transparent;
    background-position: 100%;
    background-size: 200% 100%;
    background-clip: text;
    -webkit-background-clip: text;
    animation: 1.6s ease-in-out infinite dsh-retry-shimmer;
  }

  .dsh-retry-details {
    overflow-wrap: anywhere;
    font-size: var(--dsh-content-font-size-secondary, 13px);
    line-height: calc(18px + var(--dsh-content-font-delta-secondary, 0px));
    gap: 4px;
    margin-top: 4px;
    padding-left: 14px;
    display: grid;
  }

  .dsh-retry-item {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .dsh-retry-label {
    color: var(--dsw-alias-label-secondary, #5c626d);
    font-weight: 500;
  }
</style>
