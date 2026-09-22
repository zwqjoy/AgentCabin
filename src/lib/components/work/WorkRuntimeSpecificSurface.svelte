<script lang="ts">
  import type { SessionStore } from "$lib/stores/session-store.svelte";
  import WorkPiExtensionSurfaces from "$lib/components/work/WorkPiExtensionSurfaces.svelte";

  interface Props {
    session: SessionStore;
    provider?: string;
  }

  let { session, provider = "unknown" }: Props = $props();
</script>

{#if provider === "pi"}
  <WorkPiExtensionSurfaces hostState={session.piExtensionHostState} notice={session.latestNotice} />
{/if}
