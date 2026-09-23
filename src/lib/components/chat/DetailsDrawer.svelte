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
  <div class="chat-drawer-backdrop" role="presentation" onclick={onClose}></div>

  <!-- Drawer container -->
  <aside class="chat-drawer-root" aria-label={t("dsh_details_title")}>
    <div class="chat-drawer-header">
      <div class="chat-drawer-title">
        {toolName ? `${t("dsh_details_title")}: ${toolName}` : t("dsh_details_title")}
      </div>
      <button
        type="button"
        class="chat-drawer-close"
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

    <div class="chat-drawer-body">
      {#if !toolName && !formattedArgs && !formattedResult}
        <div class="chat-drawer-empty">{t("dsh_details_empty")}</div>
      {:else}
        <!-- Input section -->
        {#if formattedArgs}
          <section class="chat-drawer-section">
            <div class="chat-drawer-section-header">
              <div class="chat-drawer-section-label">{t("dsh_details_input")}</div>
              <button
                type="button"
                class="chat-drawer-copy-btn"
                onclick={() => handleCopy("input", formattedArgs)}
              >
                {copiedSection === "input" ? t("settings_debug_copied") : t("common_copy")}
              </button>
            </div>
            <pre class="chat-drawer-code"><code>{formattedArgs}</code></pre>
          </section>
        {/if}

        <!-- Output section -->
        <section class="chat-drawer-section">
          <div class="chat-drawer-section-header">
            <div class="chat-drawer-section-label">{t("dsh_details_output")}</div>
            {#if formattedResult}
              <button
                type="button"
                class="chat-drawer-copy-btn"
                onclick={() => handleCopy("output", formattedResult)}
              >
                {copiedSection === "output" ? t("settings_debug_copied") : t("common_copy")}
              </button>
            {/if}
          </div>

          {#if isRunning}
            <div class="chat-drawer-running">
              <span class="chat-drawer-spinner" aria-hidden="true"></span>
              <span>{t("dsh_details_running")}</span>
            </div>
          {:else if formattedResult}
            <pre class="chat-drawer-code" data-error={isError ? "true" : undefined}><code
                >{formattedResult}</code
              ></pre>
          {:else}
            <div class="chat-drawer-empty">{t("dsh_details_empty")}</div>
          {/if}
        </section>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .chat-drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.2);
    backdrop-filter: blur(2px);
    z-index: 90;
  }

  .chat-drawer-root {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(460px, 90vw);
    border-left: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    flex-direction: column;
    height: 100%;
    display: flex;
    z-index: 91;
    box-shadow: -4px 0 24px rgba(0, 0, 0, 0.12);
    animation: chat-drawer-enter 0.2s ease-out;
  }

  @keyframes chat-drawer-enter {
    from {
      transform: translateX(100%);
    }
    to {
      transform: translateX(0);
    }
  }

  .chat-drawer-header {
    border-bottom: 1px solid hsl(var(--border));
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 14px 16px 12px;
    display: flex;
    flex-shrink: 0;
  }

  .chat-drawer-title {
    color: hsl(var(--foreground));
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 600;
    line-height: 20px;
    overflow: hidden;
  }

  .chat-drawer-close {
    width: 28px;
    height: 28px;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    background: transparent;
    border: none;
    border-radius: 999px;
    flex: none;
    place-items: center;
    display: grid;
    transition: background 0.15s;
  }

  .chat-drawer-close:hover {
    background: hsl(var(--muted) / 0.5);
    color: hsl(var(--foreground));
  }

  .chat-drawer-body {
    flex: 1;
    min-height: 0;
    padding: 14px 16px;
    overflow-y: auto;
  }

  .chat-drawer-empty {
    color: hsl(var(--muted-foreground));
    padding: 12px 0;
    font-size: 13px;
    line-height: 20px;
    text-align: center;
  }

  .chat-drawer-section {
    margin-bottom: 18px;
  }

  .chat-drawer-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  .chat-drawer-section-label {
    color: hsl(var(--muted-foreground));
    font-size: 12px;
    font-weight: 600;
    line-height: 18px;
  }

  .chat-drawer-copy-btn {
    font-size: 11px;
    color: hsl(var(--muted-foreground));
    background: transparent;
    border: none;
    cursor: pointer;
    transition: color 0.15s;
  }

  .chat-drawer-copy-btn:hover {
    color: hsl(var(--primary));
  }

  .chat-drawer-code {
    background: hsl(var(--muted) / 0.5);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: hsl(var(--foreground));
    white-space: pre-wrap;
    word-break: break-word;
    border-radius: 8px;
    margin: 0;
    padding: 12px 14px;
    font-size: 12px;
    line-height: 18px;
    max-height: 280px;
    overflow-y: auto;
    border: 1px solid hsl(var(--border) / 0.5);
  }

  .chat-drawer-code[data-error="true"] {
    color: hsl(var(--destructive, 0 84.2% 60.2%));
    border-color: hsl(var(--destructive, 0 84.2% 60.2%) / 0.3);
  }

  .chat-drawer-running {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    color: hsl(var(--primary));
    font-size: 13px;
  }

  .chat-drawer-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid hsl(var(--primary) / 0.25);
    border-top-color: hsl(var(--primary));
    border-radius: 50%;
    animation: chat-drawer-spin 0.7s linear infinite;
  }

  @keyframes chat-drawer-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
