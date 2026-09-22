<script lang="ts">
  import { onMount } from "svelte";
  import { listDirectory, readTextFile, openProjectInVscode } from "$lib/api";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import HighlightedCode from "$lib/components/HighlightedCode.svelte";
  import VscodeIcon from "$lib/components/VscodeIcon.svelte";
  import type { DirEntry } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    cwd: string;
    filePath?: string;
    showFileTree?: boolean;
    onToggleFileTree?: () => void;
    onSelectFile?: (path: string) => void;
  }

  let {
    cwd,
    filePath = "",
    showFileTree = $bindable(true),
    onToggleFileTree,
    onSelectFile,
  }: Props = $props();

  let currentPath = $state("");
  let fileContent = $state("");
  let fileLoading = $state(false);
  let fileError = $state("");
  let viewMode = $state<"rendered" | "source">("rendered");

  // Breadcrumbs calculation
  let breadcrumbs = $derived.by(() => {
    if (!currentPath) return [];
    return currentPath.split("/").filter(Boolean);
  });

  let isMarkdown = $derived(currentPath.endsWith(".md") || currentPath.endsWith(".markdown"));

  async function loadFile(path: string) {
    if (!path) {
      fileContent = "";
      return;
    }
    fileLoading = true;
    fileError = "";
    try {
      const content = await readTextFile(path, cwd);
      fileContent = content;
      currentPath = path;
      onSelectFile?.(path);
    } catch (err: unknown) {
      const folderName = cwd.split("/").filter(Boolean).pop() || "";
      if (folderName && path.startsWith(folderName + "/")) {
        const stripped = path.slice(folderName.length + 1);
        try {
          const content = await readTextFile(stripped, cwd);
          fileContent = content;
          currentPath = stripped;
          onSelectFile?.(stripped);
          return;
        } catch {
          // ignore
        }
      }
      fileError = err instanceof Error ? err.message : String(err);
    } finally {
      fileLoading = false;
    }
  }

  $effect(() => {
    if (filePath && filePath !== currentPath) {
      void loadFile(filePath);
    }
  });

  // ── File Tree in Right Sidebar ──
  interface TreeNode {
    name: string;
    fullPath: string;
    is_dir: boolean;
    expanded: boolean;
    loading?: boolean;
    children?: TreeNode[];
  }

  let tree = $state<TreeNode[]>([]);
  let treeFilter = $state("");
  let treeLoading = $state(false);

  async function loadDir(absPath: string, parentRelPath = ""): Promise<TreeNode[]> {
    try {
      const res = await listDirectory(absPath);
      const entries = res.entries || [];
      const nodes: TreeNode[] = entries.map((e: DirEntry) => ({
        name: e.name,
        fullPath: parentRelPath ? `${parentRelPath}/${e.name}` : e.name,
        is_dir: e.is_dir,
        expanded: false,
        loading: false,
        children: undefined,
      }));
      nodes.sort((a: TreeNode, b: TreeNode) => {
        if (a.is_dir === b.is_dir) return a.name.localeCompare(b.name);
        return a.is_dir ? -1 : 1;
      });
      return nodes;
    } catch {
      return [];
    }
  }

  async function toggleNode(node: TreeNode) {
    if (!node.is_dir) {
      void loadFile(node.fullPath);
      return;
    }
    node.expanded = !node.expanded;
    if (node.expanded && node.children === undefined) {
      node.loading = true;
      try {
        const root = cwd.replace(/\/+$/, "");
        const rel = node.fullPath.replace(/^\/+/, "");
        const absPath = `${root}/${rel}`;
        node.children = await loadDir(absPath, node.fullPath);
      } finally {
        node.loading = false;
      }
    }
  }

  async function refreshTree() {
    if (!cwd) return;
    treeLoading = true;
    try {
      tree = await loadDir(cwd, "");
    } finally {
      treeLoading = false;
    }
  }

  let lastLoadedCwd = "";
  $effect(() => {
    if (cwd && cwd !== lastLoadedCwd) {
      lastLoadedCwd = cwd;
      treeLoading = true;
      void loadDir(cwd, "").then((nodes) => {
        tree = nodes;
        treeLoading = false;
      });
    }
  });

  onMount(() => {
    if (currentPath) {
      void loadFile(currentPath);
    }
  });

  function copyPath() {
    if (currentPath) {
      void navigator.clipboard.writeText(currentPath);
    }
  }

  async function openInEditor() {
    if (cwd) {
      await openProjectInVscode(cwd);
    }
  }

  function filterTreeNodes(nodes: TreeNode[], query: string): TreeNode[] {
    const result: TreeNode[] = [];
    for (const node of nodes) {
      const match =
        node.name.toLowerCase().includes(query) || node.fullPath.toLowerCase().includes(query);
      if (node.is_dir) {
        const filteredChildren = node.children ? filterTreeNodes(node.children, query) : [];
        if (match || filteredChildren.length > 0) {
          result.push({
            ...node,
            expanded: true,
            children: filteredChildren,
          });
        }
      } else if (match) {
        result.push(node);
      }
    }
    return result;
  }

  let filteredTree = $derived.by(() => {
    if (!treeFilter.trim()) return tree;
    return filterTreeNodes(tree, treeFilter.trim().toLowerCase());
  });
</script>

<div class="flex h-full flex-col overflow-hidden bg-background">
  <!-- Top Breadcrumb & Actions Bar -->
  <div
    class="flex h-10 shrink-0 items-center justify-between border-b border-border/60 bg-muted/10 px-4 text-xs"
  >
    <!-- Breadcrumb trail -->
    <div
      class="flex items-center gap-1.5 overflow-x-auto min-w-0 font-medium text-muted-foreground"
    >
      {#each breadcrumbs as part, idx}
        {#if idx > 0}
          <span class="opacity-40">›</span>
        {/if}
        <span class={idx === breadcrumbs.length - 1 ? "text-foreground font-semibold" : ""}>
          {part}
        </span>
      {/each}
      {#if breadcrumbs.length === 0}
        <span>未选择文件</span>
      {/if}
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-1.5 shrink-0">
      {#if isMarkdown}
        <button
          type="button"
          class="flex items-center gap-1 rounded-md border border-border/70 px-2 py-1 text-[11px] text-muted-foreground hover:bg-muted/50 hover:text-foreground transition-colors"
          onclick={() => (viewMode = viewMode === "rendered" ? "source" : "rendered")}
        >
          <span>{viewMode === "rendered" ? "查看源代码" : "查看渲染"}</span>
        </button>
      {/if}

      <button
        type="button"
        class="flex items-center gap-1 rounded-md border border-border/70 px-2 py-1 text-[11px] text-muted-foreground hover:bg-muted/50 hover:text-foreground transition-colors"
        title="在 VS Code 中打开"
        onclick={openInEditor}
      >
        <VscodeIcon class="h-3 w-3" />
        <span>打开</span>
      </button>

      <button
        type="button"
        class="rounded p-1 transition-colors {showFileTree
          ? 'text-foreground hover:bg-muted/60'
          : 'text-muted-foreground/60 hover:bg-muted/40 hover:text-foreground'}"
        title={showFileTree ? "收起右侧文件列表" : "展开右侧文件列表"}
        onclick={() => {
          if (onToggleFileTree) {
            onToggleFileTree();
          } else {
            showFileTree = !showFileTree;
          }
        }}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <path d="M15 3v18" />
        </svg>
      </button>
    </div>
  </div>

  <!-- Content split: Preview on left, File Tree on right -->
  <div class="flex min-h-0 flex-1">
    <!-- Left: Content area -->
    <div class="flex-1 min-w-0 overflow-y-auto p-4">
      {#if fileLoading}
        <div class="flex h-32 items-center justify-center text-xs text-muted-foreground">
          加载文件中…
        </div>
      {:else if fileError}
        <div
          class="rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-600 dark:text-red-400"
        >
          {fileError}
        </div>
      {:else if isMarkdown && viewMode === "rendered"}
        <div class="prose dark:prose-invert max-w-none text-sm leading-relaxed">
          <MarkdownContent text={fileContent} lazy={false} />
        </div>
      {:else if fileContent}
        <div class="font-mono text-xs">
          <HighlightedCode content={fileContent} filePath={currentPath} />
        </div>
      {:else}
        <div class="flex h-32 items-center justify-center text-xs text-muted-foreground">
          请从右侧文件树选择文件进行预览
        </div>
      {/if}
    </div>

    <!-- Right: File Tree Explorer -->
    {#if showFileTree}
      <aside class="flex w-56 shrink-0 flex-col border-l border-border/60 bg-muted/10 text-xs">
        <div class="flex items-center justify-between p-2 border-b border-border/60 gap-1.5">
          <label
            class="flex h-7 flex-1 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 focus-within:ring-1 focus-within:ring-ring"
          >
            <svg
              class="h-3 w-3 shrink-0 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
            </svg>
            <input
              bind:value={treeFilter}
              class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
              placeholder="筛选文件…"
            />
          </label>
          <button
            type="button"
            class="rounded p-1.5 text-muted-foreground hover:bg-muted/60 hover:text-foreground transition-colors shrink-0"
            title="刷新目录"
            onclick={refreshTree}
          >
            <svg
              class="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3L21.5 8M22 12.5a10 10 0 0 1-18.8 4.2L2.5 16"
              />
            </svg>
          </button>
        </div>

        {#snippet renderNode(node: TreeNode, depth: number)}
          <div>
            <button
              type="button"
              class="group flex w-full items-center gap-1 rounded-md px-1.5 py-1 text-left transition-colors {currentPath ===
              node.fullPath
                ? 'bg-accent text-foreground font-semibold'
                : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'}"
              style="padding-left: {depth * 12 + 6}px"
              onclick={() => toggleNode(node)}
            >
              {#if node.is_dir}
                <!-- Chevron -->
                <svg
                  class="h-3 w-3 shrink-0 text-muted-foreground/60 transition-transform duration-150 {node.expanded
                    ? 'rotate-90 text-foreground'
                    : 'group-hover:text-foreground'}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <polyline points="9 18 15 12 9 6" />
                </svg>

                <!-- Folder Icon -->
                <svg
                  class="h-3.5 w-3.5 text-amber-500 shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path
                    d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                  />
                </svg>
              {:else}
                <!-- Spacer for file alignment -->
                <span class="w-3 shrink-0"></span>
                <svg
                  class="h-3.5 w-3.5 text-muted-foreground shrink-0 opacity-70"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <path d="M14 2v6h6" />
                </svg>
              {/if}

              <span class="truncate min-w-0 flex-1">{node.name}</span>

              {#if node.loading}
                <svg
                  class="h-3 w-3 animate-spin text-muted-foreground shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                >
                  <circle
                    class="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="4"
                  ></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"></path>
                </svg>
              {/if}
            </button>

            {#if node.is_dir && node.expanded && node.children}
              <div class="space-y-0.5">
                {#each node.children as child (child.fullPath)}
                  {@render renderNode(child, depth + 1)}
                {/each}
                {#if node.children.length === 0 && !node.loading}
                  <div
                    class="text-[10px] text-muted-foreground/50 italic py-0.5"
                    style="padding-left: {(depth + 1) * 12 + 20}px"
                  >
                    (空目录)
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/snippet}

        <div class="min-h-0 flex-1 overflow-y-auto p-1.5 space-y-0.5 font-mono text-[11px]">
          {#if treeLoading}
            <div class="p-2 text-muted-foreground">加载目录中…</div>
          {:else if filteredTree.length === 0}
            <div class="p-2 text-muted-foreground text-center">无匹配文件</div>
          {:else}
            {#each filteredTree as node (node.fullPath)}
              {@render renderNode(node, 0)}
            {/each}
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</div>
