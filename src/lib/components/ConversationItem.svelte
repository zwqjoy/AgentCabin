<script lang="ts">
  import {
    conversationCanDelete,
    conversationCanEnd,
    TERMINAL_RUN_STATUSES,
    type ConversationGroup,
  } from "$lib/utils/sidebar-groups";
  import StatusBadge from "./StatusBadge.svelte";
  import { relativeTime } from "$lib/utils/format";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { hasAttention } from "$lib/stores/attention-store.svelte";
  import ConversationContextMenu from "./ConversationContextMenu.svelte";
  import { getAgentDisplayName } from "$lib/utils/agent-metadata";
  import { dispatchRunMutation } from "$lib/utils/run-mutations";

  let {
    conversation,
    selected = false,
    statusLabel = "",
    onclick,
    ondelete,
    onend,
  }: {
    conversation: ConversationGroup;
    selected?: boolean;
    statusLabel?: string;
    onclick?: () => void;
    ondelete?: (conversation: ConversationGroup) => void;
    onend?: (conversation: ConversationGroup) => void;
  } = $props();

  const run = $derived(conversation.latestRun);
  // Let CSS handle truncation (the title <span> has `truncate`). A hard JS char cap
  // here truncated titles at 28 chars regardless of available width, so they were
  // often cut off well before the rail ran out of room. (#132)
  const label = $derived(conversation.title);
  const time = $derived(relativeTime(run.last_activity_at ?? run.started_at));
  const agentLabel = $derived(getAgentDisplayName(run.agent));
  const preview = $derived.by(() => {
    const candidate = run.last_message_preview?.trim() ?? "";
    return candidate && candidate !== label ? candidate : "";
  });
  const hoverSummary = $derived(preview || run.prompt?.trim() || label);
  const isStandaloneWork = $derived(
    run.app_mode === "work" && (!run.workspace_id || run.cwd?.includes("standalone_tasks")),
  );
  const rawPath = $derived(conversation.projectPath || run.remote_cwd || run.cwd || "");
  const projectPath = $derived(isStandaloneWork ? "" : rawPath);
  const projectLabel = $derived.by(() => {
    if (isStandaloneWork) return "独立任务";
    if (conversation.projectName) return conversation.projectName;
    return rawPath.split(/[\\/]/).filter(Boolean).at(-1) || t("sidebar_uncategorized");
  });
  const canDelete = $derived(conversationCanDelete(conversation));
  const canEnd = $derived(conversationCanEnd(conversation));
  const runCount = $derived(conversation.runs.length);
  const needsAttention = $derived(hasAttention(run.id));
  // Codex completed + resumable → display as "idle"
  const displayStatus = $derived(
    run.status === "completed" && run.conversation_ref?.kind === "codex_thread"
      ? ("idle" as const)
      : run.status,
  );

  // ── Context menu state ──
  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);

  // Keep the detail card fixed to the viewport so it can float over the chat canvas instead of
  // being clipped by the scrollable sidebar panel.
  let hoverCardVisible = $state(false);
  let hoverCardStyle = $state("");
  let hoverCardEl: HTMLDivElement | undefined = $state();
  let hoverTarget: HTMLElement | undefined;

  function showHoverCard(target: HTMLElement) {
    // A hover card is only useful while the pointer is idle. Once the user has
    // opened a menu, moving within the row must not bring the card back.
    if (menuOpen) return;
    hoverTarget = target;
    const rect = target.getBoundingClientRect();
    hoverCardVisible = true;
    hoverCardStyle = `left:${rect.right + 10}px;top:${Math.max(10, rect.top)}px;`;

    requestAnimationFrame(() => {
      if (hoverTarget !== target || !hoverCardEl) return;
      const card = hoverCardEl.getBoundingClientRect();
      const left = Math.min(rect.right + 10, window.innerWidth - card.width - 10);
      const top = Math.min(Math.max(10, rect.top), window.innerHeight - card.height - 10);
      hoverCardStyle = `left:${Math.max(10, left)}px;top:${Math.max(10, top)}px;`;
    });
  }

  function hideHoverCard() {
    hoverTarget = undefined;
    hoverCardVisible = false;
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    hideHoverCard();
    menuX = e.clientX;
    menuY = e.clientY;
    menuOpen = true;
  }

  function closeMenu() {
    menuOpen = false;
  }

  // ── Menu actions ──

  async function copyToClipboard(text: string | undefined) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      dbgWarn("conv-item", "clipboard write failed", e);
    }
  }

  async function updateFlags(flags: {
    pinned?: boolean;
    archived?: boolean;
    unread?: boolean;
  }): Promise<boolean> {
    try {
      const { setRunFlags } = await import("$lib/api");
      await setRunFlags(conversation.latestRun.id, flags);
      dispatchRunMutation({
        kind: "update",
        runId: conversation.latestRun.id,
        patch: flags,
      });
      return true;
    } catch (e) {
      dbgWarn("conv-item", "setRunFlags failed", e);
      return false;
    }
  }

  function handlePin() {
    updateFlags({ pinned: !conversation.pinned });
  }

  async function handleArchive() {
    const archiving = !conversation.archived;
    const hadLiveSession = conversationCanEnd(conversation);
    const ok = await updateFlags({ archived: archiving });
    if (!ok || !archiving || !hadLiveSession) return;
    // The backend stops live sessions when a conversation is archived, so mirror
    // the terminal status locally. Otherwise the conversation keeps reading as
    // active in the sidebar/history until the next full reload.
    for (const member of conversation.runs) {
      if (TERMINAL_RUN_STATUSES.includes(member.status)) continue;
      dispatchRunMutation({ kind: "update", runId: member.id, patch: { status: "stopped" } });
    }
  }

  function handleMarkUnread() {
    updateFlags({ unread: !conversation.unread });
  }

  async function handleReveal() {
    try {
      const { revealInFinder } = await import("$lib/api");
      await revealInFinder(run.cwd);
    } catch (e) {
      dbgWarn("conv-item", "revealInFinder failed", e);
    }
  }

  function handleCopyCwd() {
    copyToClipboard(run.cwd);
  }

  function handleCopySessionId() {
    copyToClipboard(run.session_id);
  }

  function handleDelete() {
    ondelete?.(conversation);
  }

  function handleEnd() {
    onend?.(conversation);
  }

  // ── Inline rename (self-contained, mirrors RunListItem) ──

  let editing = $state(false);
  let editValue = $state("");
  let editInputEl: HTMLInputElement | undefined = $state();

  function startRename() {
    editValue = conversation.title;
    editing = true;
    requestAnimationFrame(() => {
      editInputEl?.select();
    });
  }

  async function commitRename() {
    editing = false;
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== conversation.title) {
      try {
        const { renameRun } = await import("$lib/api");
        await renameRun(conversation.latestRun.id, trimmed);
        dbg("conv-item", "renamed", {
          runId: conversation.latestRun.id,
          name: trimmed,
        });
        dispatchRunMutation({
          kind: "update",
          runId: conversation.latestRun.id,
          patch: { name: trimmed },
        });
      } catch (e) {
        dbgWarn("conv-item", "rename failed", e);
        // runs will refresh on next poll
      }
    }
  }

  function cancelRename() {
    editing = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (editing) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      hideHoverCard();
      onclick?.();
    }
  }

  function handleClick() {
    if (editing) return;
    hideHoverCard();
    onclick?.();
  }
</script>

<div
  class="ui-sidebar-item app-shell-conversation-item chat-conversation-item group w-full text-left px-2.5 py-1.5 rounded-lg transition-colors text-xs cursor-pointer
    {selected
    ? 'bg-primary/10 text-primary font-medium'
    : 'hover:bg-sidebar-accent/50 text-sidebar-foreground'}"
  role="button"
  tabindex="0"
  onclick={handleClick}
  onmouseenter={(e) => showHoverCard(e.currentTarget as HTMLElement)}
  onmouseleave={hideHoverCard}
  onfocus={(e) => showHoverCard(e.currentTarget as HTMLElement)}
  onblur={hideHoverCard}
  onkeydown={handleKeydown}
  oncontextmenu={handleContextMenu}
>
  <div class="flex items-center justify-between gap-1.5">
    <div class="flex items-center gap-1.5 min-w-0 flex-1">
      <span
        class="chat-agent-mark chat-agent-{run.agent} shrink-0"
        title={agentLabel}
        aria-label={agentLabel}
      ></span>
      {#if conversation.isFavorite}
        <svg
          class="h-3 w-3 shrink-0 text-yellow-500"
          viewBox="0 0 24 24"
          fill="currentColor"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polygon
            points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"
          />
        </svg>
      {/if}
      {#if editing}
        <input
          bind:this={editInputEl}
          bind:value={editValue}
          class="min-w-0 flex-1 bg-transparent text-sm outline-none border-b border-primary"
          onblur={commitRename}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitRename();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              cancelRename();
            }
            e.stopPropagation();
          }}
          onclick={(e) => e.stopPropagation()}
        />
      {:else}
        <span
          class="chat-conversation-title truncate"
          title={conversation.title}
          role="button"
          tabindex="-1"
          ondblclick={(e) => {
            e.stopPropagation();
            startRename();
          }}>{label}</span
        >
      {/if}
    </div>
    <div class="flex items-center gap-1 shrink-0">
      {#if statusLabel}
        {@const isUrgent =
          statusLabel.includes("等待") ||
          statusLabel.includes("批准") ||
          statusLabel.includes("输入")}
        <span
          class="inline-flex max-w-[5.5rem] items-center gap-1 truncate rounded-full px-1.5 py-0.5 text-[10px] font-semibold leading-none {isUrgent
            ? 'bg-amber-500/20 text-amber-600 dark:text-amber-400 border border-amber-500/30'
            : 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'}"
          title={statusLabel}
          aria-label={statusLabel}
        >
          {#if isUrgent}
            <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 animate-pulse"></span>
          {/if}
          <span class="truncate">{statusLabel}</span>
        </span>
      {/if}
      {#if runCount > 1}
        <span
          class="inline-flex h-3.5 min-w-[14px] items-center justify-center rounded-full bg-muted px-1 text-[10px] font-medium text-muted-foreground"
          title={t("sidebar_conversations", { count: String(runCount) })}>{runCount}</span
        >
      {/if}
      <!-- Pinned indicator (always visible when pinned) -->
      {#if conversation.pinned}
        <button
          class="p-0.5 rounded hover:bg-accent text-foreground transition-opacity shrink-0"
          onclick={(e) => {
            e.stopPropagation();
            hideHoverCard();
            handlePin();
          }}
          title={t("ctxMenu_unpin")}
          aria-label={t("ctxMenu_unpin")}
        >
          <svg class="h-3 w-3 -rotate-45" viewBox="0 0 24 24" fill="currentColor">
            <path d="M16 12V4h1V2H7v2h1v8l-2 2v2h5v6l1 1 1-1v-6h5v-2l-2-2z" />
          </svg>
        </button>
      {/if}

      <!-- Quick actions on hover (pin, archive) -->
      <div class="hidden group-hover:flex items-center gap-0.5 shrink-0">
        {#if !conversation.pinned}
          <button
            class="p-0.5 rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            onclick={(e) => {
              e.stopPropagation();
              hideHoverCard();
              handlePin();
            }}
            title={t("ctxMenu_pin")}
            aria-label={t("ctxMenu_pin")}
          >
            <svg
              class="h-3 w-3 -rotate-45"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M16 12V4h1V2H7v2h1v8l-2 2v2h5v6l1 1 1-1v-6h5v-2l-2-2z" />
            </svg>
          </button>
        {/if}
        <button
          class="p-0.5 rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          onclick={(e) => {
            e.stopPropagation();
            hideHoverCard();
            handleArchive();
          }}
          title={conversation.archived ? t("ctxMenu_unarchive") : t("ctxMenu_archive")}
        >
          <svg
            class="h-3 w-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect x="2" y="3" width="20" height="5" rx="1" />
            <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
            <path d="M10 12h4" />
          </svg>
        </button>
      </div>

      {#if conversation.unread}
        <span class="chat-unread-dot h-1.5 w-1.5 shrink-0 rounded-full bg-primary"></span>
      {/if}
      <StatusBadge status={displayStatus} attention={needsAttention} class="shrink-0" />
      <span
        class="chat-conversation-time text-[11px] text-muted-foreground/60 tabular-nums shrink-0 group-hover:hidden"
        >{time}</span
      >
    </div>
  </div>
</div>

{#if hoverCardVisible && !menuOpen}
  <div
    bind:this={hoverCardEl}
    class="chat-conversation-hover-card"
    style={hoverCardStyle}
    role="tooltip"
    aria-hidden="true"
  >
    <div class="flex items-start gap-2.5">
      <span class="chat-agent-mark chat-agent-{run.agent} mt-1 shrink-0" aria-hidden="true"></span>
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-3">
          <p class="chat-conversation-hover-title">{label}</p>
          <span class="chat-conversation-hover-time shrink-0">{time}</span>
        </div>
        <p class="chat-conversation-hover-agent">{agentLabel}</p>
      </div>
    </div>

    <p class="chat-conversation-hover-summary">{hoverSummary}</p>

    <div class="chat-conversation-hover-details">
      <div class="chat-conversation-hover-detail">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M3 7.5 12 3l9 4.5v9L12 21l-9-4.5z" />
          <path d="M3 7.5 12 12l9-4.5M12 12v9" />
        </svg>
        <span class="truncate" title={projectPath || projectLabel}>{projectLabel}</span>
      </div>
      {#if projectPath}
        <div class="chat-conversation-hover-path truncate" title={projectPath}>{projectPath}</div>
      {/if}
      {#if run.model}
        <div class="chat-conversation-hover-detail">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 3v18M3 12h18" />
            <circle cx="12" cy="12" r="8" />
          </svg>
          <span class="truncate" title={run.model}>{run.model}</span>
        </div>
      {/if}
      <div class="chat-conversation-hover-footer">
        <span>{t("sidebar_messages", { count: String(conversation.totalMessages) })}</span>
        {#if run.remote_host_name}
          <span>{run.remote_host_name}</span>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if menuOpen}
  <ConversationContextMenu
    {conversation}
    x={menuX}
    y={menuY}
    {canDelete}
    {canEnd}
    onclose={closeMenu}
    onrename={startRename}
    onpin={handlePin}
    onmarkunread={handleMarkUnread}
    onarchive={handleArchive}
    onend={handleEnd}
    ondelete={handleDelete}
    onreveal={handleReveal}
    oncopycwd={handleCopyCwd}
    oncopysessionid={handleCopySessionId}
  />
{/if}
