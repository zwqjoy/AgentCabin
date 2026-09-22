<script lang="ts">
  import type { ConversationGroup } from "$lib/utils/sidebar-groups";
  import { t } from "$lib/i18n/index.svelte";
  import { onMount } from "svelte";

  // ── Menu item model ──
  // 数据驱动的菜单项定义，将展示与逻辑分离。
  // 组件只负责渲染 + 触发回调，业务逻辑由父组件处理。

  type MenuEntry =
    | {
        kind: "item";
        id: string;
        label: string;
        icon: IconName;
        disabled?: boolean;
        danger?: boolean;
      }
    | { kind: "separator" };

  type IconName =
    | "pin"
    | "unpin"
    | "edit"
    | "archive"
    | "unarchive"
    | "mail"
    | "folder"
    | "copy"
    | "id"
    | "stop"
    | "delete";

  // ── Icon paths (24x24, stroke-based, consistent with project style) ──
  const ICONS: Record<IconName, string> = {
    pin: '<path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>',
    unpin:
      '<path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/><line x1="4" y1="4" x2="20" y2="20"/>',
    edit: '<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>',
    archive:
      '<rect x="2" y="3" width="20" height="5" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>',
    unarchive:
      '<rect x="2" y="3" width="20" height="5" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M12 12v5"/><path d="M9.5 14.5 12 12l2.5 2.5"/>',
    mail: '<path d="M22 6 12 13 2 6"/><rect x="2" y="4" width="20" height="16" rx="2"/>',
    folder:
      '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>',
    copy: '<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
    id: '<path d="M4 4h16v16H4z"/><path d="M9 9h6v6H9z"/>',
    stop: '<rect x="5" y="5" width="14" height="14" rx="2"/>',
    delete:
      '<path d="M3 6h18"/><path d="M19 6v14c0 1.1-.9 2-2 2H7c-1.1 0-2-.9-2-2V6"/><path d="M8 6V4c0-1.1.9-2 2-2h4c1.1 0 2 .9 2 2v2"/>',
  };

  let {
    conversation,
    x,
    y,
    canDelete = false,
    canEnd = false,
    onclose,
    onrename,
    onpin,
    onmarkunread,
    onarchive,
    onend,
    ondelete,
    onreveal,
    oncopycwd,
    oncopysessionid,
  }: {
    conversation: ConversationGroup;
    x: number;
    y: number;
    canDelete?: boolean;
    canEnd?: boolean;
    onclose: () => void;
    onrename?: () => void;
    onpin?: () => void;
    onmarkunread?: () => void;
    onarchive?: () => void;
    onend?: () => void;
    ondelete?: () => void;
    onreveal?: () => void;
    oncopycwd?: () => void;
    oncopysessionid?: () => void;
  } = $props();

  let menuEl: HTMLDivElement | undefined = $state();

  // ── Position: clamp to viewport to avoid overflow ──
  let pos = $state({ x, y });

  $effect(() => {
    // Recompute on every open (x/y change = new open)
    if (!menuEl) return;
    const rect = menuEl.getBoundingClientRect();
    let nx = x;
    let ny = y;
    // Right overflow: shift left
    if (x + rect.width > window.innerWidth - 8) {
      nx = Math.max(8, x - rect.width);
    }
    // Bottom overflow: flip up
    if (y + rect.height > window.innerHeight - 8) {
      ny = Math.max(8, y - rect.height);
    }
    pos = { x: nx, y: ny };
  });

  // ── Build menu entries from conversation state + available callbacks ──
  const isPinned = $derived(conversation.pinned);
  const isUnread = $derived(conversation.unread);

  const entries = $derived<MenuEntry[]>([
    // ── Group: State management ──
    {
      kind: "item",
      id: "pin",
      icon: isPinned ? "unpin" : "pin",
      label: isPinned ? t("ctxMenu_unpin") : t("ctxMenu_pin"),
    },
    { kind: "item", id: "rename", icon: "edit", label: t("ctxMenu_rename") },
    {
      kind: "item",
      id: "archive",
      icon: conversation.archived ? "unarchive" : "archive",
      label: conversation.archived ? t("ctxMenu_unarchive") : t("ctxMenu_archive"),
    },
    ...(canEnd
      ? [{ kind: "item" as const, id: "end", icon: "stop" as const, label: t("ctxMenu_end") }]
      : []),
    {
      kind: "item",
      id: "delete",
      icon: "delete",
      label: t("ctxMenu_delete"),
      disabled: !canDelete,
      danger: true,
    },
    {
      kind: "item",
      id: "markunread",
      icon: "mail",
      label: isUnread ? t("ctxMenu_markRead") : t("ctxMenu_markUnread"),
    },
    { kind: "separator" },
    // ── Group: File system ──
    { kind: "item", id: "reveal", icon: "folder", label: t("ctxMenu_revealInFinder") },
    { kind: "item", id: "copycwd", icon: "copy", label: t("ctxMenu_copyCwd") },
    { kind: "separator" },
    // ── Group: Identity ──
    {
      kind: "item",
      id: "copysessionid",
      icon: "id",
      label: t("ctxMenu_copySessionId"),
      disabled: !conversation.latestRun.session_id,
    },
  ]);

  function handleAction(id: string) {
    const actions: Record<string, (() => void) | undefined> = {
      pin: onpin,
      rename: onrename,
      archive: onarchive,
      end: onend,
      markunread: onmarkunread,
      reveal: onreveal,
      copycwd: oncopycwd,
      copysessionid: oncopysessionid,
      delete: ondelete,
    };
    const fn = actions[id];
    if (fn) fn();
    onclose();
  }

  // ── Dismiss: outside click + Escape ──
  function onDocMouseDown(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) onclose();
  }
  function onDocKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        if (node.parentNode) {
          node.parentNode.removeChild(node);
        }
      },
    };
  }

  onMount(() => {
    document.addEventListener("mousedown", onDocMouseDown, true);
    document.addEventListener("keydown", onDocKeydown);
    return () => {
      document.removeEventListener("mousedown", onDocMouseDown, true);
      document.removeEventListener("keydown", onDocKeydown);
    };
  });
</script>

<div
  use:portal
  bind:this={menuEl}
  class="context-menu"
  style="position:fixed; left:{pos.x}px; top:{pos.y}px; z-index:9999;"
>
  {#each entries as entry}
    {#if entry.kind === "separator"}
      <div class="ctx-sep"></div>
    {:else}
      <button
        class="ctx-item"
        class:disabled={entry.disabled}
        class:danger={entry.danger}
        onclick={() => !entry.disabled && handleAction(entry.id)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            if (!entry.disabled) handleAction(entry.id);
          }
        }}
        role="menuitem"
        tabindex={entry.disabled ? -1 : 0}
      >
        <svg
          class="ctx-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          {@html ICONS[entry.icon]}
        </svg>
        <span class="ctx-label">{entry.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-menu {
    min-width: 200px;
    max-width: 260px;
    padding: 4px;
    border-radius: 8px;
    background: hsl(var(--popover));
    border: 1px solid hsl(var(--border));
    box-shadow:
      0 4px 6px -1px rgb(0 0 0 / 0.1),
      0 2px 4px -2px rgb(0 0 0 / 0.1);
    font-size: 12px;
    user-select: none;
    animation: ctx-fade-in 80ms ease-out;
  }

  .ctx-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 4px;
    color: hsl(var(--popover-foreground));
    cursor: pointer;
    transition: background-color 80ms;
    text-align: left;
  }

  .ctx-item:hover:not(.disabled) {
    background: hsl(var(--accent));
  }

  .ctx-item.disabled {
    opacity: 0.4;
    cursor: default;
  }

  .ctx-item.danger {
    color: hsl(0 72% 51%);
  }

  .ctx-icon {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }

  .ctx-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-sep {
    height: 1px;
    margin: 4px 2px;
    background: hsl(var(--border));
  }

  @keyframes ctx-fade-in {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
