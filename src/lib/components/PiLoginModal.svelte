<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import * as api from "$lib/api";
  import { getTransport } from "$lib/transport";
  import { t } from "$lib/i18n/index.svelte";
  import XTerminal from "$lib/components/XTerminal.svelte";
  import type { PiAuthResult } from "$lib/types";

  let {
    cwd = "/",
    onClose,
    onStatusChange,
  }: {
    cwd?: string;
    onClose: () => void;
    onStatusChange?: (status: PiAuthResult) => void;
  } = $props();

  const sessionId = `pi-login-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  let terminalReady = $state(false);
  let starting = $state(true);
  let error = $state("");
  let authStatus = $state<PiAuthResult | null>(null);
  let terminalRef: XTerminal | undefined = $state();
  let unlistenData: (() => void) | undefined;
  let unlistenExit: (() => void) | undefined;
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let launchTimers: ReturnType<typeof setTimeout>[] = [];

  async function refreshAuthStatus() {
    try {
      const next = await api.checkPiAuth();
      authStatus = next;
      onStatusChange?.(next);
    } catch {
      // The terminal remains usable even when a status probe is unavailable.
    }
  }

  async function handleReady(cols: number, rows: number) {
    if (terminalReady) return;
    terminalReady = true;
    const transport = getTransport();

    unlistenData = await transport.listen<api.PtyDataEvent>("pty_data", (event) => {
      if (event.session_id !== sessionId) return;
      terminalRef?.writeText(event.data);
    });
    unlistenExit = await transport.listen<api.PtyExitEvent>("pty_exit", (event) => {
      if (event.session_id !== sessionId) return;
      starting = false;
    });

    try {
      await api.ptyCreate(sessionId, cwd || "/", cols, rows);
      starting = false;
      // Login does not need project sessions, skills, or extensions. Keeping
      // the temporary Pi process minimal avoids slow home-directory scans and
      // lets the OAuth command reach the TUI after startup has settled.
      launchTimers.push(
        setTimeout(() => {
          void api.ptyWrite(sessionId, "pi --no-session --no-extensions --no-skills\r");
        }, 180),
      );
      launchTimers.push(
        setTimeout(() => {
          void api.ptyWrite(sessionId, "/login openai-codex\n");
        }, 3600),
      );
      await refreshAuthStatus();
      pollTimer = setInterval(() => void refreshAuthStatus(), 1200);
    } catch (e) {
      starting = false;
      error = String(e);
    }
  }

  function handleData(data: string) {
    void api.ptyWrite(sessionId, data).catch((e) => {
      error = String(e);
    });
  }

  function handleResize(cols: number, rows: number) {
    void api.ptyResize(sessionId, cols, rows).catch(() => {});
  }

  async function closeTerminal() {
    if (pollTimer) clearInterval(pollTimer);
    for (const timer of launchTimers) clearTimeout(timer);
    launchTimers = [];
    unlistenData?.();
    unlistenExit?.();
    unlistenData = undefined;
    unlistenExit = undefined;
    await api.ptyKill(sessionId).catch(() => {});
    onClose();
  }

  onMount(() => {
    void refreshAuthStatus();
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    for (const timer of launchTimers) clearTimeout(timer);
    unlistenData?.();
    unlistenExit?.();
    void api.ptyKill(sessionId).catch(() => {});
  });
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 backdrop-blur-[2px]"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) void closeTerminal();
  }}
>
  <div
    class="flex w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-border bg-background shadow-2xl"
    role="dialog"
    aria-modal="true"
    aria-labelledby="pi-login-title"
    tabindex="-1"
  >
    <header class="flex items-start justify-between gap-4 border-b border-border px-5 py-4">
      <div>
        <h2 id="pi-login-title" class="text-base font-semibold">{t("settings_pi_loginTitle")}</h2>
        <p class="mt-1 text-xs text-muted-foreground">{t("settings_pi_loginDesc")}</p>
      </div>
      <button
        class="rounded-md p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        aria-label={t("common_close")}
        onclick={() => void closeTerminal()}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="h-4 w-4">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </header>

    <div class="h-[min(62vh,540px)] min-h-[300px] bg-[#0a0a0a]">
      <XTerminal
        bind:this={terminalRef}
        onReady={handleReady}
        onResize={handleResize}
        onData={handleData}
      />
    </div>

    <footer class="flex flex-wrap items-center gap-3 border-t border-border px-5 py-3">
      {#if error}
        <p class="min-w-0 flex-1 text-xs text-red-500">{error}</p>
      {:else if authStatus?.logged_in}
        <p class="flex min-w-0 flex-1 items-center gap-2 text-xs text-emerald-500">
          <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
          {t("settings_pi_loginSuccess")}
        </p>
      {:else}
        <p class="min-w-0 flex-1 text-xs text-muted-foreground">
          {#if starting}{t("settings_pi_loginStarting")}
          {:else}{t("settings_pi_loginHint")}{/if}
        </p>
      {/if}
      <button
        class="rounded-md border border-border px-3 py-1.5 text-xs font-medium transition-colors hover:bg-accent"
        onclick={() => void refreshAuthStatus()}
      >
        {t("settings_pi_loginRefresh")}
      </button>
      <button
        class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        onclick={() => void closeTerminal()}
      >
        {t("common_close")}
      </button>
    </footer>
  </div>
</div>
