<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let {
    open = false,
    toolName = "",
    args,
    result,
    isError = false,
    isRunning = false,
    onClose,
  }: {
    open?: boolean;
    toolName?: string;
    args?: unknown;
    result?: unknown;
    isError?: boolean;
    isRunning?: boolean;
    onClose?: () => void;
  } = $props();

  let formattedArgs = $derived.by(() => {
    if (args === undefined || args === null) return "";
    if (typeof args === "string") {
      try {
        return JSON.stringify(JSON.parse(args), null, 2);
      } catch {
        return args;
      }
    }
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  });

  let formattedResult = $derived.by(() => {
    if (result === undefined || result === null) return "";
    if (typeof result === "string") return result;
    try {
      return JSON.stringify(result, null, 2);
    } catch {
      return String(result);
    }
  });

  let copiedSection = $state<"input" | "output" | null>(null);

  async function handleCopy(type: "input" | "output", text: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      copiedSection = type;
      setTimeout(() => (copiedSection = null), 1500);
    } catch {
      // ignore
    }
  }
</script>

{#if open}
  <!-- Drawer backdrop (mobile / overlay) -->
  <div class="dsh-drawer-backdrop" role="presentation" onclick={onClose}></div>

  <!-- Drawer container -->
  <aside class="dsh-drawer-root" aria-label={t("dsh_details_title")}>
    <div class="dsh-drawer-header">
      <div class="dsh-drawer-title">
        {toolName ? `${t("dsh_details_title")}: ${toolName}` : t("dsh_details_title")}
      </div>
      <button
        type="button"
        class="dsh-drawer-close"
        aria-label={t("dsh_details_close")}
        onclick={onClose}
      >
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path
            d="M4 4l8 8M12 4l-8 8"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </div>

    <div class="dsh-drawer-body">
      {#if !toolName && !formattedArgs && !formattedResult}
        <div class="dsh-drawer-empty">{t("dsh_details_empty")}</div>
      {:else}
        <!-- Input section -->
        {#if formattedArgs}
          <section class="dsh-drawer-section">
            <div class="dsh-drawer-section-header">
              <div class="dsh-drawer-section-label">{t("dsh_details_input")}</div>
              <button
                type="button"
                class="dsh-drawer-copy-btn"
                onclick={() => handleCopy("input", formattedArgs)}
              >
                {copiedSection === "input" ? t("settings_debug_copied") : t("common_copy")}
              </button>
            </div>
            <pre class="dsh-drawer-code"><code>{formattedArgs}</code></pre>
          </section>
        {/if}

        <!-- Output section -->
        <section class="dsh-drawer-section">
          <div class="dsh-drawer-section-header">
            <div class="dsh-drawer-section-label">{t("dsh_details_output")}</div>
            {#if formattedResult}
              <button
                type="button"
                class="dsh-drawer-copy-btn"
                onclick={() => handleCopy("output", formattedResult)}
              >
                {copiedSection === "output" ? t("settings_debug_copied") : t("common_copy")}
              </button>
            {/if}
          </div>

          {#if isRunning}
            <div class="dsh-drawer-running">
              <span class="dsh-drawer-spinner" aria-hidden="true"></span>
              <span>{t("dsh_details_running")}</span>
            </div>
          {:else if formattedResult}
            <pre class="dsh-drawer-code" data-error={isError ? "true" : undefined}><code
                >{formattedResult}</code
              ></pre>
          {:else}
            <div class="dsh-drawer-empty">{t("dsh_details_empty")}</div>
          {/if}
        </section>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .dsh-drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.2);
    backdrop-filter: blur(2px);
    z-index: 90;
  }

  .dsh-drawer-root {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(460px, 90vw);
    border-left: 0.5px solid var(--dsw-alias-border-l2, rgba(0, 0, 0, 0.1));
    background: var(--dsw-alias-bg-base, #ffffff);
    flex-direction: column;
    height: 100%;
    display: flex;
    z-index: 91;
    box-shadow: -4px 0 24px rgba(0, 0, 0, 0.12);
    animation: dsh-drawer-enter 0.2s ease-out;
  }

  @keyframes dsh-drawer-enter {
    from {
      transform: translateX(100%);
    }
    to {
      transform: translateX(0);
    }
  }

  .dsh-drawer-header {
    border-bottom: 0.5px solid var(--dsw-alias-border-l2, rgba(0, 0, 0, 0.1));
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 14px 16px 12px;
    display: flex;
    flex-shrink: 0;
  }

  .dsh-drawer-title {
    color: var(--dsw-alias-label-primary, #0f1115);
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 600;
    line-height: 20px;
    overflow: hidden;
  }

  .dsh-drawer-close {
    width: 28px;
    height: 28px;
    color: var(--dsw-alias-label-secondary, #5c626d);
    cursor: pointer;
    background: transparent;
    border: none;
    border-radius: 999px;
    flex: none;
    place-items: center;
    display: grid;
    transition: background 0.15s;
  }

  .dsh-drawer-close:hover {
    background: var(--dsw-alias-interactive-bg-hover, rgba(0, 0, 0, 0.05));
    color: var(--dsw-alias-label-primary, #0f1115);
  }

  .dsh-drawer-body {
    flex: 1;
    min-height: 0;
    padding: 14px 16px;
    overflow-y: auto;
  }

  .dsh-drawer-empty {
    color: var(--dsw-alias-label-tertiary, #8b909a);
    padding: 12px 0;
    font-size: 13px;
    line-height: 20px;
    text-align: center;
  }

  .dsh-drawer-section {
    margin-bottom: 18px;
  }

  .dsh-drawer-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  .dsh-drawer-section-label {
    color: var(--dsw-alias-label-secondary, #5c626d);
    font-size: 12px;
    font-weight: 600;
    line-height: 18px;
  }

  .dsh-drawer-copy-btn {
    font-size: 11px;
    color: var(--dsw-alias-label-tertiary, #8b909a);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: color 0.15s;
  }

  .dsh-drawer-copy-btn:hover {
    color: var(--dsw-alias-brand-primary, #4d6bfe);
  }

  .dsh-drawer-code {
    background: var(--dsw-alias-markdown-code-block, #f5f6f8);
    font-family: var(--ds-font-family-code, monospace);
    color: var(--dsw-alias-label-primary, #0f1115);
    white-space: pre-wrap;
    word-break: break-word;
    border-radius: 8px;
    margin: 0;
    padding: 12px 14px;
    font-size: 12px;
    line-height: 18px;
    max-height: 280px;
    overflow-y: auto;
    border: 0.5px solid var(--dsw-alias-border-l1, rgba(0, 0, 0, 0.06));
  }

  .dsh-drawer-code[data-error="true"] {
    color: var(--dsw-alias-state-error-primary, #ef4444);
    border-color: color-mix(
      in srgb,
      var(--dsw-alias-state-error-primary, #ef4444) 30%,
      transparent
    );
  }

  .dsh-drawer-running {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    color: var(--dsw-alias-brand-primary, #4d6bfe);
    font-size: 13px;
  }

  .dsh-drawer-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, var(--dsw-alias-brand-primary, #4d6bfe) 25%, transparent);
    border-top-color: var(--dsw-alias-brand-primary, #4d6bfe);
    border-radius: 50%;
    animation: dsh-drawer-spin 0.7s linear infinite;
  }

  @keyframes dsh-drawer-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
