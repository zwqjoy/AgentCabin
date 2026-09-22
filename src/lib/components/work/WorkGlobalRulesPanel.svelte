<script lang="ts">
  import { onMount } from "svelte";
  import { getWorkRulesInfo, saveWorkRules } from "$lib/api/work";
  import { revealInFinder } from "$lib/api";
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { dbgWarn } from "$lib/utils/debug";

  let loading = $state(true);
  let saving = $state(false);
  let rulesPath = $state("");
  let content = $state("");
  let savedContent = $state("");
  let error = $state("");
  let viewMode = $state<"edit" | "preview">("preview");
  let toastVisible = $state(false);
  let toastFading = $state(false);
  let templateMenuOpen = $state(false);

  let isDirty = $derived(content !== savedContent);

  const PRESET_TEMPLATES = [
    {
      title: "通用办公与分析规范",
      description: "包含思考流程、交付物排版、客观求证等办公通用原则",
      content: `# Work 全局工作规范

## 核心原则
1. **目标驱动**：在执行任务前，先理清用户的核心诉求与交付目标。
2. **严谨求证**：涉及数据、事实或引用时，优先使用真实工具（如连接器、Browser 等）查证。
3. **结构化输出**：汇报与交付必须条理清晰，多用表格、列表与可视化结构，避免冗长堆叠。

## 交互与工具调用
- 在需要用户决策时，主动提供清晰的选项与推荐建议；
- 执行重要操作或生成关键报告时，优先提供阶段性进度与最终结果摘要。
`,
    },
    {
      title: "严谨报告与交付物规范",
      description: "强调阶段化交付、事实佐证与文档产出质量",
      content: `# 交付物与专业报告规范

## 交付标准
1. **格式规范**：正式文档产出需包含完整的【标题、背景摘要、核心要点、详细分析、结论与建议】。
2. **可执行性**：建议与方案应具备落地可行性，明确责任主体与预期时间节点。
3. **语言风格**：保持专业、客观、精炼的商业化表述风格。
`,
    },
    {
      title: "精简高效交互规范",
      description: "直接给出结论与方案，减少冗余客套",
      content: `# 精简高效工作规范

1. **直奔主题**：直接提供用户所需的核心答案或解决方案，省略无必要的寒暄。
2. **分点陈述**：多用短句和清晰标号，重点内容加粗突出。
3. **代码与操作**：给出完整、可直接执行的步骤或配置。
`,
    },
  ];

  async function loadRules() {
    loading = true;
    error = "";
    try {
      const info = await getWorkRulesInfo();
      rulesPath = info.path;
      content = info.content;
      savedContent = info.content;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      dbgWarn("WorkGlobalRulesPanel", "Failed to load work rules", e);
    } finally {
      loading = false;
    }
  }

  async function handleSave() {
    if (saving) return;
    saving = true;
    error = "";
    try {
      await saveWorkRules(content);
      savedContent = content;
      toastFading = false;
      toastVisible = true;
      setTimeout(() => {
        toastFading = true;
        setTimeout(() => {
          toastVisible = false;
        }, 250);
      }, 2500);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      dbgWarn("WorkGlobalRulesPanel", "Failed to save work rules", e);
    } finally {
      saving = false;
    }
  }

  async function handleReveal() {
    if (!rulesPath) return;
    try {
      await revealInFinder(rulesPath);
    } catch (e) {
      dbgWarn("WorkGlobalRulesPanel", "revealInFinder failed", e);
    }
  }

  async function copyPath() {
    if (!rulesPath) return;
    try {
      await navigator.clipboard.writeText(rulesPath);
      toastFading = false;
      toastVisible = true;
      setTimeout(() => {
        toastFading = true;
        setTimeout(() => {
          toastVisible = false;
        }, 250);
      }, 2000);
    } catch (e) {
      dbgWarn("WorkGlobalRulesPanel", "copyPath failed", e);
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
    if (e.key === "Escape" && templateMenuOpen) {
      e.preventDefault();
      templateMenuOpen = false;
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void handleSave();
    }
  }

  onMount(() => {
    void loadRules();
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<!-- Toast Notification -->
{#if toastVisible}
  <div
    class="fixed top-4 left-1/2 -translate-x-1/2 z-50 transition-all duration-200 {toastFading
      ? 'opacity-0 scale-95'
      : 'opacity-100 scale-100'}"
  >
    <div
      class="flex items-center gap-2 rounded-xl border border-emerald-500/20 bg-emerald-600 px-4 py-2.5 text-xs font-medium text-white shadow-xl shadow-emerald-950/20"
    >
      <svg
        class="h-4 w-4 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M20 6 9 17l-5-5" />
      </svg>
      <span>全局规则已保存，将在下次启动 Work 会话时自动生效</span>
    </div>
  </div>
{/if}

<div class="space-y-4">
  <!-- Header Card -->
  <div class="rounded-xl border border-border bg-card p-4 shadow-sm sm:p-5">
    <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
      <div class="flex items-start gap-3.5">
        <div
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-blue-500 ring-1 ring-blue-500/20"
        >
          <svg
            class="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" />
            <path d="m9 12 2 2 4-4" />
          </svg>
        </div>
        <div>
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold text-foreground">Work 全局规则</h2>
            <span
              class="rounded-md border border-blue-500/20 bg-blue-500/10 px-1.5 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400"
            >
              AGENTS.md
            </span>
          </div>
          <p class="mt-1 text-xs text-muted-foreground leading-relaxed max-w-2xl">
            配置 Work 模式下 Runtime 默认遵循的全局系统指令与行为规范。所有 Work 工作空间
            会话均会自动加载该规则。
          </p>
        </div>
      </div>

      <!-- File Path & Actions -->
      <div class="flex flex-wrap items-center gap-2 text-xs">
        {#if rulesPath}
          <div
            class="flex items-center gap-1.5 rounded-lg border border-border/80 bg-muted/40 px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground"
            title={rulesPath}
          >
            <span class="max-w-[180px] truncate sm:max-w-[240px]">{rulesPath}</span>
            <button
              type="button"
              class="rounded p-0.5 hover:text-foreground transition-colors"
              title="复制路径"
              onclick={copyPath}
            >
              <svg
                class="h-3.5 w-3.5"
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
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-muted"
            onclick={handleReveal}
          >
            <svg
              class="h-3.5 w-3.5 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
              />
            </svg>
            <span>在访达中显示</span>
          </button>
        {/if}
      </div>
    </div>
  </div>

  <!-- Editor Container -->
  <div class="rounded-xl border border-border bg-card shadow-sm overflow-hidden flex flex-col">
    <!-- Toolbar -->
    <div
      class="flex items-center justify-between border-b border-border/80 bg-muted/20 px-3 py-2 shrink-0 sm:px-4"
    >
      <!-- Mode Switcher & Presets -->
      <div class="flex items-center gap-2">
        <div class="flex rounded-lg border border-border/80 bg-background p-0.5">
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-colors {viewMode ===
            'edit'
              ? 'bg-foreground text-background shadow-xs'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (viewMode = "edit")}
          >
            <svg
              class="h-3.5 w-3.5"
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
            class="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-colors {viewMode ===
            'preview'
              ? 'bg-foreground text-background shadow-xs'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (viewMode = "preview")}
          >
            <svg
              class="h-3.5 w-3.5"
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

        <!-- Preset Template Dropdown -->
        <div class="relative">
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-lg border border-border/80 bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
            onclick={() => (templateMenuOpen = !templateMenuOpen)}
          >
            <svg
              class="h-3.5 w-3.5 text-muted-foreground/80"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="m16 16 3-8 3 8c-.87.65-1.92 1-3 1s-2.13-.35-3-1Z" />
              <path d="m2 16 3-8 3 8c-.87.65-1.92 1-3 1s-2.13-.35-3-1Z" />
              <path d="M7 21h10" />
              <path d="M12 3v18" />
              <path d="M3 7h2c2 0 5-1 7-2 2 1 5 2 7 2h2" />
            </svg>
            <span>预设模板</span>
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
              class="absolute left-0 top-full mt-1.5 z-50 w-72 rounded-xl border border-border bg-popover p-1.5 shadow-xl"
            >
              <div class="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase">
                选择快捷规范模板
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
      </div>

      <!-- Save & Status -->
      <div class="flex items-center gap-3">
        {#if isDirty}
          <div class="flex items-center gap-1.5 text-[11px] text-amber-500 font-medium">
            <span class="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse"></span>
            <span>未保存</span>
          </div>
        {:else if !loading}
          <div class="flex items-center gap-1 text-[11px] text-muted-foreground">
            <svg
              class="h-3.5 w-3.5 text-emerald-500"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M20 6 9 17l-5-5" />
            </svg>
            <span>已保存</span>
          </div>
        {/if}

        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-sm transition-all hover:bg-primary/90 disabled:opacity-50"
          disabled={saving || loading || !isDirty}
          onclick={handleSave}
        >
          {#if saving}
            <svg class="h-3.5 w-3.5 animate-spin" viewBox="0 0 24 24" fill="none">
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
              class="h-3.5 w-3.5"
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

    <!-- Error Banner -->
    {#if error}
      <div
        class="border-b border-destructive/20 bg-destructive/10 px-4 py-2 text-xs text-destructive flex items-center justify-between"
      >
        <span>{error}</span>
        <button
          type="button"
          class="underline font-medium hover:opacity-80"
          onclick={() => void loadRules()}>重试</button
        >
      </div>
    {/if}

    <!-- Content Area -->
    <div class="min-h-[420px] max-h-[calc(100vh-340px)] overflow-y-auto bg-background/50">
      {#if loading}
        <div class="flex h-64 flex-col items-center justify-center gap-2 text-muted-foreground">
          <svg class="h-6 w-6 animate-spin" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            ></path>
          </svg>
          <span class="text-xs">正在加载全局规则...</span>
        </div>
      {:else if viewMode === "edit"}
        <div class="flex-1 flex flex-col p-1 min-h-[460px]">
          <CodeEditor
            bind:content
            filePath="AGENTS.md"
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
              <svg
                class="h-8 w-8 text-muted-foreground/40"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
              <p class="text-xs">暂无规则内容，切换至“编辑”模式或选择预设模板开始配置。</p>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Footer Status -->
    <div
      class="flex items-center justify-between border-t border-border/80 bg-muted/20 px-4 py-2 text-[11px] text-muted-foreground shrink-0"
    >
      <div class="flex items-center gap-3">
        <span>支持 Markdown 语法</span>
        <span>·</span>
        <span>快捷键 <code>⌘ + S</code> / <code>Ctrl + S</code> 快速保存</span>
      </div>
      <div>
        <span>{content.length} 字符</span>
      </div>
    </div>
  </div>
</div>
