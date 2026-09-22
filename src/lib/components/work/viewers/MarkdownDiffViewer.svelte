<script lang="ts">
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { createPatch } from "diff";

  interface Props {
    content: string;
    originalContent?: string;
    fileName?: string;
    isDiff?: boolean;
  }

  let { content, originalContent = "", fileName = "document.md", isDiff = false }: Props = $props();

  let hasDiff = $derived(Boolean(originalContent && originalContent !== content) || isDiff);
  let mode = $state<"rendered" | "source" | "diff">("rendered");
  let copied = $state(false);

  let wordCount = $derived.by(() => {
    const text = content.trim();
    if (!text) return 0;
    return text.length;
  });

  // Calculate diff lines
  interface DiffLine {
    type: "add" | "del" | "context" | "header";
    text: string;
    oldNum?: number;
    newNum?: number;
  }

  let diffLines = $derived.by((): DiffLine[] => {
    if (!hasDiff) return [];
    let patchText = "";
    if (isDiff) {
      patchText = content;
    } else {
      patchText = createPatch(fileName, originalContent, content, "Original", "Current");
    }

    const lines: DiffLine[] = [];
    let oldLine = 0;
    let newLine = 0;

    for (const line of patchText.split("\n")) {
      if (line.startsWith("@@")) {
        const match = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)/);
        if (match) {
          oldLine = parseInt(match[1], 10);
          newLine = parseInt(match[2], 10);
        }
        lines.push({ type: "header", text: line });
      } else if (line.startsWith("+") && !line.startsWith("+++")) {
        lines.push({ type: "add", text: line.slice(1), newNum: newLine });
        newLine++;
      } else if (line.startsWith("-") && !line.startsWith("---")) {
        lines.push({ type: "del", text: line.slice(1), oldNum: oldLine });
        oldLine++;
      } else if (!line.startsWith("---") && !line.startsWith("+++") && !line.startsWith("Index:")) {
        lines.push({
          type: "context",
          text: line.startsWith(" ") ? line.slice(1) : line,
          oldNum: oldLine,
          newNum: newLine,
        });
        oldLine++;
        newLine++;
      }
    }
    return lines;
  });

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(content);
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
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mode === 'rendered'
          ? 'bg-background text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (mode = "rendered")}
      >
        排版预览
      </button>
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mode === 'source'
          ? 'bg-background text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (mode = "source")}
      >
        源码
      </button>
      {#if hasDiff}
        <button
          type="button"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mode === 'diff'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (mode = "diff")}
        >
          版本变动 (Diff)
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-2.5">
      <span class="text-[11px] text-muted-foreground">
        {wordCount} 字符
      </span>

      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => void copyContent()}
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
          <span>复制内容</span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Main View Container -->
  <div class="flex-1 overflow-auto overscroll-contain">
    {#if mode === "rendered"}
      <div class="mx-auto max-w-4xl p-6 sm:p-8">
        <MarkdownContent text={content} lazy={false} />
      </div>
    {:else if mode === "source"}
      <div class="h-full">
        <CodeEditor
          {content}
          filePath={fileName}
          readonly={true}
          class="h-full text-xs font-mono"
        />
      </div>
    {:else if mode === "diff"}
      <div class="min-w-full font-mono text-xs">
        {#each diffLines as line}
          {#if line.type === "header"}
            <div
              class="sticky top-0 z-10 border-y border-blue-500/20 bg-blue-500/10 px-4 py-1 font-semibold text-blue-400"
            >
              {line.text}
            </div>
          {:else if line.type === "add"}
            <div
              class="flex border-l-2 border-emerald-500 bg-emerald-500/10 px-2 py-0.5 text-emerald-700 dark:text-emerald-300"
            >
              <span class="w-10 select-none text-right text-muted-foreground/60 pr-2"
                >{line.newNum ?? ""}</span
              >
              <span class="w-4 select-none text-emerald-500 font-bold">+</span>
              <span class="flex-1 whitespace-pre-wrap">{line.text}</span>
            </div>
          {:else if line.type === "del"}
            <div
              class="flex border-l-2 border-red-500 bg-red-500/10 px-2 py-0.5 text-red-700 dark:text-red-300"
            >
              <span class="w-10 select-none text-right text-muted-foreground/60 pr-2"
                >{line.oldNum ?? ""}</span
              >
              <span class="w-4 select-none text-red-500 font-bold">-</span>
              <span class="flex-1 whitespace-pre-wrap">{line.text}</span>
            </div>
          {:else}
            <div class="flex px-2 py-0.5 text-foreground/80 hover:bg-muted/30">
              <span class="w-10 select-none text-right text-muted-foreground/40 pr-2"
                >{line.newNum ?? ""}</span
              >
              <span class="w-4 select-none text-muted-foreground/30"> </span>
              <span class="flex-1 whitespace-pre-wrap">{line.text}</span>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>
