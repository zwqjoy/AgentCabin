<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ChatSearchHighlighter } from "$lib/utils/chat-search-highlighter";

  interface Props {
    container: HTMLElement | null;
    open?: boolean;
    onClose?: () => void;
    class?: string;
  }

  let {
    container = null,
    open = $bindable(false),
    onClose,
    class: className = "",
  }: Props = $props();

  let query = $state("");
  let totalCount = $state(0);
  let currentIndex = $state(-1);
  let inputEl = $state<HTMLInputElement | null>(null);

  const highlighter = new ChatSearchHighlighter();

  $effect(() => {
    highlighter.setContainer(container);
    if (open && query.trim()) {
      runSearch(query);
    }
  });

  $effect(() => {
    if (open) {
      tick().then(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    } else {
      clearAndClose();
    }
  });

  function runSearch(text: string) {
    if (!open) return;
    const count = highlighter.search(text);
    totalCount = count;
    currentIndex = count > 0 ? highlighter.getCurrentIndex() : -1;
  }

  function handleInput(event: Event) {
    const val = (event.target as HTMLInputElement).value;
    query = val;
    runSearch(val);
  }

  export function next() {
    if (totalCount === 0) return;
    highlighter.next();
    currentIndex = highlighter.getCurrentIndex();
  }

  export function prev() {
    if (totalCount === 0) return;
    highlighter.prev();
    currentIndex = highlighter.getCurrentIndex();
  }

  export function focus() {
    open = true;
    tick().then(() => {
      inputEl?.focus();
      inputEl?.select();
    });
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) {
        prev();
      } else {
        next();
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  function clearQuery() {
    query = "";
    highlighter.clear();
    totalCount = 0;
    currentIndex = -1;
    inputEl?.focus();
  }

  function clearAndClose() {
    highlighter.clear();
    query = "";
    totalCount = 0;
    currentIndex = -1;
  }

  function close() {
    open = false;
    clearAndClose();
    onClose?.();
  }

  onMount(() => {
    return () => {
      highlighter.clear();
    };
  });
</script>

{#if open}
  <div
    class="flex items-center gap-1 rounded-xl border border-border/80 bg-background/95 px-2 py-1 shadow-lg backdrop-blur-md transition-all sm:gap-1.5 {className}"
    role="search"
  >
    <!-- Search Input -->
    <div class="relative flex items-center min-w-[140px] sm:min-w-[180px]">
      <svg
        class="pointer-events-none absolute left-2 h-3.5 w-3.5 text-muted-foreground"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <path d="m21 21-4.3-4.3" />
      </svg>
      <input
        bind:this={inputEl}
        value={query}
        oninput={handleInput}
        onkeydown={handleKeyDown}
        type="search"
        placeholder="在对话中查找..."
        aria-label="查找对话内容"
        class="h-7 w-full rounded-lg border border-border/60 bg-muted/40 pl-7 pr-6 text-xs text-foreground placeholder:text-muted-foreground/60 outline-none transition-colors focus:border-primary/50 focus:bg-background focus:ring-1 focus:ring-primary/20"
      />
      {#if query}
        <button
          type="button"
          onclick={clearQuery}
          class="absolute right-1.5 flex h-4 w-4 items-center justify-center rounded-full text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="清空搜索"
        >
          ×
        </button>
      {/if}
    </div>

    <!-- Match Counter -->
    <span
      class="shrink-0 px-1 text-[11px] font-medium tabular-nums text-muted-foreground"
      aria-live="polite"
    >
      {#if query.trim()}
        {totalCount > 0 ? `${currentIndex + 1}/${totalCount}` : "0/0"}
      {:else}
        0/0
      {/if}
    </span>

    <!-- Previous Match -->
    <button
      type="button"
      onclick={prev}
      disabled={totalCount === 0}
      title="上一个 (Shift+Enter)"
      aria-label="上一个匹配"
      class="flex h-6 w-6 items-center justify-center rounded-md text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 disabled:hover:bg-transparent"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="m18 15-6-6-6 6" />
      </svg>
    </button>

    <!-- Next Match -->
    <button
      type="button"
      onclick={next}
      disabled={totalCount === 0}
      title="下一个 (Enter)"
      aria-label="下一个匹配"
      class="flex h-6 w-6 items-center justify-center rounded-md text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 disabled:hover:bg-transparent"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="m6 9 6 6 6-6" />
      </svg>
    </button>

    <!-- Close Button -->
    <button
      type="button"
      onclick={close}
      title="关闭 (Esc)"
      aria-label="关闭查找"
      class="flex h-6 w-6 items-center justify-center rounded-md text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="M18 6 6 18M6 6l12 12" />
      </svg>
    </button>
  </div>
{/if}
