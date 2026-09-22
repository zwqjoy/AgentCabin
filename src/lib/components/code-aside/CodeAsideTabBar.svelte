<script lang="ts">
  import { onMount } from "svelte";
  import type { CodeAsideTab, CodeAsideTabType } from "$lib/types/code-aside";
  import { CODE_ASIDE_SHORTCUTS } from "$lib/types/code-aside";

  interface Props {
    tabs: CodeAsideTab[];
    activeTabId: string;
    isMaximized: boolean;
    onSelectTab: (id: string) => void;
    onCloseTab: (id: string) => void;
    onAddTab: (type: CodeAsideTabType) => void;
    onToggleMaximize: () => void;
    onCloseAside: () => void;
  }

  let {
    tabs,
    activeTabId,
    isMaximized,
    onSelectTab,
    onCloseTab,
    onAddTab,
    onToggleMaximize,
    onCloseAside,
  }: Props = $props();

  let addMenuOpen = $state(false);
  let addMenuRef = $state<HTMLDivElement | null>(null);
  let addButtonRef = $state<HTMLButtonElement | null>(null);
  let menuPos = $state<{ top: number; left: number }>({ top: 0, left: 0 });

  function toggleAddMenu(e: MouseEvent) {
    e.stopPropagation();
    if (addMenuOpen) {
      addMenuOpen = false;
      return;
    }
    if (addButtonRef) {
      const rect = addButtonRef.getBoundingClientRect();
      menuPos = {
        top: rect.bottom + 4,
        left: Math.max(8, Math.min(rect.left, window.innerWidth - 190)),
      };
    }
    addMenuOpen = true;
  }

  function handleDocumentClick(e: MouseEvent) {
    if (
      addMenuOpen &&
      addMenuRef &&
      addButtonRef &&
      !addMenuRef.contains(e.target as Node) &&
      !addButtonRef.contains(e.target as Node)
    ) {
      addMenuOpen = false;
    }
  }

  function handleDocumentKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape" && addMenuOpen) {
      addMenuOpen = false;
    }
  }

  onMount(() => {
    window.addEventListener("mousedown", handleDocumentClick);
    window.addEventListener("keydown", handleDocumentKeyDown);
    return () => {
      window.removeEventListener("mousedown", handleDocumentClick);
      window.removeEventListener("keydown", handleDocumentKeyDown);
    };
  });

  const menuItems: { type: CodeAsideTabType; label: string; icon: string; shortcut: string }[] = [
    { type: "terminal", label: "终端", icon: "terminal", shortcut: CODE_ASIDE_SHORTCUTS.terminal },
    { type: "browser", label: "浏览器", icon: "browser", shortcut: CODE_ASIDE_SHORTCUTS.browser },
    { type: "file", label: "文件", icon: "file", shortcut: CODE_ASIDE_SHORTCUTS.file },
    { type: "review", label: "审查", icon: "review", shortcut: CODE_ASIDE_SHORTCUTS.review },
  ];
</script>

<div
  class="flex h-9 shrink-0 items-center justify-between border-b border-border/60 bg-muted/20 px-2 select-none"
>
  <!-- Left: Tab items + Add button -->
  <div class="flex items-center gap-1 overflow-x-auto min-w-0 pr-2 scrollbar-hide">
    {#each tabs as tab (tab.id)}
      {@const isActive = tab.id === activeTabId}
      <div
        role="tab"
        tabindex="0"
        aria-selected={isActive}
        class="group relative flex items-center gap-1.5 rounded-md px-2.5 py-0.5 text-xs font-medium transition-all cursor-pointer border {isActive
          ? 'bg-background text-foreground shadow-xs border-border/80'
          : 'bg-transparent text-muted-foreground border-transparent hover:bg-muted/40 hover:text-foreground'}"
        onclick={() => onSelectTab(tab.id)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onSelectTab(tab.id);
          }
        }}
      >
        <!-- Icon -->
        {#if tab.type === "review"}
          <svg
            class="h-3.5 w-3.5 shrink-0 opacity-75"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <path d="M9 3v18" />
            <path d="M14 9h5" />
            <path d="M14 15h5" />
          </svg>
        {:else if tab.type === "terminal"}
          <svg
            class="h-3.5 w-3.5 shrink-0 opacity-75"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="4 17 10 11 4 5" />
            <line x1="12" y1="19" x2="20" y2="19" />
          </svg>
        {:else if tab.type === "browser"}
          <svg
            class="h-3.5 w-3.5 shrink-0 opacity-75"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="2" y1="12" x2="22" y2="12" />
            <path
              d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
            />
          </svg>
        {:else}
          <svg
            class="h-3.5 w-3.5 shrink-0 opacity-75"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <path d="M14 2v6h6" />
          </svg>
        {/if}

        <span class="truncate max-w-[130px]">{tab.title}</span>

        {#if tab.closable !== false}
          <button
            type="button"
            class="ml-0.5 rounded p-0.5 text-muted-foreground/60 hover:bg-muted hover:text-foreground transition-colors"
            title="关闭标签页"
            onclick={(e) => {
              e.stopPropagation();
              onCloseTab(tab.id);
            }}
          >
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        {/if}
      </div>
    {/each}

    <!-- Add Tab Button '+' -->
    {#if tabs.length > 0}
      <button
        bind:this={addButtonRef}
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60 hover:text-foreground transition-colors {addMenuOpen
          ? 'bg-muted text-foreground'
          : ''}"
        title="新建标签页"
        onclick={toggleAddMenu}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
        >
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>
    {/if}
  </div>

  <!-- Right: Window controls -->
  <div class="flex items-center gap-1 shrink-0">
    <button
      type="button"
      class="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
      onclick={onToggleMaximize}
      title={isMaximized ? "还原面板" : "最大化面板"}
    >
      {#if isMaximized}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="4" y="8" width="12" height="12" rx="1.5" />
          <path d="M8 4h10a2 2 0 0 1 2 2v10" />
        </svg>
      {:else}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <polyline points="15 3 21 3 21 9" />
          <polyline points="9 21 3 21 3 15" />
          <line x1="21" y1="3" x2="14" y2="10" />
          <line x1="3" y1="21" x2="10" y2="14" />
        </svg>
      {/if}
    </button>

    <button
      type="button"
      class="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
      onclick={onCloseAside}
      title="关闭侧边栏"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <rect width="18" height="18" x="3" y="3" rx="2" />
        <path d="M15 3v18" />
      </svg>
    </button>
  </div>
</div>

<!-- Floating Dropdown Menu for '+' Add Tab (rendered with fixed positioning to escape overflow clipping) -->
{#if addMenuOpen}
  <div
    bind:this={addMenuRef}
    class="fixed z-[9999] w-48 rounded-xl border border-border/80 bg-popover/95 p-1 shadow-2xl text-xs backdrop-blur-md"
    style="top: {menuPos.top}px; left: {menuPos.left}px;"
  >
    {#each menuItems as item}
      <button
        type="button"
        class="flex w-full items-center justify-between rounded-lg px-2.5 py-1.5 text-left text-foreground hover:bg-accent hover:text-accent-foreground transition-colors cursor-pointer"
        onclick={(e) => {
          e.stopPropagation();
          addMenuOpen = false;
          onAddTab(item.type);
        }}
      >
        <div class="flex items-center gap-2">
          {#if item.icon === "terminal"}
            <svg
              class="h-3.5 w-3.5 opacity-70 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <polyline points="4 17 10 11 4 5" /><line x1="12" y1="19" x2="20" y2="19" />
            </svg>
          {:else if item.icon === "browser"}
            <svg
              class="h-3.5 w-3.5 opacity-70 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="10" />
              <path
                d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
              />
            </svg>
          {:else if item.icon === "file"}
            <svg
              class="h-3.5 w-3.5 opacity-70 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <path d="M14 2v6h6" />
            </svg>
          {:else if item.icon === "review"}
            <svg
              class="h-3.5 w-3.5 opacity-70 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect x="3" y="3" width="18" height="18" rx="2" />
              <path d="M9 3v18" />
            </svg>
          {/if}
          <span>{item.label}</span>
        </div>
        <span class="text-[10px] text-muted-foreground font-mono">{item.shortcut}</span>
      </button>
    {/each}
  </div>
{/if}
