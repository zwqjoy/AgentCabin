<script lang="ts">
  import { onDestroy } from "svelte";
  import { createRoot, type Root } from "react-dom/client";
  import React from "react";
  import { DshChatContainer, type DshTurnItem } from "./DshChatContainer";

  let {
    turns = [],
    isRunning = false,
    onOpenFile,
    onCopyText,
  }: {
    turns: DshTurnItem[];
    isRunning?: boolean;
    onOpenFile?: (path: string) => void;
    onCopyText?: (text: string) => void;
  } = $props();

  let containerEl = $state<HTMLDivElement | null>(null);
  let reactRoot: Root | null = null;

  async function defaultOpenFile(p: string) {
    if (onOpenFile) {
      onOpenFile(p);
      return;
    }
    const wsId =
      typeof window !== "undefined"
        ? new URLSearchParams(window.location.search).get("workspace") || ""
        : "";
    if (wsId) {
      try {
        const { openWorkFile } = await import("$lib/api/work");
        await openWorkFile(wsId, p);
        return;
      } catch {
        // Fallback to openPath
      }
    }
    try {
      const { openPath } = await import("$lib/platform/shell");
      await openPath(p);
    } catch {
      // Ignore
    }
  }

  function renderReact() {
    if (!reactRoot && containerEl) {
      reactRoot = createRoot(containerEl);
    }
    if (reactRoot) {
      reactRoot.render(
        React.createElement(DshChatContainer, {
          turns,
          isRunning,
          onOpenFile: defaultOpenFile,
          onCopyText,
        }),
      );
    }
  }

  $effect(() => {
    // Re-render whenever turns, isRunning, or container changes
    if (containerEl) {
      renderReact();
    }
  });

  onDestroy(() => {
    if (reactRoot) {
      reactRoot.unmount();
      reactRoot = null;
    }
  });
</script>

<div bind:this={containerEl} class="w-full h-full min-h-0 flex-1 relative"></div>
