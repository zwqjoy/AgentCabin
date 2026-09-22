<script lang="ts">
  import {
    listConfiguredMcpServers,
    removeMcpServer,
    listCodexMcpServers,
    removeCodexMcpServer,
    listGrokMcpServers,
    removeGrokMcpServer,
    listPiMcpServers,
    removePiMcpServer,
    listMcpCatalog,
    deleteMcpCatalogServer,
    getMcpBindings,
    setMcpBinding,
  } from "$lib/api";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { t } from "$lib/i18n/index.svelte";
  import type { ConfiguredMcpServer } from "$lib/types";

  type McpTargetRealm = "native" | "pi" | "work" | "all";

  let {
    projectCwd = "",
    visible = false,
    targetRealm = "native",
    targetAgent = "all",
    canToggle = true,
    operationLoading = $bindable<string | null>(null),
    showToast,
    confirmAction = $bindable<{
      title: string;
      message: string;
      onConfirm: () => void;
    } | null>(null),
  }: {
    projectCwd: string;
    visible?: boolean;
    targetRealm?: McpTargetRealm;
    targetAgent?: "all" | "claude" | "codex" | "grok";
    canToggle?: boolean;
    operationLoading: string | null;
    showToast: (message: string, type: "success" | "error") => void;
    confirmAction: {
      title: string;
      message: string;
      onConfirm: () => void;
    } | null;
  } = $props();

  // ── Helpers ──

  function serverKey(s: ConfiguredMcpServer): string {
    return `${s.agent ?? "claude"}:${s.scope}:${s.name}`;
  }

  function displayTransport(t: string): string {
    return t === "streamable-http" ? "HTTP" : t;
  }

  function agentLabel(agent?: string): string {
    switch (agent) {
      case "codex":
        return "Codex";
      case "grok":
        return "Grok";
      case "pi":
        return "Pi";
      case "work":
        return "Work";
      default:
        return "Claude";
    }
  }

  function agentBadgeClass(agent?: string): string {
    switch (agent) {
      case "codex":
        return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400";
      case "grok":
        return "bg-violet-500/10 text-violet-600 dark:text-violet-400";
      case "pi":
        return "bg-purple-500/10 text-purple-600 dark:text-purple-400";
      case "work":
        return "bg-blue-500/10 text-blue-600 dark:text-blue-400";
      default:
        return "bg-orange-500/10 text-orange-600 dark:text-orange-400";
    }
  }

  interface McpGroup {
    name: string;
    bindings: ConfiguredMcpServer[];
  }

  // ── State ──
  let servers = $state<ConfiguredMcpServer[]>([]);
  let loading = $state(false);
  let selectedServer = $state<ConfiguredMcpServer | null>(null);
  let togglingServer = $state<string | null>(null);

  let configuredAgentFilter = $state<"all" | "claude" | "codex" | "grok" | "pi" | "work">("all");
  let configuredScopeFilter = $state<"all" | "user" | "project" | "shared">("all");

  $effect(() => {
    if (targetAgent !== "all") configuredAgentFilter = "all";
  });

  let filteredServers = $derived.by(() => {
    return servers.filter((s) => {
      const agentNorm = s.agent ?? "claude";
      if (targetRealm === "native" && agentNorm === "pi") return false;
      if (targetRealm === "pi" && agentNorm !== "pi") return false;
      if (targetRealm === "work" && agentNorm !== "work") return false;
      if (targetAgent !== "all" && agentNorm !== targetAgent) return false;
      if (configuredAgentFilter !== "all" && agentNorm !== configuredAgentFilter) {
        return false;
      }
      if (configuredScopeFilter !== "all" && s.scope !== configuredScopeFilter) {
        return false;
      }
      return true;
    });
  });

  let groupedServers = $derived.by<McpGroup[]>(() => {
    const map = new Map<string, ConfiguredMcpServer[]>();
    for (const s of filteredServers) {
      const list = map.get(s.name) ?? [];
      list.push(s);
      map.set(s.name, list);
    }
    return Array.from(map.entries())
      .map(([name, bindings]) => ({ name, bindings }))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  // ── Init — reload when tab becomes visible ──

  $effect(() => {
    if (visible) {
      loadServers();
    }
  });

  async function loadServers() {
    loading = true;
    try {
      if (targetRealm === "pi" || targetRealm === "work") {
        const [catalog, bindings] = await Promise.all([
          listMcpCatalog().catch((e) => {
            dbgWarn("mcp-panel", "listMcpCatalog failed", e);
            return [];
          }),
          getMcpBindings().catch((e) => {
            dbgWarn("mcp-panel", "getMcpBindings failed", e);
            return [];
          }),
        ]);

        const catalogServers: ConfiguredMcpServer[] = catalog.map((cat) => {
          const binding = bindings.find((b) => b.serverId.toLowerCase() === cat.id.toLowerCase());
          return {
            name: cat.id,
            server_type: cat.transport || "stdio",
            scope: "shared",
            command: cat.command,
            args: cat.args ?? [],
            url: cat.url,
            env_keys: cat.envSchema ? Object.keys(cat.envSchema) : [],
            header_keys: cat.headersSchema ? Object.keys(cat.headersSchema) : [],
            agent: targetRealm === "work" ? ("work" as const) : ("pi" as const),
            _enabled: binding?.enabled ?? true,
          } as ConfiguredMcpServer & {
            _enabled: boolean;
          };
        });

        servers = catalogServers;
      } else if (targetRealm === "native") {
        const [claude, codex, grok] = await Promise.all([
          targetAgent === "all" || targetAgent === "claude"
            ? listConfiguredMcpServers(projectCwd || undefined).catch((e) => {
                dbgWarn("mcp-panel", "listConfiguredMcpServers failed", e);
                return [];
              })
            : Promise.resolve([]),
          targetAgent === "all" || targetAgent === "codex"
            ? listCodexMcpServers(projectCwd || undefined).catch((e) => {
                dbgWarn("mcp-panel", "listCodexMcpServers failed", e);
                return [];
              })
            : Promise.resolve([]),
          targetAgent === "all" || targetAgent === "grok"
            ? listGrokMcpServers(projectCwd || undefined).catch((e) => {
                dbgWarn("mcp-panel", "listGrokMcpServers failed", e);
                return [];
              })
            : Promise.resolve([]),
        ]);
        servers = [...claude, ...codex, ...grok];
      } else {
        const [claude, codex, grok, pi] = await Promise.all([
          listConfiguredMcpServers(projectCwd || undefined).catch((e) => {
            dbgWarn("mcp-panel", "listConfiguredMcpServers failed", e);
            return [];
          }),
          listCodexMcpServers(projectCwd || undefined).catch((e) => {
            dbgWarn("mcp-panel", "listCodexMcpServers failed", e);
            return [];
          }),
          listGrokMcpServers(projectCwd || undefined).catch((e) => {
            dbgWarn("mcp-panel", "listGrokMcpServers failed", e);
            return [];
          }),
          listPiMcpServers(projectCwd || undefined).catch((e) => {
            dbgWarn("mcp-panel", "listPiMcpServers failed", e);
            return [];
          }),
        ]);
        servers = [...claude, ...codex, ...grok, ...pi];
      }
      dbg("mcp-configured", "loaded", {
        count: servers.length,
      });
    } catch (e) {
      dbgWarn("mcp-configured", "load error", e);
      servers = [];
    } finally {
      loading = false;
    }
  }

  async function refreshServers() {
    try {
      await loadServers();
    } catch (e) {
      dbgWarn("mcp-configured", "refresh error", e);
    }
  }

  async function handleToggle(server: ConfiguredMcpServer & { _enabled?: boolean }) {
    const current = server._enabled ?? true;
    const newEnabled = !current;
    const key = serverKey(server);
    togglingServer = key;
    try {
      await setMcpBinding(server.name, newEnabled);
      server._enabled = newEnabled;
      if (selectedServer && serverKey(selectedServer) === serverKey(server)) {
        (selectedServer as any)._enabled = newEnabled;
      }
      await refreshServers();
      showToast(`已全局${newEnabled ? "启用" : "停用"} MCP`, "success");
    } catch (e) {
      showToast(t("mcp_errorGeneric", { error: String(e) }), "error");
    } finally {
      togglingServer = null;
    }
  }

  function isSharedModeServer(server: ConfiguredMcpServer): boolean {
    const expectedAgent = targetRealm === "work" ? "work" : targetRealm === "pi" ? "pi" : "";
    return (
      Boolean(expectedAgent) &&
      server.agent === expectedAgent &&
      (server.scope === "shared" || server.scope === "user")
    );
  }

  function handleRemove(server: ConfiguredMcpServer) {
    const isSharedCatalogServer = isSharedModeServer(server);
    confirmAction = {
      title: isSharedCatalogServer ? t("mcp_removeSharedTitle") : t("mcp_removeTitle"),
      message: isSharedCatalogServer
        ? t("mcp_removeSharedConfirm", { name: server.name })
        : t("mcp_removeConfirm", { name: server.name, scope: server.scope }),
      onConfirm: async () => {
        operationLoading = serverKey(server);
        try {
          let result;
          if (isSharedCatalogServer) {
            await deleteMcpCatalogServer(server.name);
            result = {
              success: true,
              message: t("mcp_removedSharedServer", { name: server.name }),
            };
          } else if (server.agent === "codex") {
            result = await removeCodexMcpServer(server.name, server.scope, projectCwd || undefined);
          } else if (server.agent === "grok") {
            result = await removeGrokMcpServer(server.name, server.scope, projectCwd || undefined);
          } else if (server.agent === "pi") {
            result = await removePiMcpServer(server.name, server.scope, projectCwd || undefined);
          } else {
            result = await removeMcpServer(server.name, server.scope, projectCwd || undefined);
          }
          showToast(
            isSharedCatalogServer
              ? result.message
              : result.success
                ? t("mcp_removedServer", { name: server.name })
                : result.message,
            result.success ? "success" : "error",
          );
          if (result.success) {
            if (selectedServer && serverKey(selectedServer) === serverKey(server)) {
              selectedServer = null;
            }
            await refreshServers();
          }
          dbg("mcp-configured", "remove result", result);
        } catch (e) {
          showToast(t("mcp_errorGeneric", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  function typeBadgeColor(serverType: string): string {
    switch (serverType) {
      case "stdio":
        return "bg-blue-500/10 text-blue-600 dark:text-blue-400";
      case "http":
      case "streamable-http":
        return "bg-teal-500/10 text-teal-600 dark:text-teal-400";
      case "sse":
        return "bg-purple-500/10 text-purple-600 dark:text-purple-400";
      default:
        return "bg-muted text-muted-foreground";
    }
  }

  function scopeBadgeColor(scope: string): string {
    switch (scope) {
      case "local":
        return "bg-amber-500/10 text-amber-600 dark:text-amber-400";
      case "user":
        return "bg-muted text-muted-foreground";
      case "project":
        return "bg-blue-500/10 text-blue-600 dark:text-blue-400";
      case "shared":
        return "bg-purple-500/10 text-purple-600 dark:text-purple-400";
      default:
        return "bg-muted text-muted-foreground";
    }
  }
</script>

{#if loading}
  <div class="flex items-center justify-center py-8">
    <div
      class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
    ></div>
    <span class="ml-2 text-xs text-muted-foreground">{t("mcp_loadingConfigured")}</span>
  </div>
{:else}
  <!-- Configured Filters Header Toolbar -->
  <div
    class="mb-4 flex items-center justify-between gap-3 flex-wrap border-b border-border/40 pb-3"
  >
    <div class="text-xs font-medium text-muted-foreground">
      已配置 ({filteredServers.length})
    </div>

    <div class="flex items-center gap-3 flex-wrap">
      <!-- Agent filter group -->
      {#if targetRealm !== "pi" && targetRealm !== "work" && targetAgent === "all"}
        <div class="flex rounded-md border border-border p-0.5 shrink-0">
          <button
            class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredAgentFilter ===
            'all'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (configuredAgentFilter = "all")}
          >
            全部
          </button>
          <button
            class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredAgentFilter ===
            'claude'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (configuredAgentFilter = "claude")}
          >
            {t("extend_agentBadge_claude")}
          </button>
          <button
            class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredAgentFilter ===
            'codex'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (configuredAgentFilter = "codex")}
          >
            {t("extend_agentBadge_codex")}
          </button>
          <button
            class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredAgentFilter ===
            'grok'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (configuredAgentFilter = "grok")}
          >
            Grok
          </button>
          {#if targetRealm === "all"}
            <button
              class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredAgentFilter ===
              'pi'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (configuredAgentFilter = "pi")}>Pi</button
            >
          {/if}
        </div>
      {/if}

      <!-- Scope filter group -->
      <div class="flex rounded-md border border-border p-0.5 shrink-0">
        <button
          class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredScopeFilter ===
          'all'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (configuredScopeFilter = "all")}
        >
          全部
        </button>
        <button
          class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredScopeFilter ===
          'user'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (configuredScopeFilter = "user")}
        >
          {t("plugin_scopeUser")}
        </button>
        <button
          class="rounded px-2.5 py-1 text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed {configuredScopeFilter ===
          'project'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          disabled={!projectCwd}
          onclick={() => (configuredScopeFilter = "project")}
        >
          {t("plugin_scopeProject")}
        </button>
        {#if targetRealm === "pi" || targetRealm === "work"}
          <button
            class="rounded px-2.5 py-1 text-xs font-medium transition-colors {configuredScopeFilter ===
            'shared'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (configuredScopeFilter = "shared")}
          >
            共享
          </button>
        {/if}
      </div>
    </div>
  </div>

  {#if servers.length === 0}
    <div class="flex flex-col items-center justify-center py-12 text-center">
      <div
        class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
      >
        <svg
          class="h-6 w-6 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><rect width="20" height="8" x="2" y="2" rx="2" ry="2" /><rect
            width="20"
            height="8"
            x="2"
            y="14"
            rx="2"
            ry="2"
          /><line x1="6" x2="6.01" y1="6" y2="6" /><line x1="6" x2="6.01" y1="18" y2="18" /></svg
        >
      </div>
      <h2 class="text-sm font-medium text-foreground mb-1">{t("mcp_noConfigured")}</h2>
      <p class="text-xs text-muted-foreground max-w-sm">
        {t("mcp_useDiscoverTab")}
      </p>
    </div>
  {:else if filteredServers.length === 0}
    <div class="flex flex-col items-center justify-center py-12 text-center">
      <p class="text-xs text-muted-foreground">没有找到符合当前过滤条件的已配置 MCP 服务器</p>
    </div>
  {:else}
    <div class="flex gap-3" style="height: calc(100vh - 300px); min-height: 300px;">
      <!-- Left: scrollable grouped server list -->
      <div class="w-[280px] shrink-0 overflow-y-auto space-y-2 pr-1">
        {#each groupedServers as group (group.name)}
          <div>
            <!-- Group header: MCP name -->
            <div class="px-3 py-1 flex items-center gap-1.5">
              <span class="text-xs font-semibold text-foreground truncate">{group.name}</span>
              <span class="text-[10px] text-muted-foreground">({group.bindings.length})</span>
            </div>
            <!-- Per-host bindings -->
            {#each group.bindings as server (serverKey(server))}
              {@const enabled = canToggle
                ? ((server as ConfiguredMcpServer & { _enabled?: boolean })._enabled ?? true)
                : true}
              {@const isCatalogServer = isSharedModeServer(server) && server.scope === "shared"}
              <div
                class="w-full text-left rounded-lg border px-3 py-1.5 ml-2 transition-colors cursor-pointer {selectedServer &&
                serverKey(selectedServer) === serverKey(server)
                  ? 'border-primary/50 bg-primary/5'
                  : isCatalogServer && !enabled
                    ? 'border-border/30 bg-muted/10 opacity-60 hover:opacity-80'
                    : 'border-border/50 bg-muted/30 hover:bg-muted/50'}"
                onclick={() => (selectedServer = server)}
                onkeydown={(e) => {
                  if (e.key === "Enter") selectedServer = server;
                }}
                role="button"
                tabindex="0"
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="flex items-center gap-1.5 min-w-0">
                    <span
                      class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {agentBadgeClass(
                        server.agent,
                      )}"
                    >
                      {agentLabel(server.agent)}
                    </span>
                    <span
                      class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {scopeBadgeColor(
                        server.scope,
                      )}"
                    >
                      {server.scope}
                    </span>
                    <span
                      class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {typeBadgeColor(
                        server.server_type,
                      )}"
                    >
                      {displayTransport(server.server_type)}
                    </span>
                  </div>
                  <div class="flex items-center gap-1 shrink-0">
                    {#if isCatalogServer && canToggle}
                      <button
                        type="button"
                        class="inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[9px] font-medium transition-all disabled:opacity-50 {enabled
                          ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 font-semibold'
                          : 'border-border/70 bg-background/50 text-muted-foreground hover:bg-muted hover:text-foreground'}"
                        disabled={togglingServer === serverKey(server)}
                        onclick={(e) => {
                          e.stopPropagation();
                          handleToggle(server as any);
                        }}
                        title={enabled ? "全局停用 MCP" : "全局启用 MCP"}
                      >
                        <span
                          class="h-1.5 w-1.5 rounded-full {enabled
                            ? 'bg-emerald-500'
                            : 'bg-muted-foreground/40'}"
                        ></span>
                        全局 {enabled ? "开" : "关"}
                      </button>
                    {/if}
                    {#if !(server.agent === "codex" && server.scope === "project")}
                      <button
                        class="rounded p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                        onclick={(e) => {
                          e.stopPropagation();
                          handleRemove(server);
                        }}
                        title={isSharedModeServer(server)
                          ? t("mcp_removeSharedServerTooltip")
                          : t("mcp_removeServerTooltip")}
                        disabled={operationLoading === serverKey(server)}
                      >
                        {#if operationLoading === serverKey(server)}
                          <div
                            class="h-3.5 w-3.5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                          ></div>
                        {:else}
                          <svg
                            class="h-3.5 w-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            ><path d="M3 6h18" /><path
                              d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"
                            /><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /></svg
                          >
                        {/if}
                      </button>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/each}
      </div>

      <!-- Right: detail panel -->
      <div class="flex-1 min-w-0 overflow-y-auto">
        {#if selectedServer}
          <div class="rounded-lg border border-border/50 bg-muted/20 p-4 space-y-3">
            <!-- Header -->
            <div class="flex items-start justify-between gap-2">
              <div class="flex-1 min-w-0">
                <h3 class="text-sm font-semibold text-foreground">{selectedServer.name}</h3>
                <div class="flex items-center gap-1.5 mt-1">
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {typeBadgeColor(
                      selectedServer.server_type,
                    )}"
                  >
                    {displayTransport(selectedServer.server_type)}
                  </span>
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {scopeBadgeColor(
                      selectedServer.scope,
                    )}"
                  >
                    {selectedServer.scope}
                  </span>
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {agentBadgeClass(
                      selectedServer.agent,
                    )}"
                  >
                    {agentLabel(selectedServer.agent)}
                  </span>
                </div>
              </div>
              <button
                class="shrink-0 text-muted-foreground hover:text-foreground"
                onclick={() => (selectedServer = null)}
                title={t("common_close")}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg
                >
              </button>
            </div>

            <!-- Command + args (stdio) -->
            {#if selectedServer.command}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_command")}
                </div>
                <div class="rounded-md bg-muted/40 px-3 py-2 font-mono text-xs text-foreground">
                  {selectedServer.command}{#if selectedServer.args?.length > 0}{" " +
                      selectedServer.args.join(" ")}{/if}
                </div>
              </div>
            {/if}

            <!-- URL (http/sse) -->
            {#if selectedServer.url}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">{t("mcp_url")}</div>
                <div
                  class="rounded-md bg-muted/40 px-3 py-2 font-mono text-xs text-foreground truncate"
                >
                  {selectedServer.url}
                </div>
              </div>
            {/if}

            <!-- Env keys -->
            {#if selectedServer.env_keys?.length > 0}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_envVars")}
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each selectedServer.env_keys as key}
                    <span
                      class="rounded-md bg-muted/40 px-2 py-1 font-mono text-[10px] text-foreground"
                      >{key}</span
                    >
                  {/each}
                </div>
              </div>
            {/if}

            <!-- Header keys -->
            {#if selectedServer.header_keys?.length > 0}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_headers")}
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each selectedServer.header_keys as key}
                    <span
                      class="rounded-md bg-muted/40 px-2 py-1 font-mono text-[10px] text-foreground"
                      >{key}</span
                    >
                  {/each}
                </div>
              </div>
            {/if}

            <!-- Source file path -->
            {#if selectedServer.source_path}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_sourceFile")}
                </div>
                <div
                  class="rounded-md bg-muted/40 px-3 py-2 font-mono text-[10px] text-muted-foreground truncate"
                  title={selectedServer.source_path}
                >
                  {selectedServer.source_path}
                </div>
              </div>
            {/if}

            <!-- Enable / Disable control for shared catalog servers -->
            {#if canToggle && isSharedModeServer(selectedServer)}
              {@const selEnabled = (selectedServer as any)._enabled ?? true}
              <div class="border-t border-border pt-3 space-y-2">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-1.5 text-xs text-foreground">
                    <span
                      class="h-2 w-2 rounded-full {selEnabled
                        ? 'bg-emerald-500'
                        : 'bg-muted-foreground/40'}"
                    ></span>
                    <span>全局：{selEnabled ? "启用" : "停用"}</span>
                  </div>
                  <button
                    type="button"
                    class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors disabled:opacity-50 {selEnabled
                      ? 'bg-emerald-600'
                      : 'bg-muted-foreground/30'}"
                    title={selEnabled ? "全局停用 MCP" : "全局启用 MCP"}
                    disabled={togglingServer === serverKey(selectedServer)}
                    onclick={() => handleToggle(selectedServer as any)}
                  >
                    {#if togglingServer === serverKey(selectedServer)}
                      <span class="absolute inset-0 flex items-center justify-center">
                        <div
                          class="h-3 w-3 border border-white/60 border-t-white rounded-full animate-spin"
                        ></div>
                      </span>
                    {:else}
                      <span
                        class="inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform {selEnabled
                          ? 'translate-x-4.5'
                          : 'translate-x-0.5'}"
                      ></span>
                    {/if}
                  </button>
                </div>
              </div>
            {/if}

            <!-- Remove button (hidden for Codex project-scope) -->
            {#if !(selectedServer.agent === "codex" && selectedServer.scope === "project")}
              <div class="border-t border-border pt-3">
                <button
                  class="rounded-md border border-destructive/30 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                  onclick={() => handleRemove(selectedServer!)}
                  disabled={operationLoading === serverKey(selectedServer)}
                >
                  {operationLoading === serverKey(selectedServer)
                    ? t("mcp_removing")
                    : isSharedModeServer(selectedServer)
                      ? t("mcp_removeSharedServer")
                      : t("mcp_removeServer")}
                </button>
              </div>
            {/if}
          </div>
        {:else}
          <div
            class="rounded-lg border border-dashed border-border/50 p-6 flex items-center justify-center h-full"
          >
            <p class="text-xs text-muted-foreground">{t("mcp_selectServerDetails")}</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
{/if}
