<script lang="ts">
  import { onMount } from "svelte";
  import Card from "$lib/components/Card.svelte";
  import {
    listDshPlugins,
    registerDshPlugin,
    toggleDshPlugin,
    unregisterDshPlugin,
    updateDshPlugin,
    type DshPluginManifest,
  } from "$lib/dsh-api";

  let plugins = $state<DshPluginManifest[]>([]);
  let loading = $state(true);
  let busyId = $state("");
  let error = $state("");
  let notice = $state("");
  let packageId = $state("");
  let packageReference = $state("");

  const builtinIds = new Set(["dsh-skill", "dsh-web", "dsh-mcp-client"]);

  async function load() {
    loading = true;
    error = "";
    try {
      plugins = await listDshPlugins();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });

  async function toggle(plugin: DshPluginManifest) {
    if (busyId) return;
    busyId = plugin.id;
    error = "";
    try {
      const updated = await toggleDshPlugin(plugin.id, !plugin.enabled);
      plugins = plugins.map((item) => (item.id === updated.id ? updated : item));
      notice = `${updated.name} 已${updated.enabled ? "启用" : "停用"}；新会话生效。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function register() {
    const id = packageId.trim();
    const reference = packageReference.trim();
    const name = id;
    if (!id || !reference || busyId) return;
    busyId = "register";
    error = "";
    notice = "";
    try {
      const item = await registerDshPlugin({
        id,
        name,
        ...(reference.startsWith("/") ? { path: reference } : { package: reference }),
        security: "codeOnly",
        enabled: true,
      });
      plugins = [...plugins, item];
      packageId = "";
      packageReference = "";
      notice = `${item.name} 已安装为 Code-only 插件；Work 会自动隔离。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function update(plugin: DshPluginManifest) {
    if (busyId || builtinIds.has(plugin.id)) return;
    busyId = `update:${plugin.id}`;
    error = "";
    try {
      const updated = await updateDshPlugin(plugin.id);
      plugins = plugins.map((item) => (item.id === updated.id ? updated : item));
      notice = `${plugin.name} 已更新${updated.version ? `至 v${updated.version}` : ""}。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function remove(plugin: DshPluginManifest) {
    if (busyId || builtinIds.has(plugin.id)) return;
    busyId = `remove:${plugin.id}`;
    error = "";
    try {
      await unregisterDshPlugin(plugin.id);
      plugins = plugins.filter((item) => item.id !== plugin.id);
      notice = `${plugin.name} 已卸载并移除。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }
</script>

<Card class="space-y-4 p-6">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      DSH Plugin 生态
    </h2>
    <p class="mt-1 text-xs leading-5 text-muted-foreground">
      DSH 插件按 Runtime 隔离。第三方插件默认只允许 Code；Work 工具必须经过 AgentCabin
      Bridge、Policy 与 Approval。
    </p>
  </div>

  {#if loading}
    <div class="text-xs text-muted-foreground">正在加载插件清单…</div>
  {:else}
    <div class="space-y-2">
      {#each plugins as plugin (plugin.id)}
        <div
          class="flex items-center gap-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5"
        >
          <span
            class="h-2 w-2 shrink-0 rounded-full {plugin.enabled
              ? 'bg-emerald-500'
              : 'bg-muted-foreground/40'}"
          ></span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="truncate text-xs font-medium text-foreground">{plugin.name}</span>
              <span
                class="rounded bg-blue-500/10 px-1.5 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400"
              >
                {plugin.security === "safeInWork" ? "Safe in Work" : "Code only"}
              </span>
            </div>
            <p class="truncate font-mono text-[10px] text-muted-foreground">
              {plugin.package ?? plugin.path ?? plugin.id}
            </p>
          </div>
          <button
            type="button"
            class="rounded-md border border-border/70 px-2.5 py-1 text-[11px] text-foreground hover:bg-accent disabled:opacity-50"
            disabled={!!busyId}
            onclick={() => void toggle(plugin)}
          >
            {plugin.enabled ? "停用" : "启用"}
          </button>
          {#if !builtinIds.has(plugin.id)}
            {#if plugin.package}
              <button
                type="button"
                class="rounded-md border border-border/70 px-2.5 py-1 text-[11px] text-foreground hover:bg-accent disabled:opacity-50"
                disabled={!!busyId}
                onclick={() => void update(plugin)}
              >
                更新
              </button>
            {/if}
            <button
              type="button"
              class="rounded-md px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50"
              disabled={!!busyId}
              onclick={() => void remove(plugin)}
            >
              移除
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="border-t border-border/50 pt-3">
    <div class="mb-2 text-xs font-medium text-foreground">安装第三方插件（Code-only）</div>
    <div class="grid gap-2 sm:grid-cols-[12rem_1fr_auto]">
      <input
        bind:value={packageId}
        placeholder="插件 ID"
        class="rounded-md border bg-transparent px-2.5 py-1.5 text-xs outline-none focus:ring-1 focus:ring-primary"
      />
      <input
        bind:value={packageReference}
        placeholder="npm 包名（如 is-sorted）或绝对路径"
        class="rounded-md border bg-transparent px-2.5 py-1.5 text-xs outline-none focus:ring-1 focus:ring-primary"
      />
      <button
        type="button"
        class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground disabled:opacity-50"
        disabled={!packageId.trim() || !packageReference.trim() || !!busyId}
        onclick={() => void register()}
      >
        安装
      </button>
    </div>
  </div>

  {#if notice}
    <p class="text-xs text-emerald-600 dark:text-emerald-400">{notice}</p>
  {/if}
  {#if error}
    <p class="text-xs text-destructive" role="alert">{error}</p>
  {/if}
</Card>
