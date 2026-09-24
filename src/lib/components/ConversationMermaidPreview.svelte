<script lang="ts">
  import DOMPurify from "dompurify";

  let { content, pending = false }: { content: string; pending?: boolean } = $props();
  let svgContent = $state("");
  let renderError = $state("");
  let rendering = $state(false);
  let showSource = $state(false);
  let copied = $state(false);
  let renderGeneration = 0;
  let requestedRenderKey = "";
  let renderedKey = "";

  $effect(() => {
    const source = content.trim();
    const isPending = pending;
    const sourceView = showSource;
    if (!source || isPending || sourceView) {
      rendering = false;
      return;
    }

    const theme =
      typeof document !== "undefined" && document.documentElement.classList.contains("dark")
        ? "dark"
        : "default";
    const key = `${theme}\u0000${source}`;
    if (key === requestedRenderKey || key === renderedKey) return;

    const generation = ++renderGeneration;
    requestedRenderKey = key;
    renderError = "";
    rendering = true;
    void (async () => {
      try {
        const module = await import("mermaid");
        const mermaid = module.default;
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          // Render labels as SVG <text>; DOMPurify's SVG profile intentionally
          // removes <foreignObject>, which Mermaid otherwise uses for labels.
          htmlLabels: false,
          theme,
          fontFamily: "inherit",
        });
        const id = `agentcabin-mermaid-${generation}-${Math.random().toString(36).slice(2, 8)}`;
        const result = await mermaid.render(id, source);
        if (generation !== renderGeneration) return;
        svgContent = DOMPurify.sanitize(result.svg, {
          USE_PROFILES: { svg: true, svgFilters: true },
        });
        renderedKey = key;
      } catch (cause) {
        if (generation !== renderGeneration) return;
        renderError = cause instanceof Error ? cause.message : String(cause);
      } finally {
        if (generation === renderGeneration) rendering = false;
      }
    })();
  });

  async function copySource() {
    try {
      await navigator.clipboard.writeText(content);
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {
      copied = false;
    }
  }
</script>

<section
  class="not-prose my-3 overflow-hidden rounded-xl border border-border bg-background"
  aria-label="Mermaid 图表预览"
>
  <div
    class="flex min-h-10 items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-xs"
  >
    <span class="mr-auto font-medium">Mermaid 图表</span>
    <button
      type="button"
      class="rounded px-2 py-1 hover:bg-muted"
      aria-pressed={!showSource}
      onclick={() => (showSource = false)}>图表</button
    >
    <button
      type="button"
      class="rounded px-2 py-1 hover:bg-muted"
      aria-pressed={showSource}
      onclick={() => (showSource = true)}>源码</button
    >
    <button type="button" class="rounded px-2 py-1 hover:bg-muted" onclick={copySource}>
      {copied ? "已复制" : "复制源码"}
    </button>
  </div>

  {#if showSource}
    <pre class="max-h-[32rem] overflow-auto p-4 text-xs"><code>{content}</code></pre>
  {:else if svgContent}
    <div class="max-h-[min(36rem,75vh)] overflow-auto p-4">
      <div class="mermaid-svg mx-auto flex min-h-24 min-w-0 w-full items-center justify-center">
        <!-- Mermaid output is sanitized to SVG-only markup before insertion. -->
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html svgContent}
      </div>
      {#if rendering || pending}
        <p class="mt-2 text-center text-xs text-muted-foreground" role="status">
          {pending ? "正在接收更新…" : "正在更新图表…"}
        </p>
      {/if}
      {#if renderError}
        <p class="mt-2 text-xs text-destructive">新图表解析失败：{renderError}</p>
      {/if}
    </div>
  {:else if pending}
    <p class="p-4 text-sm text-muted-foreground">正在接收 Mermaid 图表代码…</p>
  {:else if rendering}
    <p class="p-4 text-sm text-muted-foreground">正在渲染图表…</p>
  {:else if renderError}
    <div class="p-4 text-sm text-destructive">
      <p class="font-medium">Mermaid 图表解析失败</p>
      <pre class="mt-2 whitespace-pre-wrap text-xs">{renderError}</pre>
      <button
        type="button"
        class="mt-3 rounded border border-border px-2.5 py-1 text-xs text-foreground hover:bg-accent"
        onclick={() => (showSource = true)}>查看源码</button
      >
    </div>
  {/if}
</section>

<style>
  .mermaid-svg :global(svg) {
    max-width: 100%;
    height: auto;
  }
</style>
