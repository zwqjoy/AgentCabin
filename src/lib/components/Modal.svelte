<script lang="ts">
  import { trapFocus } from "$lib/utils/focus-trap";

  let {
    open = $bindable(false),
    title = "",
    closeable = true,
    children,
  }: {
    open?: boolean;
    title?: string;
    closeable?: boolean;
    children?: import("svelte").Snippet;
  } = $props();

  let dialogEl: HTMLDivElement | undefined = $state();

  // Auto-focus dialog container when opened so Escape hits onkeydown here
  $effect(() => {
    if (open) {
      requestAnimationFrame(() => {
        const first = dialogEl?.querySelector<HTMLElement>(
          "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href]",
        );
        (first ?? dialogEl)?.focus();
      });
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (!closeable) {
        e.stopPropagation();
        return;
      }
      e.stopPropagation();
      open = false;
    }
    trapFocus(e, dialogEl ?? null);
  }

  function handleBackdropClick() {
    if (!closeable) return;
    open = false;
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={handleKeydown}
  >
    <!-- Backdrop -->
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
      onclick={handleBackdropClick}
      role="presentation"
    ></div>

    <!-- Content -->
    <div
      class="relative z-50 flex max-h-[calc(100vh-2rem)] w-[calc(100%-2rem)] max-w-lg flex-col overflow-hidden rounded-lg border bg-background p-6 shadow-lg sm:max-h-[calc(100vh-3rem)]"
    >
      {#if title}
        <h2 class="mb-4 shrink-0 text-lg font-semibold">{title}</h2>
      {/if}
      {#if children}
        <div class="min-h-0 overflow-y-auto pr-1">
          {@render children()}
        </div>
      {/if}
    </div>
  </div>
{/if}
