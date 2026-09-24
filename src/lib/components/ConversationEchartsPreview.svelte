<script lang="ts">
  import { onMount } from "svelte";
  import {
    PREVIEW_DEFAULT_HEIGHT,
    PREVIEW_MIN_HEIGHT,
    injectPreviewBridge,
    parsePreviewHeightMessage,
  } from "$lib/utils/html-preview-bridge";
  import { injectCsp } from "$lib/utils/html-security";

  let { content, pending = false }: { content: string; pending?: boolean } = $props();
  let echartsSource = $state("");
  let rendererError = $state("");
  let showCode = $state(false);
  let copyStatus = $state("");
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let measured = $state({ src: "", height: PREVIEW_DEFAULT_HEIGHT });

  onMount(() => {
    let cancelled = false;
    void import("echarts/dist/echarts.min.js?raw")
      .then((module) => {
        if (!cancelled) echartsSource = module.default;
      })
      .catch(() => {
        if (!cancelled) rendererError = "无法加载 ECharts 图表渲染器。";
      });
    return () => {
      cancelled = true;
    };
  });

  const oversized = $derived(
    new TextEncoder().encode(content).byteLength + echartsSource.length > 5 * 1024 * 1024,
  );

  // The option runs only in an opaque-origin iframe. Escaping the script end
  // marker keeps option strings/comments from terminating the generated script.
  const document = $derived.by(() => {
    if (oversized || pending || !echartsSource) return "";
    const safeLibrary = echartsSource.replace(/<\/script/gi, "<\\/script");
    const safeOption = content.replace(/<\/script/gi, "<\\/script");
    const runtime = `(function(){
      var root = document.getElementById("chart");
      try {
        var option = (${safeOption});
        root.textContent = "";
        var chart = echarts.init(root, null, { renderer: "canvas" });
        chart.setOption(option, true);
        function resize(){ chart.resize(); }
        window.addEventListener("resize", resize);
        if (typeof ResizeObserver === "function") new ResizeObserver(resize).observe(root);
      } catch (error) {
        root.textContent = "图表配置无法渲染：" + (error && error.message ? error.message : String(error));
        root.style.cssText = "padding:16px;color:#a14e50;font:14px/1.5 sans-serif;white-space:pre-wrap";
      }
    })();`;
    const html = `<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="margin:0;background:#fff"><div id="chart" style="width:100%;height:360px;min-height:240px;padding:16px;box-sizing:border-box;color:#5f6670;font:14px/1.5 sans-serif">正在加载图表…</div><script>${safeLibrary}</scr${"ipt"}><script>${runtime}</scr${"ipt"}></body></html>`;
    return injectPreviewBridge(injectCsp(html));
  });

  const frameHeight = $derived(
    measured.src === document ? measured.height : PREVIEW_DEFAULT_HEIGHT,
  );
  let recentParentHeights: number[] = [];
  $effect(() => {
    void document;
    recentParentHeights = [];
  });

  function handleFrameHeight(event: MessageEvent) {
    if (event.source !== frameEl?.contentWindow) return;
    const height = parsePreviewHeightMessage(event.data);
    if (height === null) return;
    const target = Math.max(PREVIEW_MIN_HEIGHT, height);
    if (measured.src === document && Math.abs(measured.height - target) < 1) return;
    if (recentParentHeights.includes(target)) {
      measured = { src: document, height: Math.max(target, measured.height) };
      return;
    }
    recentParentHeights.push(target);
    if (recentParentHeights.length > 5) recentParentHeights.shift();
    measured = { src: document, height: target };
  }

  $effect(() => {
    window.addEventListener("message", handleFrameHeight);
    return () => window.removeEventListener("message", handleFrameHeight);
  });

  async function copy() {
    try {
      await navigator.clipboard.writeText(content);
      copyStatus = "已复制";
    } catch {
      copyStatus = "复制失败";
    }
  }
</script>

<section
  class="not-prose my-3 overflow-hidden rounded-xl border border-border bg-background"
  aria-label="ECharts 图表预览"
>
  <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-2 text-xs">
    <span class="mr-auto font-medium">{pending ? "正在生成图表配置…" : "ECharts 图表"}</span>
    <button
      type="button"
      class="rounded px-2 py-1 hover:bg-muted"
      aria-pressed={!showCode}
      onclick={() => (showCode = false)}>预览</button
    >
    <button
      type="button"
      class="rounded px-2 py-1 hover:bg-muted"
      aria-pressed={showCode}
      onclick={() => (showCode = true)}>配置</button
    >
    <button type="button" class="rounded px-2 py-1 hover:bg-muted" onclick={copy}>复制配置</button>
    <span role="status">{copyStatus}</span>
  </div>
  {#if showCode}
    <pre class="max-h-[32rem] overflow-auto p-4 text-xs"><code>{content}</code></pre>
  {:else if oversized}
    <p class="p-4 text-sm text-muted-foreground">配置超过预览大小限制，请查看配置。</p>
  {:else if pending}
    <p class="p-4 text-sm text-muted-foreground">正在接收 ECharts 配置…</p>
  {:else if rendererError}
    <p class="p-4 text-sm text-destructive">{rendererError}</p>
  {:else if !echartsSource}
    <p class="p-4 text-sm text-muted-foreground">正在加载图表渲染器…</p>
  {:else}
    <iframe
      bind:this={frameEl}
      title="ECharts 图表预览"
      srcdoc={document}
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      style="height: {frameHeight}px"
      class="block max-h-[min(32rem,70vh)] w-full border-0 bg-white"
    ></iframe>
  {/if}
</section>
