<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    label: string;
    title?: string;
    expanded?: boolean;
    selected?: boolean;
    isUncategorized?: boolean;
    count?: number;
    badges?: Snippet;
    menu?: Snippet;
    hasMenu?: boolean;
    menuOpen?: boolean;
    onToggle: () => void;
    onClick?: () => void;
    children?: Snippet;
    class?: string;
  };

  let {
    label,
    title,
    expanded = false,
    selected = false,
    isUncategorized = false,
    count,
    badges,
    menu,
    hasMenu = false,
    menuOpen = false,
    onToggle,
    onClick,
    children,
    class: className = "",
  }: Props = $props();

  function handleRowClick() {
    if (onClick) {
      onClick();
    } else {
      onToggle();
    }
  }
</script>

<div class="group/folder chat-project-group mb-0.5 {className}">
  <!-- Folder row -->
  <div
    class="group relative flex w-full items-center rounded-lg transition-colors {selected ||
    expanded
      ? 'bg-sidebar-accent/50 text-sidebar-foreground font-medium'
      : 'text-sidebar-foreground/75 hover:bg-sidebar-accent/40 hover:text-sidebar-foreground'}"
  >
    <!-- Chevron toggle button -->
    <button
      type="button"
      class="flex h-7 w-6 shrink-0 items-center justify-center text-sidebar-foreground/45 transition-colors hover:text-sidebar-foreground focus-visible:outline-none"
      aria-label={expanded ? `收起 ${label}` : `展开 ${label}`}
      aria-expanded={expanded}
      onclick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
    >
      <svg
        class="h-3 w-3 shrink-0 transition-transform duration-150 {expanded
          ? 'rotate-90 text-primary'
          : ''}"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M9 18l6-6-6-6" />
      </svg>
    </button>

    <!-- Main button / click area -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-lg py-1.5 {hasMenu
        ? 'pr-7'
        : 'pr-2.5'} text-left text-xs select-none"
      title={title ?? label}
      onclick={handleRowClick}
    >
      <!-- Icon -->
      {#if isUncategorized}
        <svg
          class="h-3.5 w-3.5 shrink-0 {expanded ? 'text-primary' : 'text-sidebar-foreground/50'}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
          <path
            d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"
          />
        </svg>
      {:else}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0 {expanded ? 'text-primary' : 'text-sidebar-foreground/50'}"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
          />
        </svg>
      {/if}

      <!-- Label -->
      <span class="min-w-0 flex-1 truncate">{label}</span>

      <!-- Optional custom badges -->
      {#if badges}
        {@render badges()}
      {/if}

      <!-- Count badge -->
      {#if count !== undefined && count > 0}
        <span
          class="shrink-0 rounded-full bg-sidebar-foreground/10 px-1.5 py-px text-[10px] font-medium text-sidebar-foreground/50"
        >
          {count}
        </span>
      {/if}
    </div>

    <!-- Actions menu slot -->
    {#if menu}
      {@render menu()}
    {/if}
  </div>

  <!-- Expanded child list -->
  {#if expanded && children}
    <div class="mt-0.5 space-y-0.5 pl-3 chat-project-children">
      {@render children()}
    </div>
  {/if}
</div>
