<script lang="ts">
  import { onMount } from "svelte";
  import {
    getAgentPluginBindings,
    installAgentPlugin,
    listAgentPlugins,
    setAgentPluginBinding,
    setAgentPluginTrust,
    uninstallAgentPlugin,
    updateAgentPlugin,
  } from "$lib/api";
  import type { AgentPluginSummary } from "$lib/types";

  interface Props {
    canManage?: boolean;
    expertKind?: "expert" | "expert-team";
  }

  let { canManage = true, expertKind }: Props = $props();
  let plugins = $state<AgentPluginSummary[]>([]);
  let source = $state("");
  let loading = $state(true);
  let busyId = $state("");
  let error = $state("");
  let notice = $state("");
  let visiblePlugins = $derived(
    plugins.filter((item) =>
      expertKind
        ? item.packageFormat === "workbuddy" && item.expertKind === expertKind
        : item.packageFormat !== "workbuddy",
    ),
  );
  let displayedCount = $derived(visiblePlugins.length);

  async function load() {
    loading = true;
    error = "";
    try {
      const [items, bindings] = await Promise.all([
        listAgentPlugins(),
        getAgentPluginBindings().catch(() => []),
      ]);
      const bindingMap = new Map(bindings.map((binding) => [binding.pluginId, binding.enabled]));
      plugins = items.map((item) => ({
        ...item,
        enabled: bindingMap.get(item.id) ?? item.enabled,
      }));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });

  function replacePlugin(updated: AgentPluginSummary) {
    plugins = plugins.some((item) => item.id === updated.id)
      ? plugins.map((item) => (item.id === updated.id ? updated : item))
      : [...plugins, updated];
  }

  async function chooseLocal() {
    if (!canManage || busyId) return;
    error = "";
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        title: expertKind === "expert-team" ? "导入 WorkBuddy 专家团" : "导入插件包",
        multiple: false,
        directory: true,
      });
      if (selected && typeof selected === "string") {
        source = selected;
        await install();
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function install() {
    const value = source.trim();
    if (!value || busyId) return;
    busyId = "install";
    error = "";
    notice = "";
    try {
      const installed = await installAgentPlugin(value);
      replacePlugin(installed);
      source = "";
      notice = "已导入 " + (installed.displayName ?? installed.name) + "，默认未信任、未启用。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function toggleTrust(item: AgentPluginSummary) {
    if (!canManage || busyId) return;
    busyId = "trust:" + item.id;
    error = "";
    notice = "";
    try {
      const updated = await setAgentPluginTrust(item.id, !item.trusted);
      replacePlugin(updated);
      notice = updated.trusted
        ? "已信任 " + updated.name + "。"
        : "已撤销 " + updated.name + " 的信任并停用。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function toggleEnabled(item: AgentPluginSummary, enabled: boolean) {
    if (!canManage || !item.trusted || busyId) return;
    busyId = "binding:" + item.id;
    error = "";
    try {
      await setAgentPluginBinding(item.id, enabled);
      replacePlugin({
        ...item,
        enabled,
      });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function update(item: AgentPluginSummary) {
    if (!canManage || busyId) return;
    busyId = "update:" + item.id;
    error = "";
    notice = "";
    try {
      replacePlugin(await updateAgentPlugin(item.id));
      notice = "已从来源更新 " + item.name + "。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function uninstall(item: AgentPluginSummary) {
    if (!canManage || busyId) return;
    try {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm("卸载插件“" + item.name + "”？插件数据目录会保留。", {
        title: "卸载插件",
        kind: "warning",
      });
      if (!ok) return;
    } catch {
      return;
    }
    busyId = "uninstall:" + item.id;
    error = "";
    notice = "";
    try {
      await uninstallAgentPlugin(item.id);
      plugins = plugins.filter((candidate) => candidate.id !== item.id);
      notice = "已卸载 " + item.name + "，插件数据已保留。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  function sourceLabel(item: AgentPluginSummary): string {
    if (!item.source) return "本地目录导入";
    return item.source.kind === "github" ? "GitHub" : "本地目录";
  }

  function componentCount(item: AgentPluginSummary, kind: string): number {
    return item.components?.filter((component) => component.kind === kind).length ?? 0;
  }
</script>

<section class="space-y-4 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-violet-500"></span>
        <h2 class="text-sm font-semibold text-foreground">
          {!expertKind ? "Agent Plugins" : expertKind === "expert-team" ? "专家团" : "专家"}
        </h2>
        <span
          class="rounded-full bg-violet-500/10 px-2 py-0.5 text-[11px] text-violet-700 dark:text-violet-300"
          >{displayedCount}</span
        >
      </div>
      <p class="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
        {expertKind
          ? "直接导入 WorkBuddy .codebuddy-plugin 专家包；包内 Skill、Agent 指令和 MCP 依赖会一起加载。"
          : "导入 Agent Plugin 或 Claude Code Plugin；Skills 和 MCP 复用 AgentCabin Host，其他 Claude 组件仅识别，不执行。"}
      </p>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground/80">
        组件配置只读展示；环境变量和请求头仅显示键名，不把凭据展示到界面。
      </p>
    </div>
    {#if canManage}
      <div class="flex w-full flex-col gap-2 sm:w-auto sm:min-w-[min(100%,32rem)]">
        <div class="flex gap-2">
          <input
            bind:value={source}
            class="min-h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 text-xs outline-none placeholder:text-muted-foreground/70 focus:border-primary"
            placeholder="GitHub URL 或插件包目录"
            aria-label="插件包来源"
            onkeydown={(event) => {
              if (event.key === "Enter") void install();
            }}
          />
          <button
            type="button"
            class="min-h-9 shrink-0 rounded-lg bg-foreground px-3 text-[11px] font-semibold text-background transition-opacity hover:opacity-90 disabled:opacity-50"
            disabled={!source.trim() || Boolean(busyId)}
            onclick={() => void install()}
          >
            {busyId === "install" ? "安装中…" : "安装"}
          </button>
        </div>
        <button
          type="button"
          class="self-start text-[11px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
          disabled={Boolean(busyId)}
          onclick={() => void chooseLocal()}
        >
          选择本地插件包
        </button>
      </div>
    {/if}
  </div>

  {#if error}
    <div
      class="rounded-lg border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-400"
      role="alert"
    >
      {error}
    </div>
  {/if}
  {#if notice}
    <div
      class="rounded-lg border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300"
    >
      {notice}
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center gap-2 py-8 text-sm text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary/25 border-t-primary"
      ></span>
      正在读取 {!expertKind ? "Agent Plugins" : expertKind === "expert-team" ? "专家团" : "专家"}…
    </div>
  {:else if visiblePlugins.length === 0}
    <div
      class="rounded-xl border border-dashed border-border px-4 py-8 text-center text-xs text-muted-foreground"
    >
      {!expertKind
        ? "还没有导入 Agent Plugin。"
        : expertKind === "expert-team"
          ? "还没有导入专家团。"
          : "还没有导入专家。"}可粘贴 GitHub 地址，或选择本地插件目录。
    </div>
  {:else}
    <div class="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">
      {#each visiblePlugins as item (item.id)}
        <article
          class="flex min-w-0 flex-col rounded-xl border border-border/70 bg-background/45 p-3 transition-colors hover:border-border hover:bg-background/70"
        >
          <div class="flex min-w-0 items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                <h3
                  class="truncate text-sm font-semibold text-foreground"
                  title={item.displayName ?? item.name}
                >
                  {item.displayName ?? item.name}
                </h3>
                {#if item.version}<span class="shrink-0 text-[10px] text-muted-foreground"
                    >v{item.version}</span
                  >{/if}
              </div>
              <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                <span
                  class="rounded-full px-2 py-0.5 text-[10px] {item.trusted
                    ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                    : 'bg-amber-500/10 text-amber-700 dark:text-amber-300'}"
                  >{item.trusted ? "已信任" : "未信任"}</span
                >
                <span class="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
                  >{sourceLabel(item)}</span
                >
                {#if item.expertKind}
                  <span
                    class="rounded-full bg-violet-500/10 px-2 py-0.5 text-[10px] text-violet-700 dark:text-violet-300"
                  >
                    {item.expertKind === "expert-team" ? "WorkBuddy 专家团" : "WorkBuddy 专家"}
                  </span>
                {:else}
                  <span class="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
                    {item.packageFormat === "claude-code" ? "Claude Code" : "Agent Plugin"}
                  </span>
                {/if}
              </div>
            </div>
          </div>

          {#if item.description}
            <p class="mt-2 line-clamp-2 min-h-9 text-xs leading-[18px] text-muted-foreground">
              {item.description}
            </p>
          {:else}
            <p class="mt-2 min-h-9 text-xs leading-[18px] text-muted-foreground">暂无简介</p>
          {/if}

          <div class="mt-2 flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground">
            <span class="rounded-md bg-muted/70 px-2 py-1">Skills {item.skills.length}</span>
            <span class="rounded-md bg-muted/70 px-2 py-1">MCP {item.mcpServers.length}</span>
            {#if item.members.length > 0}
              <span class="rounded-md bg-muted/70 px-2 py-1">{item.members.length} 位成员</span>
            {/if}
            {#if item.warnings.length > 0}
              <span class="rounded-md bg-amber-500/10 px-2 py-1 text-amber-700 dark:text-amber-300"
                >警告 {item.warnings.length}</span
              >
            {/if}
          </div>

          <div
            class="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2.5"
          >
            <label
              class="inline-flex min-h-7 cursor-pointer items-center gap-1.5 rounded-md px-1 text-[11px] text-muted-foreground has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50"
            >
              <input
                type="checkbox"
                checked={item.enabled}
                disabled={!canManage || !item.trusted || Boolean(busyId)}
                onchange={(event) =>
                  void toggleEnabled(item, (event.currentTarget as HTMLInputElement).checked)}
              />
              全局启用
            </label>
            <div class="flex flex-wrap items-center gap-1.5">
              {#if canManage}
                <button
                  type="button"
                  class="rounded-md border border-border px-2 py-1 text-[10px] transition-colors hover:bg-muted disabled:opacity-50"
                  disabled={Boolean(busyId)}
                  onclick={() => void toggleTrust(item)}
                >
                  {busyId === "trust:" + item.id ? "处理中…" : item.trusted ? "撤销信任" : "信任"}
                </button>
                {#if item.canUpdate}
                  <button
                    type="button"
                    class="rounded-md border border-border px-2 py-1 text-[10px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                    disabled={Boolean(busyId)}
                    onclick={() => void update(item)}
                  >
                    {busyId === "update:" + item.id ? "更新中…" : "更新"}
                  </button>
                {/if}
                <button
                  type="button"
                  class="rounded-md border border-destructive/30 px-2 py-1 text-[10px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                  disabled={Boolean(busyId)}
                  onclick={() => void uninstall(item)}
                >
                  卸载
                </button>
              {/if}
            </div>
          </div>

          <details class="group mt-1 border-t border-border/60 pt-2">
            <summary
              class="flex cursor-pointer list-none items-center justify-between gap-2 py-1 text-[11px] text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden"
            >
              <span>查看成员、组件与配置</span>
              <span class="transition-transform group-open:rotate-180" aria-hidden="true">⌄</span>
            </summary>
            <div class="mt-2 space-y-3 border-t border-border/50 pt-3">
              {#if item.members.length > 0}
                <div>
                  <h4 class="mb-1 text-[11px] font-semibold text-foreground">
                    成员 · {item.members.length}
                  </h4>
                  <p class="text-[11px] leading-4 text-muted-foreground">
                    {item.members.map((member) => member.name).join("、")}
                  </p>
                </div>
              {/if}
              {#if item.source?.location}
                <p
                  class="truncate font-mono text-[10px] text-muted-foreground/70"
                  title={item.source.location}
                >
                  {item.source.location}
                </p>
              {/if}
              {#if item.error}
                <div
                  class="rounded-md border border-red-500/20 bg-red-500/5 px-2.5 py-2 text-xs text-red-600 dark:text-red-400"
                >
                  清单无效：{item.error}
                </div>
              {/if}
              {#if !expertKind}
                <div class="text-xs text-muted-foreground">
                  {item.packageFormat === "claude-code" ? "Claude Code Plugin" : "Agent Plugin"}
                  · Compatibility: {item.compatibility?.level === "full"
                    ? "Full · 完全兼容"
                    : item.compatibility?.level === "partial"
                      ? "Partial · 部分兼容"
                      : "Unsupported · 不可运行"}
                  <p>
                    Skills {item.skills.length} · MCP {item.mcpServers.length}
                    {#each ["commands", "agents", "hooks"] as kind}
                      · {kind} {componentCount(item, kind)}
                    {/each}
                  </p>
                  {#if item.compatibility?.supported.length}
                    <p>支持执行：{item.compatibility.supported.join(", ")}</p>
                  {/if}
                  {#if item.compatibility?.detectedUnsupported.length}
                    <p>已识别，当前不执行：{item.compatibility.detectedUnsupported.join(", ")}</p>
                  {/if}
                  {#each item.compatibility?.blocked ?? [] as blocked}
                    <p class="text-destructive">Blocked: {blocked}</p>
                  {/each}
                </div>
              {/if}
              {#if item.warnings.length > 0}
                <details
                  class="rounded-md border border-amber-500/20 bg-amber-500/5 px-2.5 py-2 text-xs text-amber-700 dark:text-amber-300"
                >
                  <summary class="cursor-pointer">解析警告（{item.warnings.length}）</summary>
                  <ul class="mt-1 space-y-1 pl-4">
                    {#each item.warnings as warning}
                      <li>{warning}</li>
                    {/each}
                  </ul>
                </details>
              {/if}

              {#if !item.trusted}
                <p class="text-[10px] text-amber-700 dark:text-amber-300">信任后才能启用</p>
              {/if}

              <div class="grid gap-3 border-t border-border/50 pt-3 lg:grid-cols-2">
                <div class="min-w-0">
                  <div class="mb-2 flex items-center justify-between gap-2">
                    <h4 class="text-[11px] font-semibold text-foreground">Skills</h4>
                    <span class="text-[10px] text-muted-foreground">{item.skills.length}</span>
                  </div>
                  {#if item.skills.length === 0}
                    <p class="text-[11px] text-muted-foreground">未声明 Skill。</p>
                  {:else}
                    <div class="space-y-2">
                      {#each item.skills as skill (skill.id)}
                        <div class="rounded-lg border border-border/60 bg-card/50 px-2.5 py-2">
                          <div class="flex items-start justify-between gap-2">
                            <div class="min-w-0">
                              <p class="text-xs font-medium text-foreground">{skill.name}</p>
                              <p
                                class="mt-0.5 break-all font-mono text-[10px] text-violet-700 dark:text-violet-300"
                              >
                                {skill.id}
                              </p>
                            </div>
                            <span
                              class="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                              >只读</span
                            >
                          </div>
                          {#if skill.description}
                            <p class="mt-1 text-[11px] leading-4 text-muted-foreground">
                              {skill.description}
                            </p>
                          {/if}
                          <p
                            class="mt-1 truncate font-mono text-[10px] text-muted-foreground/70"
                            title={skill.path}
                          >
                            {skill.path}
                          </p>
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>
                <div class="min-w-0">
                  <div class="mb-2 flex items-center justify-between gap-2">
                    <h4 class="text-[11px] font-semibold text-foreground">MCP</h4>
                    <span class="text-[10px] text-muted-foreground">{item.mcpServers.length}</span>
                  </div>
                  {#if item.mcpServers.length === 0}
                    <p class="text-[11px] text-muted-foreground">未声明 MCP Server。</p>
                  {:else}
                    <div class="space-y-2">
                      {#each item.mcpServers as server (server.id)}
                        <div class="rounded-lg border border-border/60 bg-card/50 px-2.5 py-2">
                          <div class="flex items-start justify-between gap-2">
                            <div class="min-w-0">
                              <p class="text-xs font-medium text-foreground">{server.name}</p>
                              <p
                                class="mt-0.5 break-all font-mono text-[10px] text-violet-700 dark:text-violet-300"
                              >
                                {server.id}
                              </p>
                            </div>
                            <span
                              class="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                              >{server.transport}</span
                            >
                          </div>
                          {#if server.command}
                            <p
                              class="mt-1 truncate font-mono text-[10px] text-muted-foreground"
                              title={server.command}
                            >
                              {server.command}
                            </p>
                          {:else if server.url}
                            <p
                              class="mt-1 truncate font-mono text-[10px] text-muted-foreground"
                              title={server.url}
                            >
                              {server.url}
                            </p>
                          {/if}
                          {#if server.cwd}
                            <p
                              class="mt-1 truncate font-mono text-[10px] text-muted-foreground/80"
                              title={server.cwd}
                            >
                              cwd: {server.cwd}
                            </p>
                          {/if}
                          {#if server.args.length > 0}
                            <p
                              class="mt-1 truncate font-mono text-[10px] text-muted-foreground/80"
                              title={server.args.join(" ")}
                            >
                              args: {server.args.join(" ")}
                            </p>
                          {/if}
                          <div class="mt-1 flex flex-wrap gap-1 text-[10px] text-muted-foreground">
                            {#if server.envKeys.length > 0}<span
                                class="rounded bg-muted px-1.5 py-0.5"
                                >env: {server.envKeys.join(", ")}</span
                              >{/if}
                            {#if server.headerKeys.length > 0}<span
                                class="rounded bg-muted px-1.5 py-0.5"
                                >headers: {server.headerKeys.join(", ")}</span
                              >{/if}
                            <span class="rounded bg-muted px-1.5 py-0.5">只读配置</span>
                          </div>
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          </details>
        </article>
      {/each}
    </div>
  {/if}
</section>
