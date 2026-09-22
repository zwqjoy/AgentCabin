<script lang="ts">
  import * as api from "$lib/api";
  import type { CodexFeature } from "$lib/api";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { t } from "$lib/i18n/index.svelte";

  type Props = {
    config: Record<string, unknown>;
    projectConfig?: Record<string, unknown>;
  };

  let { config, projectConfig = {} }: Props = $props();
  let localConfig = $state<Record<string, unknown>>({});
  let localProjectConfig = $state<Record<string, unknown>>({});
  let runtimeFeatures = $state<CodexFeature[]>([]);
  let runtimeLoading = $state(false);
  let savingName = $state<string | null>(null);
  let error = $state("");

  $effect(() => {
    localConfig = config;
    localProjectConfig = projectConfig;
  });

  // Reuse the last Codex run when available so settings can show the installed
  // CLI's feature catalog without putting low-level flags in the chat surface.
  $effect(() => {
    if (runtimeLoading || runtimeFeatures.length > 0 || typeof window === "undefined") return;
    const runId = localStorage.getItem("agentcabin:last-codex-run-id");
    if (!runId) return;
    runtimeLoading = true;
    void api
      .listCodexFeatures(runId)
      .then((result) => {
        runtimeFeatures = result.data ?? [];
      })
      .catch(() => {
        // A session may have ended while settings was opening. Config remains valid.
      })
      .finally(() => {
        runtimeLoading = false;
      });
  });

  const FEATURE_COPY: Record<string, { label: string; description: string }> = {
    network_proxy: {
      label: "settings_codexFeature_networkProxyLabel",
      description: "settings_codexFeature_networkProxyDesc",
    },
    prevent_sleep_while_running: {
      label: "settings_codexFeature_preventSleepLabel",
      description: "settings_codexFeature_preventSleepDesc",
    },
    shell_tool: {
      label: "settings_codexFeature_shellToolLabel",
      description: "settings_codexFeature_shellToolDesc",
    },
    secret_auth_storage: {
      label: "settings_codexFeature_secretAuthLabel",
      description: "settings_codexFeature_secretAuthDesc",
    },
    unified_exec: {
      label: "settings_codexFeature_unifiedExecLabel",
      description: "settings_codexFeature_unifiedExecDesc",
    },
    shell_snapshot: {
      label: "settings_codexFeature_shellSnapshotLabel",
      description: "settings_codexFeature_shellSnapshotDesc",
    },
    code_mode_host: {
      label: "settings_codexFeature_codeModeHostLabel",
      description: "settings_codexFeature_codeModeHostDesc",
    },
  };

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  function featureTable(value: unknown): Record<string, boolean> {
    if (!isRecord(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter(([, enabled]) => typeof enabled === "boolean"),
    ) as Record<string, boolean>;
  }

  let userFeatures = $derived(featureTable(localConfig.features));
  let projectFeatures = $derived(featureTable(localProjectConfig.features));
  let featureNames = $derived(
    [
      ...new Set([
        ...runtimeFeatures.map((feature) => feature.name),
        ...Object.keys(userFeatures),
        ...Object.keys(projectFeatures),
      ]),
    ].sort(),
  );

  function runtimeFeature(name: string): CodexFeature | undefined {
    return runtimeFeatures.find((feature) => feature.name === name);
  }

  function copyFor(name: string) {
    return FEATURE_COPY[name];
  }

  function featureLabel(name: string): string {
    const runtime = runtimeFeature(name);
    if (runtime?.displayName) return runtime.displayName;
    const copy = copyFor(name);
    return copy ? t(copy.label as Parameters<typeof t>[0]) : name;
  }

  function featureDescription(name: string): string {
    const runtime = runtimeFeature(name);
    if (runtime?.description) return runtime.description;
    const copy = copyFor(name);
    return copy
      ? t(copy.description as Parameters<typeof t>[0])
      : t("settings_codexFeature_unknownDesc");
  }

  function isToggleable(name: string): boolean {
    const stage = runtimeFeature(name)?.stage;
    return !stage || stage === "beta" || stage === "stable";
  }

  function featureEnabled(name: string): boolean {
    if (name in projectFeatures) return projectFeatures[name];
    if (name in userFeatures) return userFeatures[name];
    return runtimeFeature(name)?.enabled ?? false;
  }

  function patchLocalFeature(name: string, enabled: boolean | null) {
    const nextFeatures = { ...userFeatures };
    if (enabled === null) delete nextFeatures[name];
    else nextFeatures[name] = enabled;
    localConfig = { ...localConfig, features: nextFeatures };
  }

  async function updateFeature(name: string, enabled: boolean | null) {
    if (name in projectFeatures) return;
    savingName = name;
    error = "";
    try {
      dbg("features", "settings toggle", { name, enabled });
      await api.setCodexFeature(name, enabled);
      patchLocalFeature(name, enabled);
    } catch (e) {
      dbgWarn("features", "settings toggle failed", e);
      error = String(e);
    } finally {
      savingName = null;
    }
  }
</script>

<div class="ui-card p-5 space-y-4">
  <div class="flex items-start justify-between gap-4">
    <div>
      <h3 class="text-sm font-semibold">{t("settings_codexFeatures_title")}</h3>
      <p class="mt-1 max-w-2xl text-xs leading-relaxed text-muted-foreground">
        {t("settings_codexFeatures_desc")}
      </p>
    </div>
    <span
      class="shrink-0 rounded-full border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-500"
    >
      {t("settings_codexFeatures_advanced")}
    </span>
  </div>

  <div class="flex items-start gap-2 rounded-lg border border-border/70 bg-muted/30 px-3 py-2.5">
    <svg
      class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" />
    </svg>
    <p class="text-xs leading-relaxed text-muted-foreground">
      {t("settings_codexFeatures_hint")}
    </p>
  </div>

  {#if featureNames.length === 0}
    <div class="rounded-lg border border-dashed border-border px-4 py-5 text-center">
      <p class="text-sm font-medium text-foreground">{t("settings_codexFeatures_empty")}</p>
      <p class="mt-1 text-xs text-muted-foreground">{t("settings_codexFeatures_emptyDesc")}</p>
    </div>
  {:else}
    <div class="divide-y divide-border/60 rounded-lg border border-border/70">
      {#each featureNames as name (name)}
        {@const projectOverride = name in projectFeatures}
        {@const userOverride = name in userFeatures}
        {@const runtime = runtimeFeature(name)}
        {@const enabled = featureEnabled(name)}
        {@const toggleable = isToggleable(name)}
        <div class="flex items-start justify-between gap-4 px-3.5 py-3">
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <p class="text-sm font-medium text-foreground">{featureLabel(name)}</p>
              {#if projectOverride}
                <span
                  class="rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-500"
                >
                  {t("settings_codexFeatures_projectOverride")}
                </span>
              {:else if runtime?.stage && runtime.stage !== "stable"}
                <span
                  class="rounded border border-border bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                >
                  {runtime.stage === "beta" ? t("features_stageBeta") : runtime.stage}
                </span>
              {/if}
            </div>
            <p class="mt-1 text-xs leading-relaxed text-muted-foreground">
              {featureDescription(name)}
            </p>
            <code class="mt-1.5 inline-block text-[10px] text-muted-foreground/60">{name}</code>
          </div>
          <div class="flex shrink-0 items-center gap-2 pt-0.5">
            {#if userOverride && !projectOverride}
              <button
                type="button"
                class="rounded-md px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                disabled={savingName === name}
                onclick={() => updateFeature(name, null)}
              >
                {t("settings_codexFeatures_reset")}
              </button>
            {/if}
            <button
              type="button"
              role="switch"
              aria-checked={enabled}
              aria-label={featureLabel(name)}
              disabled={projectOverride || !toggleable || savingName === name}
              onclick={() => updateFeature(name, !enabled)}
              class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 {enabled
                ? 'bg-primary'
                : 'bg-muted-foreground/30'}"
            >
              <span
                class="inline-block h-4 w-4 rounded-full bg-white shadow transition-transform {enabled
                  ? 'translate-x-6'
                  : 'translate-x-1'}"
              ></span>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if error}
    <p
      class="rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive"
    >
      {error}
    </p>
  {/if}
</div>
