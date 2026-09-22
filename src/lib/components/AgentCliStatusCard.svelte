<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Card from "$lib/components/Card.svelte";
  import { getGrokStatus } from "$lib/grok-api";
  import { t } from "$lib/i18n/index.svelte";

  type Agent = "claude" | "codex" | "pi" | "grok" | "dsh";

  type CliStatus = {
    found: boolean;
    path?: string;
    version?: string;
    versionSupported?: boolean;
    minimumVersion?: string;
    configPath?: string;
  };

  let {
    agent,
    title,
    profileMode,
    configLabel,
    showConfig = true,
  }: {
    agent: Agent;
    title: string;
    profileMode?: "code" | "work";
    configLabel?: string;
    showConfig?: boolean;
  } = $props();
  let status = $state<CliStatus | null>(null);
  let loading = $state(false);
  let error = $state("");

  async function refresh() {
    if (loading) return;
    loading = true;
    error = "";
    try {
      if (agent === "grok") {
        const next = await getGrokStatus();
        status = {
          found: next.found,
          path: next.path,
          version: next.version,
          configPath: next.configPath,
        };
      } else {
        const next = await api.checkAgentCli(agent);
        const profile =
          agent === "pi" && profileMode ? await api.getPiProfileInfo(profileMode) : null;
        status = {
          found: next.found,
          path: next.path,
          version: next.version,
          versionSupported: next.version_supported,
          minimumVersion: next.minimum_version,
          configPath: profile?.settingsPath ?? next.config_path,
        };
      }
    } catch (e) {
      status = null;
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });
</script>

<Card class="space-y-4 p-5">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h3 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        {title}
      </h3>
      <p class="mt-1 text-xs text-muted-foreground">{t("settings_cliConfig_statusDesc")}</p>
    </div>
    <button
      class="shrink-0 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
      disabled={loading}
      onclick={() => void refresh()}
    >
      {loading ? t("settings_pi_checking") : t("settings_codex_refresh")}
    </button>
  </div>

  {#if error}
    <div class="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-500">
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
    <div class="grid gap-x-6 gap-y-4 rounded-lg border border-border/60 p-4 text-sm sm:grid-cols-2">
      <div>
        <span class="text-xs text-muted-foreground">{t("settings_pi_installStatus")}</span>
        <p
          class={status.found && status.versionSupported !== false
            ? "text-emerald-500"
            : "text-red-500"}
        >
          {status.found && status.versionSupported === false
            ? t("settings_pi_upgradeRequired", { minimum: status.minimumVersion ?? "0.84.4" })
            : status.found
              ? t("settings_pi_installed")
              : t("settings_pi_notInstalled")}
        </p>
      </div>
      <div>
        <span class="text-xs text-muted-foreground">{t("settings_cliConfig_version")}</span>
        <p>{status.version ?? "—"}</p>
      </div>
      <div class="min-w-0">
        <span class="text-xs text-muted-foreground">{t("settings_pi_cliPath")}</span>
        <p class="break-all font-mono text-xs leading-5">{status.path ?? "—"}</p>
      </div>
      {#if showConfig}
        <div class="min-w-0">
          <span class="text-xs text-muted-foreground"
            >{configLabel ?? t("settings_cliConfig_configPath")}</span
          >
          <p class="break-all font-mono text-xs leading-5">{status.configPath ?? "—"}</p>
        </div>
      {/if}
    </div>
  {/if}
</Card>
