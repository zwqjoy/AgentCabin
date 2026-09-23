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

<div class="approval-card" data-risk={riskLevel}>
  <div class="approval-header">
    <div class="approval-title-row">
      <span class="approval-shield-icon" aria-hidden="true">
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
      <span class="approval-title">{t("dsh_approval_title")}</span>
      {#if toolName}
        <span class="approval-tool-badge">{toolName}</span>
      {/if}
    </div>
    {#if formattedArgs}
      <button
        type="button"
        class="approval-copy-btn"
        title={copied ? t("settings_debug_copied") : t("common_copy")}
        onclick={handleCopy}
      >
        <span class="text-[11px] font-mono">{copied ? "已复制" : "复制"}</span>
      </button>
    {/if}
  </div>

  <!-- Description / Reason -->
  <div class="approval-desc">
    {reason || t("dsh_approval_desc")}
  </div>

  <!-- Command or Arguments block -->
  {#if formattedArgs}
    <div class="approval-code-wrap">
      <pre class="approval-code"><code>{formattedArgs}</code></pre>
    </div>
  {/if}

  <!-- Action buttons -->
  <div class="approval-actions">
    {#if onDeny}
      <button type="button" class="approval-deny-btn" onclick={onDeny}>
        {t("dsh_approval_deny")}
      </button>
    {/if}
    {#if onAllow}
      <button type="button" class="approval-allow-btn" onclick={onAllow}>
        <svg
          class="size-3.5"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M3.5 8.5l3 3 6-6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span>{t("dsh_approval_allow")}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  .approval-card {
    border-radius: 12px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    padding: 14px 16px;
    margin: 8px 0;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .approval-card[data-risk="high"] {
    border-color: rgba(239, 68, 68, 0.35);
    background: color-mix(in srgb, rgba(239, 68, 68, 0.04) 100%, transparent);
  }

  .approval-card[data-risk="medium"] {
    border-color: rgba(245, 158, 11, 0.35);
    background: color-mix(in srgb, rgba(245, 158, 11, 0.04) 100%, transparent);
  }

  .approval-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .approval-title-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .approval-shield-icon {
    color: hsl(var(--primary));
    display: flex;
    align-items: center;
  }

  .approval-card[data-risk="high"] .approval-shield-icon {
    color: #ef4444;
  }

  .approval-card[data-risk="medium"] .approval-shield-icon {
    color: #f59e0b;
  }

  .approval-title {
    font-size: 13px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .approval-tool-badge {
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    padding: 1px 6px;
    border-radius: 4px;
    background: hsl(var(--muted));
    color: hsl(var(--muted-foreground));
    border: 1px solid hsl(var(--border) / 0.5);
  }

  .approval-copy-btn {
    color: hsl(var(--muted-foreground));
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: color 0.15s;
  }

  .approval-copy-btn:hover {
    color: hsl(var(--foreground));
  }

  .approval-desc {
    font-size: 12px;
    color: hsl(var(--muted-foreground));
    line-height: 18px;
  }

  .approval-code-wrap {
    max-height: 160px;
    overflow-y: auto;
    border-radius: 8px;
    background: hsl(var(--muted) / 0.5);
    border: 1px solid hsl(var(--border) / 0.5);
    padding: 8px 10px;
  }

  .approval-code {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    line-height: 16px;
    color: hsl(var(--foreground));
    white-space: pre-wrap;
    word-break: break-all;
  }

  .approval-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 2px;
  }

  .approval-deny-btn {
    padding: 5px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    background: transparent;
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted-foreground));
    transition: all 0.15s;
  }

  .approval-deny-btn:hover {
    background: hsl(var(--muted) / 0.5);
    color: hsl(var(--foreground));
  }

  .approval-allow-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground, 0 0% 100%));
    border: 1px solid transparent;
    transition: all 0.15s;
  }

  .approval-allow-btn:hover {
    opacity: 0.9;
  }
</style>
