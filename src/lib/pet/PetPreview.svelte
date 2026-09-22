<script lang="ts">
  import PetCanvas from "./PetCanvas.svelte";
  import GrokBotPet from "./GrokBotPet.svelte";
  import { loadPetManifest, loadSpriteSheet, type SpriteSheet } from "./pet-sprites";
  import type { PetAggregateState, PetMode } from "./types";

  interface Props {
    petId?: string;
    mode?: PetMode;
    size?: "default" | "thumbnail";
  }

  let { petId = "agentcabin-bot", mode = "idle", size = "default" }: Props = $props();
  let sprite = $state<SpriteSheet | null>(null);
  let previewLabel = $state("AgentCabin Bot");

  const previewState: PetAggregateState = {
    mode: "idle",
    runningCount: 0,
    errorCount: 0,
    activeRunId: null,
    timestamp: Date.now(),
  };

  $effect(() => {
    previewState.mode = mode;
  });

  $effect(() => {
    const requestedPetId = petId;
    let disposed = false;
    sprite = null;

    if (requestedPetId === "grokbot") {
      previewLabel = "GrokBot";
      return;
    }

    void (async () => {
      try {
        const manifest = await loadPetManifest(requestedPetId);
        const loaded = await loadSpriteSheet(manifest);
        if (!disposed) {
          previewLabel = manifest.displayName;
          sprite = loaded;
        }
      } catch (error) {
        console.warn("[pet-preview] Failed to load built-in pet preview:", error);
      }
    })();

    return () => {
      disposed = true;
    };
  });
</script>

<div
  class="relative flex items-center justify-center overflow-hidden rounded-xl border border-border/70 bg-background/60 {size ===
  'thumbnail'
    ? 'h-16 w-16 rounded-lg'
    : 'h-36 w-32'}"
  aria-label={`${previewLabel} preview`}
>
  {#if petId === "grokbot"}
    <GrokBotPet petState={{ ...previewState, mode }} />
  {:else}
    <PetCanvas {sprite} state={{ ...previewState, mode }} active={true} />
  {/if}
</div>
