<script lang="ts">
  import { discoverWorkBuddyExperts, installAgentPlugin } from "$lib/api";
  import Modal from "$lib/components/Modal.svelte";
  import type {
    AgentPluginSummary,
    WorkBuddyDiscoveredExpert,
    WorkBuddyDiscoveryResult,
  } from "$lib/types";

  type Filter = "all" | "expert" | "expert-team";

  let {
    open = $bindable(false),
    expertKind,
    onImported = () => {},
  }: {
    open?: boolean;
    expertKind?: "expert" | "expert-team";
    onImported?: (plugin: AgentPluginSummary) => void;
  } = $props();

  let result = $state<WorkBuddyDiscoveryResult | null>(null);
  let filter = $state<Filter>(expertKind ?? "all");
  let search = $state("");
  let loading = $state(false);
  let busyId = $state("");
  let error = $state("");
  let notice = $state("");
  let wasOpen = false;

  let experts = $derived(
    result?.packages.filter((item) => item.expertKind === "expert").length ?? 0,
  );
  let teams = $derived(
    result?.packages.filter((item) => item.expertKind === "expert-team").length ?? 0,
  );
  let visiblePackages = $derived.by(() => {
    const query = search.trim().toLocaleLowerCase();
    return (result?.packages ?? []).filter((item) => {
      if (filter !== "all" && item.expertKind !== filter) return false;
      if (!query) return true;
      return [item.displayName, item.name, item.profession, item.description]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase().includes(query));
    });
  });

  $effect(() => {
    if (open && !wasOpen) {
      wasOpen = true;
      filter = expertKind ?? "all";
      search = "";
      notice = "";
      void scan();
    } else if (!open) {
      wasOpen = false;
    }
  });

  async function scan(root?: string) {
    loading = true;
    error = "";
    try {
      result = await discoverWorkBuddyExperts(root);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      result = null;
    } finally {
      loading = false;
    }
  }

  async function chooseOtherDirectory() {
    error = "";
    try {
      const { open: openDialog } = await import("$lib/platform/dialog");
      const selected = await openDialog({
        title: "选择 WorkBuddy 专家目录",
        multiple: false,
        directory: true,
      });
      if (selected && typeof selected === "string") await scan(selected);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function importPackage(item: WorkBuddyDiscoveredExpert) {
    if (busyId || item.alreadyInstalled) return;
    busyId = item.id;
    error = "";
    notice = "";
    try {
      const installed = await installAgentPlugin(item.path);
      notice = `已导入 ${installed.displayName ?? installed.name}，默认未信任、未启用。`;
      onImported(installed);
      await scan(result?.root);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  function kindLabel(kind: WorkBuddyDiscoveredExpert["expertKind"]) {
    return kind === "expert-team" ? "专家团" : "单专家";
  }

  function teamMemberSummary(item: WorkBuddyDiscoveredExpert) {
    const lead = item.members.find((member) => member.role === "lead");
    const others = item.members.filter((member) => member.role !== "lead");
    return [
      lead ? `Lead：${lead.name}` : "",
      others.length ? `成员：${others.map((member) => member.name).join("、")}` : "",
    ]
      .filter(Boolean)
      .join(" · ");
  }
</script>

<Modal bind:open title="从 WorkBuddy 导入">
  <div class="space-y-3">
    <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
      <p>
        {#if result?.available}
          已发现 {experts} 个专家、{teams} 个专家团
        {:else}
          未发现 WorkBuddy 本地专家
        {/if}
      </p>
      {#if loading}
        <span>扫描中…</span>
      {/if}
    </div>

    <label class="sr-only" for="workbuddy-expert-search">搜索专家</label>
    <input
      id="workbuddy-expert-search"
      bind:value={search}
      class="min-h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
      placeholder="搜索专家…"
      disabled={loading || !result?.available}
    />

    <div class="flex gap-1 border-b border-border/70 pb-2" aria-label="筛选专家类型">
      {#each [{ id: "expert", label: "专家" }, { id: "expert-team", label: "专家团" }, { id: "all", label: "全部" }] as option (option.id)}
        <button
          type="button"
          class="rounded-md px-2.5 py-1.5 text-xs transition-colors {filter === option.id
            ? 'bg-foreground text-background'
            : 'text-muted-foreground hover:bg-muted'}"
          aria-pressed={filter === option.id}
          onclick={() => (filter = option.id as Filter)}
        >
          {option.label}
        </button>
      {/each}
    </div>

    {#if error}
      <p
        class="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-400"
        role="alert"
      >
        {error}
      </p>
    {/if}
    {#if notice}
      <p class="text-xs text-emerald-700 dark:text-emerald-300" role="status">{notice}</p>
    {/if}
    {#if result?.warnings.length}
      <p class="text-[11px] text-amber-700 dark:text-amber-300">
        已跳过 {result.warnings.length} 个无法安全读取的目录或损坏包。
      </p>
    {/if}

    {#if loading && !result}
      <div class="py-8 text-center text-sm text-muted-foreground">正在扫描 WorkBuddy 本地专家…</div>
    {:else if !result?.available}
      <div class="py-6 text-center text-sm text-muted-foreground">
        <p>未发现 WorkBuddy 本地专家，可以选择其他目录。</p>
      </div>
    {:else if visiblePackages.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">
        {search.trim() ? "没有匹配的专家。" : "这个目录中没有可导入的专家或专家团。"}
      </div>
    {:else}
      <div class="max-h-[min(48vh,28rem)] divide-y divide-border/70 overflow-y-auto pr-1">
        {#each visiblePackages as item (item.id)}
          <article class="flex items-start justify-between gap-3 py-3 first:pt-1 last:pb-1">
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
                <h3 class="truncate text-sm font-medium text-foreground">
                  {item.displayName ?? item.name}
                </h3>
                <span class="text-[11px] text-muted-foreground">
                  {item.profession ?? kindLabel(item.expertKind)}
                </span>
              </div>
              {#if item.description}
                <p class="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
                  {item.description}
                </p>
              {/if}
              {#if item.expertKind === "expert-team"}
                <p class="mt-1 text-[11px] text-muted-foreground">
                  专家团 · {item.members.length} 位成员
                  {#if item.members.length}
                    · {teamMemberSummary(item)}
                  {/if}
                </p>
              {:else}
                <p class="mt-1 text-[11px] text-muted-foreground">
                  单专家 · Skills {item.skillCount} · MCP {item.mcpCount}
                </p>
              {/if}
              {#if item.expertKind === "expert-team"}
                <p class="mt-1 text-[11px] text-muted-foreground">
                  Skills {item.skillCount} · MCP {item.mcpCount}
                </p>
              {/if}
            </div>
            <button
              type="button"
              class="min-h-8 shrink-0 rounded-md px-3 text-xs font-medium transition-colors {item.alreadyInstalled
                ? 'bg-muted text-muted-foreground'
                : 'bg-foreground text-background hover:opacity-90'}"
              disabled={item.alreadyInstalled || Boolean(busyId) || loading}
              onclick={() => void importPackage(item)}
            >
              {#if item.alreadyInstalled}
                已导入
              {:else if busyId === item.id}
                导入中…
              {:else}
                导入
              {/if}
            </button>
          </article>
        {/each}
      </div>
    {/if}

    <div class="flex justify-end border-t border-border/70 pt-3">
      <button
        type="button"
        class="min-h-8 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        disabled={loading || Boolean(busyId)}
        onclick={() => void chooseOtherDirectory()}
      >
        选择其他目录…
      </button>
    </div>
  </div>
</Modal>
