<script lang="ts">
  import { onMount } from "svelte";
  import { listMemoryFiles, readTextFile, writeTextFile, revealInFinder } from "$lib/api";
  import type { MemoryFileCandidate } from "$lib/types";
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { dbgWarn } from "$lib/utils/debug";

  let {
    showToast,
    initialAgent = "all",
    nativeOnly = false,
  }: {
    showToast: (message: string, type: "success" | "error") => void;
    initialAgent?: "all" | "claude" | "codex" | "grok" | "pi" | "pi-code";
    nativeOnly?: boolean;
  } = $props();

  type AgentType = "claude" | "codex" | "grok" | "pi" | "pi-code";
  type AgentFilter = "all" | AgentType;

  interface GlobalRuleItem {
    id: string;
    path: string;
    label: string;
    agent: AgentType;
    exists: boolean;
    description: string;
  }

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let agentFilter = $state<AgentFilter>("all");
  let selectedFileId = $state<string>("");
  let content = $state("");
  let savedContent = $state("");
  let viewMode = $state<"edit" | "preview">("preview");
  let templateMenuOpen = $state(false);
  let rawFiles = $state<MemoryFileCandidate[]>([]);

  let isDirty = $derived(content !== savedContent);

  $effect(() => {
    agentFilter =
      nativeOnly && (initialAgent === "pi" || initialAgent === "pi-code") ? "all" : initialAgent;
    selectedFileId = "";
  });

  function getAgentForCandidate(c: MemoryFileCandidate): AgentType {
    if (c.path.includes("/profiles/code/") || c.path.includes("\\profiles\\code\\")) {
      return "pi-code";
    }
    if (
      c.path.includes("/.grok/") ||
      c.path.includes("\\.grok\\") ||
      c.path.includes("/grok-home/") ||
      c.path.includes("\\grok-home\\")
    ) {
      return "grok";
    }
    if (c.path.includes("/.pi/") || c.path.includes("\\.pi\\")) return "pi";
    if (
      c.path.includes("/.codex/") ||
      c.path.includes("\\.codex\\") ||
      (c.label.includes("AGENTS") && !c.path.includes("/.pi/"))
    )
      return "codex";
    return "claude";
  }

  function getFileDescription(agent: AgentType, label: string): string {
    if (agent === "claude") {
      return "Claude Code 全局通用系统指令与编码规范";
    }
    if (agent === "codex") {
      return "Codex CLI 全局通用系统指令与工作规范";
    }
    if (agent === "pi-code") {
      return "Pi Code 独立编程指令与能力规范";
    }
    if (agent === "grok") {
      return "Grok 原生全局规则与项目行为规范";
    }
    return "Pi Coding Agent 全局通用指令与能力规范";
  }

  let ruleItems = $derived.by<GlobalRuleItem[]>(() => {
    return rawFiles
      .filter(
        (f) =>
          f.scope === "global" && !f.label.includes(".local.") && !f.label.includes(".override."),
      )
      .map((f) => {
        const agent = getAgentForCandidate(f);
        return {
          id: `${agent}:${f.label}:${f.path}`,
          path: f.path,
          label: f.label,
          agent,
          exists: f.exists,
          description: getFileDescription(agent, f.label),
        };
      })
      .filter((item) => !nativeOnly || (item.agent !== "pi" && item.agent !== "pi-code"));
  });

  let filteredRuleItems = $derived.by<GlobalRuleItem[]>(() => {
    if (agentFilter === "all") return ruleItems;
    return ruleItems.filter((item) => item.agent === agentFilter);
  });

  let selectedItem = $derived.by<GlobalRuleItem | undefined>(() => {
    if (!selectedFileId && filteredRuleItems.length > 0) {
      return filteredRuleItems[0];
    }
    return ruleItems.find((item) => item.id === selectedFileId) ?? filteredRuleItems[0];
  });

  const PRESET_TEMPLATES = [
    {
      title: "中文优先与简洁编码规范",
      description: "中文沟通、简洁实现、不加冗余抽象与过渡设计",
      content: `# 全局编码规范

## 语言与沟通
- 始终使用中文回复用户，保持专业、精炼。
- 代码、标识符、API 名称、命令与原文字符保留原始语言。

## 编码原则
1. **简洁第一**：用最少的代码解决问题，拒绝过度工程与不必要的抽象层。
2. **精准修改**：仅修改与任务直接相关的代码，保留原有编码风格与习惯。
3. **安全求证**：不确定时主动提问，存在明显更优方案时直接提出建议。
`,
    },
    {
      title: "严谨工程与质量保障规范",
      description: "测试驱动、类型安全、边界条件与异常处理优先",
      content: `# 严谨工程与质量规范

## 质量要求
1. **类型安全**：严格遵循 TypeScript / 强类型规范，避免使用 any 或忽略类型校验。
2. **测试先行**：修改或新增核心逻辑前，明确测试用例；修复 Bug 时编写复现用例。
3. **变更自证**：完成代码修改后，主动执行自动化测试与类型校验以确保通过。

## 架构原则
- 保持单一职责原则与高内聚低耦合；
- 谨慎引入新的第三方依赖，优先利用现有类库能力。
`,
    },
    {
      title: "高效终端与工具协作规范",
      description: "合理调用终端与工具、避免阻塞与盲目重试",
      content: `# 工具协作与终端规范

1. **非阻塞执行**：执行耗时命令或开发服务时，使用后台运行并检查状态。
2. **失败诊断**：命令或测试失败时，仔细分析错误原因并针对性修复，避免盲目重复尝试。
3. **透明可控**：在执行涉及文件删除、破坏性变更的操作前，清晰告知用户。
`,
    },
  ];

  async function loadGlobalFiles() {
    loading = true;
    error = "";
    try {
      rawFiles = await listMemoryFiles();
      if (filteredRuleItems.length > 0 && !selectedFileId) {
        selectedFileId = filteredRuleItems[0].id;
        await loadFileContent(filteredRuleItems[0].path);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      dbgWarn("CodeGlobalRulesPanel", "Failed to list memory files", e);
    } finally {
      loading = false;
    }
  }

  async function loadFileContent(path: string) {
    if (!path) return;
    try {
      content = await readTextFile(path);
      savedContent = content;
    } catch {
      // If file doesn't exist yet, start with empty content
      content = "";
      savedContent = "";
    }
  }

  async function handleSelectFile(item: GlobalRuleItem) {
    if (isDirty) {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm("当前文件有未保存的修改，切换将丢失修改，是否继续？", {
        title: "未保存的修改",
        kind: "warning",
      });
      if (!ok) return;
    }
    selectedFileId = item.id;
    await loadFileContent(item.path);
  }

  async function handleSave() {
    if (!selectedItem || saving) return;
    saving = true;
    error = "";
    try {
      await writeTextFile(selectedItem.path, content);
      savedContent = content;
      showToast(t("memory_saved") || "全局规则已成功保存", "success");
      // Refresh existence status
      rawFiles = await listMemoryFiles();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      showToast(`保存失败: ${error}`, "error");
      dbgWarn("CodeGlobalRulesPanel", "Failed to save rule file", e);
    } finally {
      saving = false;
    }
  }

  async function handleReveal() {
    if (!selectedItem?.path) return;
    try {
      await revealInFinder(selectedItem.path);
    } catch (e) {
      dbgWarn("CodeGlobalRulesPanel", "revealInFinder failed", e);
    }
  }

  async function copyPath() {
    if (!selectedItem?.path) return;
    try {
      await navigator.clipboard.writeText(selectedItem.path);
      showToast("文件路径已复制到剪贴板", "success");
    } catch (e) {
      dbgWarn("CodeGlobalRulesPanel", "copyPath failed", e);
    }
  }

  async function applyTemplate(tplContent: string) {
    if (content.trim()) {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm("应用模板将覆盖当前编辑框中的内容，是否继续？", {
        title: "覆盖内容",
        kind: "warning",
      });
      if (!ok) return;
    }
    content = tplContent;
    templateMenuOpen = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void handleSave();
    }
  }

  function agentBadgeClass(agent: AgentType): string {
    switch (agent) {
      case "codex":
        return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
      case "grok":
        return "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20";
      case "pi":
      case "pi-code":
        return "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20";
      default:
        return "bg-orange-500/10 text-orange-600 dark:text-orange-400 border-orange-500/20";
    }
  }

  function agentDisplayName(agent: AgentType): string {
    switch (agent) {
      case "codex":
        return "Codex";
      case "grok":
        return "Grok";
      case "pi":
        return "Pi";
      case "pi-code":
        return "Pi Code";
      default:
        return "Claude";
    }
  }

  // When agent filter changes, auto-select the first available item if current is filtered out
  $effect(() => {
    if (filteredRuleItems.length > 0) {
      const currentExistsInFiltered = filteredRuleItems.some((item) => item.id === selectedFileId);
      if (!currentExistsInFiltered) {
        selectedFileId = filteredRuleItems[0].id;
        void loadFileContent(filteredRuleItems[0].path);
      }
    }
  });

  onMount(() => {
    void loadGlobalFiles();
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<div class="space-y-4">
  <!-- Header & Agent Filter Toolbar -->
  <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
    <div>
      <h2 class="text-sm font-semibold text-foreground">全局规则配置</h2>
      <p class="text-xs text-muted-foreground mt-0.5">
        按 Agent 分别配置各 AI 助手全局继承的指令、开发规范与系统规则
      </p>
    </div>

    <!-- Agent Filter Pills -->
    <div class="flex items-center gap-1 rounded-lg border border-border bg-muted/20 p-1 shrink-0">
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter === 'all'
          ? 'bg-primary text-primary-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (agentFilter = "all")}
      >
        全部 ({ruleItems.length})
      </button>
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter === 'claude'
          ? 'bg-primary text-primary-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (agentFilter = "claude")}
      >
        Claude ({ruleItems.filter((i) => i.agent === "claude").length})
      </button>
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter === 'codex'
          ? 'bg-primary text-primary-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (agentFilter = "codex")}
      >
        Codex ({ruleItems.filter((i) => i.agent === "codex").length})
      </button>
      <button
        type="button"
        class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter === 'grok'
          ? 'bg-primary text-primary-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (agentFilter = "grok")}
      >
        Grok ({ruleItems.filter((i) => i.agent === "grok").length})
      </button>
      {#if !nativeOnly}
        <button
          type="button"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter === 'pi'
            ? 'bg-primary text-primary-foreground shadow-xs'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (agentFilter = "pi")}
        >
          Pi ({ruleItems.filter((i) => i.agent === "pi").length})
        </button>
        <button
          type="button"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors {agentFilter ===
          'pi-code'
            ? 'bg-primary text-primary-foreground shadow-xs'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (agentFilter = "pi-code")}
        >
          Pi Code ({ruleItems.filter((i) => i.agent === "pi-code").length})
        </button>
      {/if}
    </div>
  </div>

  {#if loading && ruleItems.length === 0}
    <div class="flex h-64 items-center justify-center gap-2 text-muted-foreground">
      <div
        class="h-4 w-4 animate-spin rounded-full border-2 border-primary/30 border-t-primary"
      ></div>
      <span class="text-xs">正在加载全局规则文件…</span>
    </div>
  {:else if filteredRuleItems.length === 0}
    <div
      class="flex h-64 flex-col items-center justify-center gap-2 text-center text-muted-foreground rounded-xl border border-dashed border-border p-6"
    >
      <p class="text-xs">当前筛选分类下暂无规则文件</p>
    </div>
  {:else}
    <!-- Master-Detail Double Column View -->
    <div class="flex flex-col lg:flex-row gap-4 min-h-[540px]">
      <!-- Left Column: File List -->
      <div class="w-full lg:w-72 shrink-0 space-y-2 overflow-y-auto max-h-[calc(100vh-320px)] pr-1">
        {#each filteredRuleItems as item (item.id)}
          {@const active = selectedItem?.id === item.id}
          <button
            type="button"
            class="flex w-full flex-col items-start rounded-xl border p-3 text-left transition-all {active
              ? 'border-primary/50 bg-primary/5 shadow-xs'
              : 'border-border/60 bg-card hover:border-border hover:bg-muted/40'}"
            onclick={() => handleSelectFile(item)}
          >
            <div class="flex w-full items-center justify-between gap-2">
              <div class="flex items-center gap-1.5 min-w-0">
                <span
                  class="rounded-md border px-1.5 py-0.5 text-[10px] font-semibold {agentBadgeClass(
                    item.agent,
                  )}"
                >
                  {agentDisplayName(item.agent)}
                </span>
                <span class="font-mono text-xs font-semibold text-foreground truncate">
                  {item.label}
                </span>
              </div>
              <span
                class="rounded-full px-1.5 py-0.2 text-[9px] font-medium {item.exists
                  ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                  : 'bg-muted text-muted-foreground'}"
              >
                {item.exists ? "已存在" : "未创建"}
              </span>
            </div>

            <p class="mt-1.5 text-[11px] text-muted-foreground leading-snug line-clamp-2">
              {item.description}
            </p>

            <span class="mt-2 font-mono text-[10px] text-muted-foreground/60 truncate w-full">
              {item.path}
            </span>
          </button>
        {/each}
      </div>

      <!-- Right Column: Editor & Preview Area -->
      {#if selectedItem}
        <div
          class="flex flex-1 flex-col rounded-xl border border-border bg-card shadow-sm overflow-hidden min-h-[500px]"
        >
          <!-- Toolbar -->
          <div
            class="flex flex-wrap items-center justify-between gap-2 border-b border-border/80 bg-muted/20 px-3.5 py-2.5 shrink-0"
          >
            <!-- File Info & Path -->
            <div class="flex items-center gap-2 min-w-0">
              <span
                class="rounded-md border px-1.5 py-0.5 text-[10px] font-semibold {agentBadgeClass(
                  selectedItem.agent,
                )}"
              >
                {agentDisplayName(selectedItem.agent)}
              </span>
              <span class="font-mono text-xs font-semibold text-foreground truncate">
                {selectedItem.label}
              </span>
              <div
                class="hidden sm:flex items-center gap-1 font-mono text-[10px] text-muted-foreground/80 bg-muted/50 rounded px-2 py-0.5 max-w-[220px] truncate"
                title={selectedItem.path}
              >
                <span class="truncate">{selectedItem.path}</span>
                <button
                  type="button"
                  class="hover:text-foreground transition-colors shrink-0"
                  title="复制路径"
                  onclick={copyPath}
                >
                  <svg
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
                    <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
                  </svg>
                </button>
              </div>
            </div>

            <!-- Actions: Edit/Preview + Templates + Save -->
            <div class="flex items-center gap-2">
              <!-- Edit/Preview switch -->
              <div class="flex rounded-lg border border-border/80 bg-background p-0.5">
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-colors {viewMode ===
                  'edit'
                    ? 'bg-foreground text-background shadow-xs'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (viewMode = "edit")}
                >
                  <svg
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    <path d="m15 5 4 4" />
                  </svg>
                  <span>编辑</span>
                </button>
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-colors {viewMode ===
                  'preview'
                    ? 'bg-foreground text-background shadow-xs'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (viewMode = "preview")}
                >
                  <svg
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                  <span>预览</span>
                </button>
              </div>

              <!-- Presets -->
              <div class="relative">
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-lg border border-border/80 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
                  onclick={() => (templateMenuOpen = !templateMenuOpen)}
                >
                  <span>预设模版</span>
                  <svg
                    class="h-3 w-3 text-muted-foreground/60"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="m6 9 6 6 6-6" />
                  </svg>
                </button>

                {#if templateMenuOpen}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="fixed inset-0 z-40"
                    onclick={() => (templateMenuOpen = false)}
                    onkeydown={() => {}}
                  ></div>
                  <div
                    class="absolute right-0 top-full mt-1.5 z-50 w-72 rounded-xl border border-border bg-popover p-1.5 shadow-xl"
                  >
                    <div
                      class="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase"
                    >
                      选择常用规范模版
                    </div>
                    {#each PRESET_TEMPLATES as tpl}
                      <button
                        type="button"
                        class="flex w-full flex-col items-start rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-muted/70"
                        onclick={() => applyTemplate(tpl.content)}
                      >
                        <span class="text-xs font-medium text-foreground">{tpl.title}</span>
                        <span class="mt-0.5 text-[10px] text-muted-foreground leading-snug"
                          >{tpl.description}</span
                        >
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>

              <!-- Reveal in finder -->
              <button
                type="button"
                class="hidden md:flex items-center gap-1 rounded-lg border border-border/80 bg-background px-2.5 py-1 text-xs font-medium text-foreground transition-colors hover:bg-muted"
                title="在访达/文件管理器中查看"
                onclick={handleReveal}
              >
                <svg
                  class="h-3 w-3 text-muted-foreground"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path
                    d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
                  />
                </svg>
                <span>访达</span>
              </button>

              <!-- Save button -->
              <button
                type="button"
                class="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1 text-xs font-medium text-primary-foreground shadow-xs transition-all hover:bg-primary/90 disabled:opacity-50"
                disabled={saving || !isDirty}
                onclick={handleSave}
              >
                {#if saving}
                  <svg class="h-3 w-3 animate-spin" viewBox="0 0 24 24" fill="none">
                    <circle
                      class="opacity-25"
                      cx="12"
                      cy="12"
                      r="10"
                      stroke="currentColor"
                      stroke-width="4"
                    ></circle>
                    <path
                      class="opacity-75"
                      fill="currentColor"
                      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                    ></path>
                  </svg>
                  <span>保存中...</span>
                {:else}
                  <svg
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
                    <polyline points="17 21 17 13 7 13 7 21" />
                    <polyline points="7 3 7 8 15 8" />
                  </svg>
                  <span>保存</span>
                  <span class="rounded bg-primary-foreground/20 px-1 py-0.2 text-[9px]">⌘S</span>
                {/if}
              </button>
            </div>
          </div>

          <!-- Editor Body -->
          <div class="flex-1 flex flex-col min-h-[460px] bg-background/50">
            {#if viewMode === "edit"}
              <div class="flex-1 flex flex-col p-1 min-h-[460px]">
                <CodeEditor
                  bind:content
                  filePath={selectedItem.label}
                  onsave={handleSave}
                  class="flex-1 min-h-[460px] text-xs font-mono"
                />
              </div>
            {:else}
              <div class="flex-1 overflow-y-auto p-6 min-h-[460px]">
                {#if content.trim()}
                  <article class="prose prose-sm dark:prose-invert max-w-none text-xs">
                    <MarkdownContent text={content} />
                  </article>
                {:else}
                  <div
                    class="flex h-48 flex-col items-center justify-center gap-2 text-center text-muted-foreground"
                  >
                    <p class="text-xs">当前规则文件内容为空，切换至“编辑”或应用模版开始编写。</p>
                  </div>
                {/if}
              </div>
            {/if}
          </div>

          <!-- Footer Info -->
          <div
            class="flex items-center justify-between border-t border-border/80 bg-muted/20 px-4 py-2 text-[11px] text-muted-foreground shrink-0"
          >
            <div class="flex items-center gap-2">
              {#if isDirty}
                <span class="flex items-center gap-1 text-amber-500 font-medium">
                  <span class="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse"></span>
                  未保存变更
                </span>
              {:else}
                <span class="flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
                  <svg
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M20 6 9 17l-5-5" />
                  </svg>
                  已同步
                </span>
              {/if}
              <span>·</span>
              <span class="hidden sm:inline">{selectedItem.description}</span>
            </div>
            <div>
              <span>{content.length} 字符</span>
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
