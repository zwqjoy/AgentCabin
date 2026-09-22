<script lang="ts">
  import { onMount } from "svelte";
  import type { AppRealm, PiSubMode } from "$lib/stores/app-mode.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    realm?: AppRealm;
    piSubMode?: PiSubMode;
    onSelectRealm: (realm: AppRealm, piSubMode?: PiSubMode) => void;
    workEnabled?: boolean;
  }

  let { realm = "pi", piSubMode = "code", onSelectRealm, workEnabled = true }: Props = $props();

  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();
  let trigger: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let menuPosition = $state({ left: 0, top: 0 });

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

  let currentHarness = $derived.by<"code" | "work">(() => {
    if (realm === "pi" && piSubMode === "work") return "work";
    return "code";
  });

  let displayLabel = $derived.by<string>(() => {
    return currentHarness === "work" ? "Work" : "Code";
  });

  interface ModeOption {
    id: "code" | "work";
    realm: AppRealm;
    subMode: PiSubMode;
    title: string;
    description: string;
    tag: string;
    tagClass: string;
    dotClass: string;
    visible?: boolean;
  }

  let options = $derived<ModeOption[]>([
    {
      id: "code",
      realm: "pi",
      subMode: "code",
      title: "Code",
      description: t("sidebar_piSubModeCodeDesc"),
      tag: "载体",
      tagClass: "bg-teal-500/10 text-teal-600 dark:text-teal-400",
      dotClass: "bg-teal-500",
      visible: true,
    },
    {
      id: "work",
      realm: "pi",
      subMode: "work",
      title: "Work",
      description: t("sidebar_piSubModeWorkDesc"),
      tag: "载体",
      tagClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
      dotClass: "bg-blue-500",
      visible: workEnabled,
    },
  ]);

  function isSelected(optHarness: "code" | "work"): boolean {
    return currentHarness === optHarness;
  }

  function selectOption(opt: ModeOption) {
    open = false;
    onSelectRealm(opt.realm, opt.subMode);
  }

  function updateMenuPosition() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const menuWidth = 288;
    let left = rect.left;
    if (typeof window !== "undefined" && left + menuWidth > window.innerWidth - 8) {
      left = Math.max(8, window.innerWidth - menuWidth - 8);
    }
    menuPosition = { left, top: rect.bottom + 6 };
  }

  function toggleMenu() {
    open = !open;
    if (open) updateMenuPosition();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") open = false;
  }

  onMount(() => {
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && !root?.contains(target) && !menuEl?.contains(target)) {
        open = false;
      }
    };
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeydown);
    window.addEventListener("resize", updateMenuPosition);
    window.addEventListener("scroll", updateMenuPosition, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("resize", updateMenuPosition);
      window.removeEventListener("scroll", updateMenuPosition, true);
    };
  });
</script>

<div bind:this={root} class="relative shrink-0">
  <button
    bind:this={trigger}
    type="button"
    class="mode-switcher-trigger flex h-9 items-center gap-2 rounded-xl border border-sidebar-border/70 bg-sidebar-accent/40 px-3 text-sidebar-foreground transition-colors hover:bg-sidebar-accent/70 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="切换工作载体"
    onclick={toggleMenu}
  >
    <span
      class="h-2 w-2 rounded-full shrink-0 {currentHarness === 'work'
        ? 'bg-blue-500'
        : 'bg-teal-500'}"
    ></span>
    <span class="text-sm font-bold tracking-tight text-sidebar-foreground leading-none">
      {displayLabel}
    </span>
    <svg
      viewBox="0 0 16 16"
      class="h-3.5 w-3.5 text-sidebar-foreground/70 transition-transform duration-150 {open
        ? 'rotate-180'
        : ''}"
      fill="none"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"><path d="m4 6 4 4 4-4" /></svg
    >
  </button>

  {#if open}
    <div
      use:portal
      bind:this={menuEl}
      class="mode-switcher-menu fixed z-[9999] w-72 max-w-[calc(100vw-1rem)] overflow-hidden rounded-2xl border border-sidebar-border/80 bg-popover p-1.5 text-popover-foreground shadow-xl backdrop-blur-md"
      style={`left: ${menuPosition.left}px; top: ${menuPosition.top}px;`}
      role="menu"
      aria-label="工作载体选择"
    >
      <div
        class="px-2.5 pt-1.5 pb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground"
      >
        工作载体
      </div>
      {#each options as opt}
        {#if opt.visible !== false}
          {@const selected = isSelected(opt.id)}
          <button
            type="button"
            role="menuitemradio"
            aria-checked={selected}
            class="mode-switcher-menu-item group flex w-full items-center justify-between rounded-xl px-3 py-2.5 text-left transition-colors hover:bg-sidebar-accent/50 {selected
              ? 'bg-sidebar-accent/40'
              : ''}"
            onclick={() => selectOption(opt)}
          >
            <div class="flex min-w-0 flex-1 flex-col pr-2">
              <div class="flex items-center gap-2">
                <span class="h-2 w-2 rounded-full {opt.dotClass} shrink-0"></span>
                <span class="text-sm font-semibold text-sidebar-foreground leading-snug">
                  {opt.title}
                </span>
                <span class="rounded px-1.5 py-0.2 text-[10px] font-medium {opt.tagClass}">
                  {opt.tag}
                </span>
              </div>
              <span class="text-xs text-sidebar-foreground/60 leading-normal mt-1 pl-4">
                {opt.description}
              </span>
            </div>
            {#if selected}
              <svg
                viewBox="0 0 16 16"
                class="h-4 w-4 shrink-0 text-sidebar-foreground"
                fill="none"
                stroke="currentColor"
                stroke-width="2.2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"><path d="m3 8.5 3.5 3.5 6.5-8" /></svg
              >
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  /* The shell gives every header button icon-button dimensions. The mode menu is a
     floating control and must opt out of that shared rule. */
  :global(.chat-sidebar-panel .app-shell-sidebar-header button.mode-switcher-trigger) {
    width: auto !important;
    height: 36px !important;
    min-width: 0 !important;
  }

  :global(.mode-switcher-menu-item) {
    width: 100% !important;
    height: auto !important;
    min-height: 48px !important;
    flex-shrink: 0 !important;
  }
</style>
