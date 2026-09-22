<script lang="ts">
  import {
    createPromptTemplate,
    deletePromptTemplate,
    listPromptTemplates,
    updatePromptTemplate,
  } from "$lib/api";
  import type { PromptTemplate } from "$lib/types";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { t } from "$lib/i18n/index.svelte";

  let {
    visible = false,
    showToast,
    confirmAction = $bindable<{
      title: string;
      message: string;
      onConfirm: () => void;
    } | null>(null),
  }: {
    visible?: boolean;
    showToast: (message: string, type: "success" | "error") => void;
    confirmAction: {
      title: string;
      message: string;
      onConfirm: () => void;
    } | null;
  } = $props();

  let templates = $state<PromptTemplate[]>([]);
  let loading = $state(true);
  let query = $state("");
  let sourceFilter = $state<"all" | "builtin" | "user">("all");
  let selectedTemplate = $state<PromptTemplate | null>(null);

  let showEditor = $state(false);
  let editingTemplate = $state<PromptTemplate | null>(null);
  let editorName = $state("");
  let editorDescription = $state("");
  let editorContent = $state("");
  let saving = $state(false);

  let filteredTemplates = $derived(
    templates.filter((template) => {
      if (sourceFilter === "builtin" && !template.builtin) return false;
      if (sourceFilter === "user" && template.builtin) return false;
      if (!query.trim()) return true;
      const normalizedQuery = query.trim().toLowerCase();
      return (
        template.name.toLowerCase().includes(normalizedQuery) ||
        (template.description?.toLowerCase().includes(normalizedQuery) ?? false) ||
        template.content.toLowerCase().includes(normalizedQuery)
      );
    }),
  );

  $effect(() => {
    if (visible) void loadTemplates();
  });

  async function loadTemplates() {
    loading = true;
    try {
      templates = await listPromptTemplates();
      if (selectedTemplate) {
        selectedTemplate =
          templates.find((template) => template.id === selectedTemplate?.id) ?? null;
      }
      dbg("prompt-templates", "loaded", { count: templates.length });
    } catch (error) {
      dbgWarn("prompt-templates", "load error", error);
      templates = [];
      selectedTemplate = null;
    } finally {
      loading = false;
    }
  }

  function openCreateModal() {
    editingTemplate = null;
    editorName = "";
    editorDescription = "";
    editorContent = "";
    showEditor = true;
  }

  function openEditModal(template: PromptTemplate) {
    if (template.builtin) return;
    editingTemplate = template;
    editorName = template.name;
    editorDescription = template.description ?? "";
    editorContent = template.content;
    showEditor = true;
  }

  async function handleSave() {
    const name = editorName.trim().replace(/^\//, "");
    if (!name) {
      showToast(t("promptTemplates_nameRequired"), "error");
      return;
    }
    if (!editorContent.trim()) {
      showToast(t("promptTemplates_contentRequired"), "error");
      return;
    }

    saving = true;
    try {
      if (editingTemplate) {
        const result = await updatePromptTemplate(
          editingTemplate.id,
          name,
          editorDescription,
          editorContent,
        );
        showToast(result.message || t("promptTemplates_updated"), "success");
      } else {
        const created = await createPromptTemplate(name, editorDescription, editorContent);
        showToast(t("promptTemplates_created", { name: `/${created.name}` }), "success");
        selectedTemplate = created;
      }
      showEditor = false;
      await loadTemplates();
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      saving = false;
    }
  }

  function handleDelete(template: PromptTemplate) {
    if (template.builtin) return;
    confirmAction = {
      title: t("promptTemplates_deleteTitle"),
      message: t("promptTemplates_deleteMessage", { name: `/${template.name}` }),
      onConfirm: async () => {
        try {
          const result = await deletePromptTemplate(template.id);
          showToast(result.message || t("promptTemplates_deleted"), "success");
          if (selectedTemplate?.id === template.id) selectedTemplate = null;
          await loadTemplates();
        } catch (error) {
          showToast(String(error), "error");
        }
      },
    };
  }

  async function cloneBuiltin(template: PromptTemplate) {
    if (!template.builtin) return;
    saving = true;
    try {
      const created = await createPromptTemplate(
        template.name,
        template.description ?? "",
        template.content,
      );
      showToast(t("promptTemplates_cloned", { name: `/${created.name}` }), "success");
      selectedTemplate = created;
      await loadTemplates();
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      saving = false;
    }
  }
</script>

<div class="space-y-4">
  <div class="flex items-start justify-between gap-4">
    <div>
      <h2 class="text-sm font-semibold text-foreground">{t("promptTemplates_title")}</h2>
      <p class="mt-1 text-xs text-muted-foreground">{t("promptTemplates_description")}</p>
    </div>
    <button
      type="button"
      class="shrink-0 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
      onclick={openCreateModal}
    >
      + {t("promptTemplates_new")}
    </button>
  </div>

  <div class="flex items-center gap-2">
    <div class="relative flex-1">
      <svg
        class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
      </svg>
      <input
        type="search"
        placeholder={t("promptTemplates_search")}
        class="w-full rounded-md border border-border bg-background py-1.5 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        bind:value={query}
      />
    </div>
    <div class="flex shrink-0 rounded-md border border-border p-0.5">
      {#each [["all", "promptTemplates_all"], ["builtin", "promptTemplates_builtin"], ["user", "promptTemplates_user"]] as filter}
        <button
          type="button"
          class="rounded px-2.5 py-1 text-xs font-medium transition-colors {sourceFilter ===
          filter[0]
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (sourceFilter = filter[0] as "all" | "builtin" | "user")}
        >
          {t(filter[1] as import("$lib/i18n/types").MessageKey)}
        </button>
      {/each}
    </div>
  </div>

  {#if loading}
    <div class="flex items-center justify-center py-12 text-xs text-muted-foreground">
      <div
        class="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-primary/30 border-t-primary"
      ></div>
      {t("promptTemplates_loading")}
    </div>
  {:else if filteredTemplates.length === 0}
    <div
      class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border py-14 text-center"
    >
      <svg
        class="mb-3 h-8 w-8 text-muted-foreground/60"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        aria-hidden="true"
      >
        <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
        <polyline points="14 2 14 8 20 8" />
      </svg>
      <h3 class="mb-1 text-xs font-medium text-foreground">{t("promptTemplates_empty")}</h3>
      <p class="mb-3 max-w-sm text-[11px] text-muted-foreground">
        {t("promptTemplates_emptyDescription")}
      </p>
      <button
        type="button"
        class="rounded-md border border-border px-3 py-1 text-xs text-foreground hover:bg-muted"
        onclick={openCreateModal}
      >
        {t("promptTemplates_create")}
      </button>
    </div>
  {:else}
    <div class="flex gap-3" style="height: calc(100vh - 310px); min-height: 300px;">
      <div class="w-[280px] shrink-0 space-y-1.5 overflow-y-auto pr-1">
        {#each filteredTemplates as template (template.id)}
          <button
            type="button"
            class="w-full rounded-lg border px-3 py-2 text-left transition-colors {selectedTemplate?.id ===
            template.id
              ? 'border-primary/50 bg-primary/5'
              : 'border-border/50 bg-muted/30 hover:bg-muted/50'}"
            onclick={() => (selectedTemplate = template)}
          >
            <span class="flex items-center justify-between gap-1">
              <span class="truncate font-mono text-xs font-medium text-foreground"
                >/{template.name}</span
              >
              <span
                class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {template.builtin
                  ? 'bg-purple-500/10 text-purple-600 dark:text-purple-400'
                  : 'bg-muted text-muted-foreground'}"
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

      <div
        class="flex flex-1 flex-col justify-between overflow-y-auto rounded-lg border border-border bg-card p-4"
      >
        {#if selectedTemplate}
          <div class="space-y-3">
            <div class="flex items-start justify-between gap-3 border-b border-border pb-3">
              <div class="min-w-0">
                <h3 class="truncate font-mono text-sm font-semibold text-foreground">
                  /{selectedTemplate.name}
                </h3>
                <p class="mt-1 text-[11px] text-muted-foreground">
                  {selectedTemplate.builtin
                    ? t("promptTemplates_builtinHint")
                    : t("promptTemplates_userHint")}
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-1.5">
                {#if selectedTemplate.builtin}
                  <button
                    type="button"
                    class="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-muted disabled:opacity-50"
                    disabled={saving}
                    onclick={() => cloneBuiltin(selectedTemplate!)}
                  >
                    {t("promptTemplates_clone")}
                  </button>
                {:else}
                  <button
                    type="button"
                    class="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-muted"
                    onclick={() => openEditModal(selectedTemplate!)}
                  >
                    {t("promptTemplates_edit")}
                  </button>
                  <button
                    type="button"
                    class="rounded border border-destructive/30 px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                    onclick={() => handleDelete(selectedTemplate!)}
                  >
                    {t("promptTemplates_delete")}
                  </button>
                {/if}
              </div>
            </div>

            {#if selectedTemplate.description}
              <p class="text-xs italic text-muted-foreground">{selectedTemplate.description}</p>
            {/if}
            <div>
              <span class="mb-1 block text-[11px] font-medium text-muted-foreground">
                {t("promptTemplates_content")}
              </span>
              <pre
                class="overflow-x-auto whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed text-foreground">{selectedTemplate.content}</pre>
            </div>
          </div>
        {:else}
          <div
            class="flex h-full items-center justify-center text-center text-xs text-muted-foreground"
          >
            {t("promptTemplates_select")}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

{#if showEditor}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
    <div class="w-full max-w-xl space-y-4 rounded-xl border border-border bg-card p-5 shadow-xl">
      <div class="flex items-center justify-between border-b border-border pb-3">
        <h3 class="text-sm font-semibold text-foreground">
          {editingTemplate
            ? t("promptTemplates_editTitle", { name: `/${editingTemplate.name}` })
            : t("promptTemplates_createTitle")}
        </h3>
        <button
          type="button"
          class="text-muted-foreground hover:text-foreground"
          aria-label={t("common_close")}
          onclick={() => (showEditor = false)}>✕</button
        >
      </div>

      <div class="space-y-3 text-xs">
        <div>
          <label class="mb-1 block font-medium text-foreground" for="prompt-template-name">
            {t("promptTemplates_name")}
          </label>
          <div class="flex items-center gap-1">
            <span class="font-mono text-muted-foreground">/</span>
            <input
              id="prompt-template-name"
              type="text"
              placeholder="refactor"
              class="flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={editorName}
            />
          </div>
        </div>
        <div>
          <label class="mb-1 block font-medium text-foreground" for="prompt-template-description">
            {t("promptTemplates_descriptionLabel")}
          </label>
          <input
            id="prompt-template-description"
            type="text"
            placeholder={t("promptTemplates_descriptionPlaceholder")}
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            bind:value={editorDescription}
          />
        </div>
        <div>
          <label class="mb-1 block font-medium text-foreground" for="prompt-template-content">
            {t("promptTemplates_content")}
          </label>
          <textarea
            id="prompt-template-content"
            rows="11"
            placeholder={t("promptTemplates_contentPlaceholder")}
            class="w-full rounded-md border border-border bg-background p-3 font-mono text-xs leading-relaxed text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            bind:value={editorContent}
          ></textarea>
        </div>
      </div>

      <div class="flex justify-end gap-2 border-t border-border pt-3">
        <button
          type="button"
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
          onclick={() => (showEditor = false)}
        >
          {t("common_cancel")}
        </button>
        <button
          type="button"
          class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          disabled={saving}
          onclick={handleSave}
        >
          {saving ? t("promptTemplates_saving") : t("common_save")}
        </button>
      </div>
    </div>
  </div>
{/if}
