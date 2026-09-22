<script lang="ts">
  import {
    checkAgentCli,
    checkAuthStatus,
    checkCodexAuth,
    checkPiAuth,
    detectInstallMethods,
    runClaudeLogin,
    runCodexLogin,
    updateAgentSettings,
    updateUserSettings,
  } from "$lib/api";
  import type { InstallMethod, PlatformPreset } from "$lib/types";
  import { PLATFORM_PRESETS, PRESET_CATEGORIES } from "$lib/utils/platform-presets";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { IS_WINDOWS } from "$lib/utils/platform";
  import { getTransport } from "$lib/transport";
  import { t } from "$lib/i18n/index.svelte";
  import { setSavedRealm, setSavedPiSubMode } from "$lib/stores/app-mode.svelte";
  import PiLoginModal from "$lib/components/PiLoginModal.svelte";
  import { getGrokStatus, runGrokLogin } from "$lib/grok-api";

  let { onComplete, onBack }: { onComplete: () => void; onBack?: () => void } = $props();

  type WizardStep =
    | "agent_choice"
    | "checking"
    | "cli_not_found"
    | "auth_choice"
    | "oauth_login"
    | "pi_login"
    | "api_key_setup"
    | "done";

  let step = $state<WizardStep>("agent_choice");
  let targetAgent = $state<"claude" | "codex" | "pi" | "grok">("pi");
  let error = $state("");

  // CLI install state
  let installMethods = $state<InstallMethod[]>([]);

  // Copy button state: method id → "copy" | "copied"
  let copyStates = $state<Record<string, string>>({});

  // Recheck state
  let rechecking = $state(false);
  let piCliIssue = $state<"missing" | "upgrade">("missing");
  let detectedPiVersion = $state("");

  // OAuth state
  let oauthLoading = $state(false);
  let installProgress = $state<string[]>([]);
  let piLoginCompleting = $state(false);

  // API key state
  let selectedPlatform = $state<PlatformPreset | null>(null);
  let apiKey = $state("");
  let customBaseUrl = $state("");
  let showKey = $state(false);
  let saving = $state(false);

  // Done state
  let doneTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  // Start checking on mount
  $effect(() => {
    if (step === "checking") {
      runInitialCheck();
    }
  });

  async function runInitialCheck() {
    dbg("wizard", "starting initial check", { targetAgent });
    try {
      const cliResult = await checkAgentCli(targetAgent);
      if (targetAgent === "pi" && (!cliResult.found || cliResult.version_supported === false)) {
        piCliIssue = cliResult.found ? "upgrade" : "missing";
        detectedPiVersion = cliResult.version ?? "";
        step = "cli_not_found";
        await loadInstallMethods();
        return;
      }
      let authed = false;
      if (targetAgent === "grok") {
        try {
          authed = (await getGrokStatus()).authenticated;
        } catch (e) {
          dbgWarn("wizard", "getGrokStatus failed", e);
          authed = false;
        }
      } else if (targetAgent === "codex") {
        // Codex auth lives in `codex login status`, not the Anthropic-specific auth_mode.
        try {
          authed = (await checkCodexAuth()).logged_in;
        } catch (e) {
          dbgWarn("wizard", "checkCodexAuth failed", e);
          authed = false;
        }
      } else if (targetAgent === "pi") {
        try {
          authed = (await checkPiAuth()).logged_in;
        } catch (e) {
          dbgWarn("wizard", "checkPiAuth failed", e);
          authed = false;
        }
      } else {
        const a = await checkAuthStatus();
        authed = a.has_oauth || a.has_api_key;
      }

      if (targetAgent === "grok") {
        if (cliResult.found && authed) {
          await updateUserSettings({ default_agent: "grok", onboarding_completed: true });
          await completeOnboarding();
        } else if (cliResult.found) {
          step = "auth_choice";
        } else {
          step = "cli_not_found";
          await loadInstallMethods();
        }
        return;
      }

      if (targetAgent === "pi") {
        if (cliResult.found && authed) {
          setSavedRealm("pi");
          setSavedPiSubMode("code");
          await updateUserSettings({ onboarding_completed: true });
          await completeOnboarding();
        } else if (cliResult.found) {
          step = "auth_choice";
        } else {
          step = "cli_not_found";
          await loadInstallMethods();
        }
        return;
      }

      dbg("wizard", "check results", {
        targetAgent,
        cliFound: cliResult.found,
        authed,
      });

      if (cliResult.found && authed) {
        await completeOnboarding();
        return;
      }

      if (cliResult.found && !authed) {
        step = "auth_choice";
        return;
      }

      // CLI not found — show install commands
      step = "cli_not_found";
      await loadInstallMethods();
    } catch (e) {
      dbgWarn("wizard", "initial check error", e);
      // If check fails, assume CLI not installed
      step = "cli_not_found";
      await loadInstallMethods();
    }
  }

  async function loadInstallMethods() {
    try {
      installMethods = await detectInstallMethods(targetAgent);
      dbg("wizard", "install methods", { targetAgent, methods: installMethods });
    } catch (e) {
      dbgWarn("wizard", "detect methods error", e);
      installMethods = [];
    }
  }

  async function copyCommand(method: InstallMethod) {
    try {
      await navigator.clipboard.writeText(method.command);
      copyStates = { ...copyStates, [method.id]: "copied" };
      setTimeout(() => {
        copyStates = { ...copyStates, [method.id]: "copy" };
      }, 1500);
    } catch (e) {
      dbgWarn("wizard", "copy failed", e);
    }
  }

  async function recheckCli() {
    rechecking = true;
    try {
      const result = await checkAgentCli(targetAgent);
      dbg("wizard", "recheck result", { targetAgent, result });
      if (targetAgent === "pi" && result.found && result.version_supported === false) {
        piCliIssue = "upgrade";
        detectedPiVersion = result.version ?? "";
        return;
      }
      if (result.found) {
        if (targetAgent === "grok") {
          const grok = await getGrokStatus();
          if (grok.authenticated) {
            await updateUserSettings({ default_agent: "grok", onboarding_completed: true });
            await completeOnboarding();
          } else {
            step = "auth_choice";
          }
        } else if (targetAgent === "pi") {
          const auth = await checkPiAuth();
          if (auth.logged_in) {
            setSavedRealm("pi");
            setSavedPiSubMode("code");
            await updateUserSettings({ onboarding_completed: true });
            await completeOnboarding();
          } else {
            step = "auth_choice";
          }
        } else {
          step = "auth_choice";
        }
      }
    } catch (e) {
      dbgWarn("wizard", "recheck error", e);
    } finally {
      rechecking = false;
    }
  }

  async function startOAuthLogin() {
    if (targetAgent === "pi") {
      error = "";
      piLoginCompleting = false;
      step = "pi_login";
      return;
    }

    step = "oauth_login";
    oauthLoading = true;
    error = "";
    installProgress = [];

    const wizTransport = getTransport();
    const unlistenAppend = await wizTransport.listen<string>("setup-progress", (payload) => {
      installProgress = [...installProgress, payload];
    });
    const unlistenReplace = await wizTransport.listen<string>(
      "setup-progress-replace",
      (payload) => {
        if (installProgress.length > 0) {
          installProgress = [...installProgress.slice(0, -1), payload];
        } else {
          installProgress = [payload];
        }
      },
    );
    const unlisten = () => {
      unlistenAppend();
      unlistenReplace();
    };

    try {
      const success =
        targetAgent === "codex"
          ? await runCodexLogin()
          : targetAgent === "grok"
            ? await runGrokLogin()
            : await runClaudeLogin();
      unlisten();

      if (success) {
        dbg("wizard", "oauth login success");
        if (targetAgent === "grok") {
          await updateUserSettings({ default_agent: "grok" });
        }
        await completeOnboarding();
      } else {
        error = t("setup_loginFailed");
      }
    } catch (e) {
      unlisten();
      dbgWarn("wizard", "oauth login error", e);
      error = String(e);
    } finally {
      oauthLoading = false;
    }
  }

  async function handlePiLoginStatus(status: { logged_in: boolean }) {
    if (!status.logged_in || piLoginCompleting) return;
    piLoginCompleting = true;
    try {
      setSavedRealm("pi");
      setSavedPiSubMode("code");
      await updateAgentSettings("pi", { model: "openai-codex/gpt-5.5" });
      await updateUserSettings({ onboarding_completed: true });
      await completeOnboarding();
    } catch (e) {
      piLoginCompleting = false;
      error = String(e);
    }
  }

  function selectPlatform(preset: PlatformPreset) {
    selectedPlatform = preset;
    apiKey = "";
    customBaseUrl = preset.base_url;
    showKey = false;
  }

  async function saveApiKey() {
    if (!selectedPlatform) return;
    saving = true;
    error = "";

    try {
      const effectiveBaseUrl =
        selectedPlatform.id === "custom" ? customBaseUrl : selectedPlatform.base_url;

      await updateUserSettings({
        auth_mode: "api",
        anthropic_api_key: apiKey || undefined,
        anthropic_base_url: effectiveBaseUrl || undefined,
        auth_env_var: selectedPlatform.auth_env_var,
        onboarding_completed: true,
      });

      dbg("wizard", "api key saved", {
        platform: selectedPlatform.id,
        hasKey: !!apiKey,
        hasBaseUrl: !!effectiveBaseUrl,
      });

      await completeOnboarding();
    } catch (e) {
      dbgWarn("wizard", "save api key error", e);
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function completeOnboarding() {
    try {
      await updateUserSettings({ onboarding_completed: true });
    } catch {
      // Non-critical — continue anyway
    }
    step = "done";
    doneTimer = setTimeout(() => {
      onComplete();
    }, 2000);
  }

  function finishNow() {
    if (doneTimer) clearTimeout(doneTimer);
    onComplete();
  }

  let availableMethods = $derived(installMethods.filter((m) => m.available));
  let unavailableMethods = $derived(installMethods.filter((m) => !m.available));

  // Display names used in agent-aware i18n strings (setup_cliNotFound,
  // setup_authDesc, setup_oauthDesc).
  let agentDisplayName = $derived(
    targetAgent === "codex"
      ? "Codex"
      : targetAgent === "pi"
        ? "Pi Agent"
        : targetAgent === "grok"
          ? "Grok"
          : "Claude Code",
  );
  let accountDisplayName = $derived(
    targetAgent === "codex"
      ? "ChatGPT"
      : targetAgent === "pi"
        ? "Pi"
        : targetAgent === "grok"
          ? "xAI"
          : "Anthropic",
  );
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-background">
  {#if onBack && step !== "done"}
    <button
      class="absolute left-6 top-6 inline-flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      aria-label={t("setup_back")}
      onclick={onBack}
    >
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"><path d="m15 18-6-6 6-6" /></svg
      >
      {t("setup_back")}
    </button>
  {/if}

  <div class="w-full max-w-xl mx-auto px-6">
    {#if step === "agent_choice"}
      <!-- Agent choice step — user picks which CLI to set up -->
      <div class="flex flex-col gap-6">
        <div class="text-center">
          <h2 class="text-xl font-semibold">{t("setup_agentChoiceTitle")}</h2>
          <p class="text-sm text-muted-foreground mt-2">{t("setup_agentChoiceDesc")}</p>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          <button
            class="relative flex flex-col items-start gap-1 rounded-lg border-2 border-primary/50 bg-card px-4 py-3 text-left hover:bg-accent transition-colors"
            onclick={() => {
              targetAgent = "pi";
              step = "checking";
            }}
          >
            <div class="flex items-center justify-between w-full">
              <span class="text-sm font-medium">Pi Agent</span>
              <span
                class="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary"
                >{t("setup_recommended")}</span
              >
            </div>
            <span class="text-xs text-muted-foreground"
              >RPC coding agent with flexible providers</span
            >
          </button>
          <button
            class="flex flex-col items-start gap-1 rounded-lg border border-border bg-card px-4 py-3 text-left hover:bg-accent transition-colors"
            onclick={() => {
              targetAgent = "claude";
              step = "checking";
            }}
          >
            <span class="text-sm font-medium">{t("setup_agentClaudeName")}</span>
            <span class="text-xs text-muted-foreground">{t("setup_agentClaudeDesc")}</span>
          </button>
          <button
            class="flex flex-col items-start gap-1 rounded-lg border border-border bg-card px-4 py-3 text-left hover:bg-accent transition-colors"
            onclick={() => {
              targetAgent = "codex";
              step = "checking";
            }}
          >
            <span class="text-sm font-medium">{t("setup_agentCodexName")}</span>
            <span class="text-xs text-muted-foreground">{t("setup_agentCodexDesc")}</span>
          </button>
          <button
            class="flex flex-col items-start gap-1 rounded-lg border border-border bg-card px-4 py-3 text-left hover:bg-accent transition-colors"
            onclick={() => {
              targetAgent = "grok";
              step = "checking";
            }}
          >
            <span class="text-sm font-medium">Grok</span>
            <span class="text-xs text-muted-foreground"
              >ACP coding agent with native models and providers</span
            >
          </button>
        </div>
      </div>
    {:else if step === "checking"}
      <!-- Checking step -->
      <div class="flex flex-col items-center gap-4 py-16">
        <div
          class="h-8 w-8 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
        ></div>
        <p class="text-sm text-muted-foreground">{t("setup_checking")}</p>
      </div>
    {:else if step === "cli_not_found"}
      <!-- CLI not found — show install commands to copy -->
      <div class="flex flex-col gap-6">
        <div class="text-center">
          <h2 class="text-xl font-semibold">
            {targetAgent === "pi" && piCliIssue === "upgrade"
              ? t("setup_piUpgradeRequired")
              : t("setup_cliNotFound", { agentName: agentDisplayName })}
          </h2>
          <p class="text-sm text-muted-foreground mt-2">
            {targetAgent === "pi" && piCliIssue === "upgrade"
              ? t("setup_piUpgradeRequiredDesc", {
                  version: detectedPiVersion || t("setup_piUnknownVersion"),
                  minimum: "0.84.4",
                })
              : t("setup_cliNotFoundDesc")}
          </p>
        </div>

        <!-- Available methods — command cards with copy buttons -->
        {#if availableMethods.length > 0}
          <div class="flex flex-col gap-3">
            {#each availableMethods as method, i}
              <div class="rounded-lg border border-border p-3 bg-muted/30">
                <div class="flex items-center gap-3">
                  <code class="flex-1 text-sm font-mono text-foreground/90 select-all"
                    >$ {method.command}</code
                  >
                  {#if i === 0}
                    <span
                      class="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary whitespace-nowrap"
                      >{t("setup_recommended")}</span
                    >
                  {/if}
                  <button
                    class="rounded-md border border-border px-2.5 py-1 text-xs hover:bg-accent transition-colors whitespace-nowrap {copyStates[
                      method.id
                    ] === 'copied'
                      ? 'text-green-600 border-green-500/30'
                      : 'text-muted-foreground'}"
                    onclick={() => copyCommand(method)}
                  >
                    {copyStates[method.id] === "copied"
                      ? t("setup_copied")
                      : t("setup_copyCommand")}
                  </button>
                </div>
                {#if method.note}
                  <p class="text-[10px] text-muted-foreground/70 mt-1">{method.note}</p>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <!-- Unavailable methods (greyed out with reason) -->
        {#if unavailableMethods.length > 0}
          <div class="flex flex-col gap-2 opacity-50">
            {#each unavailableMethods as method}
              <div class="flex items-center gap-3 rounded-lg border border-border/50 p-3">
                <span class="text-sm text-muted-foreground">{method.name}</span>
                {#if method.unavailable_reason}
                  <span class="text-xs text-muted-foreground/70">— {method.unavailable_reason}</span
                  >
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <!-- Action buttons -->
        <div class="flex items-center justify-center gap-3">
          <button
            class="rounded-md border border-border px-4 py-2 text-sm hover:bg-accent transition-colors disabled:opacity-50"
            disabled={rechecking}
            onclick={recheckCli}
          >
            {#if rechecking}
              <span class="flex items-center gap-2">
                <span
                  class="h-3 w-3 border border-foreground/30 border-t-foreground rounded-full animate-spin"
                ></span>
                {t("setup_recheck")}
              </span>
            {:else}
              {t("setup_recheck")}
            {/if}
          </button>
          {#if targetAgent === "claude"}
            <!-- API key fallback is Claude-only; Codex, Pi and Grok keep their native auth flows. -->
            <button
              class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
              onclick={() => {
                step = "api_key_setup";
              }}
            >
              {t("setup_skipCli")}
            </button>
          {/if}
        </div>

        <!-- Setup hint -->
        <p class="text-xs text-muted-foreground text-center">
          {IS_WINDOWS ? t("setup_winRecheckHint") : t("setup_setupHint")}
        </p>
      </div>
    {:else if step === "auth_choice"}
      <!-- Auth method choice -->
      <div class="flex flex-col gap-6">
        <div class="text-center">
          <h2 class="text-xl font-semibold">{t("setup_authTitle")}</h2>
          <p class="text-sm text-muted-foreground mt-2">
            {t("setup_authDesc", { agentName: agentDisplayName })}
          </p>
        </div>

        <div
          class={targetAgent === "claude"
            ? "grid grid-cols-2 gap-4"
            : "grid grid-cols-1 gap-4 max-w-sm mx-auto"}
        >
          <!-- OAuth -->
          <button
            class="flex flex-col items-center gap-3 rounded-lg border border-border p-6 text-center transition-colors hover:border-primary/50 hover:bg-accent/50"
            onclick={startOAuthLogin}
          >
            <svg
              class="h-8 w-8 text-primary"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" /><polyline
                points="10 17 15 12 10 7"
              /><line x1="15" x2="3" y1="12" y2="12" /></svg
            >
            <div>
              <p class="font-medium text-sm">{t("setup_oauthTitle")}</p>
              <p class="text-xs text-muted-foreground mt-1">
                {t("setup_oauthDesc", { accountName: accountDisplayName })}
              </p>
            </div>
            <span
              class="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary"
              >{t("setup_recommended")}</span
            >
          </button>

          <!-- API Key (Anthropic-only — Codex auth runs through `codex login`, not AgentCabin) -->
          {#if targetAgent === "claude"}
            <button
              class="flex flex-col items-center gap-3 rounded-lg border border-border p-6 text-center transition-colors hover:border-primary/50 hover:bg-accent/50"
              onclick={() => {
                step = "api_key_setup";
              }}
            >
              <svg
                class="h-8 w-8 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path
                  d="m21 2-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4"
                /></svg
              >
              <div>
                <p class="font-medium text-sm">{t("setup_apiKeyTitle")}</p>
                <p class="text-xs text-muted-foreground mt-1">{t("setup_apiKeyDesc")}</p>
              </div>
            </button>
          {/if}
        </div>
      </div>
    {:else if step === "oauth_login"}
      <!-- OAuth login in progress -->
      <div class="flex flex-col items-center gap-4 py-8">
        {#if oauthLoading}
          <div
            class="h-8 w-8 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
          ></div>
          <p class="text-sm font-medium">{t("setup_openingBrowser")}</p>
          <p class="text-xs text-muted-foreground text-center">{t("setup_completeBrowser")}</p>
        {/if}

        {#if error}
          <div class="rounded-lg border border-red-500/30 bg-red-500/5 p-3 w-full max-w-sm">
            <p class="text-sm text-red-500">{error}</p>
          </div>
        {/if}

        <button
          class="rounded-md border border-border px-4 py-2 text-xs hover:bg-accent transition-colors mt-4"
          onclick={() => {
            step = "auth_choice";
            error = "";
          }}>{t("setup_back")}</button
        >
      </div>
    {:else if step === "pi_login"}
      <PiLoginModal
        onClose={() => {
          step = "auth_choice";
          error = "";
        }}
        onStatusChange={(status) => void handlePiLoginStatus(status)}
      />
    {:else if step === "api_key_setup"}
      <!-- API key setup with platform selection -->
      <div class="flex flex-col gap-5">
        <div class="flex items-center gap-2">
          <button
            class="rounded-md p-1 hover:bg-accent transition-colors"
            onclick={() => {
              step = "auth_choice";
              selectedPlatform = null;
              error = "";
            }}
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m15 18-6-6 6-6" /></svg
            >
          </button>
          <h2 class="text-lg font-semibold">{t("setup_selectPlatform")}</h2>
        </div>

        {#if !selectedPlatform}
          <!-- Platform grid -->
          <div class="flex flex-col gap-4 max-h-[60vh] overflow-y-auto pr-1">
            {#each PRESET_CATEGORIES as category}
              {@const presets = PLATFORM_PRESETS.filter((p) => p.category === category.id)}
              {#if presets.length > 0}
                <div>
                  <p
                    class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2"
                  >
                    {category.label}
                  </p>
                  <div class="grid grid-cols-3 gap-2">
                    {#each presets as preset}
                      <button
                        class="flex flex-col gap-0.5 rounded-lg border border-border p-3 text-left transition-colors hover:border-primary/50 hover:bg-accent/50"
                        onclick={() => selectPlatform(preset)}
                      >
                        <span class="text-sm font-medium truncate">{preset.name}</span>
                        <span class="text-[10px] text-muted-foreground truncate"
                          >{preset.description}</span
                        >
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <!-- Platform config form -->
          <div class="flex flex-col gap-4">
            <div
              class="flex items-center gap-2 rounded-lg border border-primary/30 bg-primary/5 p-3"
            >
              <span class="font-medium text-sm">{selectedPlatform.name}</span>
              <span class="text-xs text-muted-foreground">{selectedPlatform.description}</span>
              <button
                class="ml-auto text-xs text-muted-foreground hover:text-foreground transition-colors"
                onclick={() => {
                  selectedPlatform = null;
                }}>{t("setup_change")}</button
              >
            </div>

            <!-- Custom: extra Base URL input -->
            {#if selectedPlatform.id === "custom"}
              <div class="flex flex-col gap-1.5">
                <label class="text-xs font-medium text-muted-foreground">{t("setup_baseUrl")}</label
                >
                <input
                  type="text"
                  bind:value={customBaseUrl}
                  placeholder="https://api.example.com"
                  class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm focus:outline-none focus:border-ring"
                />
              </div>
            {/if}

            <!-- API Key input -->
            <div class="flex flex-col gap-1.5">
              <label class="text-xs font-medium text-muted-foreground"
                >{t("setup_apiKeyLabel")}</label
              >
              <div class="relative">
                <input
                  type={showKey ? "text" : "password"}
                  bind:value={apiKey}
                  placeholder={selectedPlatform.key_placeholder}
                  class="w-full rounded-md border border-border bg-background px-3 py-2 pr-16 text-sm font-mono focus:outline-none focus:border-ring"
                />
                <button
                  class="absolute right-2 top-1/2 -translate-y-1/2 rounded px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  onclick={() => {
                    showKey = !showKey;
                  }}>{showKey ? t("setup_hide") : t("setup_show")}</button
                >
              </div>
              {#if selectedPlatform.id === "ollama"}
                <p class="text-xs text-muted-foreground">{t("setup_noKeyNeeded")}</p>
              {/if}
            </div>

            <!-- Auth type info -->
            <p class="text-xs text-muted-foreground">
              {selectedPlatform.auth_env_var === "ANTHROPIC_API_KEY"
                ? t("setup_authTypeApiKey")
                : t("setup_authTypeBearer")}
            </p>

            {#if error}
              <div class="rounded-lg border border-red-500/30 bg-red-500/5 p-2">
                <p class="text-xs text-red-500">{error}</p>
              </div>
            {/if}

            <button
              class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
              disabled={saving || (selectedPlatform.id !== "ollama" && !apiKey)}
              onclick={saveApiKey}
            >
              {#if saving}
                <span class="flex items-center gap-2 justify-center">
                  <span
                    class="h-3 w-3 border border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin"
                  ></span>
                  {t("setup_saving")}
                </span>
              {:else}
                {t("setup_saveAndContinue")}
              {/if}
            </button>
          </div>
        {/if}
      </div>
    {:else if step === "done"}
      <!-- Done! -->
      <div class="flex flex-col items-center gap-4 py-16">
        <div class="flex h-16 w-16 items-center justify-center rounded-full bg-green-500/10">
          <svg
            class="h-8 w-8 text-green-500"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
          >
        </div>
        <h2 class="text-xl font-semibold">{t("setup_allSet")}</h2>
        <p class="text-sm text-muted-foreground">{t("setup_allSetDesc")}</p>
        <button
          class="rounded-md bg-primary px-6 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors mt-2"
          onclick={finishNow}>{t("setup_start")}</button
        >
      </div>
    {/if}
  </div>
</div>
