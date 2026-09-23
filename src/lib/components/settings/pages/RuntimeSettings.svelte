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
    piContent?: Snippet;
  }

  let {
    settings,
    activeSubTab = "pi",
    onSubTabChange = () => {},
    onUpdateSettings,
    codexContent,
    claudeContent,
    grokContent,
    piContent,
  }: Props = $props();

  // Only Pi Agent is shown; Codex, Claude Code, Grok remain hidden behind official clients.
  const SUB_TABS: Array<{ id: RuntimeSubTab; label: string; dotClass: string }> = [
    // { id: "codex", label: "Codex", dotClass: "bg-emerald-500" },
    // { id: "claude", label: "Claude Code", dotClass: "bg-amber-500" },
    // { id: "grok", label: "Grok", dotClass: "bg-violet-500" },
    { id: "pi", label: "Pi Agent", dotClass: "bg-purple-500" },
  ];
</script>

<div class="space-y-6">
  <!-- Agent Settings Header & Sub-tabs -->
  <div class="space-y-4">
    <div>
      <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        Agent 运行时
      </h2>
      <p class="mt-1 text-xs text-muted-foreground">
        管理 Pi Agent 运行时的认证、模型、沙箱及 CLI 配置。
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
    {:else if activeSubTab === "pi"}
      {#if piContent}
        {@render piContent()}
      {/if}
    {/if}
  </div>
</div>
