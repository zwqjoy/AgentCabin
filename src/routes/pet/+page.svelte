<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { getTransport } from "$lib/transport";
  import PetCanvas from "$lib/pet/PetCanvas.svelte";
  import GrokBotPet from "$lib/pet/GrokBotPet.svelte";
  import { attachWindowDrag } from "$lib/pet/pet-interaction";
  import { loadPetManifest, loadSpriteSheet, type SpriteSheet } from "$lib/pet/pet-sprites";
  import { PetStateMachine } from "$lib/pet/pet-state";
  import {
    PET_EVENTS,
    type PetAggregateState,
    type PetNotification,
    type PetSettingsPayload,
  } from "$lib/pet/types";

  let containerEl = $state<HTMLDivElement | null>(null);
  let sprite = $state<SpriteSheet | null>(null);
  let dragging = $state(false);
  let petEnabled = $state(false);
  let petId = $state("agentcabin-bot");
  let grokBotActivation = $state(0);
  let clickInteractionEnabled = $state(true);
  let grokbotColor = $state<import("$lib/pet/grokbot-data").ColorId>("blue");
  let grokbotShape = $state<import("$lib/pet/grokbot-data").ShapeId>("blob");
  let grokbotParts = $state<import("$lib/pet/grokbot-data").GrokBotPartId[]>([]);
  let grokbotAccessories = $state<import("$lib/pet/grokbot-data").GrokBotAccessoryId[]>([]);
  let notification = $state<PetNotification | null>(null);
  let currentState = $state<PetAggregateState>({
    mode: "idle",
    runningCount: 0,
    errorCount: 0,
    activeRunId: null,
    timestamp: Date.now(),
  });

  const stateMachine = new PetStateMachine((nextState) => {
    currentState = nextState;
  });

  let unlistens: Array<() => void> = [];
  let detachDrag: (() => void) | null = null;

  let spriteRequest = 0;

  async function loadSprites(id = petId) {
    const request = ++spriteRequest;
    try {
      const nextManifest = await loadPetManifest(id);
      const nextSprite = await loadSpriteSheet(nextManifest);
      if (request !== spriteRequest) return;
      sprite = nextSprite;
    } catch (error) {
      // The Canvas renderer deliberately has a geometric fallback so a missing
      // packaged asset cannot prevent the pet window from opening.
      console.warn("[pet/+page] Spritesheet load failed, using canvas fallback:", error);
      if (request === spriteRequest) {
        sprite = null;
      }
    }
  }

  onMount(() => {
    let mounted = true;

    // Force transparent background on html/body via inline styles.
    // app.css sets body { bg-background } which would otherwise make the
    // pet window opaque. Inline styles beat any stylesheet rule.
    document.documentElement.classList.add("pet-transparent");
    document.documentElement.style.setProperty("background", "transparent", "important");
    document.body.style.setProperty("background", "transparent", "important");
    document.body.style.setProperty("background-color", "transparent", "important");

    const setup = async () => {
      const localUnlistens: Array<() => void> = [];
      try {
        const transport = getTransport();
        // Register listeners before loading assets and before READY. The Rust
        // side sends the current snapshot in response to READY, so a newly
        // created window does not depend on a prior event staying buffered.
        const unlistenState = await transport.listen<PetAggregateState>(
          PET_EVENTS.STATE,
          (payload) => {
            if (mounted) stateMachine.update(payload);
          },
        );
        localUnlistens.push(unlistenState);
        const unlistenNotification = await transport.listen<PetNotification>(
          PET_EVENTS.NOTIFICATION,
          (payload) => {
            if (mounted) notification = payload;
          },
        );
        localUnlistens.push(unlistenNotification);
        const unlistenSettings = await transport.listen<PetSettingsPayload>(
          PET_EVENTS.SETTINGS,
          (payload) => {
            if (!mounted) return;
            petEnabled = payload.petEnabled;
            clickInteractionEnabled = payload.petClickInteractionEnabled;
            grokbotColor = payload.petGrokbotColor as typeof grokbotColor;
            grokbotShape = payload.petGrokbotShape as typeof grokbotShape;
            grokbotParts = payload.petGrokbotParts as typeof grokbotParts;
            grokbotAccessories = payload.petGrokbotAccessories as typeof grokbotAccessories;
            if (payload.petId !== petId) {
              petId = payload.petId;
              void loadSprites(payload.petId);
            }
          },
        );
        localUnlistens.push(unlistenSettings);

        if (!mounted) {
          for (const unlisten of localUnlistens) unlisten();
          return;
        }
        unlistens.push(...localUnlistens);
        localUnlistens.length = 0;

        try {
          const s = await transport.invoke<Record<string, unknown>>("get_user_settings");
          if (s && mounted) {
            petEnabled = (s.pet_enabled as boolean) ?? petEnabled;
            clickInteractionEnabled =
              (s.pet_click_interaction_enabled as boolean) ?? clickInteractionEnabled;
            grokbotColor = (s.pet_grokbot_color as typeof grokbotColor) ?? grokbotColor;
            grokbotShape = (s.pet_grokbot_shape as typeof grokbotShape) ?? grokbotShape;
            grokbotParts = (s.pet_grokbot_parts as typeof grokbotParts) ?? grokbotParts;
            grokbotAccessories =
              (s.pet_grokbot_accessories as typeof grokbotAccessories) ?? grokbotAccessories;
            if (s.pet_id && s.pet_id !== petId) {
              petId = s.pet_id as string;
              void loadSprites(petId);
            }
          }
        } catch (e) {
          console.warn("[pet/+page] Initial get_user_settings failed:", e);
        }

        await transport.emit(PET_EVENTS.READY, {});
      } catch (error) {
        for (const unlisten of localUnlistens) unlisten();
        console.warn("[pet/+page] Event listen setup failed:", error);
      }

      if (mounted) void loadSprites();
    };

    if (containerEl) {
      detachDrag = attachWindowDrag(containerEl, {
        onDragStart: () => (dragging = true),
        onDragEnd: () => (dragging = false),
        onTap: () => {
          if (petId === "grokbot" && clickInteractionEnabled) grokBotActivation += 1;
        },
      });
    }

    void setup();

    return () => {
      mounted = false;
    };
  });

  onDestroy(() => {
    stateMachine.destroy();
    detachDrag?.();
    detachDrag = null;
    for (const unlisten of unlistens) unlisten();
    unlistens = [];
  });
</script>

<svelte:head>
  <title>AgentCabin Pet</title>
  <style>
    :global(html),
    :global(body),
    :global(body > div) {
      width: 100vw;
      height: 100vh;
      margin: 0;
      padding: 0;
      overflow: hidden;
      background: transparent !important;
      user-select: none;
      -webkit-user-select: none;
    }
  </style>
</svelte:head>

<div
  bind:this={containerEl}
  class="relative block h-full w-full cursor-grab overflow-hidden select-none active:cursor-grabbing"
  style="background: transparent; touch-action: none;"
>
  {#if petId === "grokbot"}
    <GrokBotPet
      petState={currentState}
      {dragging}
      activationNonce={grokBotActivation}
      {clickInteractionEnabled}
      color={grokbotColor}
      shape={grokbotShape}
      parts={grokbotParts}
      accessories={grokbotAccessories}
    />
  {:else}
    <PetCanvas {sprite} state={currentState} {dragging} {notification} active={true} />
  {/if}
</div>
