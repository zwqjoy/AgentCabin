<script lang="ts">
  import GrokBotPet from "./GrokBotPet.svelte";
  import {
    COLORS,
    GROKBOT_ACCESSORIES,
    GROKBOT_PARTS,
    GROKBOT_STATE_GROUPS,
    GROKBOT_STATE_NAMES,
    POOLS,
    SHAPES,
    type GrokBotAccessoryId,
    type GrokBotPartId,
    type GrokBotQuickAction,
    type GrokBotStateKey,
  } from "./grokbot-data";
  import { renderGrokBotShareCard } from "./grokbot-share";
  import type { PetAggregateState } from "./types";
  import type { PetSettings } from "./pet-settings";

  interface Props {
    settings: PetSettings;
    onChange: (patch: Partial<PetSettings>) => void | Promise<void>;
  }

  const STATE_PAGE_SIZE = 6;
  const QUICK_ACTIONS: readonly GrokBotQuickAction[] = [
    "bounce",
    "shake",
    "peek",
    "pinch",
    "squish",
    "wave",
  ];
  const QUICK_ACTION_NAMES: Record<GrokBotQuickAction, string> = {
    bounce: "蹦跳",
    shake: "摇头",
    peek: "探头",
    pinch: "捏脸",
    squish: "挤压",
    wave: "挥手",
  };
  const previewPetState: PetAggregateState = {
    mode: "idle",
    runningCount: 0,
    errorCount: 0,
    activeRunId: null,
    timestamp: 0,
  };

  let { settings, onChange }: Props = $props();
  let activeGroupId = $state<(typeof GROKBOT_STATE_GROUPS)[number]["id"]>("lifecycle");
  let statePage = $state(0);
  let previewState = $state<GrokBotStateKey>("idle");
  let activationNonce = $state(0);
  let autoTour = $state(false);
  let tourIndex = $state(0);
  let lastQuickAction = $state<GrokBotQuickAction | null>(null);
  let shareMessage = $state("");

  const activeGroup = $derived(
    GROKBOT_STATE_GROUPS.find((group) => group.id === activeGroupId) ?? GROKBOT_STATE_GROUPS[0],
  );
  const statePageCount = $derived(
    Math.max(1, Math.ceil(activeGroup.states.length / STATE_PAGE_SIZE)),
  );
  const visibleStates = $derived(
    activeGroup.states.slice(statePage * STATE_PAGE_SIZE, (statePage + 1) * STATE_PAGE_SIZE),
  );
  const allStates = GROKBOT_STATE_GROUPS.flatMap((group) => group.states);

  $effect(() => {
    if (!autoTour) return;
    const timer = window.setInterval(() => {
      const next = allStates[tourIndex % allStates.length];
      tourIndex += 1;
      selectState(next);
      const action = QUICK_ACTIONS[tourIndex % QUICK_ACTIONS.length];
      triggerQuickAction(action);
    }, 3600);
    return () => window.clearInterval(timer);
  });

  function choose<T>(items: readonly T[]): T {
    return items[Math.floor(Math.random() * items.length)];
  }

  function persist(patch: Partial<PetSettings>) {
    shareMessage = "";
    void onChange(patch);
  }

  function selectGroup(id: (typeof GROKBOT_STATE_GROUPS)[number]["id"]) {
    activeGroupId = id;
    statePage = 0;
  }

  function selectState(state: GrokBotStateKey) {
    previewState = state;
    const group = GROKBOT_STATE_GROUPS.find((candidate) =>
      (candidate.states as readonly GrokBotStateKey[]).includes(state),
    );
    if (group && group.id !== activeGroupId) {
      activeGroupId = group.id;
      statePage = 0;
    }
  }

  function togglePart(id: GrokBotPartId) {
    const next = settings.grokbotParts.includes(id)
      ? settings.grokbotParts.filter((item) => item !== id)
      : [...settings.grokbotParts, id];
    persist({ grokbotParts: next });
  }

  function toggleAccessory(id: GrokBotAccessoryId) {
    const next = settings.grokbotAccessories.includes(id)
      ? settings.grokbotAccessories.filter((item) => item !== id)
      : [...settings.grokbotAccessories, id];
    persist({ grokbotAccessories: next });
  }

  function triggerQuickAction(action: GrokBotQuickAction) {
    lastQuickAction = action;
    activationNonce += 1;
  }

  function randomize() {
    const parts = GROKBOT_PARTS.filter(() => Math.random() < 0.42).map((item) => item.id);
    const accessories = GROKBOT_ACCESSORIES.filter(() => Math.random() < 0.22).map(
      (item) => item.id,
    );
    const nextState = choose(allStates);
    selectState(nextState);
    triggerQuickAction(choose(QUICK_ACTIONS));
    persist({
      grokbotColor: choose(COLORS).id,
      grokbotShape: choose(SHAPES).id,
      grokbotParts: parts,
      grokbotAccessories: accessories,
    });
  }

  function downloadShareCard() {
    try {
      const dataUrl = renderGrokBotShareCard({
        color: settings.grokbotColor,
        shape: settings.grokbotShape,
        parts: settings.grokbotParts,
        accessories: settings.grokbotAccessories,
        state: previewState,
      });
      const link = document.createElement("a");
      link.href = dataUrl;
      link.download = `grokbot-${previewState}.png`;
      link.click();
      shareMessage = "PNG 分享卡已生成";
    } catch (error) {
      console.warn("[GrokBotCustomizer] share card failed", error);
      shareMessage = "当前环境暂不支持生成 PNG";
    }
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card p-6 shadow-sm">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        GrokBot 定制器
      </h2>
      <p class="mt-1 text-xs leading-relaxed text-muted-foreground">
        颜色、形态、部件、配饰和上游 39 种状态都可以即时预览。
      </p>
    </div>
    <div class="flex gap-2">
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-2 text-xs font-medium text-foreground transition-colors hover:bg-muted"
        onclick={randomize}
      >
        随机化
      </button>
      <button
        type="button"
        class={`rounded-lg border px-3 py-2 text-xs font-medium transition-colors ${
          autoTour
            ? "border-primary bg-primary text-primary-foreground"
            : "border-border text-foreground hover:bg-muted"
        }`}
        aria-pressed={autoTour}
        onclick={() => (autoTour = !autoTour)}
      >
        {autoTour ? "停止巡演" : "自动巡演"}
      </button>
    </div>
  </div>

  <div class="mt-5 grid gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
    <div
      class="flex min-h-56 items-center justify-center rounded-2xl border border-border/60 bg-muted/20 p-4"
    >
      <GrokBotPet
        petState={previewPetState}
        stateOverride={previewState}
        color={settings.grokbotColor}
        shape={settings.grokbotShape}
        parts={settings.grokbotParts}
        accessories={settings.grokbotAccessories}
        {activationNonce}
        quickAction={lastQuickAction ?? undefined}
        quickActionNonce={activationNonce}
        clickInteractionEnabled={false}
      />
    </div>
    <div class="min-w-0 space-y-4">
      <div class="rounded-xl border border-border/60 bg-background/50 p-3">
        <div class="flex items-center justify-between gap-3">
          <span class="text-xs text-muted-foreground">当前预览</span>
          <span class="font-mono text-xs text-foreground">
            表情 {String(POOLS[previewState][0] ?? 0).padStart(2, "0")} · {GROKBOT_STATE_NAMES[
              previewState
            ]}
          </span>
        </div>
        {#if lastQuickAction}
          <p class="mt-2 text-xs text-primary">最近动作：{QUICK_ACTION_NAMES[lastQuickAction]}</p>
        {/if}
      </div>
      <div>
        <div class="mb-2 flex items-center justify-between">
          <h3 class="text-xs font-semibold text-foreground">快捷动作</h3>
          <span class="text-[11px] text-muted-foreground">6 种果冻互动</span>
        </div>
        <div class="grid grid-cols-3 gap-2 sm:grid-cols-6">
          {#each QUICK_ACTIONS as action}
            <button
              type="button"
              class="rounded-lg border border-border/70 px-2 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
              onclick={() => triggerQuickAction(action)}
            >
              {QUICK_ACTION_NAMES[action]}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>

  <div class="mt-6 space-y-5">
    <div>
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-xs font-semibold text-foreground">颜色</h3>
        <span class="text-[11px] text-muted-foreground"
          >{COLORS.find((color) => color.id === settings.grokbotColor)?.name}</span
        >
      </div>
      <div class="grid grid-cols-5 gap-2 sm:grid-cols-10">
        {#each COLORS as color}
          <button
            type="button"
            class={`flex flex-col items-center gap-1 rounded-lg border p-2 transition-colors ${
              settings.grokbotColor === color.id
                ? "border-primary bg-primary/[0.06]"
                : "border-border/60 hover:border-primary/40"
            }`}
            aria-pressed={settings.grokbotColor === color.id}
            title={color.name}
            onclick={() => persist({ grokbotColor: color.id })}
          >
            <span
              class="h-5 w-5 rounded-full border border-black/10"
              style={`background:${color.hex}`}
            ></span>
            <span class="text-[10px] text-muted-foreground">{color.name}</span>
          </button>
        {/each}
      </div>
    </div>

    <div>
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-xs font-semibold text-foreground">形态</h3>
        <span class="text-[11px] text-muted-foreground"
          >{SHAPES.find((shape) => shape.id === settings.grokbotShape)?.name}</span
        >
      </div>
      <div class="grid grid-cols-4 gap-2 sm:grid-cols-8">
        {#each SHAPES as shape}
          <button
            type="button"
            class={`rounded-lg border px-2 py-2 text-[11px] transition-colors ${
              settings.grokbotShape === shape.id
                ? "border-primary bg-primary/[0.06] text-foreground"
                : "border-border/60 text-muted-foreground hover:border-primary/40 hover:text-foreground"
            }`}
            aria-pressed={settings.grokbotShape === shape.id}
            onclick={() => persist({ grokbotShape: shape.id })}
          >
            {shape.name}
          </button>
        {/each}
      </div>
    </div>

    <div>
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-xs font-semibold text-foreground">身体部件</h3>
        <span class="text-[11px] text-muted-foreground"
          >{settings.grokbotParts.length
            ? `${settings.grokbotParts.length} 件`
            : "默认无部件"}</span
        >
      </div>
      <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {#each GROKBOT_PARTS as part}
          <button
            type="button"
            class={`flex items-center gap-2 rounded-lg border px-3 py-2 text-left text-xs transition-colors ${
              settings.grokbotParts.includes(part.id)
                ? "border-primary bg-primary/[0.06] text-foreground"
                : "border-border/60 text-muted-foreground hover:border-primary/40 hover:text-foreground"
            }`}
            aria-pressed={settings.grokbotParts.includes(part.id)}
            onclick={() => togglePart(part.id)}
          >
            <span class="font-mono text-sm">{part.glyph}</span><span>{part.name}</span>
          </button>
        {/each}
      </div>
    </div>

    <div>
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-xs font-semibold text-foreground">趣味配饰</h3>
        <span class="text-[11px] text-muted-foreground"
          >{settings.grokbotAccessories.length
            ? `${settings.grokbotAccessories.length} 件`
            : "默认无配饰"}</span
        >
      </div>
      <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {#each GROKBOT_ACCESSORIES as accessory}
          <button
            type="button"
            class={`flex items-center gap-2 rounded-lg border px-3 py-2 text-left text-xs transition-colors ${
              settings.grokbotAccessories.includes(accessory.id)
                ? "border-primary bg-primary/[0.06] text-foreground"
                : "border-border/60 text-muted-foreground hover:border-primary/40 hover:text-foreground"
            }`}
            aria-pressed={settings.grokbotAccessories.includes(accessory.id)}
            onclick={() => toggleAccessory(accessory.id)}
          >
            <span class="font-mono text-sm">{accessory.glyph}</span><span>{accessory.name}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>

  <div class="mt-6 border-t border-border/60 pt-5">
    <div class="mb-2 flex items-center justify-between gap-3">
      <div>
        <h3 class="text-xs font-semibold text-foreground">全部状态动作</h3>
        <p class="mt-1 text-[11px] text-muted-foreground">
          上游 39 种状态，支持手动预览和自动巡演。
        </p>
      </div>
      <span class="shrink-0 text-[11px] text-muted-foreground">{allStates.length} 种</span>
    </div>
    <div class="flex gap-2 overflow-x-auto pb-2">
      {#each GROKBOT_STATE_GROUPS as group}
        <button
          type="button"
          class={`shrink-0 rounded-full border px-3 py-1.5 text-[11px] transition-colors ${
            activeGroupId === group.id
              ? "border-foreground bg-foreground text-background"
              : "border-border/60 text-muted-foreground hover:border-primary/40"
          }`}
          aria-pressed={activeGroupId === group.id}
          onclick={() => selectGroup(group.id)}
        >
          {group.name}
        </button>
      {/each}
    </div>
    <div class="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3">
      {#each visibleStates as state}
        <button
          type="button"
          class={`rounded-lg border px-3 py-2 text-left text-xs transition-colors ${
            previewState === state
              ? "border-primary bg-primary/[0.06] text-foreground"
              : "border-border/60 text-muted-foreground hover:border-primary/40 hover:text-foreground"
          }`}
          aria-pressed={previewState === state}
          onclick={() => selectState(state)}
        >
          <span class="font-mono text-[10px] text-muted-foreground">{state}</span>
          <span class="mt-1 block">{GROKBOT_STATE_NAMES[state]}</span>
        </button>
      {/each}
    </div>
    {#if statePageCount > 1}
      <div class="mt-3 flex items-center justify-between">
        <button
          type="button"
          class="rounded-lg border border-border/60 px-3 py-1.5 text-[11px] text-muted-foreground disabled:opacity-40"
          disabled={statePage === 0}
          onclick={() => (statePage -= 1)}
        >
          ← 上一页
        </button>
        <span class="font-mono text-[11px] text-muted-foreground"
          >{statePage + 1} / {statePageCount}</span
        >
        <button
          type="button"
          class="rounded-lg border border-border/60 px-3 py-1.5 text-[11px] text-muted-foreground disabled:opacity-40"
          disabled={statePage >= statePageCount - 1}
          onclick={() => (statePage += 1)}
        >
          下一页 →
        </button>
      </div>
    {/if}
  </div>

  <div
    class="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-4"
  >
    <div class="text-[11px] text-muted-foreground">
      生成本地 1080 × 1440 PNG 分享卡，不上传图片。
    </div>
    <button
      type="button"
      class="rounded-lg bg-primary px-3 py-2 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90"
      onclick={downloadShareCard}
    >
      下载 PNG 分享卡
    </button>
  </div>
  {#if shareMessage}
    <p class="mt-2 text-right text-[11px] text-muted-foreground" role="status">{shareMessage}</p>
  {/if}
</section>
