<script lang="ts">
  import CodeEditor from "$lib/components/CodeEditor.svelte";

  interface Props {
    content: string;
    fileName?: string;
  }

  let { content, fileName = "diagram.mermaid" }: Props = $props();

  let mode = $state<"diagram" | "source">("diagram");
  let svgContent = $state("");
  let renderError = $state("");
  let rendering = $state(false);
  let zoomLevel = $state(1);
  let copied = $state(false);
  let containerRef = $state<HTMLDivElement>();

  let cleanCode = $derived.by(() => {
    let raw = content.trim();
    // Strip markdown code fences if present (e.g. ```mermaid ... ```)
    if (raw.startsWith("```")) {
      const lines = raw.split("\n");
      if (lines.length >= 2 && lines[lines.length - 1].trim() === "```") {
        raw = lines.slice(1, -1).join("\n").trim();
      }
    }
    return raw;
  });

  async function renderDiagram(code: string) {
    if (!code) {
      svgContent = "";
      renderError = "";
      return;
    }
    rendering = true;
    renderError = "";
    try {
      const mermaidModule = await import("mermaid");
      const mermaid = mermaidModule.default || mermaidModule;
      mermaid.initialize({
        startOnLoad: false,
        theme: document.documentElement.classList.contains("dark") ? "dark" : "default",
        securityLevel: "loose",
        fontFamily: "inherit",
      });

      const id = `mermaid-preview-${Math.random().toString(36).slice(2, 9)}`;
      const { svg } = await mermaid.render(id, code);
      svgContent = svg;
    } catch (cause) {
      renderError = cause instanceof Error ? cause.message : String(cause);
      svgContent = "";
    } finally {
      rendering = false;
    }
  }

  $effect(() => {
    if (mode === "diagram") {
      void renderDiagram(cleanCode);
    }
  });

  function zoomIn() {
    zoomLevel = Math.min(zoomLevel + 0.2, 3);
  }

  function zoomOut() {
    zoomLevel = Math.max(zoomLevel - 0.2, 0.4);
  }

  function resetZoom() {
    zoomLevel = 1;
  }

  async function copySvgOrCode() {
    try {
      const toCopy = mode === "diagram" && svgContent ? svgContent : content;
      await navigator.clipboard.writeText(toCopy);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // ignore
    }
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background">
  <!-- Toolbar -->
  <div
    class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card/60 px-4 py-2.5"
  >
    <div class="flex items-center gap-1.5 rounded-lg border border-border/80 bg-muted/50 p-0.5">
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mode === 'diagram'
          ? 'bg-background text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (mode = "diagram")}
      >
        图表视图
      </button>
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mode === 'source'
          ? 'bg-background text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (mode = "source")}
      >
        源码视图
      </button>
    </div>

    <div class="flex items-center gap-2">
      {#if mode === "diagram" && svgContent}
        <div class="flex items-center gap-1 border-r border-border/60 pr-2">
          <button
            type="button"
            class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={zoomOut}
            title="缩小"
          >
            -
          </button>
          <button
            type="button"
            class="px-2 py-1 text-xs font-mono text-muted-foreground hover:text-foreground"
            onclick={resetZoom}
            title="重置缩放"
          >
            {Math.round(zoomLevel * 100)}%
          </button>
          <button
            type="button"
            class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={zoomIn}
            title="放大"
          >
            +
          </button>
        </div>
      {/if}

      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => void copySvgOrCode()}
      >
        {#if copied}
          <svg
            class="h-3.5 w-3.5 text-emerald-500"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="20 6 9 17 4 12"></polyline>
          </svg>
          <span class="text-emerald-600 dark:text-emerald-400">已复制</span>
        {:else}
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
          <span>{mode === "diagram" ? "复制 SVG" : "复制代码"}</span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Content Container -->
  <div class="flex-1 overflow-auto p-4 overscroll-contain" bind:this={containerRef}>
    {#if mode === "diagram"}
      {#if rendering}
        <div class="flex h-full items-center justify-center text-xs text-muted-foreground">
          正在渲染图表...
        </div>
      {:else if renderError}
        <div class="rounded-xl border border-red-500/20 bg-red-500/5 p-4 text-xs text-red-500">
          <div class="font-semibold">Mermaid 图表解析错误</div>
          <pre class="mt-2 whitespace-pre-wrap font-mono text-[11px] opacity-80">{renderError}</pre>
          <div class="mt-3">
            <button
              type="button"
              class="rounded-md border border-border bg-background px-2.5 py-1 text-xs text-foreground hover:bg-accent"
              onclick={() => (mode = "source")}
            >
              查看源码
            </button>
          </div>
        </div>
      {:else if svgContent}
        <div
          class="flex min-h-full items-center justify-center transition-transform duration-100 ease-out"
          style="transform: scale({zoomLevel}); transform-origin: center center;"
        >
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html svgContent}
        </div>
      {:else}
        <div class="flex h-full items-center justify-center text-xs text-muted-foreground">
          无可用图表内容
        </div>
      {/if}
    {:else}
      <div class="h-full rounded-xl border border-border/60 overflow-hidden">
        <CodeEditor
          {content}
          filePath={fileName}
          readonly={true}
          class="h-full text-xs font-mono"
        />
      </div>
    {/if}
  </div>
</div>
