<script lang="ts">
  import { SETTINGS_SEARCH_INDEX, searchSettings, type SearchIndexItem } from "./search-index";

  interface Props {
    searchQuery: string;
    onSearchChange: (query: string) => void;
    onSelectResult: (tab: string, section?: string) => void;
  }

  let { searchQuery, onSearchChange, onSelectResult }: Props = $props();

  let isDropdownOpen = $state(false);

  let searchResults = $derived(searchSettings(searchQuery).slice(0, 8));

  function handleSelect(item: SearchIndexItem) {
    onSelectResult(item.tab, item.section);
    isDropdownOpen = false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      isDropdownOpen = false;
      onSearchChange("");
    }
  }
</script>

<div class="relative w-full">
  <div class="relative flex items-center">
    <div
      class="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-2.5 text-muted-foreground/70"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
    </div>
    <input
      type="text"
      class="h-8 w-full rounded-lg border border-border/60 bg-muted/30 pl-8 pr-7 text-xs text-foreground placeholder:text-muted-foreground/60 transition-colors hover:border-border focus:border-primary focus:bg-background focus:outline-none focus:ring-1 focus:ring-primary"
      placeholder="搜索设置..."
      value={searchQuery}
      oninput={(e) => {
        const val = (e.target as HTMLInputElement).value;
        onSearchChange(val);
        isDropdownOpen = val.trim().length > 0;
      }}
      onfocus={() => {
        if (searchQuery.trim().length > 0) isDropdownOpen = true;
      }}
      onkeydown={handleKeyDown}
    />
    {#if searchQuery}
      <button
        type="button"
        class="absolute inset-y-0 right-0 flex items-center pr-2 text-muted-foreground/60 hover:text-foreground"
        onclick={() => {
          onSearchChange("");
          isDropdownOpen = false;
        }}
        aria-label="清除搜索"
      >
        <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    {/if}
  </div>

  {#if isDropdownOpen && searchResults.length > 0}
    <!-- Backdrop to close -->
    <div
      class="fixed inset-0 z-40"
      onclick={() => (isDropdownOpen = false)}
      role="presentation"
    ></div>

    <div
      class="absolute left-0 right-0 top-full z-50 mt-1 max-h-72 overflow-y-auto rounded-xl border border-border/80 bg-popover/95 p-1 text-popover-foreground shadow-lg backdrop-blur-md"
    >
      <div class="px-2 py-1 text-[10px] font-medium text-muted-foreground">
        找到 {searchResults.length} 条设置项
      </div>
      {#each searchResults as item (item.id)}
        <button
          type="button"
          class="flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
          onclick={() => handleSelect(item)}
        >
          <div class="min-w-0 flex-1 truncate font-medium text-foreground">
            {item.title}
          </div>
          <span
            class="shrink-0 rounded bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground"
          >
            {item.group}
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>
