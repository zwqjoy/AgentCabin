<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Card from "$lib/components/Card.svelte";
  import { getGrokCliConfig, updateGrokCliConfig, type GrokCliConfig } from "$lib/grok-api";
  import { t } from "$lib/i18n/index.svelte";

  let config = $state<GrokCliConfig>({});
  let permissionMode = $state("ask");
  let compactThreshold = $state("85");
  let loading = $state(false);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state("");

  function normalizePermissionMode(value: string | undefined): string {
    if (value === "auto_all" || value === "bypassPermissions") return "always-approve";
    if (value === "auto_read" || value === "acceptEdits") return "ask";
    if (value === "auto") return "auto";
    if (value === "default" || value === "dontAsk" || value === "plan") return "ask";
    return value === "always-approve" ? value : "ask";
  }

  async function refresh() {
    if (loading) return;
    loading = true;
    error = "";
    try {
      const [nextConfig, nextAgentSettings] = await Promise.all([
        getGrokCliConfig(),
        api.getAgentSettings("grok"),
      ]);
      config = nextConfig;
      permissionMode = normalizePermissionMode(
        nextAgentSettings.permission_mode ?? nextConfig.permissionMode,
      );
      compactThreshold = String(nextConfig.autoCompactThresholdPercent ?? 85);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function saveConfig(patch: Partial<GrokCliConfig>): Promise<boolean> {
    if (saving) return false;
    saving = true;
    error = "";
    try {
      config = await updateGrokCliConfig(patch);
      saved = true;
      setTimeout(() => (saved = false), 1500);
      return true;
    } catch (e) {
      error = t("settings_grokConfig_saveError", { error: String(e) });
      return false;
    } finally {
      saving = false;
    }
  }

  async function savePermissionMode(value: string) {
    permissionMode = value;
    try {
      const configSaved = await saveConfig({ permissionMode: value });
      if (configSaved) {
        await api.updateAgentSettings("grok", { permission_mode: value });
      }
    } catch (e) {
      error = t("settings_grokConfig_saveError", { error: String(e) });
    }
  }

  function saveCompactThreshold() {
    const value = Number(compactThreshold.trim());
    if (!Number.isInteger(value) || value < 50 || value > 99) {
      error = t("settings_grokConfig_saveError", { error: "50–99" });
      compactThreshold = String(config.autoCompactThresholdPercent ?? 85);
      return;
    }
    void saveConfig({ autoCompactThresholdPercent: value });
  }

  onMount(() => {
    void refresh();
  });
</script>

{#if error}
  <div class="rounded-lg border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-500">
    {error}
  </div>
{/if}

{#if loading && Object.keys(config).length === 0}
  <Card class="p-6">
    <div class="flex items-center gap-2 text-sm text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"
      ></span>
      {t("settings_pi_checking")}
    </div>
  </Card>
{:else}
  <Card class="space-y-4 p-6">
    <div class="flex items-center justify-between gap-4">
      <div>
        <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          {t("settings_cliConfig_behavior")}
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">{t("settings_grokConfig_authHint")}</p>
      </div>
      {#if saved}
        <span class="text-xs text-emerald-500">{t("settings_grokConfig_saved")}</span>
      {/if}
    </div>

    <div class="flex items-center justify-between gap-4 py-1">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{t("settings_grokConfig_permissionMode")}</p>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {t("settings_grokConfig_permissionModeDesc")}
        </p>
      </div>
      <div class="flex shrink-0 gap-1.5">
        {#each [{ value: "ask", label: t("settings_grokConfig_optAsk") }, { value: "auto", label: t("settings_cliConfig_optAuto") }, { value: "always-approve", label: t("settings_grokConfig_optAlwaysApprove") }] as option (option.value)}
          <button
            class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
              {permissionMode === option.value
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            disabled={saving}
            onclick={() => void savePermissionMode(option.value)}
          >
            {option.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 border-t border-border/40 pt-3">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{t("settings_grokConfig_autoCompact")}</p>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {t("settings_grokConfig_autoCompactDesc")}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <input
          class="w-20 rounded-md border bg-transparent px-3 py-1.5 text-right font-mono text-sm"
          bind:value={compactThreshold}
          inputmode="numeric"
          aria-label={t("settings_grokConfig_autoCompact")}
          onblur={saveCompactThreshold}
          onkeydown={(event) => {
            if (event.key === "Enter") (event.target as HTMLInputElement).blur();
          }}
        />
        <span class="text-xs text-muted-foreground">%</span>
      </div>
    </div>

    {#each [{ key: "loadEnvrc", label: t("settings_grokConfig_loadEnvrc"), description: t("settings_grokConfig_loadEnvrcDesc"), value: config.loadEnvrc ?? true }, { key: "respectGitignore", label: t("settings_grokConfig_respectGitignore"), description: t("settings_grokConfig_respectGitignoreDesc"), value: config.respectGitignore ?? false }, { key: "showThinkingBlocks", label: t("settings_grokConfig_showThinking"), description: t("settings_grokConfig_showThinkingDesc"), value: config.showThinkingBlocks ?? true }, { key: "groupToolVerbs", label: t("settings_grokConfig_groupToolCalls"), description: t("settings_grokConfig_groupToolCallsDesc"), value: config.groupToolVerbs ?? true }, { key: "rememberToolApprovals", label: t("settings_grokConfig_rememberApprovals"), description: t("settings_grokConfig_rememberApprovalsDesc"), value: config.rememberToolApprovals ?? false }] as option (option.key)}
      <div class="flex items-center justify-between gap-4 border-t border-border/40 pt-3">
        <div class="min-w-0 flex-1">
          <p class="text-sm font-medium">{option.label}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{option.description}</p>
        </div>
        <button
          class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200
            {option.value ? 'bg-primary' : 'bg-neutral-700'}"
          aria-label={option.label}
          disabled={saving}
          onclick={() => void saveConfig({ [option.key]: !option.value } as Partial<GrokCliConfig>)}
        >
          <span
            class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200
              {option.value ? 'translate-x-6' : 'translate-x-1'}"
          ></span>
        </button>
      </div>
    {/each}
  </Card>

  <Card class="space-y-4 p-6">
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      {t("settings_cliConfig_appearance")}
    </h2>

    <div class="flex items-center justify-between gap-4 py-1">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{t("settings_grokConfig_screenMode")}</p>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {t("settings_grokConfig_screenModeDesc")}
        </p>
      </div>
      <div class="flex shrink-0 gap-1.5">
        {#each [{ value: "fullscreen", label: t("settings_grokConfig_optFullscreen") }, { value: "minimal", label: t("settings_grokConfig_optMinimal") }] as option (option.value)}
          <button
            class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
              {(config.screenMode ?? 'fullscreen') === option.value
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            disabled={saving}
            onclick={() => void saveConfig({ screenMode: option.value })}
          >
            {option.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 border-t border-border/40 pt-3">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{t("settings_grokConfig_inputMode")}</p>
        <p class="mt-0.5 text-xs text-muted-foreground">{t("settings_grokConfig_inputModeDesc")}</p>
      </div>
      <div class="flex shrink-0 gap-1.5">
        {#each [{ value: true, label: t("settings_grokConfig_optReadline") }, { value: false, label: t("settings_grokConfig_optVim") }] as option (String(option.value))}
          <button
            class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
              {(config.simpleMode ?? true) === option.value
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            disabled={saving}
            onclick={() => void saveConfig({ simpleMode: option.value })}
          >
            {option.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 border-t border-border/40 pt-3">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{t("settings_grokConfig_vimScrollback")}</p>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {t("settings_grokConfig_vimScrollbackDesc")}
        </p>
      </div>
      <button
        class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200
          {(config.vimMode ?? false) ? 'bg-primary' : 'bg-neutral-700'}"
        aria-label={t("settings_grokConfig_vimScrollback")}
        disabled={saving}
        onclick={() => void saveConfig({ vimMode: !(config.vimMode ?? false) })}
      >
        <span
          class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200
            {(config.vimMode ?? false) ? 'translate-x-6' : 'translate-x-1'}"
        ></span>
      </button>
    </div>
  </Card>

  <Card class="space-y-4 p-6">
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      {t("settings_cliConfig_advanced")}
    </h2>

    {#each [{ label: t("settings_grokConfig_autoUpdate"), description: t("settings_grokConfig_autoUpdateDesc"), value: config.autoUpdate ?? true, key: "autoUpdate" }, { label: t("settings_grokConfig_telemetry"), description: t("settings_grokConfig_telemetryDesc"), value: config.telemetry ?? false, key: "telemetry" }, { label: t("settings_grokConfig_codebaseIndexing"), description: t("settings_grokConfig_codebaseIndexingDesc"), value: config.codebaseIndexing ?? true, key: "codebaseIndexing" }, { label: t("settings_grokConfig_remoteFetch"), description: t("settings_grokConfig_remoteFetchDesc"), value: config.remoteFetch ?? true, key: "remoteFetch" }] as option (option.key)}
      <div
        class="flex items-center justify-between gap-4 py-1 {option.key !== 'autoUpdate'
          ? 'border-t border-border/40 pt-3'
          : ''}"
      >
        <div class="min-w-0 flex-1">
          <p class="text-sm font-medium">{option.label}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{option.description}</p>
        </div>
        <button
          class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200
            {option.value ? 'bg-primary' : 'bg-neutral-700'}"
          aria-label={option.label}
          disabled={saving}
          onclick={() => void saveConfig({ [option.key]: !option.value } as Partial<GrokCliConfig>)}
        >
          <span
            class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200
              {option.value ? 'translate-x-6' : 'translate-x-1'}"
          ></span>
        </button>
      </div>
    {/each}
  </Card>
{/if}
