<script lang="ts">
  import { untrack } from "svelte";
  import { computeSurfaceRect } from "$lib/utils/browser-surface-bounds";
  import {
    attachEmbeddedBrowser,
    detachEmbeddedBrowser,
    setEmbeddedBrowserBounds,
    setEmbeddedBrowserVisible,
  } from "$lib/platform/browser";

  interface Props {
    viewId: string;
    runId: string;
    initialUrl?: string;
    visible?: boolean;
    onEndpoint?: (endpoint: {
      host: string;
      port: number;
      token: string;
      targetId: string;
    }) => void;
    onLoadingChange?: (loading: boolean) => void;
    onError?: (message: string) => void;
  }

  let {
    viewId,
    runId,
    initialUrl = "",
    visible = true,
    onEndpoint,
    onLoadingChange,
    onError,
  }: Props = $props();

  let host: HTMLDivElement | null = $state(null);
  let attached = $state(false);
  let error = $state("");

  function report(): void {
    if (!host) return;
    void setEmbeddedBrowserBounds(viewId, getSurfaceRect());
  }

  function getSurfaceRect() {
    if (!host) return { x: 0, y: 0, width: 0, height: 0 };
    const rect = computeSurfaceRect(host.getBoundingClientRect(), {
      x: 0,
      y: 0,
      width: window.innerWidth,
      height: window.innerHeight,
    });
    return rect;
  }

  $effect(() => {
    // BrowserInspector stays mounted while Work switches between runs. The
    // native view and the Rust registration are run-scoped, so a prop change
    // must replace the old attachment instead of leaving the new run on the
    // managed fallback. Do not make a normal URL update recreate the view.
    const currentViewId = viewId;
    const currentRunId = runId;
    const currentUrl = untrack(() => initialUrl);
    let observer: ResizeObserver | null = null;
    let cancelled = false;
    attached = false;
    error = "";
    onLoadingChange?.(true);

    void attachEmbeddedBrowser({
      viewId: currentViewId,
      runId: currentRunId,
      url: currentUrl || undefined,
      rect: getSurfaceRect(),
    })
      .then((result) => {
        if (cancelled) {
          void detachEmbeddedBrowser(currentViewId);
          return;
        }
        attached = true;
        onLoadingChange?.(false);
        onEndpoint?.({ ...result.endpoint, targetId: result.targetId });
        report();
      })
      .catch((e: unknown) => {
        error = e instanceof Error ? e.message : String(e);
        onLoadingChange?.(false);
        onError?.(error);
      });

    if (host) {
      observer = new ResizeObserver(() => report());
      observer.observe(host);
    }
    window.addEventListener("resize", report);
    window.addEventListener("scroll", report, true);

    return () => {
      cancelled = true;
      observer?.disconnect();
      window.removeEventListener("resize", report);
      window.removeEventListener("scroll", report, true);
      void detachEmbeddedBrowser(currentViewId);
    };
  });

  $effect(() => {
    if (attached) void setEmbeddedBrowserVisible(viewId, visible);
  });
</script>

<div
  bind:this={host}
  class="relative h-full min-h-0 w-full bg-white dark:bg-zinc-900"
  data-testid="embedded-browser-surface"
>
  {#if !attached}
    <div class="flex h-full items-center justify-center text-xs text-muted-foreground">
      {error || "正在连接受控浏览器…"}
    </div>
  {/if}
</div>
