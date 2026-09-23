<script lang="ts">
  import {
    PREVIEW_DEFAULT_HEIGHT,
    PREVIEW_MIN_HEIGHT,
    injectPreviewBridge,
    parsePreviewHeightMessage,
  } from "$lib/utils/html-preview-bridge";
  import { injectCsp } from "$lib/utils/html-security";
  let { content, pending = false }: { content: string; pending?: boolean } = $props();
  let showCode = $state(false);
  let copyStatus = $state("");
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let measured = $state({ src: "", height: PREVIEW_DEFAULT_HEIGHT });
  const oversized = $derived(new TextEncoder().encode(content).length > 5 * 1024 * 1024);

  // While the block is still streaming, mirror the partial markup into the
  // frame so the preview builds up progressively (like streaming chat apps)
  // instead of staying blank until the fence closes. Each srcdoc swap reloads
  // the document, so updates are throttled; the finished document always
  // renders immediately.
  const STREAM_SYNC_INTERVAL = 500;
  let partial = $state("");
  let lastPartialSync = 0;
  $effect(() => {
    const latest = content;
    if (!pending) {
      partial = "";
      return;
    }
    const elapsed = Date.now() - lastPartialSync;
    if (elapsed >= STREAM_SYNC_INTERVAL) {
      lastPartialSync = Date.now();
      partial = latest;
      return;
    }
    const timer = setTimeout(() => {
      lastPartialSync = Date.now();
      partial = latest;
    }, STREAM_SYNC_INTERVAL - elapsed);
    return () => clearTimeout(timer);
  });

  const renderSource = $derived(pending ? partial : content);
  // Place CSP before all generated markup, including any content before its <head>.
  const document = $derived(oversized ? "" : injectPreviewBridge(injectCsp("") + renderSource));
  // Heights belong to one rendered document; a new one starts from the default
  // until its own measurement arrives.
  const frameHeight = $derived(
    measured.src === document ? measured.height : PREVIEW_DEFAULT_HEIGHT,
  );

  let recentParentHeights: number[] = [];
  $effect(() => {
    // Reset oscillation history when the rendered document changes
    void document;
    recentParentHeights = [];
  });

  // The sandboxed frame reports its own content height so the preview hugs the
  // chart instead of leaving a blank band below it.
  function handleFrameHeight(event: MessageEvent) {
    if (event.source !== frameEl?.contentWindow) return;
    const height = parsePreviewHeightMessage(event.data);
    if (height !== null) {
      const target = Math.max(PREVIEW_MIN_HEIGHT, height);
      if (measured.src === document && Math.abs(measured.height - target) < 1) return;
      if (recentParentHeights.includes(target)) {
        // Oscillation guard in host: lock to maximum height to stop layout vibration
        const locked = Math.max(target, measured.height);
        measured = { src: document, height: locked };
        return;
      }
      recentParentHeights.push(target);
      if (recentParentHeights.length > 5) recentParentHeights.shift();
      measured = { src: document, height: target };
    }
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
  aria-label="HTML 图表预览"
>
  <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-2 text-xs">
    <span class="mr-auto font-medium">{pending ? "正在生成图表…" : "HTML 预览"}</span>
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
      onclick={() => (showCode = true)}>代码</button
    >
    <button type="button" class="rounded px-2 py-1 hover:bg-muted" onclick={copy}>复制代码</button>
    <span role="status">{copyStatus}</span>
  </div>
  {#if showCode}
    <pre class="max-h-[32rem] overflow-auto p-4 text-xs"><code>{content}</code></pre>
  {:else if oversized}
    <p class="p-4 text-sm text-muted-foreground">内容超过 5 MB，请查看代码。</p>
  {:else if pending && !partial}
    <p class="p-4 text-sm text-muted-foreground">正在接收 HTML 内容…</p>
  {:else}
    <iframe
      bind:this={frameEl}
      title="HTML 图表预览"
      srcdoc={document}
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      style="height: {frameHeight}px"
      class="block max-h-[min(32rem,70vh)] w-full border-0 bg-white"
    ></iframe>
  {/if}
</section>
