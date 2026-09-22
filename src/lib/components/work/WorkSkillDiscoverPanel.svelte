<script lang="ts">
  import { onMount } from "svelte";
  import { checkCommunityHealth, searchCommunitySkills } from "$lib/api";
  import { importWorkSkillZip, installWorkCommunitySkill } from "$lib/api/work";
  import { formatInstallCount } from "$lib/utils/format";
  import { trapFocus } from "$lib/utils/focus-trap";
  import type { CommunitySkillResult, ProviderHealth } from "$lib/types";
  import type { WorkResourceSummary } from "$lib/types/work";

  interface Props {
    installed: WorkResourceSummary[];
    onInstalled: (resource: WorkResourceSummary) => void;
  }

  let { installed, onInstalled }: Props = $props();
  let health = $state<ProviderHealth | null>(null);
  let query = $state("");
  let popular = $state<CommunitySkillResult[]>([]);
  let results = $state<CommunitySkillResult[]>([]);
  let loading = $state(true);
  let searching = $state(false);
  let installingId = $state("");
  let error = $state("");
  let success = $state("");
  let importModalOpen = $state(false);
  let importPath = $state("");
  let importSlug = $state("");
  let importing = $state(false);
  let importDialog = $state<HTMLDivElement>();
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  const categories = ["文档", "数据分析", "调研", "PPT", "自动化"];
  let displayResults = $derived(query.trim().length >= 2 ? results : popular);

  $effect(() => {
    if (!importModalOpen || !importDialog) return;
    requestAnimationFrame(() => {
      importDialog
        ?.querySelector<HTMLElement>(
          "input:not([disabled]), button:not([disabled]), select:not([disabled])",
        )
        ?.focus();
    });
  });

  function localSlug(skillId: string): string {
    const last = skillId.split("/").filter(Boolean).at(-1) ?? skillId;
    return last
      .replace(/[^a-zA-Z0-9_-]/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "")
      .toLocaleLowerCase();
  }

  async function chooseImportZip() {
    error = "";
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        title: "导入 Work 技能",
        multiple: false,
        directory: false,
        filters: [{ name: "Skill ZIP", extensions: ["zip"] }],
      });
      if (!selected || typeof selected !== "string") return;
      importPath = selected;
      const filename = selected.split(/[/\\\\]/).pop() ?? "";
      importSlug = localSlug(filename.replace(/\.zip$/i, ""));
      importModalOpen = true;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function importZip() {
    if (!importPath || !importSlug.trim() || importing) return;
    importing = true;
    error = "";
    success = "";
    try {
      const resource = await importWorkSkillZip(importPath, importSlug.trim());
      onInstalled(resource);
      success = `已导入“${resource.name}”，下次 Work 会话生效。`;
      importModalOpen = false;
      importPath = "";
      importSlug = "";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      importing = false;
    }
  }

  function isInstalled(skill: CommunitySkillResult): boolean {
    const slug = localSlug(skill.skill_id);
    return installed.some(
      (resource) =>
        resource.id.toLocaleLowerCase() === slug ||
        resource.name.toLocaleLowerCase() === skill.name.toLocaleLowerCase(),
    );
  }

  async function loadPopular() {
    loading = true;
    error = "";
    try {
      const [nextHealth, nextPopular] = await Promise.all([
        checkCommunityHealth(),
        searchCommunitySkills("skill", 18),
      ]);
      health = nextHealth;
      popular = nextPopular;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  function scheduleSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    const value = query.trim();
    if (value.length < 2) {
      results = [];
      return;
    }
    searchTimer = setTimeout(() => void search(value), 300);
  }

  async function search(value = query.trim()) {
    if (value.length < 2) return;
    searching = true;
    error = "";
    try {
      results = await searchCommunitySkills(value, 30);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      results = [];
    } finally {
      searching = false;
    }
  }

  function chooseCategory(category: string) {
    query = category;
    void search(category);
  }

  async function install(skill: CommunitySkillResult) {
    if (installingId || isInstalled(skill)) return;
    installingId = skill.id;
    error = "";
    success = "";
    try {
      const resource = await installWorkCommunitySkill(skill.source, skill.skill_id);
      onInstalled(resource);
      success = `已安装“${skill.name}”，保存至 ~/.agentcabin/skills/，下次启动会话时生效。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      installingId = "";
    }
  }

  onMount(() => {
    void loadPopular();
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-violet-500"></span>
        <h2 class="text-sm font-semibold text-foreground">Skill 市场</h2>
        <span
          class="h-2 w-2 rounded-full {health === null
            ? 'bg-muted-foreground/40'
            : health.available
              ? 'bg-emerald-500'
              : 'bg-red-500'}"
          title={health?.reason ?? "正在检查市场状态"}
        ></span>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        全局 Skill 市场（skills.sh / Skyll 数据源），安装后保存到 ~/.agentcabin/skills/，Code 与
        Work 均可按需启用。
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="min-h-9 rounded-lg bg-foreground px-3 text-[11px] font-semibold text-background transition-opacity hover:opacity-90"
        onclick={() => void chooseImportZip()}
      >
        导入技能 (.zip)
      </button>
      <button
        type="button"
        class="min-h-9 rounded-lg border border-border px-3 text-[11px] font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50"
        disabled={loading}
        onclick={() => void loadPopular()}
      >
        {loading ? "加载中…" : "刷新推荐"}
      </button>
    </div>
  </div>

  <div class="mt-4 rounded-xl border border-border/60 bg-background/35 p-3">
    <label class="relative block max-w-xl">
      <span class="sr-only">搜索 Skill 市场</span>
      <svg
        class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7"></circle>
        <path d="m20 20-3.2-3.2"></path>
      </svg>
      <input
        class="min-h-10 w-full rounded-lg border border-border bg-background pl-9 pr-3 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-primary/15"
        bind:value={query}
        oninput={scheduleSearch}
        placeholder="搜索名称、领域或用途"
      />
    </label>
    <div class="mt-2 flex flex-wrap gap-1.5">
      {#each categories as category}
        <button
          type="button"
          class="min-h-8 rounded-lg bg-muted px-2.5 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={() => chooseCategory(category)}
        >
          {category}
        </button>
      {/each}
    </div>
  </div>

  {#if error}
    <div
      class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {/if}
  {#if success}
    <div
      class="mt-3 rounded-lg border border-emerald-400/20 bg-emerald-400/5 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300"
      role="status"
    >
      {success}
    </div>
  {/if}

  {#if loading || searching}
    <div class="flex items-center justify-center gap-2 py-12 text-xs text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary/25 border-t-primary"
      ></span>
      {searching ? "正在搜索 Skill…" : "正在加载 Skill 市场…"}
    </div>
  {:else if displayResults.length === 0}
    <div
      class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center text-xs text-muted-foreground"
    >
      没有找到匹配的 Skill。
    </div>
  {:else}
    <div class="mt-4 grid gap-3 lg:grid-cols-3">
      {#each displayResults as skill (skill.id)}
        <article
          class="flex min-h-44 flex-col rounded-xl border border-border/60 bg-background/50 p-4 transition-colors hover:border-border"
        >
          <div class="flex items-start justify-end gap-3">
            <span class="rounded-md bg-muted px-2 py-1 text-[9px] text-muted-foreground"
              >{formatInstallCount(skill.installs)} 次安装</span
            >
          </div>
          <h3 class="mt-3 line-clamp-2 text-xs font-semibold text-foreground">{skill.name}</h3>
          <p class="mt-1 line-clamp-2 break-all text-[10px] leading-4 text-muted-foreground">
            {skill.source}
          </p>
          <div class="mt-auto flex items-center justify-between gap-3 pt-4">
            <span class="text-[9px] text-muted-foreground">skills.sh · Skyll</span>
            <button
              type="button"
              class="min-h-9 rounded-lg px-3 text-[10px] font-semibold transition-colors {isInstalled(
                skill,
              )
                ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                : 'bg-foreground text-background hover:opacity-90'} disabled:cursor-not-allowed disabled:opacity-60"
              disabled={isInstalled(skill) || installingId !== ""}
              onclick={() => void install(skill)}
            >
              {isInstalled(skill) ? "已安装" : installingId === skill.id ? "安装中…" : "安装"}
            </button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>

{#if importModalOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
    role="presentation"
    onkeydown={(event) => {
      if (event.key === "Escape" && !importing) {
        event.preventDefault();
        event.stopPropagation();
        importModalOpen = false;
      }
      trapFocus(event, importDialog ?? null);
    }}
  >
    <div
      bind:this={importDialog}
      class="w-full max-w-md rounded-2xl border border-border bg-card p-5 shadow-xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="work-import-skill-title"
    >
      <div class="flex items-center justify-between gap-3 border-b border-border pb-3">
        <div>
          <h3 id="work-import-skill-title" class="text-sm font-semibold text-foreground">
            导入 Work 技能
          </h3>
          <p class="mt-1 text-[10px] text-muted-foreground">
            仅安装到 Work，不写入 Code 技能目录。
          </p>
        </div>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="关闭"
          onclick={() => (importModalOpen = false)}>✕</button
        >
      </div>

      <div class="mt-4 space-y-3">
        <div>
          <span class="block text-[11px] font-medium text-foreground">ZIP 文件</span>
          <div
            class="mt-1 truncate rounded-lg border border-border bg-muted/35 px-3 py-2 font-mono text-[10px] text-muted-foreground"
            title={importPath}
          >
            {importPath}
          </div>
        </div>
        <label class="block">
          <span class="text-[11px] font-medium text-foreground">技能标识</span>
          <input
            class="mt-1 min-h-10 w-full rounded-lg border border-border bg-background px-3 text-xs text-foreground outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
            bind:value={importSlug}
            placeholder="my-work-skill"
          />
        </label>
        <p class="text-[10px] leading-4 text-muted-foreground">
          压缩包根目录必须包含 SKILL.md；若技能标识已存在，将停止导入以保护现有文件。
        </p>
      </div>

      <div class="mt-5 flex justify-end gap-2 border-t border-border pt-3">
        <button
          type="button"
          class="min-h-9 rounded-lg border border-border px-3 text-[11px] text-muted-foreground hover:text-foreground"
          onclick={() => (importModalOpen = false)}>取消</button
        >
        <button
          type="button"
          class="min-h-9 rounded-lg bg-foreground px-3 text-[11px] font-semibold text-background disabled:cursor-not-allowed disabled:opacity-50"
          disabled={importing || !importSlug.trim()}
          onclick={() => void importZip()}>{importing ? "正在导入…" : "确认导入"}</button
        >
      </div>
    </div>
  </div>
{/if}
