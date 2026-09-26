<script lang="ts">
  import { untrack } from "svelte";
  import { computeSurfaceRect } from "$lib/utils/browser-surface-bounds";
  import {
    attachEmbeddedBrowser,
    unbindEmbeddedBrowserSurface,
    setEmbeddedBrowserBounds,
    setEmbeddedBrowserVisible,
    type EmbeddedBrowserTab,
    type EmbeddedBrowserTabsChanged,
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
    onTabsChange?: (tabs: EmbeddedBrowserTab[], activeTargetId: string) => void;
    onError?: (message: string) => void;
  }

  let {
    viewId,
    runId,
    initialUrl = "",
    visible = true,
    onEndpoint,
    onLoadingChange,
    onTabsChange,
    onError,
  }: Props = $props();

  let host: HTMLDivElement | null = $state(null);
  let attached = $state(false);
  let activeBindingId = $state("");
  let error = $state("");

  function report(): void {
    if (!host) return;
    const rect = getSurfaceRect();
    // A Work inspector can stay mounted while its aside is collapsed. Sending
    // a zero-sized native bound during that transition can leave Chromium with
    // a blank surface when the panel is shown again. Keep the last usable
    // bounds and let visibility control whether the native page is displayed.
    if (rect.width <= 1 || rect.height <= 1) return;
    if (activeBindingId) void setEmbeddedBrowserBounds(viewId, activeBindingId, rect);
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
    const bindingId = crypto.randomUUID();
    let observer: ResizeObserver | null = null;
    let cancelled = false;
    let attachStarted = false;
    let unlistenTabs: (() => void) | null = null;
    attached = false;
    activeBindingId = bindingId;
    error = "";
    onLoadingChange?.(true);

    const tabsListener = window.agentcabinDesktop?.listen<EmbeddedBrowserTabsChanged>(
      "browser:tabs-changed",
      (event) => {
        if (event.viewId === currentViewId && event.runId === currentRunId) {
          onTabsChange?.(event.tabs, event.activeTargetId);
        }
      },
    );
    void tabsListener?.then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenTabs = unlisten;
    });

    const attachWhenSized = () => {
      if (cancelled || attachStarted) return;
      const rect = getSurfaceRect();
      // Work's inspector starts collapsed. Do not create a WebContentsView at
      // 0×0; wait for the ResizeObserver after the user or browser activity
      // opens the panel, matching Code's first visible browser attachment.
      if (rect.width <= 1 || rect.height <= 1) return;
      attachStarted = true;
      void attachEmbeddedBrowser({
        viewId: currentViewId,
        runId: currentRunId,
        bindingId,
        url: currentUrl || undefined,
        rect,
      })
        .then((result) => {
          if (cancelled) {
            void unbindEmbeddedBrowserSurface(currentViewId, bindingId);
            return;
          }
          attached = true;
          onLoadingChange?.(false);
          onTabsChange?.(result.tabs, result.activeTargetId);
          onEndpoint?.({ ...result.endpoint, targetId: result.targetId });
          report();
        })
        .catch((e: unknown) => {
          error = e instanceof Error ? e.message : String(e);
          onLoadingChange?.(false);
          onError?.(error);
        });
    };

    if (host) {
      observer = new ResizeObserver(() => {
        attachWhenSized();
        report();
      });
      observer.observe(host);
    }
    attachWhenSized();
    window.addEventListener("resize", report);
    window.addEventListener("scroll", report, true);

    return () => {
      cancelled = true;
      unlistenTabs?.();
      observer?.disconnect();
      window.removeEventListener("resize", report);
      window.removeEventListener("scroll", report, true);
      if (activeBindingId === bindingId) activeBindingId = "";
      void unbindEmbeddedBrowserSurface(currentViewId, bindingId);
    };
  });

  $effect(() => {
    const bindingId = activeBindingId;
    if (attached && bindingId) void setEmbeddedBrowserVisible(viewId, bindingId, visible);
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
