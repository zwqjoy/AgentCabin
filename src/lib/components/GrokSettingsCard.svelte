<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import AuthModeSelector from "$lib/components/AuthModeSelector.svelte";
  import { getGrokStatus, type GrokStatus } from "$lib/grok-api";
  import type { AgentSettings, UserSettings, AgentProviderBindings } from "$lib/types";
  import { loadGrokModels } from "$lib/stores/cli-info.svelte";
  import { t } from "$lib/i18n/index.svelte";

  let status = $state<GrokStatus | null>(null);
  let agentSettings = $state<AgentSettings | null>(null);
  let userSettings = $state<UserSettings | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state("");
  let modelDraft = $state("");
  let copied = $state(false);

  let grokAuthMode = $state<"cli" | "app">("cli");
  let authModeInitDone = false;

  $effect(() => {
    if (!userSettings || authModeInitDone) return;
    authModeInitDone = true;
    const binding = userSettings.agent_provider_bindings?.grok;
    if (binding?.mode === "custom") grokAuthMode = "app";
  });

  function setGrokAuthMode(mode: "cli" | "app") {
    grokAuthMode = mode;
    if (mode === "cli") void saveGrokBinding("cli");
  }

  async function saveGrokBinding(
    mode: "cli" | "custom",
    providerId?: string,
    models?: string[],
    defaultModel?: string,
  ): Promise<boolean> {
    if (!userSettings) return false;
    try {
      const current: AgentProviderBindings = userSettings.agent_provider_bindings ?? {
        claude: { mode: "cli" },
        codex: { mode: "cli" },
        pi: { mode: "cli" },
        grok: { mode: "cli" },
      };
      const next: AgentProviderBindings = {
        ...current,
        grok: {
          mode,
          provider_id: mode === "custom" ? providerId : undefined,
          models: mode === "custom" ? models : undefined,
          model: mode === "custom" ? defaultModel : undefined,
        },
      };
      userSettings = await api.updateUserSettings({ agent_provider_bindings: next });
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  async function refresh() {
    if (loading) return;
    loading = true;
    error = "";
    try {
      const [nextStatus, nextSettings, nextUserSettings] = await Promise.all([
        getGrokStatus(),
        api.getAgentSettings("grok"),
        api.getUserSettings(),
      ]);
      status = nextStatus;
      agentSettings = nextSettings;
      userSettings = nextUserSettings;
      modelDraft = nextSettings.model ?? "";
      if (nextStatus.authenticated) void loadGrokModels(true);
    } catch (e) {
      error = String(e);
      status = null;
    } finally {
      loading = false;
    }
  }

  async function saveModel() {
    if (!agentSettings || saving) return;
    saving = true;
    error = "";
    try {
      agentSettings = await api.updateAgentSettings("grok", {
        model: modelDraft.trim() || null,
      } as Partial<AgentSettings>);
      modelDraft = agentSettings.model ?? "";
      saved = true;
      setTimeout(() => (saved = false), 1500);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function copyLoginCommand() {
    await navigator.clipboard.writeText("grok login");
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  onMount(() => {
    void refresh();
  });
</script>

<Card class="p-6 space-y-5">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">Grok</h2>
      <p class="mt-1 text-xs text-muted-foreground">
        通过 ACP (<code class="font-mono">grok agent stdio</code>) 运行。
        {#if grokAuthMode === "cli"}
          模型和认证使用 Grok 的原生配置。
        {:else}
          Provider、认证和模型由 AgentCabin 统一管理。
        {/if}
      </p>
    </div>
    <button
      class="text-xs text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
      disabled={loading}
      onclick={() => void refresh()}
    >
      {loading ? t("settings_pi_checking") : t("settings_codex_refresh")}
    </button>
  </div>

  {#if error}
    <div class="rounded-lg border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-500">
      {error}
    </div>
  {/if}

  {#if loading && !status}
    <div class="flex items-center gap-2 text-sm text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"
      ></span>
      {t("settings_pi_checking")}
    </div>
  {:else if status}
    <!-- Auth Mode selector: CLI (grok login) vs App (global provider) -->
    <AuthModeSelector
      value={grokAuthMode === "cli" ? "cli" : "app"}
      onModeChange={(mode) => setGrokAuthMode(mode)}
    />

    {#if grokAuthMode === "cli" && status.found && !status.authenticated}
      <div class="rounded-lg border border-amber-500/30 bg-amber-500/5 p-4">
        <div class="flex items-center justify-between gap-4">
          <div class="min-w-0">
            <p class="text-sm font-medium">需要完成 Grok 登录</p>
            <p class="mt-1 text-xs text-muted-foreground">
              在终端运行 <code class="font-mono">grok login</code>，完成后点击刷新。
            </p>
            {#if status.authError}
              <p class="mt-2 break-words text-xs text-amber-500">{status.authError}</p>
            {/if}
          </div>
          <Button variant="outline" size="sm" onclick={() => void copyLoginCommand()}>
            {copied ? "已复制" : "复制登录命令"}
          </Button>
        </div>
      </div>
    {/if}

    {#if status.found && grokAuthMode === "cli"}
      <div class="space-y-3 border-t border-border/60 pt-4">
        <div>
          <p class="text-sm font-medium">{t("settings_pi_model")}</p>
          <p class="mt-1 text-xs text-muted-foreground">
            留空时由 Grok 使用自己的默认模型{status.currentModel
              ? `（当前 ${status.currentModel}）`
              : ""}。AgentCabin 的选择只覆盖会话模型，不复制 Provider 凭据。
          </p>
        </div>

        <div class="flex items-center gap-2">
          <input
            class="min-w-0 flex-1 rounded-md border bg-background px-3 py-2 font-mono text-sm"
            bind:value={modelDraft}
            list={status.models.length > 0 ? "grok-model-options" : undefined}
            placeholder={status.currentModel ?? "Grok default"}
            onkeydown={(event) => {
              if (event.key === "Enter") void saveModel();
            }}
          />
          {#if status.models.length > 0}
            <datalist id="grok-model-options">
              {#each status.models as model (model.value)}
                <option value={model.value}>{model.displayName}</option>
              {/each}
            </datalist>
          {/if}
          <Button size="sm" disabled={saving} onclick={() => void saveModel()}>
            {saving ? "保存中…" : t("settings_saveSettings")}
          </Button>
        </div>

        <div class="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span>
            {status.models.length > 0
              ? `Grok 返回 ${status.models.length} 个可用模型`
              : "暂无模型目录；仍可直接输入 Grok 已配置的模型 ID"}
          </span>
          {#if saved}
            <span
              class="inline-flex items-center gap-1.5 rounded-md border border-emerald-500/35 bg-emerald-500/10 px-2.5 py-1.5 font-medium text-emerald-700 animate-fade-in dark:text-emerald-300"
              role="status"
              aria-live="polite"
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"><path d="m5 12 4 4L19 6" /></svg
              >
              {t("settings_saveSuccess")}
            </span>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</Card>
