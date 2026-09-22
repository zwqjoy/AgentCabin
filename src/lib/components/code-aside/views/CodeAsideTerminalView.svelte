<script lang="ts">
  import { onDestroy } from "svelte";
  import XTerminal from "$lib/components/XTerminal.svelte";
  import * as api from "$lib/api";
  import { getTransport } from "$lib/transport";

  interface Props {
    cwd?: string;
    sessionId?: string;
  }

  let { cwd = "/", sessionId }: Props = $props();

  const effectiveSessionId = sessionId || "aside-term-" + Math.random().toString(36).slice(2, 8);
  let termRef = $state<XTerminal | undefined>();
  let unlistenData: (() => void) | null = null;
  let unlistenExit: (() => void) | null = null;

  async function handleReady(cols: number, rows: number) {
    if (!termRef) return;
    termRef.clear();

    const transport = getTransport();
    unlistenData = await transport.listen<api.PtyDataEvent>("pty_data", (event) => {
      if (event.session_id === effectiveSessionId) {
        termRef?.writeText(event.data);
      }
    });

    unlistenExit = await transport.listen<api.PtyExitEvent>("pty_exit", (event) => {
      if (event.session_id === effectiveSessionId) {
        termRef?.writeText("\r\n\x1b[90m[进程已退出]\x1b[0m\r\n");
      }
    });

    try {
      await api.ptyCreate(effectiveSessionId, cwd, cols, rows);
    } catch (err) {
      termRef?.writeText(`\x1b[31m无法启动终端: ${err}\x1b[0m\r\n`);
    }
  }

  async function handleData(data: string) {
    await api.ptyWrite(effectiveSessionId, data);
  }

  async function handleResize(cols: number, rows: number) {
    await api.ptyResize(effectiveSessionId, cols, rows);
  }

  onDestroy(() => {
    unlistenData?.();
    unlistenExit?.();
    void api.ptyKill(effectiveSessionId).catch(() => {});
  });
</script>

<div class="h-full w-full bg-[#09090b] p-2 overflow-hidden">
  <XTerminal
    bind:this={termRef}
    onReady={handleReady}
    onData={handleData}
    onResize={handleResize}
    class="h-full w-full"
  />
</div>
