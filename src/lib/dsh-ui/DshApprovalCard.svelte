<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    toolName = "",
    command = "",
    argsRaw = "",
    reason = "",
    riskLevel = "medium", // "low" | "medium" | "high"
    onAllow,
    onDeny,
  }: {
    toolName?: string;
    command?: string;
    argsRaw?: string;
    reason?: string;
    riskLevel?: "low" | "medium" | "high";
    onAllow?: () => void;
    onDeny?: () => void;
  } = $props();

  let formattedArgs = $derived.by(() => {
    if (command) return command;
    if (!argsRaw) return "";
    try {
      return JSON.stringify(JSON.parse(argsRaw), null, 2);
    } catch {
      return argsRaw;
    }
  });

  let copied = $state(false);

  async function handleCopy() {
    if (!formattedArgs) return;
    try {
      await navigator.clipboard.writeText(formattedArgs);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // ignore
    }
  }
</script>

<div class="dsh-approval-card" data-risk={riskLevel}>
  <div class="dsh-approval-header">
    <div class="dsh-approval-title-row">
      <span class="dsh-approval-shield-icon" aria-hidden="true">
        <svg
          class="size-4"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M8 1.5l6 2.5v4.5c0 4-3 6.5-6 7.5-3-1-6-3.5-6-7.5V4l6-2.5z" />
          <path d="M6 7.5l1.5 1.5 3-3" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </span>
      <span class="dsh-approval-title">{t("dsh_approval_title")}</span>
      {#if toolName}
        <span class="dsh-approval-tool-badge">{toolName}</span>
      {/if}
    </div>
    {#if formattedArgs}
      <button
        type="button"
        class="dsh-approval-copy-btn"
        title={copied ? t("settings_debug_copied") : t("common_copy")}
        onclick={handleCopy}
      >
        {#if copied}
          <span class="text-emerald-500 text-xs font-medium">{t("settings_debug_copied")}</span>
        {:else}
          <svg
            class="size-3.5"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <rect x="5" y="5" width="8" height="8" rx="1.5" />
            <path d="M3 11V3.5A.5.5 0 0 1 3.5 3H11" stroke-linecap="round" />
          </svg>
        {/if}
      </button>
    {/if}
  </div>

  <div class="dsh-approval-desc">
    {reason || t("dsh_approval_desc")}
  </div>

  {#if formattedArgs}
    <div class="dsh-approval-code-wrap">
      <pre class="dsh-approval-code"><code>{formattedArgs}</code></pre>
    </div>
  {/if}

  <div class="dsh-approval-actions">
    {#if onDeny}
      <button type="button" class="dsh-approval-deny-btn" onclick={onDeny}>
        {t("dsh_approval_deny")}
      </button>
    {/if}
    {#if onAllow}
      <button type="button" class="dsh-approval-allow-btn" onclick={onAllow}>
        <svg class="size-3.5" viewBox="0 0 16 16" fill="currentColor">
          <path
            d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z"
          />
        </svg>
        <span>{t("dsh_approval_allow")}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  .dsh-approval-card {
    background: var(--dsw-alias-bg-layer-1, #ffffff);
    border: 1px solid var(--dsw-alias-border-l3, rgba(0, 0, 0, 0.16));
    border-radius: 12px;
    padding: 12px 14px;
    margin: 8px 0;
    box-shadow: var(--dsw-elevation-panel);
    transition: border-color 0.2s;
  }

  .dsh-approval-card[data-risk="high"] {
    border-color: color-mix(
      in srgb,
      var(--dsw-alias-state-error-primary, #ef4444) 40%,
      transparent
    );
  }

  .dsh-approval-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
  }

  .dsh-approval-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dsh-approval-shield-icon {
    color: var(--dsw-alias-brand-primary, #4d6bfe);
    display: flex;
    align-items: center;
  }

  .dsh-approval-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--dsw-alias-label-primary, #0f1115);
  }

  .dsh-approval-tool-badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--dsw-alias-markdown-inline-code, rgba(0, 0, 0, 0.05));
    color: var(--dsw-alias-label-secondary, #5c626d);
    font-family: var(--ds-font-family-code, monospace);
  }

  .dsh-approval-copy-btn {
    display: inline-flex;
    align-items: center;
    padding: 3px 6px;
    border-radius: 4px;
    background: transparent;
    border: none;
    color: var(--dsw-alias-label-tertiary, #8b909a);
    cursor: pointer;
    transition: color 0.15s;
  }

  .dsh-approval-copy-btn:hover {
    color: var(--dsw-alias-label-primary, #0f1115);
  }

  .dsh-approval-desc {
    font-size: 12px;
    line-height: 18px;
    color: var(--dsw-alias-label-secondary, #5c626d);
    margin-bottom: 8px;
  }

  .dsh-approval-code-wrap {
    background: var(--dsw-alias-markdown-code-block, #f5f6f8);
    border: 0.5px solid var(--dsw-alias-border-l1, rgba(0, 0, 0, 0.06));
    border-radius: 8px;
    padding: 8px 12px;
    max-height: 180px;
    overflow-y: auto;
    margin-bottom: 10px;
  }

  .dsh-approval-code {
    margin: 0;
    font-family: var(--ds-font-family-code, monospace);
    font-size: 12px;
    line-height: 18px;
    color: var(--dsw-alias-label-primary, #0f1115);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .dsh-approval-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .dsh-approval-deny-btn {
    padding: 5px 12px;
    font-size: 12px;
    border-radius: 6px;
    background: transparent;
    border: 0.5px solid var(--dsw-alias-border-l2, rgba(0, 0, 0, 0.12));
    color: var(--dsw-alias-label-secondary, #5c626d);
    cursor: pointer;
    transition: all 0.15s;
  }

  .dsh-approval-deny-btn:hover {
    background: var(--dsw-alias-interactive-bg-hover, rgba(0, 0, 0, 0.05));
    color: var(--dsw-alias-label-primary, #0f1115);
  }

  .dsh-approval-allow-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 14px;
    font-size: 12px;
    font-weight: 500;
    border-radius: 6px;
    background: var(--dsw-alias-brand-primary, #4d6bfe);
    border: 1px solid var(--dsw-alias-brand-primary, #4d6bfe);
    color: #ffffff;
    cursor: pointer;
    box-shadow: 0 1px 3px rgba(77, 107, 254, 0.3);
    transition: all 0.15s;
  }

  .dsh-approval-allow-btn:hover {
    filter: brightness(1.08);
  }
</style>
