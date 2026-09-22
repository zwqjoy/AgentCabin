<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  interface Props {
    /** Raw base64-encoded PDF content (without data: prefix) */
    base64: string;
  }

  let { base64 }: Props = $props();

  // ── State ──────────────────────────────────────────────────────────────────
  let containerRef: HTMLDivElement | undefined = $state();
  let canvasRef: HTMLCanvasElement | undefined = $state();
  let pdfDoc: import("pdfjs-dist").PDFDocumentProxy | undefined = $state();
  let currentPage = $state(1);
  let totalPages = $state(0);
  let zoomLevel = $state(1.2);
  let rendering = $state(false);
  let loadError = $state("");
  let pageInput = $state("1");
  let renderTask: import("pdfjs-dist").RenderTask | undefined;

  // ── Load PDF ───────────────────────────────────────────────────────────────
  async function loadPdf(b64: string) {
    if (!b64) return;
    loadError = "";
    rendering = true;
    try {
      const { getDocument, GlobalWorkerOptions } = await import("pdfjs-dist");

      // Use bundled worker via Vite URL import
      const workerUrl = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).href;
      GlobalWorkerOptions.workerSrc = workerUrl;

      // Decode base64 → Uint8Array
      const binaryStr = atob(b64);
      const bytes = new Uint8Array(binaryStr.length);
      for (let i = 0; i < binaryStr.length; i++) {
        bytes[i] = binaryStr.charCodeAt(i);
      }

      pdfDoc = await getDocument({ data: bytes }).promise;
      totalPages = pdfDoc.numPages;
      currentPage = 1;
      pageInput = "1";
      await renderPage(1);
    } catch (cause) {
      loadError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      rendering = false;
    }
  }

  // ── Render Page ────────────────────────────────────────────────────────────
  async function renderPage(num: number) {
    if (!pdfDoc || !canvasRef) return;
    if (renderTask) {
      renderTask.cancel();
      renderTask = undefined;
    }
    rendering = true;
    try {
      const page = await pdfDoc.getPage(num);
      const viewport = page.getViewport({ scale: zoomLevel });
      const canvas = canvasRef;

      canvas.width = viewport.width;
      canvas.height = viewport.height;

      // pdfjs-dist v6: pass canvas directly (canvasContext is optional/legacy)
      renderTask = page.render({ canvas, viewport });
      await renderTask.promise;
    } catch (cause: unknown) {
      // RenderingCancelledException is normal when navigating quickly
      const msg = cause instanceof Error ? cause.message : String(cause);
      if (!msg.includes("Rendering cancelled")) {
        loadError = msg;
      }
    } finally {
      rendering = false;
      renderTask = undefined;
    }
  }

  // ── Navigation ─────────────────────────────────────────────────────────────
  function goTo(n: number) {
    const clamped = Math.max(1, Math.min(totalPages, n));
    if (clamped === currentPage) return;
    currentPage = clamped;
    pageInput = String(clamped);
    void renderPage(clamped);
  }

  function handlePageInputBlur() {
    const n = parseInt(pageInput, 10);
    if (!isNaN(n)) goTo(n);
    else pageInput = String(currentPage);
  }

  function handlePageInputKey(e: KeyboardEvent) {
    if (e.key === "Enter") handlePageInputBlur();
  }

  // ── Zoom ───────────────────────────────────────────────────────────────────
  function zoomIn() {
    zoomLevel = Math.min(parseFloat((zoomLevel + 0.25).toFixed(2)), 4);
    void renderPage(currentPage);
  }

  function zoomOut() {
    zoomLevel = Math.max(parseFloat((zoomLevel - 0.25).toFixed(2)), 0.4);
    void renderPage(currentPage);
  }

  function resetZoom() {
    zoomLevel = 1.2;
    void renderPage(currentPage);
  }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") goTo(currentPage + 1);
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") goTo(currentPage - 1);
    else if (e.key === "+" || e.key === "=") zoomIn();
    else if (e.key === "-") zoomOut();
  }

  // ── Effects ────────────────────────────────────────────────────────────────
  $effect(() => {
    if (base64) {
      void loadPdf(base64);
    }
  });

  $effect(() => {
    if (pdfDoc && canvasRef) {
      void renderPage(currentPage);
    }
  });

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleKeydown);
    renderTask?.cancel();
    pdfDoc?.cleanup();
  });
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background">
  <!-- Toolbar -->
  <div
    class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card/60 px-4 py-2.5"
  >
    <!-- Page navigation -->
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
        onclick={() => goTo(currentPage - 1)}
        disabled={currentPage <= 1 || rendering}
        title="上一页 ←"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
        >
          <polyline points="15 18 9 12 15 6"></polyline>
        </svg>
      </button>

      <div class="flex items-center gap-1.5 text-xs">
        <input
          type="text"
          class="h-7 w-10 rounded-md border border-border bg-background px-1.5 text-center text-xs font-medium text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          bind:value={pageInput}
          onblur={handlePageInputBlur}
          onkeydown={handlePageInputKey}
          disabled={!pdfDoc}
          aria-label="当前页码"
        />
        <span class="text-muted-foreground">/</span>
        <span class="font-medium text-foreground">{totalPages || "—"}</span>
      </div>

      <button
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
        onclick={() => goTo(currentPage + 1)}
        disabled={currentPage >= totalPages || rendering}
        title="下一页 →"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
        >
          <polyline points="9 18 15 12 9 6"></polyline>
        </svg>
      </button>
    </div>

    <!-- Zoom controls -->
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={zoomOut}
        title="缩小 -"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
        >
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
          <line x1="8" y1="11" x2="14" y2="11"></line>
        </svg>
      </button>

      <button
        type="button"
        class="min-w-[3.5rem] rounded-md border border-border px-2 py-1 text-center text-xs font-mono text-muted-foreground transition-colors hover:text-foreground"
        onclick={resetZoom}
        title="重置缩放"
      >
        {Math.round(zoomLevel * 100)}%
      </button>

      <button
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={zoomIn}
        title="放大 +"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
        >
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
          <line x1="11" y1="8" x2="11" y2="14"></line>
          <line x1="8" y1="11" x2="14" y2="11"></line>
        </svg>
      </button>
    </div>
  </div>

  <!-- Canvas Area -->
  <div
    class="flex flex-1 flex-col items-center overflow-auto overscroll-contain bg-[hsl(var(--muted)/0.4)] p-6"
    bind:this={containerRef}
  >
    {#if loadError}
      <div
        class="mt-10 max-w-sm rounded-xl border border-red-500/20 bg-red-500/5 p-5 text-center text-xs text-red-500"
      >
        <div class="font-semibold">PDF 加载失败</div>
        <pre class="mt-2 whitespace-pre-wrap font-mono opacity-80">{loadError}</pre>
      </div>
    {:else if !pdfDoc}
      <div class="mt-10 text-xs text-muted-foreground">
        {#if rendering}
          正在加载 PDF…
        {:else}
          无可用 PDF 文档
        {/if}
      </div>
    {:else}
      <!-- Render indicator overlay -->
      {#if rendering}
        <div
          class="absolute inset-0 z-10 flex items-center justify-center bg-background/30 backdrop-blur-[1px]"
        >
          <span
            class="rounded-lg bg-card px-3 py-1.5 text-xs text-muted-foreground shadow-md border border-border"
            >渲染中…</span
          >
        </div>
      {/if}

      <!-- PDF Canvas -->
      <div class="relative w-fit rounded-lg shadow-2xl ring-1 ring-border/40">
        <canvas
          bind:this={canvasRef}
          class="block max-w-full rounded-lg"
          aria-label={`PDF 第 ${currentPage} 页，共 ${totalPages} 页`}
        ></canvas>
      </div>

      <!-- Page indicator bottom -->
      {#if totalPages > 1}
        <p class="mt-4 text-xs text-muted-foreground">
          第 {currentPage} 页 / 共 {totalPages} 页 · 使用 ← → 键翻页
        </p>
      {/if}
    {/if}
  </div>
</div>
