<script lang="ts">
  import { onMount } from "svelte";
  import {
    composerFeatureDescription,
    getComposerFeatureActions,
    isComposerFeatureActionBlocked,
  } from "$lib/utils/composer-features";

  let {
    disabled = false,
    goalAvailable = false,
    planAvailable = false,
    goalActive = false,
    planActive = false,
    discoveryPending = false,
    allowDiscoveryPendingAction = false,
    operationPending = false,
    onGoal,
    onPlan,
  }: {
    disabled?: boolean;
    goalAvailable?: boolean;
    planAvailable?: boolean;
    goalActive?: boolean;
    planActive?: boolean;
    discoveryPending?: boolean;
    allowDiscoveryPendingAction?: boolean;
    operationPending?: boolean;
    onGoal?: () => void | Promise<void>;
    onPlan?: () => void | Promise<void>;
  } = $props();

  let wrapperEl: HTMLDivElement | undefined = $state();
  let menuOpen = $state(false);

  let availableActions = $derived(getComposerFeatureActions({ goalAvailable, planAvailable }));
  let hasActions = $derived(availableActions.length > 0);
  let actionBlocked = $derived(
    isComposerFeatureActionBlocked({
      disabled,
      discoveryPending,
      operationPending,
      allowDiscoveryPendingAction,
    }),
  );

  function closeMenu() {
    menuOpen = false;
  }

  function toggleMenu() {
    if (disabled || !hasActions) return;
    menuOpen = !menuOpen;
    if (!menuOpen) return;
    requestAnimationFrame(() => {
      wrapperEl?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
    });
  }

  function choose(action: "goal" | "plan") {
    if (actionBlocked) return;
    closeMenu();
    if (action === "goal") void onGoal?.();
    else void onPlan?.();
  }

  onMount(() => {
    function onDocumentPointerDown(event: PointerEvent) {
      if (menuOpen && wrapperEl && !wrapperEl.contains(event.target as Node)) closeMenu();
    }

    function onDocumentKeydown(event: KeyboardEvent) {
      if (!menuOpen) return;
      if (event.key === "Escape") {
        event.preventDefault();
        closeMenu();
        wrapperEl?.querySelector<HTMLButtonElement>("button")?.focus();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      const items = Array.from(
        wrapperEl?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [],
      );
      if (items.length === 0) return;
      event.preventDefault();
      const current = items.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? items.length - 1
            : (current + (event.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
      items[next]?.focus();
    }

    document.addEventListener("pointerdown", onDocumentPointerDown, true);
    document.addEventListener("keydown", onDocumentKeydown);
    return () => {
      document.removeEventListener("pointerdown", onDocumentPointerDown, true);
      document.removeEventListener("keydown", onDocumentKeydown);
    };
  });
</script>

<div bind:this={wrapperEl} class="relative shrink-0">
  <button
    type="button"
    class="flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors
      {menuOpen ? 'bg-accent text-foreground' : 'hover:bg-accent hover:text-foreground'}
      disabled:cursor-not-allowed disabled:opacity-40"
    onclick={toggleMenu}
    disabled={disabled || !hasActions}
    aria-label="添加目标或计划"
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    title="添加目标或计划"
  >
    <svg
      class="h-4 w-4"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      aria-hidden="true"
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  </button>

  {#if menuOpen}
    <div
      class="absolute bottom-full left-0 z-50 mb-2 w-[min(20rem,calc(100vw-2rem))] overflow-hidden rounded-xl border border-border/80 bg-popover p-1.5 text-popover-foreground shadow-xl animate-in fade-in zoom-in-95 duration-150"
      role="menu"
      aria-label="添加目标或计划"
    >
      <div class="px-2.5 pb-1.5 pt-1 text-[11px] font-semibold text-muted-foreground">添加</div>

      {#if goalAvailable}
        <button
          type="button"
          role="menuitem"
          class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors
            {goalActive ? 'bg-accent/80' : 'hover:bg-accent/60'}
            disabled:cursor-not-allowed disabled:opacity-50"
          disabled={actionBlocked}
          onclick={() => choose("goal")}
        >
          <svg
            class="h-4 w-4 shrink-0 {goalActive ? 'text-emerald-500' : 'text-muted-foreground'}"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /><circle
              cx="12"
              cy="12"
              r="1.5"
            />
          </svg>
          <span class="min-w-0 flex-1">
            <span class="block text-xs font-medium">目标</span>
            <span class="block truncate text-[10px] text-muted-foreground">
              {composerFeatureDescription("goal", goalActive, discoveryPending)}
            </span>
          </span>
          {#if goalActive}<span class="text-[10px] text-emerald-500">已启用</span>{/if}
        </button>
      {/if}

      {#if planAvailable}
        <button
          type="button"
          role="menuitem"
          class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors
            {planActive ? 'bg-accent/80' : 'hover:bg-accent/60'}
            disabled:cursor-not-allowed disabled:opacity-50"
          disabled={actionBlocked}
          onclick={() => choose("plan")}
        >
          <svg
            class="h-4 w-4 shrink-0 {planActive ? 'text-purple-500' : 'text-muted-foreground'}"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
            <line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" />
          </svg>
          <span class="min-w-0 flex-1">
            <span class="block text-xs font-medium">计划模式</span>
            <span class="block truncate text-[10px] text-muted-foreground">
              {composerFeatureDescription("plan", planActive, discoveryPending)}
            </span>
          </span>
          {#if planActive}<span class="text-[10px] text-purple-500">已启用</span>{/if}
        </button>
      {/if}
    </div>
  {/if}
</div>
