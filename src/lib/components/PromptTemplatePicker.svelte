<script lang="ts">
  import type { PromptTemplate } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";

  let {
    open = false,
    templates = [],
    loading = false,
    onSelect,
    onClose,
  }: {
    open?: boolean;
    templates?: PromptTemplate[];
    loading?: boolean;
    onSelect?: (template: PromptTemplate) => void;
    onClose?: () => void;
  } = $props();

  let query = $state("");
  let selectedId = $state("");
  let searchInput: HTMLInputElement | undefined = $state();

  let filteredTemplates = $derived(
    templates.filter((template) => {
      if (!query.trim()) return true;
      const normalizedQuery = query.trim().toLowerCase();
      return (
        template.name.toLowerCase().includes(normalizedQuery) ||
        (template.description?.toLowerCase().includes(normalizedQuery) ?? false) ||
        template.content.toLowerCase().includes(normalizedQuery)
      );
    }),
  );
  let selectedTemplate = $derived(
    filteredTemplates.find((template) => template.id === selectedId) ??
      filteredTemplates[0] ??
      null,
  );

  $effect(() => {
    if (!open) {
      query = "";
      selectedId = "";
      return;
    }
    if (!selectedTemplate || !filteredTemplates.some((template) => template.id === selectedId)) {
      selectedId = filteredTemplates[0]?.id ?? "";
    }
  });

  $effect(() => {
    if (open) requestAnimationFrame(() => searchInput?.focus());
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose?.();
    }
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-[70] flex items-center justify-center p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="prompt-template-picker-title"
    tabindex="-1"
    onkeydown={handleKeydown}
  >
    <button
      type="button"
      class="absolute inset-0 bg-black/45 backdrop-blur-[1px]"
      aria-label={t("common_close")}
      onclick={() => onClose?.()}
    ></button>

    <div
      class="relative z-10 flex max-h-[min(78vh,680px)] w-full max-w-[800px] flex-col overflow-hidden rounded-xl border border-border bg-background shadow-2xl"
    >
      <div class="flex items-center justify-between border-b border-border px-5 py-3.5">
        <div>
          <h2 id="prompt-template-picker-title" class="text-sm font-semibold text-foreground">
            {t("prompt_templatePickerTitle")}
          </h2>
          <p class="mt-0.5 text-[11px] text-muted-foreground">
            {t("promptTemplates_description")}
          </p>
        </div>
        <button
          type="button"
          class="rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label={t("common_close")}
          onclick={() => onClose?.()}
        >
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="border-b border-border p-3">
        <div class="relative">
          <svg
            class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
          >
            <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
          </svg>
          <input
            bind:this={searchInput}
            type="search"
            placeholder={t("prompt_templatePickerSearch")}
            class="w-full rounded-lg border border-border bg-muted/20 py-2.5 pl-10 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            bind:value={query}
          />
        </div>
      </div>

      {#if loading}
        <div class="flex min-h-[280px] items-center justify-center text-xs text-muted-foreground">
          <div
            class="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-primary/30 border-t-primary"
          ></div>
          {t("prompt_templatePickerLoading")}
        </div>
      {:else if filteredTemplates.length === 0}
        <div class="flex min-h-[280px] items-center justify-center text-xs text-muted-foreground">
          {t("prompt_templatePickerEmpty")}
        </div>
      {:else}
        <div
          class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[minmax(250px,0.8fr)_minmax(0,1.2fr)]"
        >
          <div class="min-h-0 overflow-y-auto border-b border-border p-2 md:border-b-0 md:border-r">
            {#each filteredTemplates as template (template.id)}
              <button
                type="button"
                class="mb-1 w-full rounded-lg border px-3 py-2 text-left transition-colors {selectedTemplate?.id ===
                template.id
                  ? 'border-primary/50 bg-primary/5'
                  : 'border-transparent hover:border-border hover:bg-accent/50'}"
                onclick={() => (selectedId = template.id)}
              >
                <span class="flex items-center justify-between gap-2">
                  <span class="truncate font-mono text-xs font-medium text-foreground"
                    >/{template.name}</span
                  >
                  <span
                    class="shrink-0 rounded-full px-1.5 py-0.5 text-[10px] {template.builtin
                      ? 'bg-purple-500/10 text-purple-600 dark:text-purple-400'
                      : 'bg-primary/10 text-primary'}"
                  >
                    {template.builtin ? t("promptTemplates_builtin") : t("promptTemplates_user")}
                  </span>
                </span>
                {#if template.description}
                  <span class="mt-1 block truncate text-[11px] text-muted-foreground"
                    >{template.description}</span
                  >
                {/if}
              </button>
            {/each}
          </div>

          <div class="flex min-h-0 flex-col p-4">
            {#if selectedTemplate}
              <div class="min-h-0 flex-1 overflow-y-auto">
                <div class="flex items-start justify-between gap-3">
                  <div>
                    <h3 class="font-mono text-sm font-semibold text-foreground">
                      /{selectedTemplate.name}
                    </h3>
                    {#if selectedTemplate.description}
                      <p class="mt-1 text-xs text-muted-foreground">
                        {selectedTemplate.description}
                      </p>
                    {/if}
                  </div>
                  <span class="shrink-0 text-[10px] text-muted-foreground">
                    {selectedTemplate.builtin
                      ? t("promptTemplates_builtin")
                      : t("promptTemplates_user")}
                  </span>
                </div>
                <pre
                  class="mt-4 whitespace-pre-wrap rounded-lg border border-border bg-muted/30 p-3 font-mono text-xs leading-relaxed text-foreground">{selectedTemplate.content}</pre>
              </div>
              <button
                type="button"
                class="mt-4 w-full rounded-lg bg-primary px-3 py-2 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
                onclick={() => onSelect?.(selectedTemplate)}
              >
                {t("prompt_templatePickerInsert")}
              </button>
            {:else}
              <div
                class="flex h-full items-center justify-center text-center text-xs text-muted-foreground"
              >
                {t("prompt_templatePickerSelect")}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
