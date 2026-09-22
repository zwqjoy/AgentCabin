<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ProjectFolder, ConversationGroup } from "$lib/utils/sidebar-groups";
  import ConversationItem from "./ConversationItem.svelte";
  import SidebarFolderRow from "./SidebarFolderRow.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { dbgWarn } from "$lib/utils/debug";

  const PAGE_SIZE = 6;

  type BaseProps = {
    folder: ProjectFolder;
    label: string;
    expanded?: boolean;
    onToggle: () => void;
    showCount?: boolean;
    onRemove?: () => void;
  };

  type ChatProps = BaseProps & {
    children?: never;
    selectedRunId?: string;
    onSelectConversation: (runId: string) => void;
    onDelete?: (conversation: ConversationGroup) => void;
    onEnd?: (conversation: ConversationGroup) => void;
    onNewChat?: () => void;
  };

  type CustomProps = BaseProps & {
    children: Snippet;
    selectedRunId?: never;
    onSelectConversation?: never;
    onDelete?: never;
    onEnd?: never;
    onNewChat?: never;
  };

  let {
    folder,
    label,
    expanded = false,
    onToggle,
    showCount = true,
    onRemove,
    children,
    selectedRunId = "",
    onSelectConversation,
    onDelete,
    onEnd,
    onNewChat,
  }: ChatProps | CustomProps = $props();

  let visibleCount = $state(PAGE_SIZE);
  let menuOpen = $state(false);

  // Reset visible count when folder collapses
  $effect(() => {
    if (!expanded) visibleCount = PAGE_SIZE;
  });

  // Auto-expand visible count if selected run is beyond current page
  $effect(() => {
    if (!expanded || !selectedRunId || children) return;
    const idx = folder.conversations.findIndex((conv) =>
      conv.runs.some((r) => r.id === selectedRunId),
    );
    if (idx >= 0 && idx >= visibleCount) {
      visibleCount = idx + 1;
    }
  });

  // Skip conversation-related derivations when using children snippet
  const visibleConversations = $derived(
    children ? [] : folder.conversations.slice(0, visibleCount),
  );
  const hiddenCount = $derived(children ? 0 : folder.conversationCount - visibleCount);
  const hasMore = $derived(hiddenCount > 0);

  function showMore() {
    visibleCount = Math.min(visibleCount + PAGE_SIZE, folder.conversationCount);
  }

  function isConvSelected(conv: { runs: { id: string }[] }): boolean {
    return conv.runs.some((r) => r.id === selectedRunId);
  }

  // Warn once if conversation-mode callbacks are missing
  let warnedMissingCallbacks = false;
  $effect(() => {
    if (children) {
      // children mode switched back to conversation mode — reset latch
      warnedMissingCallbacks = false;
      return;
    }
    if (!warnedMissingCallbacks && !onSelectConversation) {
      warnedMissingCallbacks = true;
      if (!onSelectConversation)
        dbgWarn("ProjectFolderItem", "onSelectConversation missing in conversation mode");
    }
  });

  async function handleReveal() {
    menuOpen = false;
    if (!folder.cwd || folder.isUncategorized) return;
    try {
      const { revealInFinder } = await import("$lib/api");
      await revealInFinder(folder.cwd);
    } catch (e) {
      dbgWarn("ProjectFolderItem", "revealInFinder failed", e);
    }
  }

  async function handleCopyPath() {
    menuOpen = false;
    if (!folder.cwd) return;
    try {
      await navigator.clipboard.writeText(folder.cwd);
    } catch (e) {
      dbgWarn("ProjectFolderItem", "copy path failed", e);
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (menuOpen && !(e.target instanceof Element && e.target.closest("[data-project-menu]"))) {
      menuOpen = false;
    }
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") menuOpen = false;
  }}
/>

<SidebarFolderRow
  {label}
  title={folder.isUncategorized ? label : folder.cwd}
  {expanded}
  isUncategorized={folder.isUncategorized}
  count={showCount ? folder.conversationCount : undefined}
  hasMenu={!!onRemove && !folder.isUncategorized}
  {menuOpen}
  {onToggle}
>
  {#snippet menu()}
    {#if onRemove && !folder.isUncategorized}
      <div data-project-menu>
        <button
          type="button"
          class="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/55 transition-[opacity,background-color,color] hover:bg-sidebar-accent/80 hover:text-sidebar-foreground focus-visible:opacity-100 focus-visible:outline-none {menuOpen
            ? 'opacity-100 bg-sidebar-accent'
            : 'opacity-0 group-hover/folder:opacity-100 focus-within:opacity-100'}"
          title="项目操作"
          aria-label={`管理项目 ${label}`}
          aria-expanded={menuOpen}
          onclick={(e) => {
            e.stopPropagation();
            menuOpen = !menuOpen;
          }}
        >
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
          >
            <path d="M5 12h.01M12 12h.01M19 12h.01" />
          </svg>
        </button>

        {#if menuOpen}
          <div
            class="absolute right-1 top-7 z-30 min-w-36 overflow-hidden rounded-lg border border-sidebar-border bg-sidebar shadow-xl"
            role="menu"
          >
            {#if onNewChat}
              <button
                type="button"
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
                role="menuitem"
                onclick={() => {
                  menuOpen = false;
                  onNewChat();
                }}
              >
                <svg
                  viewBox="0 0 24 24"
                  class="h-3.5 w-3.5"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                >
                  <path d="M12 5v14M5 12h14" />
                </svg>
                新建对话
              </button>
            {/if}
            <button
              type="button"
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
              role="menuitem"
              onclick={handleReveal}
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
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
              在访达中显示
            </button>
            <button
              type="button"
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
              role="menuitem"
              onclick={handleCopyPath}
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path
                  d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
                />
              </svg>
              复制完整路径
            </button>
            <button
              type="button"
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-red-400 transition-colors hover:bg-red-400/10"
              role="menuitem"
              onclick={() => {
                menuOpen = false;
                onRemove?.();
              }}
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v5M14 11v5" />
              </svg>
              从侧边栏移除
            </button>
          </div>
        {/if}
      </div>
    {/if}
  {/snippet}

  {#snippet children()}
    {#if children}
      {@render children()}
    {:else}
      {#if onNewChat}
        <button
          type="button"
          class="chat-project-new-chat flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-sidebar-foreground/60 transition-colors hover:bg-sidebar-accent/50 hover:text-sidebar-foreground"
          onclick={(e) => {
            e.stopPropagation();
            onNewChat?.();
          }}
        >
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5 text-sidebar-foreground/45"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <path d="M12 5v14M5 12h14" />
          </svg>
          <span>{t("sidebar_newChatInFolder")}</span>
        </button>
      {/if}
      {#each visibleConversations as conv (conv.groupKey)}
        <ConversationItem
          conversation={conv}
          selected={isConvSelected(conv)}
          onclick={() => onSelectConversation?.(conv.latestRun.id)}
          ondelete={onDelete}
          onend={onEnd}
        />
      {/each}
      {#if hasMore}
        <button
          type="button"
          class="flex w-full items-center justify-between rounded-lg px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-sidebar-foreground/40 transition-colors hover:bg-sidebar-accent/40 hover:text-sidebar-foreground/70"
          onclick={showMore}
        >
          <span>更多历史对话</span>
          <span>+{Math.min(PAGE_SIZE, hiddenCount)}</span>
        </button>
      {/if}
    {/if}
  {/snippet}
</SidebarFolderRow>
