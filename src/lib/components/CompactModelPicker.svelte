<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import type { CliModelInfo, SubscriptionRateLimits } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";

  let {
    agent = "claude",
    models = [],
    currentModel = "",
    projectDefaultModel = "",
    currentEffort = "",
    effortOptions = [],
    fastModeState = "",
    onModelSwitch,
    onSetProjectDefault,
    onEffortChange,
    onFastModeSwitch,
    projectDefaultLabel,
    setProjectDefaultLabel,
  }: {
    agent?: string;
    models?: CliModelInfo[];
    currentModel?: string;
    projectDefaultModel?: string;
    currentEffort?: string;
    effortOptions?: Array<{ value: string; label: string }>;
    fastModeState?: string;
    onModelSwitch?: (model: string) => void;
    onSetProjectDefault?: () => void | Promise<void>;
    onEffortChange?: (effort: string) => void;
    onFastModeSwitch?: (mode: "on" | "off") => void;
    projectDefaultLabel?: string;
    setProjectDefaultLabel?: string;
  } = $props();

  let wrapperEl: HTMLDivElement | undefined = $state();
  let dropdownEl: HTMLDivElement | undefined = $state();
  let pickerOpen = $state(false);
  let effortOpen = $state(false);
  let dropdownStyle = $state("");
  let searchQuery = $state("");

  let rateLimits = $state<SubscriptionRateLimits | null>(null);
  let rateLimitsLoading = $state(false);

  const currentModelInfo = $derived.by(() => {
    const exact = models.find((model) => model.value === currentModel);
    if (exact) return exact;
    return models.find((m) => {
      const bareId =
        m.providerId && m.value.startsWith(`${m.providerId}/`)
          ? m.value.slice(m.providerId.length + 1)
          : m.value.includes("/")
            ? m.value.slice(m.value.indexOf("/") + 1)
            : m.value;
      return bareId === currentModel || m.displayName === currentModel;
    });
  });
  const isSubscriptionModel = $derived(
    currentModel.startsWith("openai-chatgpt-subscription") ||
      currentModelInfo?.providerId === "openai-chatgpt-subscription",
  );

  async function loadRateLimits() {
    if (!isSubscriptionModel) return;
    rateLimitsLoading = true;
    try {
      const res = await api.getChatGptSubscriptionRateLimits();
      if (res) rateLimits = res;
    } catch {
      // ignore
    } finally {
      rateLimitsLoading = false;
    }
  }

  $effect(() => {
    if (isSubscriptionModel && !rateLimits) {
      void loadRateLimits();
    }
  });

  function formatResetCountdown(resetsAt?: number | null): string {
    if (!resetsAt) return "";
    const now = Math.floor(Date.now() / 1000);
    const diffSec = Math.max(0, Math.floor(resetsAt - now));
    if (diffSec === 0) return "即将重置";
    const hours = Math.floor(diffSec / 3600);
    const mins = Math.floor((diffSec % 3600) / 60);
    if (hours > 0) {
      return `${hours}h${mins}m后重置`;
    }
    return `${mins}m后重置`;
  }

  function formatResetDate(resetsAt?: number | null): string {
    if (!resetsAt) return "";
    const d = new Date(resetsAt * 1000);
    const month = d.getMonth() + 1;
    const day = d.getDate();
    const hours = String(d.getHours()).padStart(2, "0");
    const mins = String(d.getMinutes()).padStart(2, "0");
    const weekdays = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
    const weekday = weekdays[d.getDay()];
    return `${weekday} ${month}/${day} ${hours}:${mins}重置`;
  }
  const displayModel = $derived.by(() => {
    if (currentModelInfo) {
      return currentModelInfo.displayName || currentModelInfo.value;
    }
    if (currentModel.includes("/")) {
      const slash = currentModel.lastIndexOf("/");
      return currentModel.slice(slash + 1);
    }
    return currentModel.trim() || models[0]?.displayName || "选择模型";
  });
  const displayEffort = $derived(
    effortOptions.find((option) => option.value === currentEffort)?.label ??
      (currentEffort || t("model_auto")),
  );

  const filteredModels = $derived.by(() => {
    if (!searchQuery.trim()) return models;
    const q = searchQuery.trim().toLowerCase();
    return models.filter(
      (model) =>
        model.displayName.toLowerCase().includes(q) ||
        model.value.toLowerCase().includes(q) ||
        (model.providerName && model.providerName.toLowerCase().includes(q)) ||
        (model.description && model.description.toLowerCase().includes(q)),
    );
  });

  /** 简化冗长的供应商名称，去掉"OpenAI / ChatGPT 订阅"式的前缀 */
  function simplifyProviderName(name: string): string {
    // "OpenAI / ChatGPT 订阅" → "ChatGPT 订阅"
    // "Anthropic / Claude" → "Claude"
    const slashIdx = name.indexOf(" / ");
    if (slashIdx !== -1) return name.slice(slashIdx + 3);
    // 去掉常见冗余后缀
    return (
      name
        .replace(/\s*(?:subscription|订阅)\s*$/i, "")
        .replace(/\s*\(.*?\)\s*$/, "")
        .trim() || name
    );
  }

  const groupedModels = $derived.by(() => {
    const map = new Map<string, { name: string; models: CliModelInfo[] }>();
    for (const model of filteredModels) {
      let pName = model.providerName;
      if (!pName && model.providerId) {
        pName = model.providerId;
      }
      const key = pName || "default";
      if (!map.has(key)) {
        map.set(key, { name: pName ? simplifyProviderName(pName) : "默认供应商", models: [] });
      }
      map.get(key)!.models.push(model);
    }
    return Array.from(map.values());
  });

  function updateDropdownPosition() {
    if (!wrapperEl) return;
    const rect = wrapperEl.getBoundingClientRect();
    const right = Math.max(16, window.innerWidth - rect.right);
    const bottom = window.innerHeight - rect.top + 6;
    dropdownStyle = `position: fixed; bottom: ${bottom}px; right: ${right}px; z-index: 9999;`;
  }

  function togglePicker() {
    pickerOpen = !pickerOpen;
    if (pickerOpen) {
      effortOpen = false;
      updateDropdownPosition();
    }
  }

  function selectModel(model: CliModelInfo) {
    onModelSwitch?.(model.value);
    pickerOpen = false;
    effortOpen = false;
  }

  function selectEffort(value: string) {
    effortOpen = false;
    onEffortChange?.(value);
  }

  function setProjectDefault() {
    pickerOpen = false;
    effortOpen = false;
    void onSetProjectDefault?.();
  }

  onMount(() => {
    function handleClickOutside(event: MouseEvent) {
      const target = event.target as Node;
      if (
        pickerOpen &&
        wrapperEl &&
        !wrapperEl.contains(target) &&
        dropdownEl &&
        !dropdownEl.contains(target)
      ) {
        pickerOpen = false;
        effortOpen = false;
      }
    }

    function handleKeydown(event: KeyboardEvent) {
      if (pickerOpen && event.key === "Escape") {
        pickerOpen = false;
        effortOpen = false;
      }
    }

    document.addEventListener("mousedown", handleClickOutside, true);
    document.addEventListener("keydown", handleKeydown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside, true);
      document.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<div bind:this={wrapperEl} class="relative min-w-0 max-w-[min(22rem,36vw)] shrink-0">
  <button
    type="button"
    class="ui-model-picker flex min-h-7 max-w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-foreground/80 transition-colors hover:bg-accent hover:text-foreground"
    onclick={togglePicker}
    title="切换模型和推理强度"
    aria-label="切换模型和推理强度"
    aria-expanded={pickerOpen}
  >
    <span class="min-w-0 max-w-[min(18rem,30vw)] truncate font-medium" title={displayModel}
      >{displayModel}</span
    >
    {#if isSubscriptionModel && rateLimits?.primary}
      {@const p = rateLimits.primary}
      <span
        class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-medium {p.used_percent >=
        90
          ? 'bg-rose-500/10 text-rose-500'
          : p.used_percent >= 70
            ? 'bg-amber-500/10 text-amber-500'
            : 'bg-emerald-500/10 text-emerald-600'}"
        title="5小时额度已用 {p.used_percent.toFixed(0)}%{rateLimits.secondary
          ? ` · 每周已用 ${rateLimits.secondary.used_percent.toFixed(0)}%`
          : ''}"
      >
        <span
          class="h-1.5 w-1.5 rounded-full {p.used_percent >= 90
            ? 'bg-rose-500'
            : p.used_percent >= 70
              ? 'bg-amber-500'
              : 'bg-emerald-500'}"
        ></span>
        <span>5h:{p.used_percent.toFixed(0)}%</span>
      </span>
    {/if}
    {#if currentModelInfo?.supportsEffort !== false && effortOptions.length > 0}
      <span class="flex items-center gap-1 text-muted-foreground">
        <span class="text-base leading-none" aria-hidden="true">✿</span>
        <span>{displayEffort}</span>
      </span>
    {/if}
    <svg
      class="h-3.5 w-3.5 text-muted-foreground/70"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      aria-hidden="true"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  </button>

  {#if pickerOpen}
    <div
      bind:this={dropdownEl}
      style={dropdownStyle}
      class="flex w-[min(32rem,calc(100vw-2rem))] gap-1.5 animate-in fade-in zoom-in-95 duration-150"
    >
      <div
        class="min-w-0 flex-1 overflow-hidden rounded-2xl border border-border bg-popover text-popover-foreground shadow-xl animate-in fade-in zoom-in-95 duration-150"
      >
        {#if isSubscriptionModel && (rateLimits?.primary || rateLimits?.secondary)}
          <div class="p-2 border-b border-border/60 bg-muted/30 text-[11px] space-y-1.5">
            <div
              class="flex items-center justify-between text-[10px] text-muted-foreground font-medium"
            >
              <div class="flex items-center gap-1">
                <span>⚡ ChatGPT 订阅配额</span>
                {#if rateLimits.plan_type}
                  <span class="rounded bg-primary/10 px-1 py-0.2 text-[9px] text-primary uppercase"
                    >{rateLimits.plan_type}</span
                  >
                {/if}
              </div>
              <button
                type="button"
                class="hover:text-foreground transition-colors disabled:opacity-50"
                disabled={rateLimitsLoading}
                onclick={loadRateLimits}
                title="刷新配额"
              >
                <svg
                  class="h-2.5 w-2.5 {rateLimitsLoading ? 'animate-spin' : ''}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                    d="M3 3v5h5"
                  />
                  <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path
                    d="M16 21h5v-5"
                  />
                </svg>
              </button>
            </div>
            <div class="grid grid-cols-2 gap-2 text-[10px]">
              {#if rateLimits.primary}
                <div class="rounded border bg-background/60 p-1.5">
                  <div class="flex justify-between font-medium">
                    <span>5小时额度</span>
                    <span
                      class={rateLimits.primary.used_percent >= 90
                        ? "text-rose-500"
                        : rateLimits.primary.used_percent >= 70
                          ? "text-amber-500"
                          : "text-emerald-500"}
                    >
                      {rateLimits.primary.used_percent.toFixed(0)}%
                    </span>
                  </div>
                  {#if rateLimits.primary.resets_at}
                    <div class="text-[9px] text-muted-foreground truncate">
                      {formatResetCountdown(rateLimits.primary.resets_at)}
                    </div>
                  {/if}
                </div>
              {/if}
              {#if rateLimits.secondary}
                <div class="rounded border bg-background/60 p-1.5">
                  <div class="flex justify-between font-medium">
                    <span>每周额度</span>
                    <span
                      class={rateLimits.secondary.used_percent >= 90
                        ? "text-rose-500"
                        : rateLimits.secondary.used_percent >= 70
                          ? "text-amber-500"
                          : "text-emerald-500"}
                    >
                      {rateLimits.secondary.used_percent.toFixed(0)}%
                    </span>
                  </div>
                  {#if rateLimits.secondary.resets_at}
                    <div class="text-[9px] text-muted-foreground truncate">
                      {formatResetDate(rateLimits.secondary.resets_at)}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if models.length > 5}
          <div class="p-1.5 border-b border-border/60">
            <input
              type="text"
              class="w-full rounded-lg border bg-background px-2.5 py-1 text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-hidden"
              placeholder="搜索模型或供应商..."
              bind:value={searchQuery}
            />
          </div>
        {/if}

        <div class="max-h-72 overflow-y-auto p-1">
          {#if filteredModels.length === 0}
            <div class="py-4 text-center text-xs text-muted-foreground">未找到匹配的模型</div>
          {:else}
            {#each groupedModels as group (group.name)}
              {#if groupedModels.length > 1}
                <div
                  class="px-2.5 py-1 text-[10px] font-semibold tracking-wider text-muted-foreground uppercase flex items-center gap-1.5"
                >
                  <span class="h-1.5 w-1.5 rounded-full bg-primary/60"></span>
                  <span>{group.name}</span>
                </div>
              {/if}

              {#each group.models as model (model.value)}
                {@const isSelected =
                  model.value === currentModel ||
                  (currentModelInfo && model.value === currentModelInfo.value)}
                <div
                  class="flex w-full items-center rounded-lg {isSelected
                    ? 'bg-accent text-foreground'
                    : 'text-foreground/85 hover:bg-accent/60'}"
                >
                  <button
                    type="button"
                    class="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1 text-left text-xs"
                    onclick={() => selectModel(model)}
                    title={model.value}
                  >
                    <span class="w-4 shrink-0 text-primary">{isSelected ? "✓" : ""}</span>
                    <span class="min-w-0 flex-1 truncate font-medium">{model.displayName}</span>
                    {#if model.contextWindow}
                      <span
                        class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground font-mono"
                      >
                        {Math.round(model.contextWindow / 1000)}k
                      </span>
                    {/if}
                    {#if isSelected && effortOptions.length > 0}
                      <span class="flex shrink-0 items-center gap-1 text-muted-foreground">
                        <span class="text-base leading-none" aria-hidden="true">✿</span>
                        <span>{displayEffort}</span>
                      </span>
                    {/if}
                    {#if model.description}
                      <span class="max-w-28 truncate text-xs text-muted-foreground"
                        >{model.description}</span
                      >
                    {/if}
                  </button>
                  {#if isSelected}
                    <button
                      type="button"
                      class="mr-2 shrink-0 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-background hover:text-foreground"
                      onclick={() => (effortOpen = true)}
                    >
                      ✎ {t("model_edit")}
                    </button>
                  {/if}
                </div>
              {/each}
            {/each}
          {/if}
        </div>

        {#if onSetProjectDefault && currentModel}
          {@const isProjectDefault =
            projectDefaultModel === currentModel ||
            (currentModelInfo && projectDefaultModel === currentModelInfo.value)}
          <div class="border-t border-border/70 px-1.5 py-1.5">
            <button
              type="button"
              class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors
                {isProjectDefault ? 'text-primary' : 'text-foreground/85 hover:bg-accent/60'}"
              disabled={isProjectDefault}
              onclick={setProjectDefault}
            >
              <span class="w-4 shrink-0 text-primary">{isProjectDefault ? "✓" : ""}</span>
              <span
                >{isProjectDefault
                  ? (projectDefaultLabel ?? t("model_projectDefault"))
                  : (setProjectDefaultLabel ?? t("model_setProjectDefault"))}</span
              >
            </button>
          </div>
        {/if}
      </div>

      {#if effortOpen && effortOptions.length > 0}
        <div
          class="w-52 shrink-0 self-start overflow-hidden rounded-2xl border border-border bg-popover p-1.5 text-popover-foreground shadow-xl animate-in fade-in zoom-in-95 duration-150"
        >
          <div class="px-2.5 py-2 text-sm font-medium text-muted-foreground">
            {t("model_thinkingIntensity")}
          </div>
          {#each effortOptions as option (option.value)}
            <button
              type="button"
              class="flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-sm transition-colors {option.value ===
              currentEffort
                ? 'bg-accent text-foreground'
                : 'text-foreground/85 hover:bg-accent/60'}"
              onclick={() => selectEffort(option.value)}
            >
              <span>{option.value === "" ? t("model_thinkingDisabled") : option.label}</span>
              {#if option.value === currentEffort}<span class="text-primary">✓</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
