<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdown";
  import { readFileBase64 } from "$lib/api";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { onDestroy } from "svelte";

  let {
    text = "",
    streaming = false,
    basePath = "",
    workspaceId = "",
    class: className = "",
    lazy = true,
  }: {
    text?: string;
    streaming?: boolean;
    basePath?: string;
    workspaceId?: string;
    class?: string;
    /** When true (default), defer markdown parsing until element enters viewport.
     *  Off-screen entries render raw <pre> until then. Pass false for places where
     *  the content is always visible and lazy-render would just add a re-render. */
    lazy?: boolean;
  } = $props();

  let container: HTMLDivElement | undefined = $state();
  let lazyEl: HTMLElement | undefined = $state();
  /** Set to true once element has been intersection-observed near the viewport.
   *  Sticky — once true, stays true (so scrolling away doesn't un-parse content). */
  let visibleOnce = $state(!lazy);

  // ── Lazy markdown rendering: skip parse until element is near viewport ──
  // Off-screen MarkdownContent shows raw <pre>{text}</pre>. When IntersectionObserver
  // detects approach (within 300px rootMargin), we flip visibleOnce → markdown parse.
  // This eliminates the ~150-200ms total of synchronous md-render at chat-page mount
  // when timeline has dozens of historical entries.
  $effect(() => {
    if (!lazy || streaming || visibleOnce) return;
    const el = lazyEl;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") {
      // No IntersectionObserver (e.g., very old WebView) — fall back to immediate parse.
      visibleOnce = true;
      return;
    }
    const obs = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          visibleOnce = true;
          obs.disconnect();
        }
      },
      { rootMargin: "300px 0px" },
    );
    obs.observe(el);
    return () => obs.disconnect();
  });

  // ── Streaming display: rAF-coalesced text + throttled markdown render ──
  // Text updates are coalesced to one per animation frame so high-frequency token deltas
  // don't thrash text nodes. Markdown parsing is deliberately slower (one render per
  // ~100ms) because it also runs syntax highlighting and DOM sanitization. This gives the
  // user progressive markdown without making long answers compete with token delivery.
  // Init to empty — `$state(text)` would only capture text's value at component creation,
  // and Svelte 5 warns about that pattern. The effect below runs on mount and seeds
  // displayText from current `text` (either via the !streaming branch or firstSyncDone).
  let displayText = $state("");
  let rafId: number | null = null;
  /** Latest `text` captured synchronously in the streaming effect; the coalesced rAF paints
   *  this (always the newest value). Non-reactive — only the effect writes it. */
  let pendingText = "";
  // Non-reactive flag: set/read here doesn't trigger Svelte effect tracking.
  // We use this instead of reading `displayText` inside the effect — reading $state
  // would make the rAF callback's `displayText = text` trigger an effect rerun,
  // wasting one no-op frame per real text change.
  let firstSyncDone = false;

  const STREAM_MARKDOWN_INTERVAL_MS = 100;
  let renderedText = $state("");
  let markdownTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingMarkdownText = "";
  let streamMarkdownSeeded = false;

  function cancelPendingFrame() {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  function cancelPendingMarkdownRender() {
    if (markdownTimer !== null) {
      clearTimeout(markdownTimer);
      markdownTimer = null;
    }
  }

  function flushStreamingMarkdown() {
    markdownTimer = null;
    renderedText = pendingMarkdownText;
  }

  $effect(() => {
    if (!streaming) {
      // Leaving streaming: cancel any pending rAF, sync immediately.
      cancelPendingFrame();
      displayText = text;
      firstSyncDone = false; // reset for next streaming session
      return;
    }
    // First frame on (re)entering streaming with content: sync immediately to avoid
    // visible "first character delay one frame".
    if (!firstSyncDone && text !== "") {
      displayText = text;
      firstSyncDone = true;
      return;
    }
    // Read `text` SYNCHRONOUSLY here so this $effect keeps `text` as a tracked dependency
    // and re-runs on every delta. (Bug fix: previously `text` was read only inside the rAF
    // callback — and the `text !== ""` check above short-circuits once firstSyncDone is true —
    // so after the first character the effect dropped the `text` dep and froze until streaming
    // ended, dumping the full text at once.)
    pendingText = text;
    // Streaming: at most one rAF-pending update; high-frequency tokens coalesce. The rAF paints
    // `pendingText` (latest), so deltas arriving while a frame is pending aren't lost.
    // ⚠️ Do NOT cancel rAF in $effect cleanup — Svelte runs cleanup before each rerun, so
    //    if text ticks faster than vsync we'd repeatedly cancel→reschedule and starve the flush.
    if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null;
        displayText = pendingText;
      });
    }
  });

  // Keep the markdown renderer on a separate, throttled text snapshot. The streaming text
  // itself can change much faster than marked + highlight.js + DOMPurify can reasonably run.
  $effect(() => {
    const currentText = displayText;

    if (!streaming) {
      cancelPendingMarkdownRender();
      renderedText = text;
      pendingMarkdownText = text;
      streamMarkdownSeeded = false;
      return;
    }

    pendingMarkdownText = currentText;
    if (!currentText) {
      cancelPendingMarkdownRender();
      renderedText = "";
      streamMarkdownSeeded = false;
      return;
    }

    // Paint the first meaningful chunk immediately, then throttle subsequent parses.
    if (!streamMarkdownSeeded) {
      renderedText = currentText;
      streamMarkdownSeeded = true;
      return;
    }

    if (markdownTimer === null) {
      markdownTimer = setTimeout(flushStreamingMarkdown, STREAM_MARKDOWN_INTERVAL_MS);
    }
  });

  // Cancel pending work on unmount only (not on every effect rerun).
  onDestroy(() => {
    cancelPendingFrame();
    cancelPendingMarkdownRender();
  });

  // During streaming, render the latest throttled snapshot. For lazy historical entries,
  // keep the existing viewport gate so opening a large chat remains cheap.
  let renderMarkdownNow = $derived(
    (streaming && renderedText !== "") || (!streaming && visibleOnce),
  );
  let html = $derived(renderMarkdownNow && renderedText ? renderMarkdown(renderedText) : "");

  async function openTargetFile(rawPath: string) {
    const raw = rawPath.trim().replace(/^file:\/\//, "");
    if (!raw) return;

    // 1. Dispatch custom event for CodeAsideWorkspace (e.g. Code mode chat)
    if (typeof window !== "undefined") {
      window.dispatchEvent(
        new CustomEvent("agentcabin:open-file", {
          detail: { path: raw, cwd: basePath },
        }),
      );
      window.dispatchEvent(
        new CustomEvent("agentcabin:open-aside-file", {
          detail: { path: raw, cwd: basePath },
        }),
      );
    }

    // 2. Try Work workspace if available
    const wsId =
      workspaceId ||
      (typeof window !== "undefined"
        ? new URLSearchParams(window.location.search).get("workspace") || ""
        : "");

    if (wsId) {
      try {
        const { openWorkFile } = await import("$lib/api/work");
        await openWorkFile(wsId, raw);
        return;
      } catch (e) {
        dbgWarn("markdown", "openWorkFile failed", { wsId, raw, error: e });
      }
    }

    // 3. Fallback to desktop shell openPath
    try {
      const { openPath } = await import("$lib/platform/shell");
      if (raw.startsWith("/") || /^[a-zA-Z]:/.test(raw)) {
        await openPath(raw);
        return;
      }
      if (basePath) {
        const sep = basePath.includes("\\") ? "\\" : "/";
        const full = basePath.replace(/[/\\]$/, "") + sep + raw.replace(/^[/\\]/, "");
        await openPath(full);
        return;
      }
    } catch (e) {
      dbgWarn("markdown", "openPath failed", { raw, error: e });
    }
  }

  $effect(() => {
    const root = container;
    if (!root || !html) return;

    const buttons = root.querySelectorAll<HTMLButtonElement>("[data-code-copy]");
    const cleanups: Array<() => void> = [];

    buttons.forEach((btn) => {
      const handler = async () => {
        const codeEl = btn.closest(".code-block")?.querySelector("pre code");
        if (!codeEl) return;
        try {
          await navigator.clipboard.writeText(codeEl.textContent || "");
          btn.textContent = "Copied!";
          btn.classList.add("copied");
          setTimeout(() => {
            btn.textContent = "Copy";
            btn.classList.remove("copied");
          }, 1500);
        } catch {
          // Silently fail
        }
      };
      btn.addEventListener("click", handler);
      cleanups.push(() => btn.removeEventListener("click", handler));
    });

    const clickHandler = async (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target) return;

      // Check if user clicked a file-path-link
      const fileEl = target.closest<HTMLElement>("[data-file-path]");
      if (fileEl) {
        const filePath = fileEl.getAttribute("data-file-path");
        if (filePath) {
          e.preventDefault();
          e.stopPropagation();
          await openTargetFile(filePath);
          return;
        }
      }

      // Check if user clicked a link
      const linkEl = target.closest<HTMLAnchorElement>("a");
      if (linkEl) {
        const href = linkEl.getAttribute("href");
        if (href) {
          if (/^https?:\/\//i.test(href)) {
            e.preventDefault();
            e.stopPropagation();
            try {
              const { openExternal } = await import("$lib/platform/shell");
              await openExternal(href);
            } catch {
              window.open(href, "_blank");
            }
          } else if (href.startsWith("file://") || href.includes("/")) {
            e.preventDefault();
            e.stopPropagation();
            await openTargetFile(href);
          }
        }
      }
    };
    root.addEventListener("click", clickHandler);
    cleanups.push(() => root.removeEventListener("click", clickHandler));

    const keyHandler = async (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        const target = e.target as HTMLElement | null;
        const fileEl = target?.closest<HTMLElement>("[data-file-path]");
        if (fileEl) {
          const filePath = fileEl.getAttribute("data-file-path");
          if (filePath) {
            e.preventDefault();
            await openTargetFile(filePath);
          }
        }
      }
    };
    root.addEventListener("keydown", keyHandler);
    cleanups.push(() => root.removeEventListener("keydown", keyHandler));

    return () => {
      cleanups.forEach((fn) => fn());
    };
  });

  // Resolve relative image paths against basePath (for Explorer file preview)
  $effect(() => {
    if (!container || !html || !basePath) return;

    const imgs = container.querySelectorAll<HTMLImageElement>("img");
    for (const img of imgs) {
      const src = img.getAttribute("src");
      if (!src) continue;
      // Skip URLs, data URIs, and absolute paths
      if (/^(https?:|data:|blob:)/.test(src)) continue;
      if (src.startsWith("/") || /^[a-zA-Z]:/.test(src)) continue;

      // Construct absolute path: normalize to forward slashes for Rust PathBuf
      const abs = basePath.replace(/\\/g, "/") + "/" + src.replace(/\\/g, "/");
      dbg("markdown", "resolve-img", { src, abs });

      readFileBase64(abs, basePath)
        .then(([base64, mime]) => {
          img.src = `data:${mime};base64,${base64}`;
        })
        .catch((e) => {
          dbgWarn("markdown", "img-load-failed", { src, abs, error: e });
        });
    }
  });
</script>

{#if !renderMarkdownNow}
  <pre
    bind:this={lazyEl}
    class="whitespace-pre-wrap break-words font-sans text-sm leading-relaxed text-foreground/90 m-0 {className}">{displayText}</pre>
{:else}
  <div
    bind:this={container}
    class="prose prose-sm dark:prose-invert max-w-none
      prose-p:text-foreground
      prose-a:text-primary prose-a:underline prose-a:underline-offset-2
      prose-code:before:content-none prose-code:after:content-none
      prose-pre:m-0 prose-pre:p-0 prose-pre:bg-transparent prose-pre:border-0 prose-pre:text-foreground
      prose-li:text-foreground
      {className}"
  >
    {@html html}
  </div>
{/if}
