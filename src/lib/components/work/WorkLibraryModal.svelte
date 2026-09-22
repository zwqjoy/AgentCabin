<script lang="ts">
  import type { LibraryCategory, LibraryItem, WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    item?: LibraryItem | null;
    workspaces: WorkWorkspaceSummary[];
    initialWorkspaceId?: string;
    saving?: boolean;
    onSave: (item: LibraryItem) => Promise<void>;
    onClose: () => void;
  }

  let {
    item = null,
    workspaces,
    initialWorkspaceId = "",
    saving = false,
    onSave,
    onClose,
  }: Props = $props();

  let title = $state(item?.title ?? "");
  let description = $state(item?.description ?? "");
  let category = $state<LibraryCategory>(item?.category ?? "doc");
  let selectedWorkspaceId = $state<string>(item ? (item.workspaceId ?? "") : initialWorkspaceId);
  let content = $state(item?.content ?? "");
  let tagsInput = $state(item?.tags ? item.tags.join(", ") : "");
  let collection = $state(item?.collection ?? "");
  let metadataInput = $state(
    item?.metadata
      ? Object.entries(item.metadata)
          .map(([key, value]) => `${key}=${value}`)
          .join("\n")
      : "",
  );
  let error = $state("");

  function parseMetadata(): Record<string, string> {
    const metadata: Record<string, string> = {};
    for (const line of metadataInput.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      const separator = trimmed.indexOf("=");
      if (separator <= 0) continue;
      const key = trimmed.slice(0, separator).trim();
      const value = trimmed.slice(separator + 1).trim();
      if (key && value) metadata[key] = value;
    }
    return metadata;
  }

  async function handleSubmit() {
    if (!title.trim()) {
      error = "请填写资料标题";
      return;
    }
    if (!content.trim()) {
      error = "请填写正文或模版内容";
      return;
    }

    error = "";
    const tags = tagsInput
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);

    const updatedItem: LibraryItem = {
      id: item?.id ?? "",
      workspaceId: selectedWorkspaceId ? selectedWorkspaceId : null,
      title: title.trim(),
      description: description.trim(),
      category,
      content: content.trim(),
      tags,
      sourcePath: item?.sourcePath ?? null,
      collection: collection.trim() || null,
      metadata: parseMetadata(),
      citations: item?.citations ?? [],
      sourceArtifactId: item?.sourceArtifactId ?? null,
      createdAt: item?.createdAt ?? "",
      updatedAt: item?.updatedAt ?? "",
    };

    try {
      await onSave(updatedItem);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 backdrop-blur-[2px]"
  role="dialog"
  aria-modal="true"
  aria-labelledby="modal-title"
  tabindex="-1"
  onclick={(e) => e.target === e.currentTarget && onClose()}
>
  <div class="w-full max-w-xl rounded-2xl border border-border bg-card p-6 shadow-2xl space-y-5">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h2 id="modal-title" class="text-base font-semibold text-foreground">
          {item ? "编辑资料条目" : "新建资料条目"}
        </h2>
        <p class="mt-0.5 text-xs text-muted-foreground">
          沉淀跨任务复用的文档、模版、业务规则或数据集，支持一键注入 Agent 上下文
        </p>
      </div>
      <button
        type="button"
        class="rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label="关闭"
        onclick={onClose}
      >
        ✕
      </button>
    </div>

    <form
      class="space-y-4"
      onsubmit={(e) => {
        e.preventDefault();
        void handleSubmit();
      }}
    >
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-category">
            资料类型 *
          </label>
          <select
            id="lib-category"
            class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
            bind:value={category}
          >
            <option value="doc">📄 参考文档</option>
            <option value="template">📑 常用模板</option>
            <option value="rule">⚖️ 业务规则</option>
            <option value="dataset">📊 数据集 / 样本</option>
            <option value="link">🔗 外部链接</option>
          </select>
        </div>

        <div>
          <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-scope">
            作用域范围
          </label>
          <select
            id="lib-scope"
            class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
            bind:value={selectedWorkspaceId}
          >
            <option value="">🌐 全局通用（所有工作空间可见）</option>
            {#each workspaces as ws (ws.id)}
              <option value={ws.id}>📁 仅工作空间: {ws.name}</option>
            {/each}
          </select>
        </div>
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-title">
          资料标题 *
        </label>
        <input
          id="lib-title"
          class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
          placeholder="例如：市场周报 Markdown 模版 / 代码规范指引"
          bind:value={title}
          required
        />
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-desc">
          摘要描述
        </label>
        <input
          id="lib-desc"
          class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
          placeholder="简短描述资料的用途或适用场景…"
          bind:value={description}
        />
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-content">
          正文 / 模版内容 / 规则条例 *
        </label>
        <textarea
          id="lib-content"
          rows="6"
          class="w-full rounded-xl border border-border bg-background px-3 py-2 font-mono text-xs text-foreground outline-none focus:border-primary"
          placeholder="输入 Markdown 正文、结构化模版、规则文本或 URL 链接…"
          bind:value={content}
          required
        ></textarea>
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-tags">
          标签（逗号分隔）
        </label>
        <input
          id="lib-tags"
          class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
          placeholder="例如：report, markdown, finance"
          bind:value={tagsInput}
        />
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-collection">
          集合（可选）
        </label>
        <input
          id="lib-collection"
          class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary"
          placeholder="例如：季度报告 / 产品规范"
          bind:value={collection}
        />
      </div>

      <div>
        <label class="block text-xs font-medium text-muted-foreground mb-1" for="lib-metadata">
          Metadata（每行 key=value，可选）
        </label>
        <textarea
          id="lib-metadata"
          class="min-h-16 w-full resize-y rounded-xl border border-border bg-background px-3 py-2 font-mono text-[11px] text-foreground outline-none focus:border-primary"
          placeholder="owner=产品团队\nversion=2026.08"
          bind:value={metadataInput}
        ></textarea>
      </div>

      {#if error}
        <div
          class="rounded-lg border border-red-500/30 bg-red-500/10 p-2 text-xs text-red-600 dark:text-red-400"
        >
          {error}
        </div>
      {/if}

      <div class="flex items-center justify-end gap-2 border-t border-border/60 pt-4">
        <button
          type="button"
          class="rounded-xl border border-border px-4 py-2 text-xs text-foreground hover:bg-accent"
          onclick={onClose}
        >
          取消
        </button>
        <button
          type="submit"
          class="rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          disabled={saving || !title.trim() || !content.trim()}
        >
          {saving ? "保存中…" : "保存资料"}
        </button>
      </div>
    </form>
  </div>
</div>
