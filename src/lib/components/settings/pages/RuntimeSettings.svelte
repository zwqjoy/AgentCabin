<script lang="ts">
  import type { Snippet } from "svelte";
  import type { UserSettings } from "$lib/types";
  import type { RuntimeSubTab } from "$lib/utils/settings-routing";

  interface Props {
    settings: UserSettings;
    activeSubTab?: RuntimeSubTab;
    onSubTabChange?: (tab: RuntimeSubTab) => void;
    onUpdateSettings?: (patch: Partial<UserSettings>) => Promise<void>;
    codexContent?: Snippet;
    claudeContent?: Snippet;
    grokContent?: Snippet;
    dshContent?: Snippet;
    piContent?: Snippet;
  }

  let {
    settings,
    activeSubTab = "dsh",
    onSubTabChange = () => {},
    onUpdateSettings,
    codexContent,
    claudeContent,
    grokContent,
    dshContent,
    piContent,
  }: Props = $props();

  // Codex、Claude Code、Grok 暂时隐藏（官方客户端已足够），保留 DSH 和 Pi Agent
  const SUB_TABS: Array<{ id: RuntimeSubTab; label: string; dotClass: string }> = [
    // { id: "codex", label: "Codex", dotClass: "bg-emerald-500" },
    // { id: "claude", label: "Claude Code", dotClass: "bg-amber-500" },
    // { id: "grok", label: "Grok", dotClass: "bg-violet-500" },
    { id: "dsh", label: "DeepSeek (DSH)", dotClass: "bg-cyan-500" },
    { id: "pi", label: "Pi Agent", dotClass: "bg-purple-500" },
  ];
</script>

<div class="space-y-6">
  <!-- Runtime Providers Header & Sub-tabs -->
  <div class="space-y-4">
    <div>
      <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        Runtime 运行时
      </h2>
      <p class="mt-1 text-xs text-muted-foreground">
        管理原生 Agent 运行时与独立执行环境，配置 DeepSeek (DSH) 与 Pi Agent 的认证、模型、沙箱及
        CLI。
      </p>
    </div>

    <!-- Agent Switcher Sub-tabs (macOS segmented control style) -->
    <div
      class="flex items-center gap-1.5 overflow-x-auto rounded-xl border border-border/70 bg-muted/30 p-1"
    >
      {#each SUB_TABS as tab (tab.id)}
        <button
          type="button"
          class="flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs font-medium transition-all {activeSubTab ===
          tab.id
            ? 'bg-card text-foreground shadow-xs font-semibold ring-1 ring-border/50'
            : 'text-muted-foreground hover:bg-card/50 hover:text-foreground'}"
          onclick={() => onSubTabChange(tab.id)}
        >
          <span class="h-2 w-2 rounded-full {tab.dotClass} shrink-0"></span>
          <span>{tab.label}</span>
        </button>
      {/each}
    </div>
  </div>

  <!-- Tab Contents -->
  <div class="space-y-6">
    {#if activeSubTab === "codex"}
      {#if codexContent}
        {@render codexContent()}
      {/if}
    {:else if activeSubTab === "claude"}
      {#if claudeContent}
        {@render claudeContent()}
      {/if}
    {:else if activeSubTab === "grok"}
      {#if grokContent}
        {@render grokContent()}
      {/if}
    {:else if activeSubTab === "dsh"}
      {#if dshContent}
        {@render dshContent()}
      {/if}
    {:else if activeSubTab === "pi"}
      {#if piContent}
        {@render piContent()}
      {/if}
    {/if}
  </div>
</div>
