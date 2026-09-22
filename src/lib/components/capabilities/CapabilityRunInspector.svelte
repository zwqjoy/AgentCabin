<script lang="ts">
  import type { RunEffectiveCapabilitiesView } from "$lib/types/work";

  interface Props {
    runId: string;
    capabilities: RunEffectiveCapabilitiesView | null;
    open: boolean;
    loading?: boolean;
    onClose: () => void;
  }

  let { runId, capabilities, open, loading = false, onClose }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm transition-opacity"
    onclick={onClose}
    role="presentation"
  ></div>

  <!-- Drawer -->
  <div
    class="fixed inset-y-0 right-0 z-50 flex w-full max-w-lg flex-col bg-background p-6 shadow-2xl border-l border-border transition-transform"
    role="dialog"
    aria-modal="true"
    aria-labelledby="run-inspector-title"
  >
    <!-- Header -->
    <div class="flex items-start justify-between gap-4 pb-4 border-b border-border">
      <div>
        <div class="flex items-center gap-2 mb-1">
          <span
            class="rounded bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary uppercase"
          >
            Run Capabilities
          </span>
          <span class="text-xs font-mono text-muted-foreground">
            {runId ? runId.slice(0, 8) : "Current Run"}
          </span>
        </div>
        <h2 id="run-inspector-title" class="text-base font-bold text-foreground">
          本次运行实际生效能力 (Effective Capabilities)
        </h2>
        <p class="text-xs text-muted-foreground mt-0.5">
          记录自当前 Run 启动时的真实生效能力快照，非全局配置
        </p>
      </div>

      <button
        type="button"
        class="rounded-lg p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={onClose}
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto py-4 space-y-5 text-xs">
      {#if loading}
        <div class="flex items-center justify-center py-12 text-muted-foreground">
          <span
            class="h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent mr-2"
          ></span>
          <span>加载生效能力中...</span>
        </div>
      {:else if capabilities}
        <!-- Runtime & Mode -->
        <div class="rounded-lg border border-border bg-muted/30 p-3">
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div>
              <span class="text-muted-foreground block text-[11px]">Runtime Provider</span>
              <span class="font-mono font-semibold capitalize text-foreground"
                >{capabilities.runtime}</span
              >
            </div>
            <div>
              <span class="text-muted-foreground block text-[11px]">App Mode</span>
              <span class="font-mono font-semibold capitalize text-foreground"
                >{capabilities.appMode}</span
              >
            </div>
          </div>
        </div>

        <!-- Skills -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <h3 class="font-semibold text-foreground flex items-center gap-1.5">
              <span>Skills 已挂载</span>
              <span class="text-xs text-muted-foreground"
                >({capabilities.enabledSkills.length})</span
              >
            </h3>
          </div>
          {#if capabilities.enabledSkills.length === 0}
            <p
              class="text-muted-foreground text-xs italic bg-muted/20 p-2.5 rounded border border-border/40"
            >
              无挂载的 Skill
            </p>
          {:else}
            <div class="space-y-1.5">
              {#each capabilities.enabledSkills as skill}
                <div
                  class="flex items-center justify-between rounded-lg border border-border/60 bg-muted/20 px-3 py-2"
                >
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-emerald-500 font-bold">✓</span>
                    <span class="font-medium text-foreground truncate">{skill.name}</span>
                    {#if skill.owner}
                      <span class="text-[11px] text-muted-foreground" title={skill.owner.id}
                        >内部辅助</span
                      >
                    {/if}
                  </div>
                  {#if skill.description}
                    <span class="text-[11px] text-muted-foreground truncate max-w-[180px]">
                      {skill.description}
                    </span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- MCP Servers (Safe Key Names Only) -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <h3 class="font-semibold text-foreground flex items-center gap-1.5">
              <span>MCP Servers (脱敏快照)</span>
              <span class="text-xs text-muted-foreground">({capabilities.mcpServers.length})</span>
            </h3>
          </div>
          {#if capabilities.mcpServers.length === 0}
            <p
              class="text-muted-foreground text-xs italic bg-muted/20 p-2.5 rounded border border-border/40"
            >
              未启用 MCP 服务
            </p>
          {:else}
            <div class="space-y-2">
              {#each capabilities.mcpServers as mcp}
                <div class="rounded-lg border border-border/60 bg-muted/20 p-3">
                  <div class="flex items-center justify-between mb-1.5">
                    <span class="font-medium text-foreground flex items-center gap-1.5">
                      <span class="text-emerald-500 font-bold">✓</span>
                      <span>{mcp.id}</span>
                    </span>
                    <span
                      class="rounded bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground"
                    >
                      {mcp.transport}
                    </span>
                  </div>
                  {#if mcp.envKeys.length > 0}
                    <div class="mt-1 text-[11px] text-muted-foreground">
                      <span>注入变量键名: </span>
                      <span class="font-mono text-foreground">{mcp.envKeys.join(", ")}</span>
                    </div>
                  {/if}
                  {#if mcp.headerKeys.length > 0}
                    <div class="mt-0.5 text-[11px] text-muted-foreground">
                      <span>请求头键名: </span>
                      <span class="font-mono text-foreground">{mcp.headerKeys.join(", ")}</span>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Connectors -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <h3 class="font-semibold text-foreground flex items-center gap-1.5">
              <span>Connectors</span>
              <span class="text-xs text-muted-foreground">({capabilities.connectors.length})</span>
            </h3>
          </div>
          {#if capabilities.connectors.length === 0}
            <p
              class="text-muted-foreground text-xs italic bg-muted/20 p-2.5 rounded border border-border/40"
            >
              未绑定连接器
            </p>
          {:else}
            <div class="space-y-1.5">
              {#each capabilities.connectors as conn}
                <div
                  class="flex items-center justify-between rounded-lg border border-border/60 bg-muted/20 px-3 py-2"
                >
                  <div class="flex items-center gap-2">
                    <span class="text-emerald-500 font-bold">✓</span>
                    <span class="font-medium text-foreground">{conn.name}</span>
                  </div>
                  {#if conn.entryPoint}
                    <span class="text-[11px] font-mono text-muted-foreground"
                      >{conn.entryPoint}</span
                    >
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Browser Access -->
        <div>
          <h3 class="font-semibold text-foreground mb-2">Browser 运行时</h3>
          <div
            class="rounded-lg border border-border/60 bg-muted/20 p-3 flex items-center justify-between"
          >
            <div>
              <span class="font-medium text-foreground block">Web 交互与浏览器执行</span>
              <span class="text-[11px] text-muted-foreground">
                Browser Use: {capabilities.browserUseEnabled ? "已启用" : "未开启"}
              </span>
            </div>
            <span
              class="rounded px-2 py-0.5 text-xs font-medium {capabilities.browserEnabled
                ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20'
                : 'bg-muted text-muted-foreground'}"
            >
              {capabilities.browserStatus}
            </span>
          </div>
        </div>

        <!-- Tools Allowed / Blocked -->
        <div>
          <h3 class="font-semibold text-foreground mb-2">工具管控 (Tools)</h3>
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div class="rounded-lg border border-border/60 bg-muted/20 p-3">
              <span class="text-muted-foreground block text-[11px] mb-1">允许工具 (Allowed)</span>
              <span class="text-lg font-bold font-mono text-foreground">
                {capabilities.allowedTools.length}
              </span>
              {#if capabilities.allowedTools.length > 0}
                <div class="mt-2 flex flex-wrap gap-1">
                  {#each capabilities.allowedTools as tool}
                    <span
                      class="rounded bg-background px-1.5 py-0.5 text-[10px] font-mono text-foreground border border-border/50"
                    >
                      {tool}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>

            <div class="rounded-lg border border-border/60 bg-muted/20 p-3">
              <span class="text-muted-foreground block text-[11px] mb-1">拦截工具 (Blocked)</span>
              <span class="text-lg font-bold font-mono text-foreground">
                {capabilities.disallowedTools.length}
              </span>
              {#if capabilities.disallowedTools.length > 0}
                <div class="mt-2 flex flex-wrap gap-1">
                  {#each capabilities.disallowedTools as tool}
                    <span
                      class="rounded bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-mono text-rose-600 dark:text-rose-400 border border-rose-500/20"
                    >
                      {tool}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        </div>

        <!-- Diagnostics -->
        {#if capabilities.diagnostics.length > 0}
          <div>
            <h3 class="font-semibold text-foreground mb-2">诊断信息 (Diagnostics)</h3>
            <div class="space-y-1.5">
              {#each capabilities.diagnostics as diag}
                <div
                  class="rounded-lg bg-amber-500/10 p-2.5 text-[11px] text-amber-800 dark:text-amber-200 border border-amber-500/20"
                >
                  {diag}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {:else}
        <div class="text-center py-12 text-muted-foreground">暂无此 Run 的生效能力记录</div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="pt-4 border-t border-border flex justify-end">
      <button
        type="button"
        class="rounded-lg bg-muted px-4 py-1.5 text-xs font-medium text-foreground hover:bg-muted/80 transition-colors"
        onclick={onClose}
      >
        关闭
      </button>
    </div>
  </div>
{/if}
