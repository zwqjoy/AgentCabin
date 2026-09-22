<!--
  GrokBotPet.svelte
  Renders the LaoA-GrokBot SVG character and connects it to AgentCabin's PetMode state.
  Source engine: https://github.com/zhulin025/LaoA-GrokBot (MIT License)
  Original author: 老A玩AI (zhulin025)
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { GrokBotEngine } from "./GrokBotEngine";
  import {
    COLORS,
    SHAPES,
    type ColorId,
    type GrokBotAccessoryId,
    type GrokBotPartId,
    type GrokBotQuickAction,
    type GrokBotStateKey,
    type ShapeId,
  } from "./grokbot-data";
  import type { PetAggregateState } from "./types";

  interface Props {
    // Avoid the `state` prop name here: Svelte's `$state` rune treats a local
    // `state` binding as a legacy store subscription and breaks type checking.
    petState: PetAggregateState;
    dragging?: boolean;
    /** Body shape. Default: 'blob' */
    shape?: ShapeId;
    /** Body color id. Default: 'blue' */
    color?: ColorId;
    /** Increment to trigger one desktop-pet activation after a pointer tap. */
    activationNonce?: number;
    /** Whether pointer/keyboard activation is enabled. */
    clickInteractionEnabled?: boolean;
    parts?: GrokBotPartId[];
    accessories?: GrokBotAccessoryId[];
    /** Optional manual state used by the settings customizer preview. */
    stateOverride?: GrokBotStateKey;
    /** Optional explicit quick action used by the settings customizer. */
    quickAction?: GrokBotQuickAction;
    /** Increment to trigger the explicit quick action once. */
    quickActionNonce?: number;
  }

  let {
    petState,
    dragging = false,
    shape = "blob",
    color = "blue",
    activationNonce = 0,
    clickInteractionEnabled = true,
    parts = [],
    accessories = [],
    stateOverride,
    quickAction,
    quickActionNonce = 0,
  }: Props = $props();

  // ── Resolved shape path & color ──────────────────────────────────────────
  const shapeDef = $derived(SHAPES.find((s) => s.id === shape) ?? SHAPES[0]);
  const colorDef = $derived(COLORS.find((c) => c.id === color) ?? COLORS[6]);

  // ── Engine + refs ─────────────────────────────────────────────────────────
  let containerEl = $state<HTMLDivElement | null>(null);
  let eye0El = $state<SVGPathElement | null>(null);
  let eye1El = $state<SVGPathElement | null>(null);
  let bodyPathEl = $state<SVGPathElement | null>(null);
  let clipPathEl = $state<SVGPathElement | null>(null);

  let engine: GrokBotEngine | null = null;

  // ── Build SVG path string from a ring of [x,y] points ────────────────────
  function toPath(ring: number[][]): string {
    return "M" + ring.map((p) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join("L") + "Z";
  }

  // ── Engine frame callback ─────────────────────────────────────────────────
  function handleFrame(frame: { rings: number[][][]; blinkScale: number }) {
    if (!eye0El || !eye1El) return;
    eye0El.setAttribute("d", toPath(frame.rings[0]));
    eye1El.setAttribute("d", toPath(frame.rings[1]));
  }

  // ── Gaze: track pointer over container ───────────────────────────────────
  function handlePointerMove(e: PointerEvent) {
    if (!containerEl || dragging) return;
    const box = containerEl.getBoundingClientRect();
    const nx = ((e.clientX - box.left) / box.width) * 2 - 1;
    const ny = ((e.clientY - box.top) / box.height) * 2 - 1;
    engine?.setGaze(nx, ny);
  }

  function handlePointerLeave() {
    engine?.clearGaze();
  }

  // ── State sync ─────────────────────────────────────────────────────────────
  let prevMode: PetAggregateState["mode"] | undefined;
  let prevStateOverride: GrokBotStateKey | undefined;

  $effect(() => {
    const override = stateOverride;
    const mode = petState.mode;
    if (override !== prevStateOverride) {
      prevStateOverride = override;
      if (override) engine?.setState(override);
      else engine?.setPetMode(mode);
    }
    if (!override && mode !== prevMode) {
      prevMode = mode;
      engine?.setPetMode(mode);
    }
  });

  function activate() {
    if (dragging || !clickInteractionEnabled) return;
    engine?.triggerRandomAction();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    activate();
  }

  let previousActivationNonce = 0;
  $effect(() => {
    const nonce = activationNonce;
    if (nonce === previousActivationNonce) return;
    previousActivationNonce = nonce;
    activate();
  });

  let previousQuickActionNonce = 0;
  $effect(() => {
    const nonce = quickActionNonce;
    const action = quickAction;
    if (nonce === previousQuickActionNonce) return;
    previousQuickActionNonce = nonce;
    if (action) engine?.triggerAction(action);
  });

  // ── Dragging: pause gaze while window is being dragged ────────────────────
  $effect(() => {
    if (dragging) engine?.clearGaze();
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────────
  onMount(() => {
    engine = new GrokBotEngine(handleFrame, containerEl ?? undefined);
    if (stateOverride) engine.setState(stateOverride);
    else engine.setPetMode(petState.mode);
  });

  onDestroy(() => {
    engine?.destroy();
    engine = null;
  });
  const clipId = "grokbot-clip-" + Math.random().toString(36).slice(2, 9);
</script>

<div
  bind:this={containerEl}
  class="grokbot-wrap"
  style="--bot-color: {colorDef.hex};"
  onpointermove={handlePointerMove}
  onpointerleave={handlePointerLeave}
  onkeydown={handleKeydown}
  role="button"
  tabindex="0"
>
  <svg
    class="grokbot-svg"
    viewBox="-28 -28 285 285"
    role="img"
    aria-label="GrokBot desktop pet"
    xmlns="http://www.w3.org/2000/svg"
  >
    <defs>
      <clipPath id={clipId}>
        <path bind:this={clipPathEl} d={shapeDef.path} />
      </clipPath>
    </defs>

    <!-- Body -->
    {#if accessories.includes("cape")}
      <g class="grokbot-cape" aria-hidden="true">
        <path d="M25 79Q-2 119 13 210Q65 192 90 168Z" />
        <path d="M204 79Q231 119 216 210Q164 192 139 168Z" />
      </g>
    {/if}
    {#if parts.includes("antenna")}
      <g class="grokbot-part grokbot-part-stroke" aria-hidden="true">
        <path d="M114 18V-5" />
        <circle cx="114" cy="-12" r="8" />
      </g>
    {/if}
    {#if parts.includes("tail")}
      <g class="grokbot-part grokbot-part-stroke" aria-hidden="true">
        <path d="M205 154C246 151 254 181 230 198C216 208 214 220 227 228" />
      </g>
    {/if}
    {#if parts.includes("hands")}
      <g class="grokbot-part grokbot-part-stroke" aria-hidden="true">
        <path d="M25 132C5 136-8 148-17 165M204 132C224 136 237 148 246 165" />
        <circle cx="-20" cy="170" r="10" />
        <circle cx="249" cy="170" r="10" />
      </g>
    {/if}
    {#if parts.includes("feet")}
      <g class="grokbot-part grokbot-part-stroke" aria-hidden="true">
        <path d="M72 202V224M157 202V224" />
        <ellipse cx="62" cy="230" rx="24" ry="10" />
        <ellipse cx="167" cy="230" rx="24" ry="10" />
      </g>
    {/if}
    <g clip-path="url(#{clipId})">
      <!-- Body fill -->
      <path class="grokbot-body" d={shapeDef.path} bind:this={bodyPathEl} />
      <!-- Belly highlight -->
      <ellipse class="grokbot-belly" cx="114" cy="155" rx="68" ry="80" />
    </g>

    <!-- Body outline (outside clip) -->
    <path class="grokbot-body-outline" d={shapeDef.path} />

    <!-- Eyes (paths updated by engine each frame) -->
    <path class="grokbot-eye" bind:this={eye0El} d="" />
    <path class="grokbot-eye" bind:this={eye1El} d="" />

    {#if accessories.includes("straw-hat")}
      <g class="grokbot-straw-hat" aria-hidden="true">
        <path d="M63 28Q72-24 114-28Q156-24 165 28Z" />
        <ellipse cx="114" cy="31" rx="91" ry="18" />
        <path class="grokbot-hat-band" d="M64 10Q114 22 164 10L166 27Q114 38 62 27Z" />
      </g>
    {/if}
    {#if accessories.includes("glasses")}
      <g class="grokbot-glasses" aria-hidden="true">
        <circle cx="72" cy="108" r="37" />
        <circle cx="157" cy="108" r="37" />
        <path d="M109 106Q114 99 120 106M35 102L12 94M194 102L217 94" />
      </g>
    {/if}
    {#if accessories.includes("bowtie")}
      <g class="grokbot-bowtie" aria-hidden="true">
        <path d="M114 172L78 151Q62 143 64 176Q65 205 82 194L114 178Z" />
        <path d="M114 172L150 151Q166 143 164 176Q163 205 146 194L114 178Z" />
        <circle cx="114" cy="175" r="12" />
      </g>
    {/if}
  </svg>
</div>

<style>
  .grokbot-wrap {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
  }

  .grokbot-svg {
    width: 100%;
    height: 100%;
    overflow: visible;
    filter: drop-shadow(0 8px 16px rgba(0, 0, 0, 0.15));
    animation: grokbot-idle-float 4s ease-in-out infinite;
    transform-origin: center bottom;
    will-change: transform;
  }

  @keyframes grokbot-idle-float {
    0%,
    100% {
      transform: translateY(0) scale(1, 1);
    }
    50% {
      transform: translateY(-3px) scale(1.02, 0.98);
    }
  }

  /* ── Body ── */
  .grokbot-body {
    fill: var(--bot-color, #2f86ed);
  }

  .grokbot-belly {
    fill: rgba(255, 255, 255, 0.18);
  }

  .grokbot-body-outline {
    fill: none;
    stroke: rgba(0, 0, 0, 0.12);
    stroke-width: 2;
  }

  .grokbot-part {
    fill: var(--bot-color, #2f86ed);
  }

  .grokbot-part-stroke {
    stroke: var(--bot-color, #2f86ed);
    stroke-width: 6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .grokbot-cape {
    fill: #7657d8;
    opacity: 0.88;
  }

  .grokbot-straw-hat {
    fill: #efcb70;
    stroke: #b57b24;
    stroke-width: 3;
    stroke-linejoin: round;
  }

  .grokbot-hat-band {
    fill: #d94b5d;
    stroke: none;
  }

  .grokbot-glasses {
    fill: none;
    stroke: #171813;
    stroke-width: 8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .grokbot-bowtie {
    fill: #ff2d8b;
    stroke: none;
  }

  /* ── Eyes ── */
  .grokbot-eye {
    fill: #1a1a2e;
  }

  /* ─────────────────────────────────────────────────────────────────────────
     Quick jelly actions – matches GrokBotEngine.triggerAction() class names
  ───────────────────────────────────────────────────────────────────────── */

  :global(.grokbot-quick-bounce) .grokbot-svg {
    animation: grokbot-bounce 0.85s cubic-bezier(0.36, 0.07, 0.19, 0.97) both;
  }
  :global(.grokbot-quick-shake) .grokbot-svg {
    animation: grokbot-shake 0.82s cubic-bezier(0.36, 0.07, 0.19, 0.97) both;
  }
  :global(.grokbot-quick-peek) .grokbot-svg {
    animation: grokbot-peek 0.9s ease both;
  }
  :global(.grokbot-quick-pinch) .grokbot-svg {
    animation: grokbot-pinch 1.1s cubic-bezier(0.36, 0.07, 0.19, 0.97) both;
  }
  :global(.grokbot-quick-squish) .grokbot-svg {
    animation: grokbot-squish 0.9s ease both;
  }
  :global(.grokbot-quick-wave) .grokbot-svg {
    animation: grokbot-wave 1.3s ease both;
  }

  @keyframes grokbot-bounce {
    0% {
      transform: translateY(0) scaleY(1);
    }
    20% {
      transform: translateY(-22%) scaleY(1.08);
    }
    40% {
      transform: translateY(0) scaleY(0.92);
    }
    60% {
      transform: translateY(-10%) scaleY(1.04);
    }
    80% {
      transform: translateY(0) scaleY(0.97);
    }
    100% {
      transform: translateY(0) scaleY(1);
    }
  }

  @keyframes grokbot-shake {
    0%,
    100% {
      transform: rotate(0deg);
    }
    15% {
      transform: rotate(-12deg);
    }
    30% {
      transform: rotate(10deg);
    }
    45% {
      transform: rotate(-9deg);
    }
    60% {
      transform: rotate(7deg);
    }
    75% {
      transform: rotate(-4deg);
    }
    88% {
      transform: rotate(2deg);
    }
  }

  @keyframes grokbot-peek {
    0% {
      transform: translateY(0);
    }
    30% {
      transform: translateY(-14%);
    }
    60% {
      transform: translateY(-14%);
    }
    100% {
      transform: translateY(0);
    }
  }

  @keyframes grokbot-pinch {
    0% {
      transform: scaleX(1) scaleY(1);
    }
    20% {
      transform: scaleX(0.75) scaleY(1.18);
    }
    45% {
      transform: scaleX(1.15) scaleY(0.88);
    }
    65% {
      transform: scaleX(0.94) scaleY(1.06);
    }
    82% {
      transform: scaleX(1.03) scaleY(0.98);
    }
    100% {
      transform: scaleX(1) scaleY(1);
    }
  }

  @keyframes grokbot-squish {
    0% {
      transform: scaleY(1);
    }
    25% {
      transform: scaleY(0.82) scaleX(1.12);
    }
    55% {
      transform: scaleY(1.1) scaleX(0.94);
    }
    78% {
      transform: scaleY(0.97) scaleX(1.02);
    }
    100% {
      transform: scaleY(1);
    }
  }

  @keyframes grokbot-wave {
    0% {
      transform: rotate(0deg) translateX(0);
    }
    15% {
      transform: rotate(-8deg) translateX(-6%);
    }
    35% {
      transform: rotate(10deg) translateX(6%);
    }
    55% {
      transform: rotate(-6deg) translateX(-4%);
    }
    72% {
      transform: rotate(5deg) translateX(3%);
    }
    88% {
      transform: rotate(-2deg) translateX(-1%);
    }
    100% {
      transform: rotate(0deg) translateX(0);
    }
  }
</style>
