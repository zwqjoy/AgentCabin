<script lang="ts">
  import { getTransport } from "$lib/transport";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { fmtRelative } from "$lib/i18n/format";
  import { cwdDisplayLabel } from "$lib/utils/format";
  import type { CliSessionSummary, DiscoverResult } from "$lib/types";

  function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return getTransport().invoke<T>(cmd, args);
  }

  let {
    visible = false,
    cwd = "",
    onclose,
    onselectsession,
  }: {
    visible?: boolean;
    cwd?: string;
    onclose: () => void;
    onselectsession: (sessionId: string) => void;
  } = $props();

  let sessions = $state<CliSessionSummary[]>([]);
  let loading = $state(true);
  let searchQuery = $state("");
  let error = $state<string | null>(null);

  $effect(() => {
    if (visible) {
      loadSessions();
    }
  });

  async function loadSessions() {
    loading = true;
    error = null;
    try {
      dbg("pi-session-browser", "discover", { cwd });
      const result = await invoke<DiscoverResult>("discover_cli_sessions", {
        agent: "pi",
        projectCwd: cwd || null,
      });
      sessions = result.sessions;
    } catch (e) {
      dbgWarn("pi-session-browser", "discover failed", e);
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const filteredSessions = $derived(
    sessions.filter((s) => {
      if (!searchQuery.trim()) return true;
      const q = searchQuery.trim().toLowerCase();
      return (
        s.sessionId.toLowerCase().includes(q) ||
        s.firstPrompt.toLowerCase().includes(q) ||
        (s.cwd && s.cwd.toLowerCase().includes(q)) ||
        (s.model && s.model.toLowerCase().includes(q))
      );
    }),
  );

  function handleSelect(sessionId: string) {
    onselectsession(sessionId);
    onclose();
  }
</script>

{#if visible}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-fade-in"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
    }}
    role="button"
    tabindex="0"
  >
    <div
      class="flex h-[80vh] max-h-[640px] w-full max-w-2xl flex-col rounded-xl border border-border bg-background shadow-2xl overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-border px-5 py-4">
        <div>
          <h2 class="text-base font-semibold text-foreground">Pi Session Browser</h2>
          <p class="text-xs text-muted-foreground mt-0.5">
            Browse and switch active Pi session CLI files
          </p>
        </div>
        <button
          class="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          onclick={onclose}
        >
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M18 6 6 18" /><path d="m6 6 12 12" />
          </svg>
        </button>
      </div>

      <!-- Search Bar -->
      <div class="border-b border-border/60 px-5 py-3 bg-muted/20">
        <div class="relative">
          <svg
            class="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
          </svg>
          <input
            type="text"
            placeholder="Search sessions by prompt, model, or ID..."
            bind:value={searchQuery}
            class="w-full rounded-lg border border-border bg-background pl-9 pr-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto p-5">
        {#if loading}
          <div class="flex items-center justify-center py-16">
            <div
              class="h-5 w-5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
            ></div>
            <span class="ml-2 text-xs text-muted-foreground">Discovering Pi sessions...</span>
          </div>
        {:else if error}
          <div
            class="rounded-lg border border-destructive/30 bg-destructive/10 p-4 text-xs text-destructive"
          >
            {error}
          </div>
        {:else if filteredSessions.length === 0}
          <div
            class="flex flex-col items-center justify-center py-16 text-center text-muted-foreground"
          >
            <svg
              class="h-8 w-8 mb-2 opacity-50"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
            >
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
            <p class="text-xs">No Pi sessions found</p>
          </div>
        {:else}
          <div class="space-y-2">
            {#each filteredSessions as s (s.sessionId)}
              <div
                class="flex items-start justify-between gap-3 rounded-lg border border-border/60 bg-muted/20 p-3.5 hover:bg-accent/40 hover:border-border transition-all"
              >
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <span
                      class="font-mono text-xs font-semibold text-foreground truncate max-w-[200px]"
                    >
                      {s.sessionId}
                    </span>
                    {#if s.model}
                      <span
                        class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-mono text-primary"
                      >
                        {s.model}
                      </span>
                    {/if}
                    {#if s.messageCount}
                      <span class="text-[10px] text-muted-foreground">
                        {s.messageCount} msg
                      </span>
                    {/if}
                  </div>
                  <p class="text-xs text-foreground/80 line-clamp-2 mb-1.5">
                    {s.firstPrompt || "(Empty prompt)"}
                  </p>
                  <div class="flex items-center gap-3 text-[10px] text-muted-foreground">
                    {#if s.cwd}
                      <span class="truncate max-w-[180px]" title={s.cwd}>
                        📁 {cwdDisplayLabel(s.cwd)}
                      </span>
                    {/if}
                    {#if s.lastActivityAt || s.startedAt}
                      <span>🕒 {fmtRelative(s.lastActivityAt || s.startedAt)}</span>
                    {/if}
                  </div>
                </div>

                <button
                  class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shrink-0"
                  onclick={() => handleSelect(s.sessionId)}
                >
                  Switch Session
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
