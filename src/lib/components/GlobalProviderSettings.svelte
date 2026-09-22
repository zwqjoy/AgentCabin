<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import type {
    CodexAuthResult,
    GlobalProviderCredential,
    GlobalProviderModel,
    ProviderProtocol,
    SubscriptionRateLimits,
    UserSettings,
  } from "$lib/types";
  import ProviderIcon from "$lib/components/ProviderIcon.svelte";
  import { CODEX_SUBSCRIPTION_PROVIDER_ID } from "$lib/utils/codex-subscription";

  let {
    initialSettings,
    onSaved = () => {},
    onOpenCodexLogin = () => {},
    onReopenCodexLogin = () => {},
    onLogoutCodexSubscription = () => {},
    codexAuth = null,
    codexInstalled = true,
    codexLoginLoading = false,
    codexLoginError = "",
  }: {
    initialSettings: UserSettings;
    onSaved?: (settings: UserSettings) => void;
    onOpenCodexLogin?: () => void;
    onReopenCodexLogin?: () => void;
    onLogoutCodexSubscription?: () => void;
    codexAuth?: CodexAuthResult | null;
    codexInstalled?: boolean | null;
    codexLoginLoading?: boolean;
    codexLoginError?: string;
  } = $props();

  let providers = $state<GlobalProviderCredential[]>([]);
  let selectedId = $state<string | null>(null);
  let isCreatingNew = $state(false);

  // Form states
  let providerSlug = $state("");
  let name = $state("");
  let protocol = $state<ProviderProtocol>("openai-completions");
  let baseUrl = $state("");
  let apiKey = $state("");
  let showApiKey = $state(false);
  let models = $state<GlobalProviderModel[]>([]);
  let testModel = $state("");
  const availableTestModels = $derived(models.filter((m) => m.id.trim().length > 0));

  $effect(() => {
    if (availableTestModels.length > 0) {
      if (!testModel || !availableTestModels.some((m) => m.id === testModel)) {
        testModel = availableTestModels[0].id;
      }
    } else {
      testModel = "";
    }
  });
  let envKey = $state("OPENAI_API_KEY");
  let authEnvVar = $state("ANTHROPIC_AUTH_TOKEN");
  let keyless = $state(false);
  let supportsDeveloperRole = $state(false);
  let supportsReasoningEffort = $state(false);

  let saving = $state(false);
  let savedFeedback = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | null = null;
  let testing = $state(false);
  let providerTestResults = $state<
    Record<string, { status: "success" | "error"; message: string }>
  >({});
  let listing = $state(false);
  let error = $state("");
  let connectionResult = $state<"success" | "error" | null>(null);
  let connectionMessage = $state("");
  let appliedSettingsSignature = $state("");

  const codexSubscriptionLoggedIn = $derived(codexAuth?.subscription_logged_in === true);
  const codexSubscriptionProvider = $derived(
    providers.find((provider) => provider.id === CODEX_SUBSCRIPTION_PROVIDER_ID),
  );
  const customProviders = $derived(
    providers.filter((provider) => provider.id !== CODEX_SUBSCRIPTION_PROVIDER_ID),
  );

  const selectedProvider = $derived(selectedId ? providers.find((p) => p.id === selectedId) : null);

  const isCodexSelected = $derived(selectedId === CODEX_SUBSCRIPTION_PROVIDER_ID);

  let rateLimitsData = $state<SubscriptionRateLimits | null>(null);
  let rateLimitsLoading = $state(false);

  const effectiveRateLimits = $derived(
    rateLimitsData ?? codexAuth?.subscription_rate_limits ?? null,
  );

  async function refreshRateLimits() {
    rateLimitsLoading = true;
    try {
      const res = await api.getChatGptSubscriptionRateLimits();
      console.log("[ChatGPT Subscription Rate Limits]", res);
      if (res) rateLimitsData = res;
    } catch {
      // ignore
    } finally {
      rateLimitsLoading = false;
    }
  }

  $effect(() => {
    if (
      isCodexSelected &&
      codexSubscriptionLoggedIn &&
      !rateLimitsData &&
      !codexAuth?.subscription_rate_limits
    ) {
      void refreshRateLimits();
    }
  });

  function formatResetCountdown(resetsAt?: number | null): string {
    if (!resetsAt) return "未知";
    const now = Math.floor(Date.now() / 1000);
    const diffSec = Math.max(0, Math.floor(resetsAt - now));
    if (diffSec === 0) return "即将重置";
    const hours = Math.floor(diffSec / 3600);
    const mins = Math.floor((diffSec % 3600) / 60);
    if (hours > 0) {
      return `${hours} 小时 ${mins} 分钟后`;
    }
    return `${mins} 分钟后`;
  }

  function formatResetDate(resetsAt?: number | null): string {
    if (!resetsAt) return "未知";
    const d = new Date(resetsAt * 1000);
    const month = d.getMonth() + 1;
    const day = d.getDate();
    const hours = String(d.getHours()).padStart(2, "0");
    const mins = String(d.getMinutes()).padStart(2, "0");
    const weekdays = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
    const weekday = weekdays[d.getDay()];
    return `${weekday} ${month}月${day}日 ${hours}:${mins}`;
  }

  function getProgressColor(percent: number): string {
    if (percent >= 90) return "bg-rose-500";
    if (percent >= 70) return "bg-amber-500";
    return "bg-emerald-500";
  }

  function getProgressTextColor(percent: number): string {
    if (percent >= 90) return "text-rose-500";
    if (percent >= 70) return "text-amber-500";
    return "text-emerald-500";
  }

  const MODEL_EFFORT_OPTIONS = [
    { value: "off", label: "关闭" },
    { value: "low", label: "低" },
    { value: "medium", label: "中" },
    { value: "high", label: "高" },
    { value: "xhigh", label: "极高" },
    { value: "max", label: "最大" },
  ] as const;
  // Most reasoning models accept explicit effort levels, while "off" is not
  // consistently supported. Keep it opt-in so a freshly detected model has a
  // usable default; users can still enable it manually when they know it works.
  const DEFAULT_MODEL_EFFORT_LEVELS = ["low", "medium", "high"];

  function modelEffortLevels(model: GlobalProviderModel): string[] {
    if (model.supported_effort_levels?.length) return [...model.supported_effort_levels];
    return [...DEFAULT_MODEL_EFFORT_LEVELS, ...(model.supports_xhigh ? ["xhigh"] : [])];
  }

  function formatEffortSummary(model: GlobalProviderModel): string {
    if (!model.supports_reasoning) return "未启用";
    const levels = modelEffortLevels(model);
    const labels: Record<string, string> = {
      off: "关闭",
      low: "低",
      medium: "中",
      high: "高",
      max: "最大",
      xhigh: "极高",
    };
    return levels.map((l) => labels[l] ?? l).join(", ");
  }

  function sanitizeSlug(raw: string): string {
    return raw
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9_-]/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
  }

  function handleNameInput(event: Event) {
    const target = event.target as HTMLInputElement;
    name = target.value;
    if (isCreatingNew && !providerSlug) {
      providerSlug = sanitizeSlug(name);
    }
  }

  function resetEditor() {
    selectedId = null;
    isCreatingNew = true;
    providerSlug = "";
    name = "";
    protocol = "openai-completions";
    baseUrl = "";
    apiKey = "";
    showApiKey = false;
    models = [];
    testModel = "";
    envKey = "OPENAI_API_KEY";
    authEnvVar = "ANTHROPIC_AUTH_TOKEN";
    keyless = false;
    supportsDeveloperRole = false;
    supportsReasoningEffort = false;
    error = "";
    connectionResult = null;
    connectionMessage = "";
    savedFeedback = false;
    if (savedTimer) {
      clearTimeout(savedTimer);
      savedTimer = null;
    }
  }

  function openNewProvider() {
    resetEditor();
  }

  function selectProvider(provider: GlobalProviderCredential) {
    selectedId = provider.id;
    isCreatingNew = false;
    providerSlug = provider.id;
    name = provider.name;
    protocol = provider.protocol;
    baseUrl = provider.base_url;
    apiKey = provider.api_key ?? "";
    showApiKey = false;
    models = (provider.models ?? []).map((model) =>
      typeof model === "string" ? { id: model } : { ...model },
    );
    testModel = provider.test_model ?? models[0]?.id ?? "";
    envKey = provider.env_key ?? (provider.protocol === "openai-responses" ? "OPENAI_API_KEY" : "");
    authEnvVar = provider.auth_env_var ?? "ANTHROPIC_AUTH_TOKEN";
    keyless = provider.keyless ?? false;
    supportsDeveloperRole = provider.supports_developer_role ?? false;
    supportsReasoningEffort = provider.supports_reasoning_effort ?? false;
    error = "";
    connectionResult = null;
    connectionMessage = "";
    savedFeedback = false;
    if (savedTimer) {
      clearTimeout(savedTimer);
      savedTimer = null;
    }
  }

  let showModelModal = $state(false);
  let editingModelIndex = $state<number | null>(null);
  let modalDraft = $state<{
    id: string;
    name: string;
    context_window: string | number | null | undefined;
    max_tokens: string | number | null | undefined;
    supports_images: boolean;
    supports_reasoning: boolean;
    supported_effort_levels: string[];
  }>({
    id: "",
    name: "",
    context_window: "",
    max_tokens: "",
    supports_images: false,
    supports_reasoning: false,
    supported_effort_levels: ["low", "medium", "high"],
  });

  function parseOptionalNumber(val: unknown): number | undefined {
    if (val === null || val === undefined) return undefined;
    if (typeof val === "number") return Number.isFinite(val) && val > 0 ? val : undefined;
    const str = String(val).trim();
    if (!str) return undefined;
    const n = Number(str);
    return Number.isFinite(n) && n > 0 ? n : undefined;
  }

  function openAddModelModal() {
    editingModelIndex = null;
    modalDraft = {
      id: "",
      name: "",
      context_window: "",
      max_tokens: "",
      supports_images: false,
      supports_reasoning: false,
      supported_effort_levels: ["low", "medium", "high"],
    };
    showModelModal = true;
  }

  function openEditModelModal(index: number) {
    editingModelIndex = index;
    const m = models[index];
    modalDraft = {
      id: m.id,
      name: m.name ?? "",
      context_window: m.context_window ? String(m.context_window) : "",
      max_tokens: m.max_tokens ? String(m.max_tokens) : "",
      supports_images: m.supports_images ?? false,
      supports_reasoning: m.supports_reasoning ?? false,
      supported_effort_levels: m.supported_effort_levels?.length
        ? [...m.supported_effort_levels]
        : ["low", "medium", "high"],
    };
    showModelModal = true;
  }

  function toggleModalEffort(level: string) {
    const set = new Set(modalDraft.supported_effort_levels);
    if (set.has(level)) {
      set.delete(level);
    } else {
      set.add(level);
    }
    const ordered = MODEL_EFFORT_OPTIONS.map((o) => o.value).filter((val) => set.has(val));
    modalDraft.supported_effort_levels = ordered;
  }

  function saveModelModal() {
    const trimmedId = modalDraft.id.trim();
    if (!trimmedId) return;
    const cw = parseOptionalNumber(modalDraft.context_window);
    const mt = parseOptionalNumber(modalDraft.max_tokens);
    const modelItem: GlobalProviderModel = {
      id: trimmedId,
      name: modalDraft.name.trim() || undefined,
      context_window: cw,
      max_tokens: mt,
      supports_images: modalDraft.supports_images || undefined,
      supports_reasoning: modalDraft.supports_reasoning || undefined,
      supported_effort_levels:
        modalDraft.supports_reasoning && modalDraft.supported_effort_levels.length
          ? [...modalDraft.supported_effort_levels]
          : undefined,
      supports_xhigh:
        modalDraft.supports_reasoning &&
        (modalDraft.supported_effort_levels.includes("max") ||
          modalDraft.supported_effort_levels.includes("xhigh"))
          ? true
          : undefined,
    };
    if (editingModelIndex !== null && editingModelIndex >= 0 && editingModelIndex < models.length) {
      models = models.map((item, idx) => (idx === editingModelIndex ? modelItem : item));
    } else {
      models = [...models, modelItem];
    }
    if (!testModel) testModel = modelItem.id;
    showModelModal = false;
  }

  function removeModel(index: number) {
    const removedId = models[index]?.id;
    models = models.filter((_, itemIndex) => itemIndex !== index);
    if (!testModel || testModel === removedId) testModel = models[0]?.id ?? "";
  }

  function draftProvider(): GlobalProviderCredential {
    const normalizedModels = models
      .map((model) => {
        const levels = model.supports_reasoning ? modelEffortLevels(model) : undefined;
        return {
          ...model,
          id: model.id.trim(),
          name: model.name?.trim() || undefined,
          context_window: model.context_window,
          max_tokens: model.max_tokens,
          supported_effort_levels: levels?.length ? levels : undefined,
          supports_xhigh: levels?.some((l) => l === "max" || l === "xhigh") || undefined,
        };
      })
      .filter((model) => model.id);

    const finalId =
      providerSlug.trim() || sanitizeSlug(name) || (selectedId ?? `provider-${Date.now()}`);

    return {
      id: finalId,
      name: name.trim(),
      protocol,
      base_url: baseUrl.trim(),
      api_key: apiKey.trim() || undefined,
      auth_env_var: protocol === "anthropic-messages" ? authEnvVar.trim() || undefined : undefined,
      env_key: protocol !== "anthropic-messages" ? envKey.trim() || undefined : undefined,
      models: normalizedModels.length ? normalizedModels : undefined,
      test_model: testModel.trim() || undefined,
      supports_developer_role: supportsDeveloperRole || undefined,
      supports_reasoning_effort: supportsReasoningEffort || undefined,
      keyless: keyless || undefined,
    };
  }

  function validateDraft(): boolean {
    if (!name.trim() || !baseUrl.trim()) {
      error = "请填写供应商名称和 Base URL。";
      return false;
    }
    const finalId = providerSlug.trim() || sanitizeSlug(name);
    if (!finalId) {
      error = "请指定唯一的供应商 ID（用于聊天中的 provider-id/model-name 引用）。";
      return false;
    }
    if (providers.some((p) => p.id !== selectedId && p.id === finalId)) {
      error = `供应商标识 '${finalId}' 已存在，请换一个标识。`;
      return false;
    }
    const normalizedName = name.trim().toLocaleLowerCase();
    if (
      providers.some(
        (provider) =>
          provider.id !== selectedId && provider.name.trim().toLocaleLowerCase() === normalizedName,
      )
    ) {
      error = "供应商名称已存在，请换一个名称。";
      return false;
    }
    if (!keyless && !apiKey.trim() && !authEnvVar.trim() && !envKey.trim()) {
      error = "请填写 API Key，或勾选“无需 API Key”。";
      return false;
    }
    if (!models.length) {
      error = "添加供应商前，请至少添加一个模型。";
      return false;
    }
    if (models.some((model) => !model.id.trim())) {
      error = "模型 ID 不能为空，请删除空行或补全模型 ID。";
      return false;
    }
    return true;
  }

  async function persistProviders(nextProviders: GlobalProviderCredential[]) {
    saving = true;
    error = "";
    try {
      const saved = await api.updateUserSettings({ global_providers: nextProviders });
      providers = [...(saved.global_providers ?? nextProviders)];
      onSaved(saved);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function saveProvider() {
    error = "";
    if (!validateDraft()) return;
    const provider = draftProvider();
    const nextProviders =
      !isCreatingNew && selectedId
        ? providers.map((item) => (item.id === selectedId ? provider : item))
        : [...providers, provider];
    await persistProviders(nextProviders);
    if (!error) {
      selectedId = provider.id;
      isCreatingNew = false;
      savedFeedback = true;
      if (savedTimer) clearTimeout(savedTimer);
      savedTimer = setTimeout(() => {
        savedFeedback = false;
      }, 2000);
    }
  }

  async function removeProvider(provider: GlobalProviderCredential) {
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(`确定删除模型供应商“${provider.name}”吗？`, {
      title: "删除供应商",
      kind: "warning",
    });
    if (!ok) return;
    await persistProviders(providers.filter((item) => item.id !== provider.id));
    if (selectedId === provider.id) {
      resetEditor();
    }
  }

  async function testConnection() {
    error = "";
    connectionResult = null;
    const provider = draftProvider();
    const model = testModel.trim() || provider.models?.[0]?.id || "";
    if (!provider.base_url) {
      error = "请先填写 Base URL。";
      return;
    }
    if (!model) {
      error = "请先在模型列表中添加至少一个模型后再测试连接。";
      return;
    }
    testing = true;
    try {
      const result = await api.testGlobalProvider(provider, model);
      connectionResult = result.success ? "success" : "error";
      connectionMessage = result.success
        ? `连接成功 · ${result.latencyMs} ms`
        : (result.error ?? "连接失败");
      if (selectedId) {
        providerTestResults[selectedId] = {
          status: result.success ? "success" : "error",
          message: connectionMessage,
        };
      }
    } catch (e) {
      connectionResult = "error";
      connectionMessage = String(e);
      if (selectedId) {
        providerTestResults[selectedId] = { status: "error", message: String(e) };
      }
    } finally {
      testing = false;
    }
  }

  async function fetchModels() {
    error = "";
    listing = true;
    try {
      const fetched = await api.listGlobalProviderModels(draftProvider());
      const existing = new Map(models.map((model) => [model.id, model]));
      models = fetched.map((model) => {
        const prev = existing.get(model.id);
        // The provider catalog is authoritative for membership: models that
        // disappeared from /models are removed. For a matching ID, preserve
        // the complete local record so a refetch never overwrites user edits.
        if (prev) return { ...model, ...prev, id: model.id };

        // A newly detected model starts with the product defaults. The user
        // can refine its capabilities later from the inline editor.
        return {
          ...model,
          supports_reasoning: true,
          supported_effort_levels: [...DEFAULT_MODEL_EFFORT_LEVELS],
        };
      });
      if (!testModel && models.length) testModel = models[0]?.id ?? "";
    } catch (e) {
      error = String(e);
    } finally {
      listing = false;
    }
  }

  function loadSettings(value: UserSettings) {
    providers = [...(value.global_providers ?? [])];
    appliedSettingsSignature = JSON.stringify(value.global_providers ?? []);
    if (providers.length > 0 && !selectedId && !isCreatingNew) {
      selectProvider(providers[0]);
    } else if (providers.length === 0) {
      resetEditor();
    }
  }

  $effect(() => {
    const incomingProviders = initialSettings.global_providers ?? [];
    const signature = JSON.stringify(incomingProviders);
    if (signature === appliedSettingsSignature) return;
    providers = [...incomingProviders];
    appliedSettingsSignature = signature;
    if (selectedId && !providers.some((p) => p.id === selectedId)) {
      if (providers.length > 0) selectProvider(providers[0]);
      else resetEditor();
    }
  });

  onMount(() => loadSettings(initialSettings));
</script>

<div class="space-y-4">
  <!-- Top header banner -->
  <div class="flex items-center justify-between pb-3 border-b">
    <div>
      <h2 class="text-lg font-semibold tracking-tight">模型设置</h2>
      <p class="text-xs text-muted-foreground mt-0.5">
        管理自定义模型供应商，配置后可在聊天时按 <code
          class="rounded bg-muted px-1 py-0.5 font-mono text-[11px]">provider-id/model-name</code
        > 自由切换使用。
      </p>
    </div>
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent transition-colors"
      onclick={() => loadSettings(initialSettings)}
      title="刷新设置"
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67" />
      </svg>
      <span>刷新</span>
    </button>
  </div>

  <!-- Provider selector: full-width cards, up to five per row. -->
  <div class="rounded-2xl border bg-card/60 p-3.5 shadow-xs">
    <div class="mb-2 flex items-center justify-between px-1">
      <div>
        <div class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          Provider
        </div>
        <div class="mt-0.5 text-[11px] text-muted-foreground">选择一个 Provider 查看和编辑配置</div>
      </div>
      <span class="text-[10px] text-muted-foreground"
        >{customProviders.length} 个自定义 Provider</span
      >
    </div>

    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
      <!-- Platform subscription -->
      <div>
        {#if codexSubscriptionProvider}
          <button
            type="button"
            class="flex min-h-[68px] w-full items-center justify-between gap-2.5 rounded-xl border px-3 py-2.5 text-left text-xs transition-all {isCodexSelected
              ? 'bg-primary/10 text-primary font-medium border border-primary/30 shadow-xs'
              : 'border-transparent text-foreground/85 hover:border-border hover:bg-accent/70'}"
            onclick={() => selectProvider(codexSubscriptionProvider)}
          >
            <div class="flex items-center gap-2.5 min-w-0">
              <span
                class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600"
              >
                <ProviderIcon id="openai" name="OpenAI" />
              </span>
              <div class="min-w-0">
                <div class="truncate font-medium">{codexSubscriptionProvider.name}</div>
                <div class="truncate text-[10px] text-muted-foreground">
                  {codexSubscriptionLoggedIn ? "ChatGPT 订阅已登录" : "未登录订阅"}
                </div>
              </div>
            </div>
            <span
              class="h-2 w-2 rounded-full {codexSubscriptionLoggedIn
                ? 'bg-emerald-500'
                : 'bg-amber-500'}"
            ></span>
          </button>
        {:else}
          <button
            type="button"
            class="flex min-h-[68px] w-full items-center justify-between gap-2.5 rounded-xl border border-transparent px-3 py-2.5 text-left text-xs transition-all text-foreground/85 hover:border-border hover:bg-accent/70"
            onclick={onOpenCodexLogin}
            disabled={codexLoginLoading}
          >
            <div class="flex items-center gap-2.5 min-w-0">
              <span
                class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600"
              >
                <ProviderIcon id="openai" name="OpenAI" />
              </span>
              <div class="min-w-0">
                <div class="truncate font-medium">OpenAI / ChatGPT 订阅</div>
                <div class="truncate text-[10px] text-muted-foreground">
                  {codexLoginLoading ? "正在登录..." : "点击登录 ChatGPT 订阅"}
                </div>
              </div>
            </div>
            <span class="h-2 w-2 rounded-full bg-muted-foreground/30"></span>
          </button>
        {/if}
      </div>

      {#each customProviders as provider (provider.id)}
        {@const testStatus = providerTestResults[provider.id]?.status}
        {@const isCurrent = selectedId === provider.id && !isCreatingNew}
        <button
          type="button"
          class="group flex min-h-[68px] w-full items-center justify-between gap-2.5 rounded-xl border px-3 py-2.5 text-left text-xs transition-all {isCurrent
            ? 'bg-primary/10 text-primary font-medium border-primary/30 shadow-xs'
            : 'border-transparent text-foreground/85 hover:border-border hover:bg-accent/70'}"
          onclick={() => selectProvider(provider)}
        >
          <div class="flex min-w-0 items-center gap-2.5">
            <span
              class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-muted text-xs font-semibold text-foreground/80"
            >
              {provider.name.slice(0, 1).toUpperCase() || "P"}
            </span>
            <div class="min-w-0">
              <div class="truncate font-medium">{provider.name}</div>
              <div class="truncate font-mono text-[10px] text-muted-foreground">
                {provider.id} · {provider.models?.length ?? 0} 个模型
              </div>
            </div>
          </div>
          <span
            class="h-2 w-2 shrink-0 rounded-full {testStatus === 'success'
              ? 'bg-emerald-500'
              : testStatus === 'error'
                ? 'bg-destructive'
                : 'bg-muted-foreground/30'}"
            title={providerTestResults[provider.id]?.message || "就绪"}
          ></span>
        </button>
      {/each}

      <button
        type="button"
        class="flex min-h-[68px] w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-border/80 px-3 py-2.5 text-xs font-medium text-muted-foreground transition-all hover:border-primary hover:bg-primary/5 hover:text-primary"
        onclick={openNewProvider}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M12 5v14M5 12h14" />
        </svg>
        <span>添加 Provider</span>
      </button>
    </div>
  </div>

  <!-- Selected provider detail -->
  <div class="rounded-2xl border bg-card p-6 shadow-xs">
    {#if isCodexSelected && codexSubscriptionProvider}
      <!-- Special view for Codex Subscription Provider -->
      <div class="space-y-6">
        <div class="border-b pb-4">
          <h3 class="text-base font-semibold">OpenAI / ChatGPT 订阅</h3>
          <p class="mt-1 text-xs text-muted-foreground">
            通过 ChatGPT 账户登录，无需配置 API Key 或 Base
            URL。登录后模型将自动同步并在聊天中可用。
          </p>
        </div>

        <div
          class="rounded-xl border border-primary/20 bg-primary/5 p-4 flex items-center justify-between"
        >
          <div class="space-y-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-semibold">账号授权状态</span>
              <span
                class="rounded-full px-2 py-0.5 text-[10px] {codexSubscriptionLoggedIn
                  ? 'bg-emerald-500/10 text-emerald-600'
                  : 'bg-muted text-muted-foreground'}"
              >
                {codexSubscriptionLoggedIn ? "已登录" : "未登录"}
              </span>
            </div>
            {#if codexAuth?.subscription_account}
              <p class="text-xs text-muted-foreground">
                登录用户: {codexAuth.subscription_account}
              </p>
            {/if}
          </div>
          <div>
            {#if codexSubscriptionLoggedIn}
              <button
                type="button"
                class="rounded-lg border px-3 py-1.5 text-xs font-medium hover:bg-destructive/10 hover:text-destructive transition-colors"
                onclick={onLogoutCodexSubscription}
              >
                退出登录
              </button>
            {:else}
              <button
                type="button"
                class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground hover:opacity-90 transition-opacity"
                disabled={codexLoginLoading}
                onclick={onOpenCodexLogin}
              >
                {codexLoginLoading ? "正在登录..." : "登录 ChatGPT 订阅"}
              </button>
            {/if}
          </div>
        </div>

        {#if codexSubscriptionLoggedIn}
          <!-- 额度与限制看板 (Rate Limits & Quota) -->
          <div class="rounded-xl border bg-card/60 p-4 space-y-4">
            <div class="flex items-center justify-between border-b pb-2.5">
              <div class="flex items-center gap-2">
                <span class="text-xs font-semibold text-foreground">使用配额与限制</span>
                {#if effectiveRateLimits?.plan_type}
                  <span
                    class="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary uppercase"
                  >
                    {effectiveRateLimits.plan_type}
                  </span>
                {/if}
              </div>
              <button
                type="button"
                class="flex items-center gap-1.5 text-[11px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                disabled={rateLimitsLoading}
                onclick={refreshRateLimits}
              >
                <svg
                  class="h-3 w-3 {rateLimitsLoading ? 'animate-spin' : ''}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                  <path d="M3 3v5h5" />
                  <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
                  <path d="M16 21h5v-5" />
                </svg>
                <span>{rateLimitsLoading ? "正在刷新..." : "刷新额度"}</span>
              </button>
            </div>

            {#if effectiveRateLimits && (effectiveRateLimits.primary || effectiveRateLimits.secondary)}
              <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <!-- 5小时短期滑动窗口 -->
                {#if effectiveRateLimits.primary}
                  {@const p = effectiveRateLimits.primary}
                  <div class="rounded-lg border bg-background/80 p-3 space-y-2">
                    <div class="flex items-center justify-between">
                      <div class="flex items-center gap-1.5">
                        <span class="text-amber-500 text-xs">⚡</span>
                        <span class="text-xs font-medium">5 小时额度</span>
                      </div>
                      <span
                        class="text-[11px] font-semibold {getProgressTextColor(p.used_percent)}"
                      >
                        {p.used_percent.toFixed(1)}% 已用
                      </span>
                    </div>
                    <!-- Progress Bar -->
                    <div class="h-2 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        class="h-full rounded-full transition-all duration-300 {getProgressColor(
                          p.used_percent,
                        )}"
                        style="width: {Math.min(100, Math.max(0, p.used_percent))}%"
                      ></div>
                    </div>
                    <div
                      class="flex items-center justify-between text-[10px] text-muted-foreground pt-0.5"
                    >
                      <span>剩余 {(100 - p.used_percent).toFixed(1)}%</span>
                      {#if p.resets_at}
                        <span>重置: {formatResetCountdown(p.resets_at)}</span>
                      {/if}
                    </div>
                  </div>
                {/if}

                <!-- 每周长期配额 -->
                {#if effectiveRateLimits.secondary}
                  {@const s = effectiveRateLimits.secondary}
                  <div class="rounded-lg border bg-background/80 p-3 space-y-2">
                    <div class="flex items-center justify-between">
                      <div class="flex items-center gap-1.5">
                        <span class="text-sky-500 text-xs">📅</span>
                        <span class="text-xs font-medium">每周额度</span>
                      </div>
                      <span
                        class="text-[11px] font-semibold {getProgressTextColor(s.used_percent)}"
                      >
                        {s.used_percent.toFixed(1)}% 已用
                      </span>
                    </div>
                    <!-- Progress Bar -->
                    <div class="h-2 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        class="h-full rounded-full transition-all duration-300 {getProgressColor(
                          s.used_percent,
                        )}"
                        style="width: {Math.min(100, Math.max(0, s.used_percent))}%"
                      ></div>
                    </div>
                    <div
                      class="flex items-center justify-between text-[10px] text-muted-foreground pt-0.5"
                    >
                      <span>剩余 {(100 - s.used_percent).toFixed(1)}%</span>
                      {#if s.resets_at}
                        <span>重置: {formatResetDate(s.resets_at)}</span>
                      {/if}
                    </div>
                  </div>
                {/if}
              </div>
            {:else if rateLimitsLoading}
              <div
                class="py-6 text-center text-xs text-muted-foreground flex items-center justify-center gap-2"
              >
                <div
                  class="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"
                ></div>
                <span>正在获取实时额度…</span>
              </div>
            {:else if effectiveRateLimits}
              <div
                class="rounded-lg border bg-background/50 p-3 text-xs text-muted-foreground space-y-1.5"
              >
                <div class="flex items-center justify-between">
                  <span class="font-medium text-foreground">
                    已连接 ChatGPT {effectiveRateLimits.plan_type
                      ? effectiveRateLimits.plan_type.toUpperCase()
                      : "订阅"} 账号
                  </span>
                  <button
                    type="button"
                    class="rounded px-2 py-1 text-[11px] font-medium text-primary hover:underline"
                    onclick={refreshRateLimits}
                  >
                    重新检测
                  </button>
                </div>
                <p class="text-[11px]">
                  当前暂未查询到限流窗口（可能当前模型无需限流或额度已完全恢复）。
                </p>
              </div>
            {:else}
              <div
                class="flex items-center justify-between rounded-lg border bg-background/50 p-3 text-xs text-muted-foreground"
              >
                <span>点击“刷新额度”查看实时 5 小时与每周额度</span>
                <button
                  type="button"
                  class="rounded px-2 py-1 text-[11px] font-medium text-primary hover:underline"
                  onclick={refreshRateLimits}
                >
                  立即获取
                </button>
              </div>
            {/if}
          </div>
        {/if}

        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-semibold">已同步模型目录</span>
            <span class="text-xs text-muted-foreground"
              >{codexSubscriptionProvider.models?.length ?? 0} 个模型</span
            >
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            {#each codexSubscriptionProvider.models ?? [] as m (m.id)}
              <div class="rounded-lg border bg-background px-3 py-2 text-xs">
                <div class="font-medium truncate">{m.name ?? m.id}</div>
                <div class="font-mono text-[10px] text-muted-foreground truncate">{m.id}</div>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {:else}
      <!-- Form for Creating or Editing a Provider (Directly matching Figure 1) -->
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void saveProvider();
        }}
        class="space-y-5"
      >
        <!-- Title & Subtitle -->
        <div class="border-b pb-4">
          <h3 class="text-base font-semibold">
            {isCreatingNew ? "添加模型供应商" : `编辑模型供应商 · ${name || "未命名"}`}
          </h3>
          <p class="mt-1 text-xs text-muted-foreground">
            配置一个完全自定义的 API 端点和初始模型，配置后可在聊天时直接选择使用。
          </p>
        </div>

        {#if error}
          <div
            class="rounded-xl border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive"
          >
            {error}
          </div>
        {/if}

        <!-- Field 1: 名称 -->
        <div class="space-y-1.5">
          <label for="provider-name" class="block text-xs font-medium text-foreground">
            名称
          </label>
          <input
            id="provider-name"
            type="text"
            class="w-full rounded-lg border bg-background px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-hidden"
            placeholder="例如: NewAPI / DeepSeek / 智谱"
            value={name}
            oninput={handleNameInput}
          />
        </div>

        <!-- Field 2: 供应商标识 (Provider ID) -->
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="provider-slug" class="block text-xs font-medium text-foreground">
              供应商标识 (Provider ID)
            </label>
            <span class="text-[10px] text-muted-foreground"
              >用于对话中如 <code class="font-mono">{providerSlug || "provider-id"}/model-name</code
              > 标识</span
            >
          </div>
          <input
            id="provider-slug"
            type="text"
            class="w-full rounded-lg border bg-background px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-hidden"
            placeholder="newapi"
            bind:value={providerSlug}
          />
        </div>

        <!-- Field 3: Base URL -->
        <div class="space-y-1.5">
          <label for="provider-url" class="block text-xs font-medium text-foreground">
            Base URL
          </label>
          <input
            id="provider-url"
            type="text"
            class="w-full rounded-lg border bg-background px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-hidden"
            placeholder="http://127.0.0.1:3000/v1 或 https://api.deepseek.com/v1"
            bind:value={baseUrl}
          />
        </div>

        <!-- Field 4: API Key -->
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="provider-key" class="block text-xs font-medium text-foreground">
              API Key
            </label>
            <label class="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer">
              <input type="checkbox" bind:checked={keyless} class="rounded border" />
              <span>无需 API Key (本地网关/Keyless)</span>
            </label>
          </div>
          {#if !keyless}
            <div class="relative">
              <input
                id="provider-key"
                type={showApiKey ? "text" : "password"}
                class="w-full rounded-lg border bg-background px-3 py-2 pr-10 font-mono text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-hidden"
                placeholder="sk-••••••••••••••••••••••••••••••••"
                bind:value={apiKey}
              />
              <button
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                onclick={() => (showApiKey = !showApiKey)}
                tabindex="-1"
              >
                {#if showApiKey}
                  <svg
                    class="h-4 w-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path
                      d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24M1 1l22 22"
                    /></svg
                  >
                {:else}
                  <svg
                    class="h-4 w-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle
                      cx="12"
                      cy="12"
                      r="3"
                    /></svg
                  >
                {/if}
              </button>
            </div>
          {/if}
        </div>

        <!-- Field 5: API 格式 -->
        <div class="space-y-1.5">
          <label for="provider-format" class="block text-xs font-medium text-foreground">
            API 格式
          </label>
          <select
            id="provider-format"
            class="w-full rounded-lg border bg-background px-3 py-2 text-xs text-foreground focus:border-primary focus:outline-hidden"
            bind:value={protocol}
          >
            <option value="openai-completions">Chat Completions (/chat/completions)</option>
            <option value="openai-responses">OpenAI Responses (/responses)</option>
            <option value="anthropic-messages">Anthropic Messages (/v1/messages)</option>
          </select>
        </div>

        <!-- Field 6: 模型列表 (Figure 1: 模型列表) -->
        <div class="space-y-2 pt-1">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2">
              <label class="block text-xs font-medium text-foreground"> 模型列表 </label>
              <span
                class="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground font-medium"
              >
                {models.length} 个
              </span>
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors disabled:opacity-50"
                disabled={listing || !baseUrl}
                onclick={fetchModels}
                title="自动探测 /models 端点获取可用模型"
              >
                {#if listing}
                  <span
                    class="h-3 w-3 animate-spin rounded-full border-2 border-primary border-t-transparent"
                  ></span>
                  <span>探测中...</span>
                {:else}
                  <svg
                    class="h-3.5 w-3.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                      d="M3 3v5h5"
                    /></svg
                  >
                  <span>探测拉取模型</span>
                {/if}
              </button>
              <button
                type="button"
                class="flex items-center gap-1 rounded-md border border-primary/40 bg-primary/5 px-2.5 py-1 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
                onclick={openAddModelModal}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"><path d="M12 5v14M5 12h14" /></svg
                >
                <span>添加模型</span>
              </button>
            </div>
          </div>

          {#if models.length === 0}
            <!-- Empty state matching Figure 1 -->
            <div
              class="rounded-xl border border-dashed border-border/80 bg-muted/20 px-4 py-8 text-center space-y-3"
            >
              <div class="flex items-center justify-center text-muted-foreground/60">
                <svg
                  class="h-6 w-6"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                >
                  <circle cx="12" cy="12" r="10" /><path d="M12 16v-4M12 8h.01" />
                </svg>
              </div>
              <p class="text-xs text-muted-foreground">
                当前没有配置模型，添加模型后可在聊天中使用。
              </p>
              <button
                type="button"
                class="inline-flex items-center gap-1.5 rounded-lg border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent transition-colors"
                onclick={openAddModelModal}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"><path d="M12 5v14M5 12h14" /></svg
                >
                <span>添加模型</span>
              </button>
            </div>
          {:else}
            <!-- Models cards list -->
            <div class="space-y-2 max-h-72 overflow-y-auto pr-1">
              {#each models as model, index (index)}
                <div
                  class="flex items-start justify-between gap-3 rounded-xl border bg-background p-3 text-xs hover:border-foreground/20 transition-all"
                >
                  <div class="min-w-0 flex-1 space-y-1.5">
                    <div class="flex items-center gap-2">
                      <span class="font-mono font-medium text-foreground text-xs truncate">
                        {model.id}
                      </span>
                      {#if model.name}
                        <span class="text-muted-foreground text-[11px] truncate">
                          ({model.name})
                        </span>
                      {/if}
                    </div>
                    <div
                      class="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground"
                    >
                      {#if model.context_window}
                        <span class="rounded bg-muted/60 px-1.5 py-0.5">
                          上下文: {model.context_window.toLocaleString()}
                        </span>
                      {/if}
                      {#if model.max_tokens}
                        <span class="rounded bg-muted/60 px-1.5 py-0.5">
                          最大输出: {model.max_tokens.toLocaleString()}
                        </span>
                      {/if}
                      {#if model.supports_images}
                        <span
                          class="rounded bg-sky-500/10 text-sky-600 dark:text-sky-400 px-1.5 py-0.5 font-medium"
                        >
                          支持图片
                        </span>
                      {/if}
                      {#if model.supports_reasoning}
                        <span
                          class="rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 px-1.5 py-0.5 font-medium"
                        >
                          推理: {formatEffortSummary(model)}
                        </span>
                      {/if}
                    </div>
                  </div>
                  <div class="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      class="h-7 px-2.5 rounded-lg border text-[11px] font-medium text-foreground hover:bg-accent transition-colors"
                      onclick={() => openEditModelModal(index)}
                    >
                      编辑
                    </button>
                    <button
                      type="button"
                      class="h-7 w-7 rounded-lg text-muted-foreground hover:bg-destructive/10 hover:text-destructive flex items-center justify-center transition-colors"
                      onclick={() => removeModel(index)}
                      title="删除模型"
                    >
                      <svg
                        class="h-3.5 w-3.5"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg
                      >
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Add / Edit Model inline panel -->
        {#if showModelModal}
          <div
            class="rounded-xl border border-primary/20 bg-primary/[0.03] p-4 shadow-xs animate-fade-in"
          >
            <div class="mb-3 flex items-center justify-between">
              <div>
                <h3 class="text-sm font-semibold text-foreground">
                  {editingModelIndex !== null ? "编辑模型" : "添加模型"}
                </h3>
                <p class="mt-0.5 text-[11px] text-muted-foreground">
                  模型能力和参数会显示在当前 Provider 的模型卡片中。
                </p>
              </div>
              <button
                type="button"
                class="rounded-lg px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                onclick={() => (showModelModal = false)}
              >
                取消
              </button>
            </div>

            <div class="space-y-3.5">
              <div>
                <label class="text-xs font-medium text-foreground/80 mb-1 block"> 模型 ID </label>
                <input
                  type="text"
                  class="w-full rounded-xl border bg-background px-3.5 py-2 text-xs font-mono focus:border-primary focus:outline-hidden"
                  placeholder="模型 ID (例如: GLM-5.3-Flash)"
                  bind:value={modalDraft.id}
                />
              </div>

              <div>
                <label class="text-xs font-medium text-foreground/80 mb-1 block">
                  显示别名 (可选)
                </label>
                <input
                  type="text"
                  class="w-full rounded-xl border bg-background px-3.5 py-2 text-xs focus:border-primary focus:outline-hidden"
                  placeholder="显示别名 (可选)"
                  bind:value={modalDraft.name}
                />
              </div>

              <div>
                <label class="text-xs font-medium text-foreground/80 mb-1 block">
                  上下文窗口 (可选)
                </label>
                <input
                  type="number"
                  class="w-full rounded-xl border bg-background px-3.5 py-2 text-xs focus:border-primary focus:outline-hidden"
                  placeholder="留空使用默认 (例如: 128000)"
                  bind:value={modalDraft.context_window}
                />
              </div>

              <div>
                <label class="text-xs font-medium text-foreground/80 mb-1 block">
                  最大输出 Token (可选)
                </label>
                <input
                  type="number"
                  class="w-full rounded-xl border bg-background px-3.5 py-2 text-xs focus:border-primary focus:outline-hidden"
                  placeholder="留空使用默认 (例如: 16384)"
                  bind:value={modalDraft.max_tokens}
                />
              </div>

              <div>
                <label class="text-xs font-medium text-foreground/80 mb-1.5 block">
                  输入类型
                </label>
                <div class="flex items-center gap-2">
                  <div
                    class="inline-flex items-center gap-1.5 rounded-lg border border-border bg-muted/40 px-3 py-1.5 text-xs select-none opacity-80 cursor-not-allowed"
                    title="文本输入为默认锁定项"
                  >
                    <input type="checkbox" checked disabled class="accent-primary" />
                    <span>文本</span>
                    <svg
                      class="h-3 w-3 text-muted-foreground"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      ><rect width="18" height="11" x="3" y="11" rx="2" ry="2" /><path
                        d="M7 11V7a5 5 0 0 1 10 0v4"
                      /></svg
                    >
                  </div>
                  <label
                    class="inline-flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs cursor-pointer hover:bg-accent transition-colors"
                  >
                    <input
                      type="checkbox"
                      bind:checked={modalDraft.supports_images}
                      class="accent-primary cursor-pointer"
                    />
                    <span>图片</span>
                  </label>
                </div>
              </div>

              <div class="space-y-2 rounded-xl border bg-muted/15 p-3">
                <label
                  class="flex items-center gap-2 cursor-pointer text-xs font-medium text-foreground select-none"
                >
                  <input
                    type="checkbox"
                    bind:checked={modalDraft.supports_reasoning}
                    class="accent-primary cursor-pointer"
                  />
                  <span>推理 / 思考能力</span>
                </label>
                {#if modalDraft.supports_reasoning}
                  <div class="pt-2 border-t border-border/40 space-y-2 animate-fade-in">
                    <div class="text-[11px] text-muted-foreground">
                      支持档位（未勾选的档位将被禁用）：
                    </div>
                    <div class="flex flex-wrap gap-2">
                      {#each MODEL_EFFORT_OPTIONS as option}
                        <label
                          class="inline-flex items-center gap-1.5 rounded-lg border bg-background px-2.5 py-1 text-xs cursor-pointer hover:bg-accent transition-colors"
                        >
                          <input
                            type="checkbox"
                            checked={modalDraft.supported_effort_levels.includes(option.value)}
                            onchange={() => toggleModalEffort(option.value)}
                            class="accent-primary cursor-pointer"
                          />
                          <span>{option.label}</span>
                        </label>
                      {/each}
                    </div>
                    {#if modalDraft.supported_effort_levels.includes("off")}
                      <div class="text-[11px] text-amber-600 dark:text-amber-400">
                        提示：部分模型可能不支持“关闭”思考，选择后请求可能失败。
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
            <div class="mt-4 flex items-center justify-end gap-2.5 border-t pt-3">
              <button
                type="button"
                class="rounded-lg border px-4 py-2 text-xs font-medium transition-colors hover:bg-accent"
                onclick={() => (showModelModal = false)}
              >
                取消
              </button>
              <button
                type="button"
                class="rounded-lg bg-primary px-5 py-2 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-40"
                disabled={!modalDraft.id.trim()}
                onclick={saveModelModal}
              >
                保存模型
              </button>
            </div>
          </div>
        {/if}

        <!-- Bottom Footer Bar (Matching Figure 1) -->
        <div class="flex flex-wrap items-center justify-between gap-3 pt-4 border-t">
          <div class="flex flex-wrap items-center gap-2">
            {#if availableTestModels.length > 0}
              <div class="flex items-center gap-1.5">
                <span class="text-xs text-muted-foreground">测试模型:</span>
                <select
                  class="rounded-lg border bg-background px-2.5 py-1.5 text-xs text-foreground focus:border-primary focus:outline-hidden max-w-[200px] truncate"
                  bind:value={testModel}
                  onchange={() => {
                    connectionMessage = "";
                    connectionResult = null;
                  }}
                  disabled={testing}
                  title="选择用于测试连接的模型"
                >
                  {#each availableTestModels as m}
                    <option value={m.id}>{m.name?.trim() ? `${m.name} (${m.id})` : m.id}</option>
                  {/each}
                </select>
              </div>
            {/if}

            <button
              type="button"
              class="rounded-lg border px-3.5 py-2 text-xs font-medium text-foreground hover:bg-accent transition-colors disabled:opacity-50"
              disabled={testing || !baseUrl}
              onclick={testConnection}
            >
              {testing ? "正在测试..." : "测试连接"}
            </button>

            {#if connectionMessage}
              <span
                class="text-xs {connectionResult === 'success'
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-destructive'}"
              >
                {connectionMessage}
              </span>
            {/if}

            {#if !isCreatingNew && selectedId}
              {@const currProvider = selectedProvider}
              {#if currProvider}
                <button
                  type="button"
                  class="rounded-lg px-3 py-2 text-xs font-medium text-destructive hover:bg-destructive/10 transition-colors"
                  onclick={() => removeProvider(currProvider)}
                >
                  删除供应商
                </button>
              {/if}
            {/if}
          </div>

          <div class="flex items-center gap-3">
            {#if error}
              <span
                class="text-xs text-destructive flex items-center gap-1 animate-fade-in font-medium"
              >
                {error}
              </span>
            {/if}
            {#if savedFeedback}
              <span
                class="text-xs text-emerald-600 dark:text-emerald-400 font-medium flex items-center gap-1 animate-fade-in"
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
                已成功保存修改
              </span>
            {/if}
            {#if models.length === 0}
              <span class="text-xs text-muted-foreground">
                ⓘ 添加供应商前，请至少添加一个模型。
              </span>
            {/if}
            <button
              type="submit"
              class="rounded-lg px-5 py-2 text-xs font-medium transition-all flex items-center gap-1.5 disabled:opacity-50 {savedFeedback
                ? 'bg-emerald-600 hover:bg-emerald-700 text-white shadow-xs'
                : 'bg-primary text-primary-foreground hover:opacity-90'}"
              disabled={saving || models.length === 0}
            >
              {#if saving}
                <span
                  class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-current border-t-transparent"
                ></span>
                <span>正在保存...</span>
              {:else if savedFeedback}
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
                <span>已保存</span>
              {:else}
                <span>{isCreatingNew ? "添加供应商" : "保存修改"}</span>
              {/if}
            </button>
          </div>
        </div>
      </form>
    {/if}
  </div>
</div>
