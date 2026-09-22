<script lang="ts">
  import { injectCsp } from "$lib/utils/html-security";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    content: string;
    fileName?: string;
  }

  let { content, fileName = "output.html" }: Props = $props();

  // Keep the artifact inline so the viewer never grants the iframe a file URL.
  const MAX_HTML_BYTES = 5 * 1024 * 1024;

  let reloadKey = $state(0);
  let iframeLoading = $state(true);

  let isOversized = $derived(content.length > MAX_HTML_BYTES);
  let safeHtml = $derived(isOversized ? "" : injectCsp(content));

  function handleReload() {
    iframeLoading = true;
    reloadKey += 1;
  }

  function handleIframeLoad() {
    iframeLoading = false;
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background">
  <!-- Toolbar -->
  <div
    class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card/60 px-4 py-2"
  >
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <span
        class="inline-flex items-center gap-1 rounded bg-muted px-2 py-0.5 font-mono text-[10px] uppercase text-muted-foreground"
      >
        {t("htmlViewer_sandboxBadge")}
      </span>
      <span class="truncate max-w-xs">{fileName}</span>
    </div>

    <div class="flex items-center gap-1.5">
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={handleReload}
        title={t("htmlViewer_reloadTitle")}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67" />
        </svg>
        <span>{t("htmlViewer_reload")}</span>
      </button>
    </div>
  </div>

  <!-- Viewer Container -->
  <div class="relative flex-1 overflow-hidden bg-background">
    {#if isOversized}
      <div
        class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center text-xs text-muted-foreground"
      >
        <div class="rounded-full bg-amber-500/10 p-3 text-amber-500">
          <svg class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
            />
          </svg>
        </div>
        <p class="font-medium text-foreground">{t("htmlViewer_oversizedTitle")}</p>
        <p class="max-w-md">{t("htmlViewer_oversizedDesc")}</p>
      </div>
    {:else}
      {#if iframeLoading}
        <div
          class="absolute inset-0 z-10 flex items-center justify-center bg-background/50 backdrop-blur-[1px]"
        >
          <div
            class="h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"
          ></div>
        </div>
      {/if}

      {#key reloadKey}
        <iframe
          title={t("htmlViewer_previewTitle")}
          srcdoc={safeHtml}
          sandbox="allow-scripts allow-modals"
          referrerpolicy="no-referrer"
          onload={handleIframeLoad}
          class="h-full w-full border-0 bg-white"
        ></iframe>
      {/key}
    {/if}
  </div>
</div>
