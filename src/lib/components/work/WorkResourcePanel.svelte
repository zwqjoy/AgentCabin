<script lang="ts">
  import type { WorkResourceKind, WorkResourceSummary } from "$lib/types/work";

  type CatalogSource = "discover" | "enabled";

  interface Props {
    resources: WorkResourceSummary[];
    onToggle?: (id: string, enabled: boolean) => Promise<void>;
    bindings?: Record<string, boolean>;
    onDelete?: (id: string) => Promise<void>;
    source?: CatalogSource;
    title?: string;
    description?: string;
    emptyText?: string;
    searchable?: boolean;
    showAll?: boolean;
    canToggle?: boolean;
    profileLabel?: string;
  }

  let {
    resources,
    onToggle,
    bindings = {},
    onDelete,
    source = "discover",
    title = "已安装技能",
    description = "从已安装的全局资源中发现或管理能力。",
    emptyText = "目前没有可管理的资源。",
    searchable = true,
    showAll = false,
    canToggle = true,
    profileLabel = "Work",
  }: Props = $props();

  let updatingId = $state<string | null>(null);
  let localError = $state("");
  let query = $state("");
  let kindFilter = $state<WorkResourceKind | "all">("all");

  const kindOrder: WorkResourceKind[] = [
    "skill",
    "capability",
    "artifact_tool",
    "pi_extension",
    "connector",
  ];

  let installedCount = $derived(resources.length);
  let availableKinds = $derived(
    kindOrder.filter((kind) => resources.some((resource) => resource.kind === kind)),
  );
  let visibleResources = $derived.by(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return resources.filter((resource) => {
      if (source === "enabled" && !showAll && !resource.enabled) return false;
      if (kindFilter !== "all" && resource.kind !== kindFilter) return false;
      if (!normalizedQuery) return true;
      const haystack = [
        resource.id,
        resource.name,
        resource.description,
        resource.entry,
        ...resource.discovery.aliases,
        ...resource.discovery.domains,
        ...resource.discovery.verbs,
        ...resource.discovery.nouns,
        ...resource.discovery.keywords,
      ]
        .join(" ")
        .toLocaleLowerCase();
      return haystack.includes(normalizedQuery);
    });
  });

  function kindLabel(kind: WorkResourceKind): string {
    switch (kind) {
      case "skill":
        return "Skill";
      case "capability":
        return "本机能力";
      case "connector":
        return "连接器";
      case "pi_extension":
        return "Pi 扩展";
      case "artifact_tool":
        return "交付工具";
    }
  }

  function kindInitial(resource: WorkResourceSummary): string {
    if (resource.kind === "pi_extension") return "Pi";
    if (resource.kind === "artifact_tool") return "OUT";
    if (resource.kind === "capability") return "TOOL";
    return resource.name.slice(0, 1).toUpperCase();
  }

  function isEnabled(resource: WorkResourceSummary): boolean {
    return bindings[resource.id] ?? bindings[resource.name] ?? resource.enabled ?? true;
  }

  async function toggle(resource: WorkResourceSummary) {
    if (updatingId) return;
    updatingId = resource.id;
    localError = "";
    try {
      if (onToggle) {
        await onToggle(resource.id, !isEnabled(resource));
      }
    } catch (cause) {
      localError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      updatingId = null;
    }
  }

  let expandedTechId = $state<string | null>(null);

  function toggleTechDetail(id: string) {
    expandedTechId = expandedTechId === id ? null : id;
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-violet-500"></span>
        <h2 class="text-sm font-semibold text-foreground">{title}</h2>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        {description}
      </p>
    </div>
    <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
      <span class="rounded-lg bg-muted px-2.5 py-1.5">{installedCount} 个已安装</span>
    </div>
  </div>

  {#if localError}
    <div
      class="mt-4 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {localError}
    </div>
  {/if}

  {#if searchable || availableKinds.length > 1}
    <div
      class="mt-4 flex flex-col gap-3 rounded-xl border border-border/60 bg-background/35 p-3 sm:flex-row sm:items-center sm:justify-between"
    >
      {#if searchable}
        <label class="relative min-w-0 flex-1 sm:max-w-sm">
          <span class="sr-only">搜索 {profileLabel} 技能</span>
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
            placeholder={source === "discover" ? "搜索名称、领域或用途" : "搜索已启用能力"}
          />
        </label>
      {/if}

      {#if availableKinds.length > 1}
        <div class="flex flex-wrap gap-1" aria-label="资源类型">
          <button
            type="button"
            class="min-h-9 rounded-lg px-3 text-[11px] font-medium transition-colors {kindFilter ===
            'all'
              ? 'bg-foreground text-background'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
            aria-pressed={kindFilter === "all"}
            onclick={() => (kindFilter = "all")}
          >
            全部
          </button>
          {#each availableKinds as kind}
            <button
              type="button"
              class="min-h-9 rounded-lg px-3 text-[11px] font-medium transition-colors {kindFilter ===
              kind
                ? 'bg-foreground text-background'
                : 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
              aria-pressed={kindFilter === kind}
              onclick={() => (kindFilter = kind)}
            >
              {kindLabel(kind)}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if visibleResources.length === 0}
    <div class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center">
      <div class="text-xs font-medium text-foreground">
        {query.trim()
          ? "没有匹配的能力"
          : source === "enabled"
            ? "尚未启用任何能力"
            : `${profileLabel} 资源目录已就绪`}
      </div>
      <div class="mt-1 text-[11px] leading-5 text-muted-foreground">
        {query.trim() ? "请尝试其他名称、领域或用途。" : emptyText}
      </div>
    </div>
  {:else}
    <div class="mt-4 grid gap-3 lg:grid-cols-2">
      {#each visibleResources as resource (resource.id)}
        <article
          class="flex min-h-40 flex-col rounded-xl border border-border/60 bg-background/50 p-4 transition-colors hover:border-border hover:bg-background/75"
        >
          <div class="flex items-start gap-3">
            <span
              class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-[10px] font-semibold {resource.active
                ? 'bg-violet-500/10 text-violet-700 dark:text-violet-300'
                : 'bg-muted text-muted-foreground'}"
            >
              {kindInitial(resource)}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <h3 class="truncate text-xs font-semibold text-foreground">{resource.name}</h3>
                {#if resource.origin === "builtin"}
                  <span
                    class="rounded-md bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 dark:text-emerald-300"
                    >AgentCabin 内置</span
                  >
                {:else}
                  <span class="rounded-md bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground"
                    >{kindLabel(resource.kind)}</span
                  >
                {/if}
              </div>
              <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[9px]">
                <span class="rounded-full bg-muted px-2 py-0.5 text-muted-foreground font-medium"
                  >已安装</span
                >
                <span
                  class="rounded-full px-2 py-0.5 font-medium {isEnabled(resource)
                    ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                    : 'bg-muted text-muted-foreground'}"
                  >{isEnabled(resource) ? "全局已启用" : "全局已停用"}</span
                >
                {#if resource.origin === "builtin"}
                  <span class="text-muted-foreground">· 系统内置</span>
                {/if}
                {#if isEnabled(resource) && !resource.runtimeAvailable}
                  <span
                    class="rounded-full bg-amber-500/10 px-1.5 py-0.5 font-medium text-amber-700 dark:text-amber-300"
                    >当前 Runtime 不可用</span
                  >
                {/if}
              </div>
            </div>
          </div>

          <p class="mt-3 line-clamp-2 text-[11px] leading-5 text-muted-foreground">
            {resource.description || resource.entry}
          </p>

          {#if resource.execution && expandedTechId === resource.id}
            <div
              class="mt-3 rounded-lg border border-border/60 bg-muted/40 p-2.5 text-[10px] space-y-1 text-muted-foreground"
            >
              <div class="font-medium text-foreground flex justify-between">
                <span>技术详情</span>
                <span class="text-[9px] text-emerald-600 dark:text-emerald-400">已沙箱隔离</span>
              </div>
              <div class="flex justify-between">
                <span>运行环境:</span>
                <span class="font-mono text-foreground"
                  >{resource.execution.runtime === "node" ? "Node.js" : "Python"}</span
                >
              </div>
              <div class="flex justify-between">
                <span>沙箱隔离:</span>
                <span class="text-foreground">Native Sandbox (严格受限)</span>
              </div>
              <div class="flex justify-between">
                <span>网络权限:</span>
                <span class="text-foreground"
                  >{resource.execution.network === "none" ? "禁止网络访问" : "允许"}</span
                >
              </div>
              <div class="flex justify-between">
                <span>可读目录:</span>
                <span class="font-mono text-foreground"
                  >{resource.execution.readableAreas?.join(", ") || "-"}</span
                >
              </div>
              <div class="flex justify-between">
                <span>可写目录:</span>
                <span class="font-mono text-foreground"
                  >{resource.execution.writableAreas?.join(", ") || "-"}</span
                >
              </div>
            </div>
          {/if}

          <div class="mt-auto flex items-end justify-between gap-3 pt-4">
            <div class="min-w-0 text-[9px] text-muted-foreground/75">
              {#if resource.execution}
                <button
                  type="button"
                  class="text-[10px] text-primary hover:underline"
                  onclick={() => toggleTechDetail(resource.id)}
                >
                  {expandedTechId === resource.id ? "收起详情" : "查看技术参数"}
                </button>
              {:else}
                <div class="truncate font-mono">{resource.entry}</div>
              {/if}
              {#if resource.permissions.length > 0}
                <div class="mt-0.5">{resource.permissions.length} 项权限声明</div>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              {#if canToggle}
                <button
                  type="button"
                  class="min-h-8 rounded-lg border px-2.5 text-[10px] font-medium transition-colors disabled:opacity-50 {isEnabled(
                    resource,
                  )
                    ? 'border-border text-muted-foreground hover:bg-muted hover:text-foreground'
                    : 'border-primary/40 bg-primary/10 text-primary hover:bg-primary/20'}"
                  aria-label={`${isEnabled(resource) ? "停用" : "启用"} ${resource.name}`}
                  disabled={updatingId !== null}
                  onclick={() => void toggle(resource)}
                >
                  {updatingId === resource.id
                    ? "保存中…"
                    : isEnabled(resource)
                      ? "全局停用"
                      : "全局启用"}
                </button>
              {/if}
              {#if onDelete && resource.origin !== "builtin"}
                <button
                  type="button"
                  class="min-h-7 rounded-lg border border-red-500/30 px-2.5 text-[10px] font-medium text-red-500 transition-colors hover:bg-red-500/10 disabled:opacity-50"
                  disabled={updatingId !== null}
                  onclick={async () => {
                    if (confirm(`确定要从 ~/.agentcabin/skills 中卸载 ${resource.name} 吗？`)) {
                      updatingId = resource.id;
                      try {
                        await onDelete(resource.id);
                      } catch (e) {
                        localError = e instanceof Error ? e.message : String(e);
                      } finally {
                        updatingId = null;
                      }
                    }
                  }}
                >
                  卸载
                </button>
              {/if}
            </div>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>
