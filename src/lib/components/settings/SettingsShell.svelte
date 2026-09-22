<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    pageTitle?: string;
    pageDescription?: string;
    wide?: boolean;
    headerAction?: Snippet;
    sidebar: Snippet;
    children?: Snippet;
  }

  let {
    pageTitle = "",
    pageDescription = "",
    wide = false,
    headerAction,
    sidebar,
    children,
  }: Props = $props();
</script>

<div
  class="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans antialiased"
>
  <!-- Left Sidebar -->
  {@render sidebar()}

  <!-- Right Main Content -->
  <main class="flex-1 overflow-y-auto overflow-x-hidden scrollbar-thin">
    <div
      class="mx-auto w-full {wide
        ? 'max-w-[1400px] px-8 sm:px-12 py-10'
        : 'max-w-[1080px] px-8 sm:px-14 lg:px-16 py-10'} pb-24"
    >
      {#if pageTitle || headerAction}
        <div class="mb-7 flex items-start justify-between gap-6 border-b border-border/30 pb-4">
          <div class="min-w-0">
            {#if pageTitle}
              <h1 class="text-[22px] font-semibold text-foreground tracking-tight leading-tight">
                {pageTitle}
              </h1>
            {/if}
            {#if pageDescription}
              <p class="mt-1 text-[13px] text-muted-foreground/80 leading-relaxed">
                {pageDescription}
              </p>
            {/if}
          </div>
          {#if headerAction}
            <div class="shrink-0 pt-0.5">
              {@render headerAction()}
            </div>
          {/if}
        </div>
      {/if}

      {#if children}
        <div class="space-y-8">
          {@render children()}
        </div>
      {/if}
    </div>
  </main>
</div>
