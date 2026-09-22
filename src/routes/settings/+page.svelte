<script lang="ts">
  import { flushSync, onMount, getContext } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import * as api from "$lib/api";
  import { platform } from "$lib/platform";
  import { loadCliInfo, KeybindingStore } from "$lib/stores";
  import type {
    UserSettings,
    AgentSettings,
    CliModelInfo,
    CliConfigSettingDef,
    RemoteHost,
    CodexAuthResult,
    PiAuthResult,
    CliCheckResult,
    GlobalProviderCredential,
    AgentProviderBindings,
  } from "$lib/types";
  import Card from "$lib/components/Card.svelte";
  import { getSavedProjectCwd } from "$lib/utils/project-cwd";
  import {
    PLATFORM_PRESETS,
    PRESET_CATEGORIES,
    buildPlatformList,
    findCredential,
    expandModelsToTiers,
    compressModelsFromTiers,
  } from "$lib/utils/platform-presets";
  import type { PlatformPreset, PlatformCredential, CodexProviderCredential } from "$lib/types";
  import {
    CODEX_PROVIDER_PRESETS,
    type CodexProviderPreset,
  } from "$lib/utils/codex-provider-presets";
  import {
    PI_PROVIDER_PRESETS,
    normalizePiCliModel,
    type PiProviderPreset,
  } from "$lib/utils/pi-provider-presets";
  import { isDebugMode, getDebugLogCount, getDebugFilter } from "$lib/utils/debug";
  import { dbg, dbgWarn, redactSensitive } from "$lib/utils/debug";
  import { ALL_RUNTIME_PROVIDERS, type RuntimeProviderId } from "$lib/utils/agent-metadata";
  import {
    fetchRuntimeProviderStatus,
    type RuntimeProviderStatus,
  } from "$lib/utils/runtime-status";
  import { splitPath } from "$lib/utils/format";
  import { isProviderCompatible, type ProviderAgent } from "$lib/utils/provider-routing";
  import {
    buildCodexSubscriptionProvider,
    CODEX_SUBSCRIPTION_PROVIDER_ID,
  } from "$lib/utils/codex-subscription";
  import { getSavedRealm, getSavedPiSubMode, getRealmHref } from "$lib/stores/app-mode.svelte";
  import { t, currentLocale } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import DshPluginPanel from "$lib/components/DshPluginPanel.svelte";
  import SettingToggle from "$lib/components/SettingToggle.svelte";
  import AgentCliStatusCard from "$lib/components/AgentCliStatusCard.svelte";
  import CodexFeaturesSettings from "$lib/components/CodexFeaturesSettings.svelte";
  import GrokCliConfigCard from "$lib/components/GrokCliConfigCard.svelte";
  import ScopeBanner from "$lib/components/ScopeBanner.svelte";
  import PiCodeGlobalRulesPanel from "$lib/components/PiCodeGlobalRulesPanel.svelte";
  import PiExtensionsManager from "$lib/components/PiExtensionsManager.svelte";
  import PiProfileConfigCard from "$lib/components/PiProfileConfigCard.svelte";
  import RuntimeProviderUnifiedCards from "$lib/components/RuntimeProviderUnifiedCards.svelte";
  import HarnessRuntimeProviderSection from "$lib/components/HarnessRuntimeProviderSection.svelte";
  import type { InstalledPlugin } from "$lib/types";
  import { isSettingsTabActive } from "$lib/utils/settings-navigation";
  import pkg from "../../../package.json";
  import type { DesktopUseStatus, WorkBrowserHealth, WorkBrowserSummary } from "$lib/types/work";

  const PI_RUNTIME_THINKING_OPTIONS = [
    { val: "", labelKey: "settings_codexConfig_optInherit" },
    { val: "off", labelKey: "settings_codexConfig_optNone" },
    { val: "minimal", labelKey: "settings_codexConfig_optMinimal" },
    { val: "low", labelKey: "settings_codexConfig_optLow" },
    { val: "medium", labelKey: "settings_codexConfig_optMedium" },
    { val: "high", labelKey: "settings_codexConfig_optHigh" },
    { val: "xhigh", labelKey: "settings_codexConfig_optXHigh" },
  ] as const;

  let appVersion = $state("");

  import SettingsShell from "$lib/components/settings/SettingsShell.svelte";
  import SettingsSidebar from "$lib/components/settings/SettingsSidebar.svelte";
  import GeneralSettings from "$lib/components/settings/pages/GeneralSettings.svelte";
  import AppearanceSettings from "$lib/components/settings/pages/AppearanceSettings.svelte";
  import KeybindingsSettings from "$lib/components/settings/pages/KeybindingsSettings.svelte";
  import UsageSettings from "$lib/components/settings/pages/UsageSettings.svelte";
  import PetSettingsView from "$lib/components/settings/pages/PetSettings.svelte";
  import CapabilityCenterSettings from "$lib/components/settings/pages/CapabilityCenterSettings.svelte";
  import CodeSettings from "$lib/components/settings/pages/CodeSettings.svelte";
  import RuntimeSettings from "$lib/components/settings/pages/RuntimeSettings.svelte";
  import WorkSettings from "$lib/components/settings/pages/WorkSettings.svelte";
  import ModelsSettings from "$lib/components/settings/pages/ModelsSettings.svelte";
  import DoctorSettings from "$lib/components/settings/pages/DoctorSettings.svelte";
  import WebAccessSettings from "$lib/components/settings/pages/WebAccessSettings.svelte";
  import BrowserUseSettings from "$lib/components/settings/pages/BrowserUseSettings.svelte";
  import DesktopUseSettings from "$lib/components/settings/pages/DesktopUseSettings.svelte";
  import {
    resolveSettingsRoute,
    type SettingsTab,
    type RuntimeSubTab,
  } from "$lib/utils/settings-routing";

  const urlTab = $page.url.searchParams.get("tab");
  const urlCategory = $page.url.searchParams.get("category");
  const initialRoute = resolveSettingsRoute(urlTab, urlCategory);
  let activeTab = $state<SettingsTab>(initialRoute.tab);
  let activeRuntimeSubTab = $state<RuntimeSubTab>(initialRoute.runtimeSubTab ?? "dsh");

  function handleBack() {
    const realm = getSavedRealm();
    const subMode = getSavedPiSubMode();
    void goto(getRealmHref(realm, subMode));
  }

  async function saveRemoteHostDirect(host: RemoteHost) {
    const existingIndex = remoteHosts.findIndex((h) => h.name === host.name);
    const updated =
      existingIndex >= 0
        ? remoteHosts.map((h, i) => (i === existingIndex ? host : h))
        : [...remoteHosts, host];
    await api.updateUserSettings({ remote_hosts: updated } as Partial<UserSettings>);
    remoteHosts = updated;
  }

  function getPageTitle(tab: SettingsTab): string {
    switch (tab) {
      case "general":
        return "常规设置";
      case "appearance":
        return "外观偏好";
      case "keybindings":
        return "键盘快捷键";
      case "usage":
        return "使用情况和计费";
      case "pet":
        return "桌面宠物 (Pet)";
      case "capability-center":
        return "能力中心 (Capability Center)";
      case "code":
        return "Code 极客编程模式";
      case "work":
        return "Work 自主工作流模式";
      case "runtimes":
        return "Runtime 运行时";
      case "models":
        return "模型与提供商";
      case "doctor":
        return "CLI 引擎检测 (Doctor)";
      case "web-access":
        return "网络访问 (Web Access)";
      case "browser-use":
        return "浏览器自动化 (Browser Use)";
      case "desktop-use":
        return "电脑控制 (Desktop Use)";
      default:
        return "设置";
    }
  }

  function getPageDescription(tab: SettingsTab): string {
    switch (tab) {
      case "general":
        return "管理全局应用配置、权限策略、语言环境与系统集成。";
      case "appearance":
        return "定制界面主题、色彩方案、字体字号与辅助视觉效果。";
      case "keybindings":
        return "自定义全局操作快捷键与查看内置终端按键映射。";
      case "usage":
        return "查看模型调用的 Token 统计用量与历史计费分析。";
      case "pet":
        return "管理陪伴助手形象、缩放大小与屏幕交互行为。";
      case "capability-center":
        return "探索与管理技能、连接器和各运行时的能力扩展。";
      case "code":
        return "配置 Code 极客编程模式的默认 Runtime Provider、工作载体 Profile、全局规则与工作区。";
      case "work":
        return "管理自主工作流执行引擎、长程任务守则与工具投射。";
      case "runtimes":
        return "配置与管理 DeepSeek 与 Pi Agent 等原生运行时引擎环境。";
      case "models":
        return "统一配置自定义第三方 API 供应商与 ChatGPT 官方订阅凭据。";
      case "doctor":
        return "深度诊断本地 AI CLI 引擎安装状态、执行路径与运行健康度。";
      case "web-access":
        return "配置搜索供应商与网页抓取知识库连接能力。";
      case "browser-use":
        return "配置 Playwright/Chromium 自动化环境与浏览器操控通道。";
      case "desktop-use":
        return "管理系统辅助功能权限与原生屏幕自动化操控。";
      default:
        return "";
    }
  }
  let activeView = $derived($page.url.searchParams.get("view") ?? "settings");
  let embeddedAgentSettingsTab = $derived.by<"native-codex" | "native-claude" | null>(() => {
    if (activeView !== "plugins") return null;
    const tab = $page.url.searchParams.get("tab");
    return tab === "native-codex" || tab === "native-claude" ? tab : null;
  });
  let activeExtensionSection = $derived(
    $page.url.searchParams.get("section") === "plugins"
      ? "claude-plugins"
      : ($page.url.searchParams.get("section") ?? "skills"),
  );

  const pluginSectionCtx = getContext<{ active: string }>("pluginSection");
  $effect(() => {
    if (activeView === "plugins" && pluginSectionCtx) {
      pluginSectionCtx.active = activeExtensionSection;
    }
  });

  function handleSelectTab(tab: string, section?: string) {
    const route = resolveSettingsRoute(tab);
    activeTab = route.tab;
    flushSync();
    if (route.runtimeSubTab) {
      activeRuntimeSubTab = route.runtimeSubTab;
    }
    const params = new URLSearchParams();
    params.set("tab", tab);
    if (section) params.set("section", section);
    void goto(`/settings?${params.toString()}`, { replaceState: true, noScroll: true });
  }

  async function handleUpdateSettings(patch: Partial<UserSettings>): Promise<void> {
    try {
      settings = await api.updateUserSettings(patch);
    } catch (e) {
      dbgWarn("settings", "updateUserSettings failed", e);
      throw e;
    }
  }

  function openSettingsTab(tab: string, category?: string) {
    const route = resolveSettingsRoute(tab, category);
    activeTab = route.tab;
    flushSync();
    if (route.runtimeSubTab) {
      activeRuntimeSubTab = route.runtimeSubTab;
    }
    const url = category ? `/settings?tab=${tab}&category=${category}` : `/settings?tab=${tab}`;
    void goto(url, { replaceState: true, noScroll: true });
  }

  function isActiveSettingsTab(tab: SettingsTab): boolean {
    return (
      isSettingsTabActive(activeView, activeTab, tab) || (embeddedAgentSettingsTab as any) === tab
    );
  }

  let settings = $state<UserSettings | null>(null);
  let desktopToolLoading = $state(true);
  let webAccessEnabled = $state(true);
  let webAccessConfig = $state<WorkBrowserSummary | null>(null);
  let browserUseEnabled = $state(true);
  let desktopUseStatus = $state<DesktopUseStatus | null>(null);

  async function refreshDesktopToolSettings() {
    desktopToolLoading = true;
    try {
      const [webEnabled, browserEnabled, browserConfig, desktopStatus] = await Promise.allSettled([
        api.getWebAccessBinding(),
        api.getBrowserUseBinding(),
        api.getBrowserConfig(),
        api.getDesktopUseStatus(),
      ]);
      if (webEnabled.status === "fulfilled") webAccessEnabled = webEnabled.value;
      if (browserEnabled.status === "fulfilled") browserUseEnabled = browserEnabled.value;
      if (browserConfig.status === "fulfilled") webAccessConfig = browserConfig.value;
      if (desktopStatus.status === "fulfilled") desktopUseStatus = desktopStatus.value;
      if (desktopStatus.status === "rejected") {
        dbgWarn("settings", "desktop use status unavailable", desktopStatus.reason);
      }
    } catch (cause) {
      dbgWarn("settings", "desktop tool settings load failed", cause);
    } finally {
      desktopToolLoading = false;
    }
  }

  async function saveSettingsWebAccess(input: {
    provider?: string;
    enabled: boolean;
    maxResults: number;
    apiKey: string | null;
    endpointUrl?: string | null;
    allowedHosts?: string[] | null;
  }) {
    webAccessConfig = await api.saveBrowserConfig(input);
  }

  async function testSettingsWebAccess(): Promise<WorkBrowserHealth> {
    return api.testBrowser();
  }

  async function toggleSettingsWebAccess(enabled: boolean) {
    await api.setWebAccessBinding(enabled);
    webAccessEnabled = enabled;
  }

  async function toggleSettingsBrowserUse(enabled: boolean) {
    await api.setBrowserUseBinding(enabled);
    browserUseEnabled = enabled;
  }

  async function prepareSettingsBrowserRuntime(): Promise<WorkBrowserSummary> {
    webAccessConfig = await api.prepareBrowserRuntime();
    return webAccessConfig;
  }

  async function toggleSettingsDesktopUse(enabled: boolean) {
    await api.setDesktopUseBinding(enabled);
    desktopUseStatus = await api.getDesktopUseStatus();
  }

  let globalProviders = $state<GlobalProviderCredential[]>([]);
  let authMode = $state("cli");
  let anthropicApiKey = $state("");
  let anthropicBaseUrl = $state("");
  let showApiKey = $state(false);
  let generalSaved = $state(false);
  let agentSaveNotice = $state<ProviderAgent | null>(null);
  let modelOpus = $state("");
  let modelSonnet = $state("");
  let modelHaiku = $state("");
  let selectedPlatformId = $state<string | null>(null);
  let platformCredentials = $state<PlatformCredential[]>([]);
  let platformExtraEnv = $state<Array<{ key: string; value: string }>>([]);
  // Track whether user manually edited extra_env (per platform ID).
  // Untouched platforms don't write extra_env, avoiding preset defaults being baked into credentials.
  let extraEnvTouched = $state<Record<string, boolean>>({});

  // CLI Auth state
  let authOverview = $state<import("$lib/types").AuthOverview | null>(null);
  let cliLoginLoading = $state(false);
  let cliLoginError = $state("");

  // Derive merged platform list (static presets + dynamic custom endpoints)
  let platformList = $derived(buildPlatformList(platformCredentials));

  // Group platforms by category for the grid; sort alphabetically within each group,
  // but pin Anthropic first (official default) in the provider group.
  let groupedPlatforms = $derived.by(() => {
    const groups: { id: string; label: string; items: PlatformPreset[] }[] = [];
    for (const cat of PRESET_CATEGORIES) {
      const items = platformList.filter((p) => p.id !== "custom" && p.category === cat.id);
      const sorted = [...items].sort((a, b) => a.name.localeCompare(b.name));
      if (cat.id === "provider") {
        const idx = sorted.findIndex((p) => p.id === "anthropic");
        if (idx > 0) sorted.unshift(sorted.splice(idx, 1)[0]);
      }
      groups.push({ id: cat.id, label: cat.label, items: sorted });
    }
    return groups;
  });

  // Derive selected platform from id (search merged list, not just static presets)
  let selectedPlatform = $derived<PlatformPreset | null>(
    selectedPlatformId ? (platformList.find((p) => p.id === selectedPlatformId) ?? null) : null,
  );

  // Custom endpoint editing state
  // ── Local proxy detection state ──
  let localProxyStatus = $state<import("$lib/types").LocalProxyStatus | null>(null);
  let localProxyChecking = $state(false);
  let localProxyRequestId = $state(0);
  let localAdvancedOpen = $state(false);
  let localProxyStatuses = $state<Record<string, { running: boolean; needsAuth: boolean }>>({});

  // ── API connectivity test state ──
  let apiTestLoading = $state(false);
  let apiTestResult = $state<import("$lib/types").ApiTestResult | null>(null);
  let apiTestRequestId = $state(0);
  // Derive effective auth env var (tracks platformCredentials + selectedPlatformId)
  let effectiveAuthEnvVar = $derived(
    findCredential(platformCredentials, selectedPlatformId ?? "")?.auth_env_var ||
      selectedPlatform?.auth_env_var ||
      "ANTHROPIC_API_KEY",
  );
  // Clear stale test result AND invalidate in-flight requests when any relevant input changes
  $effect(() => {
    void anthropicApiKey;
    void anthropicBaseUrl;
    void modelOpus;
    void modelSonnet;
    void modelHaiku;
    void effectiveAuthEnvVar;
    return () => {
      apiTestResult = null;
      apiTestRequestId++; // invalidate in-flight request
      apiTestLoading = false;
    };
  });

  // ── Web Server state (desktop-only) ──
  let webToken = $state<string | null>(null);
  let webStatus = $state<{
    enabled: boolean;
    running: boolean;
    port: number;
    bind: string;
    warning?: string;
  } | null>(null);
  let showWebToken = $state(false);
  let webTokenCopied = $state(false);
  let webLinkCopied = $state(false);
  let webRestarting = $state(false);
  let webRestartError = $state<string | null>(null);
  let webRestartWarning = $state<string | null>(null);
  let webPortInput = $state("9476");
  let webOriginInput = $state("");
  let webBindValue = $state("127.0.0.1");
  let webOrigins = $state<string[]>([]);
  let webOriginError = $state<string | null>(null);
  let webAdvancedOpen = $state(false);
  let webLanIp = $state<string | null>(null);
  let webTunnelUrl = $state("");
  let webTunnelError = $state<string | null>(null);
  let webTunnelLinkCopied = $state(false);
  let lanIpRequestId = $state(0);

  let debugOn = $state(isDebugMode());
  let logCopied = $state(false);
  let debugFilter = $state(getDebugFilter() || "1");

  // ── UI Zoom state (desktop-only) ──
  let zoomPreview = $state(1.0);

  $effect(() => {
    if (settings) {
      zoomPreview = Math.min(1.5, Math.max(0.75, settings.ui_zoom ?? 1.0));
    }
  });

  function clampZoom(v: number): number | null {
    if (!Number.isFinite(v)) return null;
    return Math.min(1.5, Math.max(0.75, v));
  }

  let pendingZoom: number | null = null;
  let zoomFlying = false;

  async function applyZoomQueued(factor: number) {
    if (zoomFlying) {
      pendingZoom = factor;
      return;
    }

    zoomFlying = true;
    try {
      await platform.window.setZoom(factor);
      dbg("settings", "applyZoomQueued", { factor });
    } catch (e) {
      dbgWarn("settings", "applyZoomQueued failed", e);
    }
    zoomFlying = false;

    if (pendingZoom !== null) {
      const next = pendingZoom;
      pendingZoom = null;
      void applyZoomQueued(next);
    }
  }

  function previewZoom(raw: number) {
    const factor = clampZoom(raw);
    if (factor === null) return;
    zoomPreview = factor;
  }

  let displaySaved = $state(false);

  async function commitZoom(raw: number) {
    const factor = clampZoom(raw);
    if (factor === null) return;

    // Persist
    try {
      settings = await api.updateUserSettings({ ui_zoom: factor });
      dbg("settings", "commitZoom saved", { factor });
      displaySaved = true;
      setTimeout(() => (displaySaved = false), 1500);
    } catch (e) {
      dbgWarn("settings", "commitZoom save failed", e);
      // Rollback to last persisted value
      const fallback = Math.min(1.5, Math.max(0.75, settings?.ui_zoom ?? 1.0));
      zoomPreview = fallback;
      pendingZoom = null;
      void applyZoomQueued(fallback);
      return;
    }

    // Apply final value via queue (overrides any stale preview)
    pendingZoom = null;
    void applyZoomQueued(factor);
  }
  let logCount = $state(getDebugLogCount());
  let rustCmdCopied = $state(false);
  let currentUsername = $state("");

  // ── Remote host state ──
  let remoteHosts = $state<RemoteHost[]>([]);

  async function deleteRemoteHost(name: string) {
    const updated = remoteHosts.filter((h) => h.name !== name);
    try {
      await api.updateUserSettings({ remote_hosts: updated } as Partial<UserSettings>);
      remoteHosts = updated;
      dbg("settings", "remote host deleted", name);
    } catch (e) {
      dbgWarn("settings", "delete remote host failed", e);
    }
  }

  // Keybinding store from layout context
  const keybindingStore = getContext<KeybindingStore>("keybindings");
  let cliSectionOpen = $state(false);
  let codexCliSectionOpen = $state(false);
  let cliSource = $state<"defaults" | "file">("defaults");

  // Keybinding conflict warning for recording editor
  let recordingConflict = $state("");

  // Derived keybinding groups
  let appBindings = $derived(
    keybindingStore.resolved.filter((b) => b.source === "app" && b.editable),
  );
  let fixedBindings = $derived(
    keybindingStore.resolved.filter((b) => b.source === "app" && !b.editable),
  );
  let cliBindings = $derived(keybindingStore.resolved.filter((b) => b.source === "cli"));
  let codexCliBindings = $derived(keybindingStore.resolved.filter((b) => b.source === "codex"));
  let hasOverrides = $derived(keybindingStore.overrides.length > 0);

  function isOverridden(command: string): boolean {
    return keybindingStore.overrides.some((o) => o.command === command);
  }

  function getConflictWarning(key: string, context: string, excludeCmd: string): string {
    const conflict = keybindingStore.findConflict(key, context, excludeCmd);
    return conflict ? t("settings_shortcuts_conflictsWith", { label: conflict.label }) : "";
  }

  // ── Codex status ──
  let codexStatus = $state<CodexAuthResult | null>(null);
  let codexStatusLoading = $state(false);

  async function loadCodexStatus(): Promise<CodexAuthResult | null> {
    codexStatusLoading = true;
    try {
      const next = await api.checkCodexAuth();
      codexStatus = next;
      return next;
    } catch {
      codexStatus = null;
      return null;
    } finally {
      codexStatusLoading = false;
    }
  }

  async function syncCodexSubscriptionProvider(
    forceRefresh = false,
    auth: CodexAuthResult | null = codexStatus,
  ) {
    if (!settings || auth?.subscription_logged_in !== true) return;
    const existing = (settings.global_providers ?? []).find(
      (provider) => provider.id === CODEX_SUBSCRIPTION_PROVIDER_ID,
    );
    if (!forceRefresh && existing?.models?.length) return;

    try {
      const catalog = await api.getCodexModels(forceRefresh);
      const subscriptionProvider = buildCodexSubscriptionProvider(catalog, existing);
      const nextProviders = [
        ...(settings.global_providers ?? []).filter(
          (provider) => provider.id !== CODEX_SUBSCRIPTION_PROVIDER_ID,
        ),
        subscriptionProvider,
      ];
      const saved = await api.updateUserSettings({ global_providers: nextProviders });
      settings = saved;
      globalProviders = saved.global_providers ?? nextProviders;
    } catch (e) {
      // The subscription login remains usable even if native model discovery is temporarily
      // unavailable; the backend command supplies a curated fallback catalog.
      codexLoginError = `Codex 已登录，但模型同步失败：${e instanceof Error ? e.message : String(e)}`;
      dbgWarn("settings", "syncCodexSubscriptionProvider failed", e);
    }
  }

  // ── Codex per-session flags (AgentSettings on codex agent) ──
  let codexAgentSettings = $state<AgentSettings | null>(null);

  async function loadCodexAgentSettings() {
    try {
      codexAgentSettings = await api.getAgentSettings("codex");
      dbg("settings", "codex agent settings loaded", codexAgentSettings);
    } catch (e) {
      dbgWarn("settings", "loadCodexAgentSettings failed", e);
      codexAgentSettings = null;
    }
  }

  async function saveCodexAgentPatch(patch: Partial<AgentSettings>) {
    if (!codexAgentSettings) return;
    try {
      const updated = await api.updateAgentSettings("codex", patch);
      codexAgentSettings = updated;
      dbg("settings", "codex agent settings saved", patch);
    } catch (e) {
      dbgWarn("settings", "saveCodexAgentPatch failed", e);
    }
  }

  // ── Codex third-party provider ──
  // Draft form state for the Codex provider section (committed via saveCodexProvider).
  let codexProviderId = $state<string>(""); // "" = None (plain codex login)
  let codexProviderKey = $state("");
  let codexProviderModel = $state("");
  let codexProviderBaseUrl = $state(""); // editable for "custom"
  // Auth Mode toggle, mirroring Claude: "cli" = codex login (provider None),
  // "app" = app points Codex at a third-party Responses provider. Explicit
  // state (not derived) so "app" can be selected before a preset is picked.
  let codexAuthMode = $state<"cli" | "app">("cli");
  let codexTransport = $state<"app_server" | "exec">("app_server");

  // Initialize the draft from the saved provider once settings load.
  let codexProviderInitDone = false;
  $effect(() => {
    if (!settings || codexProviderInitDone) return;
    codexProviderInitDone = true;
    const cp = settings.codex_provider;
    if (cp) {
      codexProviderId = cp.id;
      codexProviderKey = cp.api_key ?? "";
      codexProviderModel = cp.model ?? "";
      codexProviderBaseUrl = cp.base_url ?? "";
      codexAuthMode = "app";
    }
  });

  function setCodexAuthMode(mode: "cli" | "app") {
    codexAuthMode = mode;
  }

  function selectCodexProvider(preset: CodexProviderPreset | null) {
    if (!preset) {
      codexProviderId = "";
      void saveCodexProvider(null);
      return;
    }
    codexProviderId = preset.id;
    codexProviderModel = preset.model;
    codexProviderBaseUrl = preset.base_url;
    if (preset.keyless) codexProviderKey = "";
  }

  async function saveCodexProvider(clear?: null) {
    if (clear === null || !codexProviderId) {
      settings = await api.updateUserSettings({ codex_provider: null } as Partial<UserSettings>);
      return;
    }
    const preset = CODEX_PROVIDER_PRESETS.find((p) => p.id === codexProviderId);
    if (!preset) return;
    const cred: CodexProviderCredential = {
      id: preset.id,
      name: preset.name,
      base_url: codexProviderBaseUrl.trim() || preset.base_url,
      env_key: preset.env_key,
      wire_api: "responses",
      model: codexProviderModel.trim(),
      api_key: preset.keyless ? undefined : codexProviderKey.trim() || undefined,
    };
    settings = await api.updateUserSettings({
      codex_provider: cred,
    } as Partial<UserSettings>);
    dbg("settings", "codex provider saved", { id: cred.id });
  }

  let codexProviderPreset = $derived(
    CODEX_PROVIDER_PRESETS.find((p) => p.id === codexProviderId) ?? null,
  );

  // ── Pi status, provider and session settings ──
  let piStatus = $state<CliCheckResult | null>(null);
  let piStatusLoading = $state(false);
  let piAuth = $state<PiAuthResult | null>(null);
  let piLoginOpen = $state(false);
  let piLoginError = $state("");
  let piLogoutLoading = $state(false);
  let piAgentSettings = $state<AgentSettings | null>(null);
  let piAuthMode = $state<"cli" | "app">("cli");
  let piProviderId = $state("");
  let piProviderKey = $state("");
  let piProviderBaseUrl = $state("");
  let piProviderModel = $state("");
  let piModelCatalog = $state<CliModelInfo[]>([]);
  let piSaveMessage = $state("");
  let piSaveError = $state("");

  let piProviderPreset = $derived(
    PI_PROVIDER_PRESETS.find((preset) => preset.id === piProviderId) ?? null,
  );

  let piProviderInitDone = false;
  $effect(() => {
    if (!settings || piProviderInitDone) return;
    piProviderInitDone = true;
    const provider = settings.pi_provider;
    if (provider) {
      piAuthMode = "app";
      piProviderId = provider.id;
      piProviderKey = provider.api_key ?? "";
      piProviderBaseUrl = provider.base_url;
      piProviderModel = provider.model;
    }
  });

  async function refreshPi() {
    piStatusLoading = true;
    try {
      [piStatus, piAgentSettings] = await Promise.all([
        api.checkAgentCli("pi"),
        api.getAgentSettings("pi"),
      ]);
      if (!piStatus.found || piStatus.version_supported === false) {
        piAuth = null;
        piModelCatalog = [];
        return;
      }
      [piAuth, piModelCatalog] = await Promise.all([
        api.checkPiAuth(),
        api.getPiModels().catch((e) => {
          dbgWarn("settings", "failed to load Pi model catalog", e);
          return [] as CliModelInfo[];
        }),
      ]);
      if (piAgentSettings.effort?.trim() === "none") {
        piAgentSettings = { ...piAgentSettings, effort: "off" };
        try {
          piAgentSettings = await api.updateAgentSettings("pi", { effort: "off" });
        } catch (e) {
          dbgWarn("settings", "failed to migrate legacy Pi thinking level", e);
        }
      }
      const configuredModel = piProviderModel || piAgentSettings.model || "";
      const provider = piAuth.provider ?? piStatus.current_provider;
      const normalizedModel =
        piAuthMode === "cli" && piAuth.logged_in
          ? normalizePiCliModel(provider, configuredModel)
          : configuredModel.trim();
      piProviderModel = normalizedModel;

      // The login modal fixes this during a fresh OAuth flow. Repeat the same
      // migration on page refresh so existing users do not keep booting Pi
      // with the old managed-provider placeholder (`glm-5.2`).
      if (normalizedModel !== (piAgentSettings.model ?? "").trim()) {
        try {
          piAgentSettings = await api.updateAgentSettings("pi", { model: normalizedModel });
        } catch (e) {
          dbgWarn("settings", "failed to migrate Pi CLI model", e);
        }
      }
      void loadPiSharedExtensions();
    } catch (e) {
      dbgWarn("settings", "refreshPi failed", e);
      piStatus = null;
    } finally {
      piStatusLoading = false;
    }
  }

  let piSharedExtensions = $state<InstalledPlugin[]>([]);

  async function loadPiSharedExtensions() {
    try {
      piSharedExtensions = await api.listPiSharedExtensions("code");
    } catch (e) {
      dbgWarn("settings", "failed to load Pi shared extensions", e);
    }
  }

  async function handleInstallPiSharedExtension(source: string) {
    const res = await api.installPiSharedExtension(source);
    if (!res.success) throw new Error(res.message || `安装失败: ${source}`);
    await loadPiSharedExtensions();
  }

  async function handleTogglePiSharedExtension(plugin: InstalledPlugin, enabled: boolean) {
    const id = plugin.pluginId ?? plugin.name;
    await api.togglePiSharedExtension("code", id, enabled);
    await loadPiSharedExtensions();
  }

  async function handleUpdatePiSharedExtension(plugin: InstalledPlugin) {
    const id = plugin.pluginId ?? plugin.name;
    const res = await api.updatePiSharedExtension(id);
    if (!res.success) throw new Error(res.message || `更新失败: ${plugin.name}`);
    await loadPiSharedExtensions();
  }

  async function handleUninstallPiSharedExtension(plugin: InstalledPlugin) {
    const id = plugin.pluginId ?? plugin.name;
    const res = await api.uninstallPiSharedExtension(id);
    if (!res.success) throw new Error(res.message || `卸载失败: ${plugin.name}`);
    await loadPiSharedExtensions();
  }

  function openPiLogin() {
    piLoginError = "";
    piLoginOpen = true;
  }

  async function handlePiAuthChanged(status: PiAuthResult) {
    piAuth = status;
    if (!status.logged_in) return;

    // Pi's native OAuth provider is selected by the model prefix. Persist it
    // in AgentCabin's Pi settings so the next RPC actor starts on Codex.
    const normalizedModel = normalizePiCliModel(status.provider ?? "openai-codex", piProviderModel);
    if (normalizedModel !== piProviderModel.trim()) {
      piProviderModel = normalizedModel;
      try {
        await savePiSettings();
      } catch (e) {
        piLoginError = String(e);
      }
    }
  }

  async function logoutPiCodex() {
    piLogoutLoading = true;
    piLoginError = "";
    try {
      await api.runPiLogout();
      await refreshPi();
    } catch (e) {
      piLoginError = String(e);
    } finally {
      piLogoutLoading = false;
    }
  }

  function setPiAuthMode(mode: "cli" | "app") {
    piAuthMode = mode;
    if (mode === "cli") piProviderId = "";
  }

  function selectPiProvider(preset: PiProviderPreset) {
    piProviderId = preset.id;
    piProviderBaseUrl = preset.base_url;
    piProviderModel = preset.model;
  }

  async function savePiSettings(skipProviderBinding = false) {
    piSaveError = "";
    piSaveMessage = "";
    try {
      const model = piAuthMode === "cli" ? piProviderModel.trim() : "";
      if (
        model &&
        piModelCatalog.length > 0 &&
        !piModelCatalog.some((item) => item.value === model)
      ) {
        piSaveError = t("settings_pi_modelInvalid", { model });
        return false;
      }
      // Authentication mode is also draft state. Persist it together with the
      // Pi-specific settings only when this button is explicitly pressed.
      if (!skipProviderBinding) {
        const bindingSaved = await saveAgentProviderBinding(
          "pi",
          piAuthMode === "cli" ? "cli" : "custom",
        );
        if (!bindingSaved) {
          piSaveError = t("settings_saveFailed");
          return false;
        }
      }
      piAgentSettings = await api.updateAgentSettings("pi", {
        // Null clears the AgentCabin override so Pi falls back to its own default model.
        model: piAuthMode === "cli" ? model || null : null,
      } as Partial<AgentSettings>);
      piSaveMessage = t("settings_saveSuccess");
      showAgentSaveNotice("pi");
      return true;
    } catch (e) {
      piSaveError = String(e);
      return false;
    }
  }

  async function savePiAgentPatch(patch: Partial<AgentSettings>) {
    try {
      piAgentSettings = await api.updateAgentSettings("pi", patch);
      dbg("settings", "savePiAgentPatch saved", patch);
    } catch (e) {
      dbgWarn("settings", "savePiAgentPatch failed", e);
    }
  }

  async function refreshCodexAll(forceSubscriptionRefresh = false) {
    const status = await loadCodexStatus();
    if (status?.installed) {
      void loadCodexAgentSettings();
    } else {
      codexAgentSettings = null;
    }
    await syncCodexSubscriptionProvider(forceSubscriptionRefresh, status);
  }

  // ── Codex login ──
  let codexLoginLoading = $state(false);
  let codexLoginError = $state("");

  async function handleCodexLogin() {
    codexLoginLoading = true;
    codexLoginError = "";
    try {
      await api.runCodexLogin();
      await refreshCodexAll(true);
    } catch (e) {
      codexLoginError = e instanceof Error ? e.message : String(e);
      dbgWarn("settings", "codex login failed", e);
    } finally {
      codexLoginLoading = false;
    }
  }

  async function handleCodexSubscriptionLogin() {
    codexLoginLoading = true;
    codexLoginError = "";
    try {
      await api.runCodexSubscriptionLogin();
      await refreshCodexAll(true);
    } catch (e) {
      codexLoginError = e instanceof Error ? e.message : String(e);
      dbgWarn("settings", "codex subscription login failed", e);
    } finally {
      codexLoginLoading = false;
    }
  }

  async function handleCodexSubscriptionReopen() {
    try {
      await api.reopenCodexSubscriptionLogin();
    } catch (e) {
      codexLoginError = e instanceof Error ? e.message : String(e);
      dbgWarn("settings", "reopen codex subscription login failed", e);
    }
  }

  async function handleCodexSubscriptionLogout() {
    codexLoginLoading = true;
    codexLoginError = "";
    try {
      await api.runCodexSubscriptionLogout();
      await refreshCodexAll(true);
    } catch (e) {
      codexLoginError = e instanceof Error ? e.message : String(e);
      dbgWarn("settings", "codex subscription logout failed", e);
    } finally {
      codexLoginLoading = false;
    }
  }

  // ── CLI Config state ──
  let cliConfig = $state<Record<string, unknown>>({});
  let projectCliConfig = $state<Record<string, unknown>>({});
  let cliConfigLoaded = $state(false);
  let cliConfigLoading = $state(false);
  let cliConfigError = $state("");

  // Custom CLI launch path/commands
  let claudePathInput = $state("");
  let claudePathSaved = $state(false);
  let codexPathInput = $state("");
  let codexPathSaved = $state(false);
  let piPathInput = $state("");
  let piPathSaved = $state(false);
  let grokPathInput = $state("");
  let grokPathSaved = $state(false);
  let dshPathInput = $state("");
  let dshPathSaved = $state(false);
  let dshCmdCopied = $state(false);
  let cliAgentTab = $state<"all" | "claude" | "codex" | "pi" | "grok" | "dsh">("all");

  // ── Worktree settings ──
  let worktreeAdvancedOpen = $state(urlTab === "worktrees");
  let worktreeRootInput = $state("");
  let worktreeBranchPrefixInput = $state("agentcabin/");
  let worktreeCleanupLimitInput = $state(15);
  let worktreeSaved = $state(false);
  let worktreeSaveError = $state("");

  $effect(() => {
    if (settings) {
      claudePathInput = settings.claude_path ?? "";
      codexPathInput = settings.codex_path ?? "";
      piPathInput = settings.pi_path ?? "";
      grokPathInput = settings.grok_path ?? "";
      dshPathInput = settings.dsh_path ?? "";
      worktreeRootInput = settings.worktree_root ?? "";
      worktreeBranchPrefixInput = settings.worktree_branch_prefix || "";
      worktreeCleanupLimitInput = settings.worktree_cleanup_limit || 15;
    }
  });

  async function saveWorktreePatch(patch: Partial<UserSettings>) {
    worktreeSaveError = "";
    try {
      settings = await api.updateUserSettings(patch);
      worktreeSaved = true;
      setTimeout(() => (worktreeSaved = false), 1500);
    } catch (e) {
      worktreeSaveError = e instanceof Error ? e.message : String(e);
      dbgWarn("settings", "saveWorktreePatch failed", e);
    }
  }

  function saveWorktreeRoot() {
    void saveWorktreePatch({ worktree_root: worktreeRootInput.trim() });
  }

  function saveWorktreeBranchPrefix() {
    void saveWorktreePatch({ worktree_branch_prefix: worktreeBranchPrefixInput.trim() });
  }

  function saveWorktreeCleanupLimit() {
    const value = Math.max(1, Math.min(1000, Math.round(worktreeCleanupLimitInput || 15)));
    worktreeCleanupLimitInput = value;
    void saveWorktreePatch({ worktree_cleanup_limit: value });
  }

  async function saveClaudePath() {
    const next = claudePathInput.trim();
    if ((settings?.claude_path ?? "") === next) return;
    try {
      settings = await api.updateUserSettings({ claude_path: next });
      claudePathSaved = true;
      setTimeout(() => (claudePathSaved = false), 1500);
      dbg("settings", "claude_path saved", { path: next });
    } catch (e) {
      dbgWarn("settings", "saveClaudePath failed", e);
    }
  }

  async function saveCodexPath() {
    const next = codexPathInput.trim();
    if ((settings?.codex_path ?? "") === next) return;
    try {
      settings = await api.updateUserSettings({ codex_path: next });
      codexPathSaved = true;
      setTimeout(() => (codexPathSaved = false), 1500);
      dbg("settings", "codex_path saved", { path: next });
    } catch (e) {
      dbgWarn("settings", "saveCodexPath failed", e);
    }
  }

  async function savePiPath() {
    const next = piPathInput.trim();
    if ((settings?.pi_path ?? "") === next) return;
    try {
      settings = await api.updateUserSettings({ pi_path: next });
      piPathSaved = true;
      setTimeout(() => (piPathSaved = false), 1500);
      dbg("settings", "pi_path saved", { path: next });
    } catch (e) {
      dbgWarn("settings", "savePiPath failed", e);
    }
  }

  async function saveGrokPath() {
    const next = grokPathInput.trim();
    if ((settings?.grok_path ?? "") === next) return;
    try {
      settings = await api.updateUserSettings({ grok_path: next });
      grokPathSaved = true;
      setTimeout(() => (grokPathSaved = false), 1500);
      dbg("settings", "grok_path saved", { path: next });
    } catch (e) {
      dbgWarn("settings", "saveGrokPath failed", e);
    }
  }

  async function saveDshPath() {
    const next = dshPathInput.trim();
    if ((settings?.dsh_path ?? "") === next) return;
    try {
      settings = await api.updateUserSettings({ dsh_path: next });
      dshPathSaved = true;
      setTimeout(() => (dshPathSaved = false), 1500);
      dbg("settings", "dsh_path saved", { path: next });
    } catch (e) {
      dbgWarn("settings", "saveDshPath failed", e);
    }
  }

  // CLI Config setting definitions
  const CLI_CONFIG_SETTINGS: CliConfigSettingDef[] = [
    // Behavior
    {
      key: "thinkingEnabled",
      label: t("settings_cliConfig_thinkingModeLabel"),
      description: t("settings_cliConfig_thinkingModeDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "effortLevel",
      label: t("settings_cliConfig_effortLevelLabel"),
      description: t("settings_cliConfig_effortLevelDesc"),
      group: "behavior",
      type: "enum",
      default: "high",
      options: [
        { value: "low", label: t("settings_codexConfig_optLow") },
        { value: "medium", label: t("settings_codexConfig_optMedium") },
        { value: "high", label: t("settings_codexConfig_optHigh") },
        { value: "xhigh", label: t("settings_codexConfig_optXHigh") },
        { value: "max", label: t("settings_cliConfig_effortLevelMax") },
      ],
    },
    {
      key: "fastMode",
      label: t("settings_cliConfig_fastModeLabel"),
      description: t("settings_cliConfig_fastModeDesc"),
      group: "behavior",
      type: "boolean",
      default: false,
    },
    {
      key: "autoCompactEnabled",
      label: t("settings_cliConfig_autoCompactLabel"),
      description: t("settings_cliConfig_autoCompactDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "fileCheckpointingEnabled",
      label: t("settings_cliConfig_fileCheckpointsLabel"),
      description: t("settings_cliConfig_fileCheckpointsDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "respectGitignore",
      label: t("settings_cliConfig_respectGitignoreLabel"),
      description: t("settings_cliConfig_respectGitignoreDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "verbose",
      label: t("settings_cliConfig_verboseLabel"),
      description: t("settings_cliConfig_verboseDesc"),
      group: "behavior",
      type: "boolean",
      default: false,
    },
    {
      key: "defaultPermissionMode",
      label: t("settings_cliConfig_permissionModeLabel"),
      description: t("settings_cliConfig_permissionModeDesc"),
      group: "behavior",
      type: "enum",
      default: undefined,
      options: [
        { value: "default", label: t("settings_cliConfig_optDefault") },
        { value: "plan", label: t("settings_cliConfig_optPlan") },
        { value: "acceptEdits", label: t("settings_cliConfig_optAutoEdit") },
        { value: "bypassPermissions", label: t("settings_cliConfig_optFullAuto") },
      ],
    },
    {
      key: "teammateMode",
      label: t("settings_cliConfig_teammateModeLabel"),
      description: t("settings_cliConfig_teammateModeDesc"),
      group: "behavior",
      type: "enum",
      default: "auto",
      options: [
        { value: "auto", label: t("settings_cliConfig_optAuto") },
        { value: "always", label: t("settings_cliConfig_optAlways") },
        { value: "never", label: t("settings_cliConfig_optNever") },
      ],
    },
    // Appearance
    {
      key: "theme",
      label: t("settings_cliConfig_cliThemeLabel"),
      description: t("settings_cliConfig_cliThemeDesc"),
      group: "appearance",
      type: "enum",
      default: "dark",
      options: [
        { value: "dark", label: t("settings_cliConfig_optDark") },
        { value: "light", label: t("settings_cliConfig_optLight") },
        { value: "light-high-contrast", label: t("settings_cliConfig_optHighContrast") },
      ],
    },
    {
      key: "prefersReducedMotion",
      label: t("settings_cliConfig_reduceMotionLabel"),
      description: t("settings_cliConfig_reduceMotionDesc"),
      group: "appearance",
      type: "boolean",
      default: false,
    },
    {
      key: "language",
      label: t("settings_cliConfig_responseLangLabel"),
      description: t("settings_cliConfig_responseLangDesc"),
      group: "appearance",
      type: "string",
      default: undefined,
    },
    {
      key: "outputStyle",
      label: t("settings_cliConfig_outputStyleLabel"),
      description: t("settings_cliConfig_outputStyleDesc"),
      group: "appearance",
      type: "string",
      default: undefined,
    },
    // Advanced
    {
      key: "autoConnectIde",
      label: t("settings_cliConfig_autoConnectIdeLabel"),
      description: t("settings_cliConfig_autoConnectIdeDesc"),
      group: "advanced",
      type: "boolean",
      default: false,
    },
    {
      key: "promptSuggestionsEnabled",
      label: t("settings_cliConfig_promptSuggestionsLabel"),
      description: t("settings_cliConfig_promptSuggestionsDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "spinnerTipsEnabled",
      label: t("settings_cliConfig_spinnerTipsLabel"),
      description: t("settings_cliConfig_spinnerTipsDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "codeDiffFooterEnabled",
      label: t("settings_cliConfig_codeDiffFooterLabel"),
      description: t("settings_cliConfig_codeDiffFooterDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "prStatusFooterEnabled",
      label: t("settings_cliConfig_prStatusFooterLabel"),
      description: t("settings_cliConfig_prStatusFooterDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "autoUpdatesChannel",
      label: t("settings_cliConfig_updateChannelLabel"),
      description: t("settings_cliConfig_updateChannelDesc"),
      group: "advanced",
      type: "enum",
      default: undefined,
      options: [
        { value: "latest", label: t("settings_cliConfig_optLatest") },
        { value: "stable", label: t("settings_cliConfig_optStable") },
      ],
    },
    {
      key: "preferredNotifChannel",
      label: t("settings_cliConfig_notifChannelLabel"),
      description: t("settings_cliConfig_notifChannelDesc"),
      group: "advanced",
      type: "enum",
      default: "auto",
      options: [
        { value: "auto", label: t("settings_cliConfig_optAuto") },
        { value: "iterm2", label: t("settings_cliConfig_optIterm2") },
        { value: "terminal_bell", label: t("settings_cliConfig_optTerminalBell") },
      ],
    },
  ];

  const behaviorSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "behavior");
  const appearanceSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "appearance");
  const advancedSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "advanced");

  // ── Codex Config state ──
  let codexConfig = $state<Record<string, unknown>>({});
  let projectCodexConfig = $state<Record<string, unknown>>({});
  let codexConfigWarning = $state<string | undefined>(undefined);

  // Codex Config setting definitions
  const CODEX_CONFIG_SETTINGS: CliConfigSettingDef[] = [
    {
      key: "model",
      label: t("settings_codexConfig_modelLabel"),
      description: t("settings_codexConfig_modelDesc"),
      group: "behavior",
      type: "string",
      default: undefined,
    },
    {
      key: "model_reasoning_effort",
      label: t("settings_codexConfig_reasoningEffortLabel"),
      description: t("settings_codexConfig_reasoningEffortDesc"),
      group: "behavior",
      type: "enum",
      default: "medium",
      options: [
        { value: "low", label: t("settings_codexConfig_effortLight") },
        { value: "medium", label: t("settings_codexConfig_effortMedium") },
        { value: "high", label: t("settings_codexConfig_optHigh") },
        { value: "xhigh", label: t("settings_codexConfig_optXHigh") },
        { value: "max", label: t("settings_codexConfig_effortMax") },
      ],
    },
    {
      key: "model_reasoning_summary",
      label: t("settings_codexConfig_reasoningSummaryLabel"),
      description: t("settings_codexConfig_reasoningSummaryDesc"),
      group: "behavior",
      type: "enum",
      default: "auto",
      options: [
        { value: "none", label: t("settings_codexConfig_optNone") },
        { value: "auto", label: t("settings_codexConfig_optAuto") },
        { value: "concise", label: t("settings_codexConfig_optConcise") },
        { value: "detailed", label: t("settings_codexConfig_optDetailed") },
      ],
    },
    {
      key: "model_verbosity",
      label: t("settings_codexConfig_verbosityLabel"),
      description: t("settings_codexConfig_verbosityDesc"),
      group: "behavior",
      type: "enum",
      default: "medium",
      options: [
        { value: "low", label: t("settings_codexConfig_optLow") },
        { value: "medium", label: t("settings_codexConfig_optMedium") },
        { value: "high", label: t("settings_codexConfig_optHigh") },
      ],
    },
    {
      key: "personality",
      label: t("settings_codexConfig_personalityLabel"),
      description: t("settings_codexConfig_personalityDesc"),
      group: "behavior",
      type: "string",
      default: undefined,
    },
    {
      key: "approval_policy",
      label: t("settings_codexConfig_approvalPolicyLabel"),
      description: t("settings_codexConfig_approvalPolicyDesc"),
      group: "behavior",
      type: "enum",
      default: "on-request",
      options: [
        { value: "on-request", label: t("settings_codexConfig_optOnRequest") },
        { value: "untrusted", label: t("settings_codexConfig_optUntrusted") },
        { value: "on-failure", label: t("settings_codexConfig_optOnFailure") },
        { value: "never", label: t("settings_codexConfig_optNever") },
      ],
    },
    {
      key: "sandbox_mode",
      label: t("settings_codexConfig_sandboxLabel"),
      description: t("settings_codexConfig_sandboxDesc"),
      group: "behavior",
      type: "enum",
      default: "read-only",
      options: [
        { value: "read-only", label: t("settings_codexConfig_optReadOnly") },
        { value: "workspace-write", label: t("settings_codexConfig_optWorkspaceWrite") },
        { value: "danger-full-access", label: t("settings_codexConfig_optFullAccess") },
      ],
    },
    {
      key: "web_search",
      label: t("settings_codexConfig_webSearchLabel"),
      description: t("settings_codexConfig_webSearchDesc"),
      group: "behavior",
      type: "enum",
      // Durable tri-state web search mode. The `cached` value is currently
      // unreachable via the per-session --search boolean (which maps to live);
      // this config key is the only way to select it.
      default: "disabled",
      options: [
        { value: "disabled", label: t("settings_codexConfig_optDisabled") },
        { value: "cached", label: t("settings_codexConfig_optCached") },
        { value: "live", label: t("settings_codexConfig_optLive") },
      ],
    },
  ];

  function getCodexConfigValue(key: string, def: CliConfigSettingDef): unknown {
    return key in codexConfig ? codexConfig[key] : def.default;
  }

  function isCodexProjectOverride(key: string): boolean {
    return key in projectCodexConfig;
  }

  /** Render display for unknown/unrecognized values */
  function codexValueDisplay(key: string, def: CliConfigSettingDef): string | null {
    const val = codexConfig[key];
    if (val === undefined || val === null) return null;
    // If it's a table/object → custom table value
    if (typeof val === "object" && !Array.isArray(val)) {
      return t("settings_codexConfig_customTable");
    }
    // If it's an enum type and value doesn't match known options
    if (def.type === "enum" && def.options) {
      const known = def.options.some((o) => o.value === val);
      if (!known && typeof val === "string") {
        return t("settings_codexConfig_unrecognized", { value: val });
      }
    }
    return null;
  }

  async function saveCodexConfigPatch(key: string, value: unknown) {
    dbg("settings", "saveCodexConfigPatch", { key, value });
    try {
      codexConfig = await api.updateCodexConfig({ [key]: value ?? null });
    } catch (e) {
      dbgWarn("settings", "saveCodexConfigPatch error", e);
    }
  }

  function getCliConfigValue(key: string, def: CliConfigSettingDef): unknown {
    return key in cliConfig ? cliConfig[key] : def.default;
  }

  function isProjectOverride(key: string): boolean {
    return key in projectCliConfig;
  }

  async function saveCliConfigPatch(key: string, value: unknown) {
    dbg("settings", "saveCliConfigPatch", { key, value });
    try {
      // null value = delete key (restore CLI default)
      cliConfig = await api.updateCliConfig({ [key]: value ?? null });
    } catch (e) {
      dbgWarn("settings", "saveCliConfigPatch error", e);
    }
  }

  async function loadCliConfig() {
    if (cliConfigLoading) return;
    cliConfigLoading = true;
    cliConfigError = "";
    try {
      const cwd = getSavedProjectCwd() || "";
      // Load Claude + Codex configs in parallel
      const [claudeConfig, codexResult, projectClaude, projectCodex] = await Promise.all([
        api.getCliConfig(),
        api.getCodexConfig(),
        cwd ? api.getProjectCliConfig(cwd) : Promise.resolve({}),
        cwd ? api.getProjectCodexConfig(cwd) : Promise.resolve({}),
      ]);

      cliConfig = claudeConfig;
      projectCliConfig = projectClaude;

      // Codex config with warning support
      codexConfig = codexResult.config ?? {};
      codexConfigWarning = codexResult.warning;
      projectCodexConfig = projectCodex;

      cliConfigLoaded = true;
      dbg("settings", "cliConfig loaded", {
        keys: Object.keys(cliConfig).length,
        projectKeys: Object.keys(projectCliConfig).length,
        codexKeys: Object.keys(codexConfig).length,
        codexWarning: codexConfigWarning,
      });
    } catch (e) {
      cliConfigError = String(e);
      dbgWarn("settings", "loadCliConfig error", e);
    } finally {
      cliConfigLoading = false;
    }
  }

  // Lazy load CLI config when tab activates
  $effect(() => {
    if (
      activeTab === "runtimes" &&
      activeRuntimeSubTab === "claude" &&
      !cliConfigLoaded &&
      !cliConfigLoading
    ) {
      loadCliConfig();
    }
  });

  // Refresh log count periodically when debug is on
  $effect(() => {
    if (!debugOn) return;
    const timer = setInterval(() => {
      logCount = getDebugLogCount();
    }, 2000);
    return () => clearInterval(timer);
  });

  function detectPlatformFromUrl(url: string, activePlatformId?: string): string | null {
    // If we have a stored active_platform_id, prefer it
    if (activePlatformId) return activePlatformId;
    if (!url) return null;
    const match = PLATFORM_PRESETS.find((p) => p.base_url && url === p.base_url);
    return match?.id ?? "custom";
  }

  /** Load display fields (key + URL) from credential store for a given platform. */
  function loadFieldsFromCredential(platformId: string | null) {
    apiTestResult = null;
    if (!platformId) {
      anthropicApiKey = "";
      anthropicBaseUrl = "";
      platformExtraEnv = [];
      return;
    }
    const cred = findCredential(platformCredentials, platformId);
    const preset = PLATFORM_PRESETS.find((p) => p.id === platformId);
    anthropicApiKey = cred?.api_key ?? "";
    // base_url: credential override > preset default > empty
    anthropicBaseUrl = cred?.base_url ?? preset?.base_url ?? "";
    // models: credential override > preset default > expand to 3 tiers
    const models = cred?.models ?? preset?.models;
    const [o, s, h] = expandModelsToTiers(models);
    modelOpus = o;
    modelSonnet = s;
    modelHaiku = h;
    // extra_env: credential explicit value (including {}) takes priority; undefined falls back to preset
    const extraEnv = cred?.extra_env !== undefined ? cred.extra_env : (preset?.extra_env ?? {});
    platformExtraEnv = Object.entries(extraEnv).map(([key, value]) => ({ key, value }));
    // Don't set touched on load — touched is only driven by UI edit actions (onblur/delete row)
    dbg("settings", "loadFieldsFromCredential", {
      platformId,
      hasKey: !!anthropicApiKey,
      url: anthropicBaseUrl,
      models: [modelOpus, modelSonnet, modelHaiku],
      extraEnvKeys: Object.keys(extraEnv),
      extraEnvSource: cred?.extra_env !== undefined ? "credential" : "preset",
    });
  }

  /** Save current editing fields into the credentials array. */
  function saveCurrentToCredential() {
    if (!selectedPlatformId) return;
    const preset = PLATFORM_PRESETS.find((p) => p.id === selectedPlatformId);
    // Compress 3 tier inputs → models array; undefined when all empty (→ backend preset fallback).
    // Do NOT fall back to preset?.models here — undefined means "use provider defaults",
    // and baking preset values into credential would prevent future preset updates from taking effect.
    const modelsToSave = compressModelsFromTiers(modelOpus, modelSonnet, modelHaiku);

    // Convert extra_env array back to Record, filter empty keys, warn on duplicates
    const extraEnvRecord: Record<string, string> = {};
    const seenKeys = new Set<string>();
    for (const { key, value } of platformExtraEnv) {
      const k = key.trim();
      if (!k) continue;
      if (seenKeys.has(k)) {
        dbgWarn("settings", `duplicate extra_env key "${k}" — last value wins`);
      }
      seenKeys.add(k);
      extraEnvRecord[k] = value;
    }

    // Only write extra_env when user has touched it; otherwise preserve credential's original value
    const extraEnvToSave = extraEnvTouched[selectedPlatformId]
      ? extraEnvRecord // always write (even empty {}), distinct from undefined
      : undefined; // don't overwrite — keep credential as-is (may be undefined or old value)

    dbg("settings", "saveCurrentToCredential: extra_env", {
      platform: selectedPlatformId,
      touched: !!extraEnvTouched[selectedPlatformId],
      keys: Object.keys(extraEnvRecord),
    });

    _upsertCredential(selectedPlatformId, {
      api_key: anthropicApiKey || undefined,
      // Always save base_url — backend needs it for ANTHROPIC_BASE_URL injection
      base_url: anthropicBaseUrl || preset?.base_url || undefined,
      auth_env_var: selectedPlatform?.auth_env_var ?? preset?.auth_env_var,
      models: modelsToSave,
      ...(extraEnvToSave !== undefined ? { extra_env: extraEnvToSave } : {}),
    });
  }

  /** Sync global fields from current display state and persist everything. */
  function syncAndSave(platformId: string) {
    const preset = PLATFORM_PRESETS.find((p) => p.id === platformId);
    saveGeneralPatch({
      anthropic_api_key: anthropicApiKey || undefined,
      anthropic_base_url: anthropicBaseUrl || undefined,
      auth_env_var: preset?.auth_env_var,
      active_platform_id: platformId,
      platform_credentials: platformCredentials,
    });
  }

  function markExtraEnvTouched() {
    if (selectedPlatformId) extraEnvTouched[selectedPlatformId] = true;
  }

  /**
   * Parse pasted env text. Supported formats:
   * - KEY=value lines (with optional `export` prefix, # comments, quoted values)
   * - JSON object: { "KEY": "value", ... }
   */
  function parseEnvText(text: string): Array<{ key: string; value: string }> {
    const trimmed = text.trim();
    // Try JSON object first
    if (trimmed.startsWith("{")) {
      try {
        const obj = JSON.parse(trimmed);
        if (obj && typeof obj === "object" && !Array.isArray(obj)) {
          const results: Array<{ key: string; value: string }> = [];
          for (const [key, val] of Object.entries(obj)) {
            if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
              results.push({ key, value: String(val) });
            }
          }
          if (results.length > 0) return results;
        }
      } catch {
        // Not valid JSON, fall through to line-based parsing
      }
    }
    // Line-based: KEY=value, export KEY=value, # comments
    const results: Array<{ key: string; value: string }> = [];
    for (const raw of trimmed.split(/\r?\n/)) {
      const line = raw.trim();
      if (!line || line.startsWith("#")) continue;
      const stripped = line.replace(/^export\s+/, "");
      const eqIdx = stripped.indexOf("=");
      if (eqIdx <= 0) continue;
      const key = stripped.slice(0, eqIdx).trim();
      let value = stripped.slice(eqIdx + 1).trim();
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        value = value.slice(1, -1);
      }
      if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
        results.push({ key, value });
      }
    }
    return results;
  }

  /** Handle paste on env key input: if content looks like KEY=value lines, bulk-add them. */
  function handleEnvKeyPaste(e: ClipboardEvent, index: number) {
    const text = e.clipboardData?.getData("text/plain") ?? "";
    const parsed = parseEnvText(text);
    if (parsed.length === 0) return; // not env format, let normal paste through
    e.preventDefault();
    // Replace current (likely empty) row with first parsed entry, append rest
    const before = platformExtraEnv.slice(0, index);
    const after = platformExtraEnv.slice(index + 1);
    platformExtraEnv = [...before, ...parsed, ...after];
    markExtraEnvTouched();
    persistCurrentPlatform();
    dbg("settings", "env paste parsed", { count: parsed.length, keys: parsed.map((p) => p.key) });
  }

  /** Unified persist: save current platform fields to credential + sync to settings. */
  function persistCurrentPlatform() {
    saveCurrentToCredential();
    if (selectedPlatformId) syncAndSave(selectedPlatformId);
  }

  // ── Local proxy detection ──

  async function checkLocalProxy() {
    if (!selectedPlatform || selectedPlatform.category !== "local" || !selectedPlatformId) return;
    localProxyChecking = true;
    localProxyStatus = null;
    const myRequestId = ++localProxyRequestId;
    const myPlatformId = selectedPlatformId;
    const urlToCheck = anthropicBaseUrl;
    dbg("settings", "checkLocalProxy start", {
      id: myPlatformId,
      url: urlToCheck,
      reqId: myRequestId,
    });
    try {
      const result = await api.detectLocalProxy(myPlatformId, urlToCheck);
      if (myRequestId !== localProxyRequestId) return;
      if (myPlatformId !== selectedPlatformId) return;
      localProxyStatus = result;
      localProxyStatuses = {
        ...localProxyStatuses,
        [myPlatformId]: { running: result.running, needsAuth: result.needsAuth },
      };
      dbg("settings", "checkLocalProxy result", result);
    } catch (e) {
      if (myRequestId !== localProxyRequestId || myPlatformId !== selectedPlatformId) return;
      localProxyStatus = {
        proxyId: myPlatformId,
        running: false,
        needsAuth: false,
        baseUrl: urlToCheck,
        error: String(e),
      };
      localProxyStatuses = {
        ...localProxyStatuses,
        [myPlatformId]: { running: false, needsAuth: false },
      };
      dbgWarn("settings", "checkLocalProxy error", e);
    } finally {
      if (myRequestId === localProxyRequestId) localProxyChecking = false;
    }
  }

  async function checkAllLocalProxies() {
    const localPresets = PLATFORM_PRESETS.filter((p) => p.category === "local");
    const results = await Promise.allSettled(
      localPresets.map((p) => {
        const cred = findCredential(platformCredentials, p.id);
        const url = cred?.base_url || p.base_url;
        return api.detectLocalProxy(p.id, url);
      }),
    );
    const statuses: Record<string, { running: boolean; needsAuth: boolean }> = {};
    results.forEach((r, i) => {
      if (r.status === "fulfilled") {
        statuses[localPresets[i].id] = { running: r.value.running, needsAuth: r.value.needsAuth };
      } else {
        statuses[localPresets[i].id] = { running: false, needsAuth: false };
      }
    });
    localProxyStatuses = statuses;
    dbg("settings", "checkAllLocalProxies", statuses);
  }

  function applyPlatformPreset(preset: PlatformPreset) {
    // 1. Save current platform's data to credentials (if modified)
    saveCurrentToCredential();
    // 2. Switch to new platform
    selectedPlatformId = preset.id;
    localAdvancedOpen = false;
    localProxyStatus = null;
    // 3. Load new platform's data from credentials
    loadFieldsFromCredential(preset.id);
    // 4. Sync global fields + persist
    syncAndSave(preset.id);
    // 5. Auto-detect if local proxy
    if (preset.category === "local") {
      checkLocalProxy();
    }
  }

  /** Upsert a credential in the local platformCredentials array. */
  function _upsertCredential(platformId: string, fields: Partial<PlatformCredential>) {
    const idx = platformCredentials.findIndex((c) => c.platform_id === platformId);
    if (idx >= 0) {
      platformCredentials[idx] = { ...platformCredentials[idx], ...fields };
    } else {
      platformCredentials = [...platformCredentials, { platform_id: platformId, ...fields }];
    }
  }

  /** Add a new custom endpoint — creates with defaults and immediately selects it. */
  function addCustomEndpoint() {
    const id = `custom-${Date.now()}`;
    const cred: PlatformCredential = {
      platform_id: id,
      name: "Custom",
      base_url: "",
      auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    };
    platformCredentials = [...platformCredentials, cred];
    saveGeneralPatch({ platform_credentials: platformCredentials });
    // Select the newly created endpoint — opens full config form below
    const preset = buildPlatformList(platformCredentials).find((p) => p.id === id);
    if (preset) applyPlatformPreset(preset);
  }

  /** Delete a custom endpoint. */
  function deleteCustomEndpoint(platformId: string) {
    // Clear selection first so applyPlatformPreset won't re-save the deleted credential
    const wasActive = selectedPlatformId === platformId;
    if (wasActive) selectedPlatformId = null;
    platformCredentials = platformCredentials.filter((c) => c.platform_id !== platformId);
    saveGeneralPatch({ platform_credentials: platformCredentials });
    // If we deleted the active platform, switch to Anthropic
    if (wasActive) {
      const anthropic = PLATFORM_PRESETS.find((p) => p.id === "anthropic")!;
      applyPlatformPreset(anthropic);
    }
  }

  function openSetupWizard() {
    window.dispatchEvent(new CustomEvent("agentcabin:show-wizard"));
  }

  onMount(async () => {
    try {
      appVersion = await platform.app.getVersion();
    } catch {
      appVersion = pkg.version;
    }
    try {
      settings = await api.getUserSettings();
      globalProviders = settings.global_providers ?? [];
      codexTransport = settings.codex_transport === "exec" ? "exec" : "app_server";
      authMode = settings.auth_mode ?? "cli";
      remoteHosts = settings.remote_hosts ?? [];
      platformCredentials = settings.platform_credentials ?? [];
      // Load display fields from credentials (not global fields)
      if (authMode === "api") {
        selectedPlatformId = detectPlatformFromUrl(
          settings.anthropic_base_url ?? "",
          settings.active_platform_id,
        );
        loadFieldsFromCredential(selectedPlatformId);
      } else {
        anthropicApiKey = settings.anthropic_api_key ?? "";
        anthropicBaseUrl = settings.anthropic_base_url ?? "";
      }
      void refreshRuntimeStatuses();
      void refreshDesktopToolSettings();
    } catch (e) {
      dbgWarn("settings", "error", e);
    }
    // Load Codex status, native per-session settings, and the shared subscription catalog.
    void refreshCodexAll();
    void refreshPi();
    // Load auth overview
    api
      .getAuthOverview()
      .then((ov) => (authOverview = ov))
      .catch((e) => {
        dbgWarn("settings", "failed to load auth overview", e);
      });
    // Load web server status + token (desktop only)
    if (getTransport().isDesktop()) {
      Promise.all([api.getWebServerStatus(), api.getWebServerToken()])
        .then(async ([status, token]) => {
          webStatus = status;
          webToken = token;
          // Initialize form fields from settings
          webPortInput = String(settings?.web_server_port ?? 9476);
          webBindValue = settings?.web_server_bind ?? "127.0.0.1";
          webOrigins = [...(settings?.web_server_allowed_origins ?? [])];
          webTunnelUrl = settings?.web_server_tunnel_url ?? "";
          dbg("settings", "webServer loaded", {
            enabled: status?.enabled,
            hasToken: !!token,
            tunnel: webTunnelUrl,
          });
          if (status?.running) await refreshLanIp(status.bind);
        })
        .catch((e) => {
          dbgWarn("settings", "webServer load failed", e);
        });
    }
    loadCliInfo();
    // Auto-detect local proxies
    checkAllLocalProxies();
    if (selectedPlatform?.category === "local") {
      checkLocalProxy();
    }
    // Detect current username + CLI keybindings source
    void (async () => {
      try {
        const home = await platform.path.homeDir();
        const parts = splitPath(home.replace(/[/\\]+$/, ""));
        currentUsername = parts[parts.length - 1] || "";
        const absPath = await platform.path.join(home, ".claude", "keybindings.json");
        await api.readTextFile(absPath);
        cliSource = "file";
      } catch {
        cliSource = "defaults";
      }
    })();
  });

  // Cross-page sync: when in-chat /login or /logout finishes, the chat page
  // dispatches `agentcabin:codex-auth-changed`. Refresh Codex status here so this
  // Settings tab doesn't show stale auth state. Strictly one-way
  // (chat → settings) to avoid double-refresh loops with handleCodexLogin.
  onMount(() => {
    const handler = () => {
      void refreshCodexAll();
    };
    window.addEventListener("agentcabin:codex-auth-changed", handler);

    return () => {
      window.removeEventListener("agentcabin:codex-auth-changed", handler);
    };
  });

  function compatibleGlobalProviders(agent: ProviderAgent) {
    return globalProviders.filter((provider) => isProviderCompatible(agent, provider.protocol));
  }

  function showAgentSaveNotice(agent: ProviderAgent) {
    agentSaveNotice = agent;
    setTimeout(() => {
      if (agentSaveNotice === agent) agentSaveNotice = null;
    }, 4000);
  }

  async function saveAgentProviderBinding(
    agent: ProviderAgent,
    mode: "cli" | "custom",
    providerId?: string,
    models?: string[],
    defaultModel?: string,
  ): Promise<boolean> {
    if (!settings) return false;
    const current: AgentProviderBindings = settings.agent_provider_bindings ?? {
      claude: { mode: "cli" },
      codex: { mode: "cli" },
      pi: { mode: "cli" },
      grok: { mode: "cli" },
    };
    const existing = current[agent];
    const selectedProviderId = providerId ?? existing?.provider_id;
    const provider = selectedProviderId
      ? compatibleGlobalProviders(agent).find((item) => item.id === selectedProviderId)
      : compatibleGlobalProviders(agent)[0];
    const selectedModels =
      mode !== "custom"
        ? undefined
        : models?.length
          ? models.filter((modelId) => provider?.models?.some((item) => item.id === modelId))
          : existing?.models?.length
            ? existing.models.filter((modelId) =>
                provider?.models?.some((item) => item.id === modelId),
              )
            : existing?.model && provider?.models?.some((item) => item.id === existing.model)
              ? [existing.model]
              : provider?.models?.slice(0, 1).map((item) => item.id);
    const selectedDefaultModel =
      mode === "custom"
        ? defaultModel && selectedModels?.includes(defaultModel)
          ? defaultModel
          : selectedModels?.[0]
        : undefined;
    const next = {
      ...current,
      [agent]: {
        mode,
        provider_id: mode === "custom" ? provider?.id : undefined,
        models: selectedModels,
        model: selectedDefaultModel,
      },
    } as AgentProviderBindings;
    try {
      settings = await api.updateUserSettings({ agent_provider_bindings: next });
      globalProviders = settings.global_providers ?? globalProviders;
      if (agent === "claude") authMode = mode === "cli" ? "cli" : "api";
      if (agent === "codex") codexAuthMode = mode === "cli" ? "cli" : "app";
      if (agent === "pi") piAuthMode = mode === "cli" ? "cli" : "app";
      if (agent !== "grok") showAgentSaveNotice(agent);
      return true;
    } catch (e) {
      dbgWarn("settings", "saveAgentProviderBinding failed", e);
      return false;
    }
  }

  async function saveCodexConfig(providerId?: string, models?: string[], defaultModel?: string) {
    const bindingSaved = await saveAgentProviderBinding(
      "codex",
      codexAuthMode === "cli" ? "cli" : "custom",
      providerId,
      models,
      defaultModel,
    );
    if (!bindingSaved) return false;
    try {
      settings = await api.updateUserSettings({ codex_transport: codexTransport });
      return true;
    } catch (e) {
      dbgWarn("settings", "saveCodexConfig failed", e);
      return false;
    }
  }

  async function saveGeneralPatch(patch: Record<string, unknown>) {
    dbg("settings", "saveGeneralPatch", redactSensitive(patch));
    try {
      settings = await api.updateUserSettings(patch as Partial<UserSettings>);
      globalProviders = settings.global_providers ?? globalProviders;
      generalSaved = true;
      setTimeout(() => (generalSaved = false), 1500);
    } catch (e) {
      dbgWarn("settings", "saveGeneralPatch error", e);
    }
  }

  let runtimeStatuses = $state<Record<string, RuntimeProviderStatus>>({
    pi: { installed: true, authenticated: true },
    codex: { installed: true, authenticated: false },
    claude: { installed: true, authenticated: false },
    grok: { installed: true, authenticated: false },
    dsh: { installed: true, authenticated: false },
  });

  async function refreshRuntimeStatuses() {
    for (const id of ALL_RUNTIME_PROVIDERS) {
      try {
        const st = await fetchRuntimeProviderStatus(id, settings);
        runtimeStatuses[id] = st;
      } catch {
        // keep fallback
      }
    }
  }

  function getEnabledAgentOrder(): RuntimeProviderId[] {
    const raw = settings?.enabled_agents ?? ["pi"];
    const enabledSet = new Set(raw);
    enabledSet.add("pi");
    return ALL_RUNTIME_PROVIDERS.filter((agent) => enabledSet.has(agent));
  }

  function toggleAgentEnabled(agent: RuntimeProviderId) {
    if (agent === "pi") return; // Pi Agent cannot be closed
    const current = getEnabledAgentOrder();
    let next: RuntimeProviderId[];
    if (current.includes(agent)) {
      next = current.filter((a: RuntimeProviderId) => a !== agent);
    } else {
      next = [...current, agent];
    }
    // Deduplicate and maintain canonical fixed order, ensuring "pi" is always present
    const nextSet = new Set(next);
    nextSet.add("pi");
    const orderedNext = ALL_RUNTIME_PROVIDERS.filter((a) => nextSet.has(a));
    let nextDefault = (settings?.default_agent as RuntimeProviderId) ?? "pi";
    if (!orderedNext.includes(nextDefault)) {
      nextDefault = "pi";
    }
    saveGeneralPatch({ enabled_agents: orderedNext, default_agent: nextDefault });
  }

  // ── Web Server helpers ──

  async function applyWebServerSettings() {
    webRestarting = true;
    webRestartError = null;
    webRestartWarning = null;
    webTunnelError = null;
    try {
      const portNum = parseInt(webPortInput, 10);
      if (isNaN(portNum) || portNum < 1024 || portNum > 65535) {
        throw new Error(t("settings_general_webPortInvalid"));
      }
      const result = await api.restartWebServer({
        enabled: true,
        port: portNum,
        bind: webBindValue,
        allowed_origins: webOrigins.length > 0 ? webOrigins : null,
        tunnel_url: webTunnelUrl.trim() || null,
      });
      webStatus = await api.getWebServerStatus();
      settings = await api.getUserSettings();
      if (!result.config_saved) {
        webRestartWarning = t("settings_general_webSaveWarning");
      }
      dbg("settings", "webServer apply", { started: result.started, saved: result.config_saved });
      if (webStatus?.running) await refreshLanIp(webStatus.bind);
    } catch (e: unknown) {
      webRestartError = (e as Error)?.message ?? String(e);
      webStatus = await api.getWebServerStatus();
      dbgWarn("settings", "webServer apply failed", e);
    } finally {
      webRestarting = false;
    }
  }

  function addWebOrigin() {
    const trimmed = webOriginInput.trim().replace(/\/+$/, "");
    if (!trimmed) return;
    try {
      const url = new URL(trimmed);
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        webOriginError = t("settings_general_webOriginInvalid");
        return;
      }
      const origin = url.origin;
      if (!webOrigins.includes(origin)) {
        webOrigins = [...webOrigins, origin];
      }
    } catch {
      webOriginError = t("settings_general_webOriginInvalid");
      return;
    }
    webOriginInput = "";
    webOriginError = null;
  }

  async function refreshLanIp(bind: string): Promise<string | null> {
    const myId = ++lanIpRequestId;
    if (bind !== "0.0.0.0" && bind !== "::" && bind !== "[::]") {
      webLanIp = null;
      return null;
    }
    try {
      const preferV6 = bind === "::" || bind === "[::]";
      const ip = await api.getLocalIp(preferV6);
      if (myId !== lanIpRequestId) return webLanIp;
      webLanIp = ip;
      return ip;
    } catch (e) {
      dbgWarn("settings", "refreshLanIp failed", e);
      if (myId !== lanIpRequestId) return webLanIp;
      webLanIp = null;
      return null;
    }
  }

  function buildLocalAccessUrl(): string | null {
    if (!webStatus?.running || !webToken) return null;
    const bind = webStatus.bind;
    const isAll = bind === "0.0.0.0" || bind === "::" || bind === "[::]";
    const rawHost = isAll ? webLanIp : bind;
    if (!rawHost) return null;
    const host = rawHost.includes(":") ? `[${rawHost}]` : rawHost;
    return `http://${host}:${webStatus.port}/login#token=${webToken}`;
  }

  function buildTunnelAccessUrl(): string | null {
    if (!webStatus?.running || !webToken) return null;
    // Use saved (applied) tunnel URL, not the draft input value
    const tunnel = settings?.web_server_tunnel_url?.trim();
    if (!tunnel) return null;
    try {
      const u = new URL(tunnel);
      // Tunnel links use ?token= (server-side auth) to survive ngrok/cloudflared
      // interstitial pages. Local links keep #token= (fragment, never sent to server).
      return `${u.origin}/login?token=${webToken}`;
    } catch {
      return null;
    }
  }

  function buildAccessUrl(): string | null {
    return buildTunnelAccessUrl() ?? buildLocalAccessUrl();
  }

  async function copyAccessLink() {
    const url = buildAccessUrl();
    if (!url) return;
    await navigator.clipboard.writeText(url);
    webLinkCopied = true;
    dbg("settings", "webLink copied");
    setTimeout(() => (webLinkCopied = false), 1500);
  }

  async function openAccessLink() {
    const url = buildAccessUrl();
    if (!url) return;
    try {
      await platform.shell.openExternal(url);
      dbg("settings", "webLink opened in browser");
    } catch (e) {
      dbgWarn("settings", "failed to open browser", e);
    }
  }
</script>

{#key currentLocale()}
  <SettingsShell
    pageTitle={getPageTitle(activeTab)}
    pageDescription={getPageDescription(activeTab)}
    wide={activeTab === "code" || activeTab === "runtimes" || activeTab === "capability-center"}
  >
    {#snippet sidebar()}
      <SettingsSidebar {activeTab} onSelectTab={handleSelectTab} onBack={handleBack} />
    {/snippet}

    {#if activeTab === "general"}
      {#if settings}
        <GeneralSettings
          {settings}
          {appVersion}
          onUpdateSettings={handleUpdateSettings}
          {remoteHosts}
          onSaveRemoteHost={saveRemoteHostDirect}
          onDeleteRemoteHost={deleteRemoteHost}
        />
      {/if}
    {:else if activeTab === "appearance"}
      {#if settings}
        <AppearanceSettings />
      {/if}
    {:else if activeTab === "keybindings"}
      <KeybindingsSettings />
    {:else if activeTab === "usage"}
      <UsageSettings />
    {:else if activeTab === "pet"}
      {#if settings}
        <PetSettingsView {settings} onUpdateSettings={handleUpdateSettings} />
      {/if}
    {:else if activeTab === "capability-center"}
      <CapabilityCenterSettings />
    {:else if activeTab === "code"}
      {#if settings}
        <CodeSettings
          {settings}
          onUpdateSettings={handleUpdateSettings}
          codeContent={codeSnippet}
        />
      {/if}
    {:else if activeTab === "runtimes"}
      {#if settings}
        <RuntimeSettings
          {settings}
          activeSubTab={activeRuntimeSubTab}
          onSubTabChange={(tab: RuntimeSubTab) => (activeRuntimeSubTab = tab)}
          onUpdateSettings={handleUpdateSettings}
          codexContent={codexSnippet}
          claudeContent={claudeSnippet}
          grokContent={grokSnippet}
          dshContent={dshSnippet}
          piContent={piSnippet}
        />
      {/if}
    {:else if activeTab === "work"}
      {#if settings}
        <WorkSettings
          {settings}
          onSaveSettings={handleUpdateSettings}
          onNavigate={(t) => handleSelectTab(t)}
        />
      {/if}
    {:else if activeTab === "models"}
      {#if settings}
        <ModelsSettings
          {settings}
          onSaved={(next) => {
            settings = next;
            globalProviders = next.global_providers ?? [];
          }}
          codexAuth={codexStatus}
          codexInstalled={codexStatus?.installed ?? null}
          {codexLoginLoading}
          {codexLoginError}
          onOpenCodexLogin={() => void handleCodexSubscriptionLogin()}
          onReopenCodexLogin={() => void handleCodexSubscriptionReopen()}
          onLogoutCodexSubscription={() => void handleCodexSubscriptionLogout()}
        />
      {/if}
    {:else if activeTab === "doctor"}
      {#if settings}
        <DoctorSettings
          {settings}
          onToggleAgent={(agent) => toggleAgentEnabled(agent)}
          onNavigate={(t) => handleSelectTab(t)}
          onOpenWizard={openSetupWizard}
        />
      {/if}
    {:else if activeTab === "web-access"}
      {#if webAccessConfig}
        <WebAccessSettings
          config={webAccessConfig}
          enabled={webAccessEnabled}
          onSave={saveSettingsWebAccess}
          onToggle={toggleSettingsWebAccess}
          onTest={testSettingsWebAccess}
        />
      {/if}
    {:else if activeTab === "browser-use"}
      {#if webAccessConfig}
        <BrowserUseSettings
          runtime={webAccessConfig}
          enabled={browserUseEnabled}
          onToggle={async (enabled) => {
            await toggleSettingsBrowserUse(enabled);
          }}
          onPrepare={prepareSettingsBrowserRuntime}
        />
      {/if}
    {:else if activeTab === "desktop-use"}
      <DesktopUseSettings
        status={desktopUseStatus}
        loading={desktopToolLoading}
        onToggle={toggleSettingsDesktopUse}
        onRefresh={async () => {
          await refreshDesktopToolSettings();
          desktopUseStatus = await api.refreshDesktopUseStatus();
        }}
        onRequestPermissions={async () => {
          desktopUseStatus = await api.requestDesktopUsePermissions();
        }}
        onOpenPermissionPane={api.openDesktopPermissionPane}
      />
    {/if}
  </SettingsShell>
{/key}

{#snippet codexSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="Codex"
      category={t("settings_nav_nativeAgents")}
      description={t("settings_scope_nativeCodexDesc")}
      tag="Runtime Provider"
      tagColor="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
    />

    <RuntimeProviderUnifiedCards
      providerId="codex"
      providerName="Codex"
      {settings}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <!-- Codex transport: app-server unlocks interactive tools -->
    {#if settings}
      <Card class="space-y-3 p-6">
        <label class="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            class="mt-0.5 rounded"
            checked={codexTransport !== "exec"}
            onchange={(e) => {
              codexTransport = (e.currentTarget as HTMLInputElement).checked
                ? "app_server"
                : "exec";
              void handleUpdateSettings({ codex_transport: codexTransport });
            }}
          />
          <span>
            <span class="text-sm font-medium">{t("settings_codexTransport_label")}</span>
            <span class="mt-0.5 block text-xs text-muted-foreground"
              >{t("settings_codexTransport_desc")}</span
            >
          </span>
        </label>
      </Card>
    {/if}

    <AgentCliStatusCard agent="codex" title="Codex" />

    <!-- Codex CLI launch command/path -->
    <Card class="p-6 space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          {t("settings_cliConfig_launch")}
        </h2>
        {#if codexPathSaved}
          <span class="text-xs text-emerald-500 flex items-center gap-1 animate-fade-in">
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
            {t("settings_general_saved")}
          </span>
        {/if}
      </div>
      <div>
        <p class="text-sm font-medium">{t("settings_cliConfig_codexPath")}</p>
        <p class="text-xs text-muted-foreground">
          {t("settings_cliConfig_codexPathDesc")}
        </p>
      </div>
      <input
        type="text"
        bind:value={codexPathInput}
        onblur={saveCodexPath}
        onkeydown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
        placeholder="codex"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        class="w-full rounded-md border bg-transparent px-3 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
      />
    </Card>

    <!-- ── Codex Config ── -->
    <Card class="p-6 space-y-4">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
        {t("settings_codexConfig_title")}
      </h2>

      {#if codexConfigWarning}
        <div
          class="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2"
        >
          <svg
            class="h-4 w-4 text-amber-400 shrink-0 mt-0.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
            />
          </svg>
          <div class="text-xs text-amber-300">
            <p>{t("settings_codexConfig_warning", { warning: codexConfigWarning })}</p>
            <p class="mt-0.5 text-amber-400/70">
              {t("settings_codexConfig_warningDisabled")}
            </p>
          </div>
        </div>
      {/if}

      {#each CODEX_CONFIG_SETTINGS as def (def.key)}
        {@const unknownDisplay = codexValueDisplay(def.key, def)}
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-sm font-medium">{def.label}</p>
              {#if isCodexProjectOverride(def.key)}
                <span
                  class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                >
                  {t("settings_codexConfig_projectOverrideApprox")}
                </span>
              {/if}
            </div>
            <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
          </div>
          {#if def.type === "enum" && def.options}
            <div class="flex items-center gap-1.5 shrink-0">
              {#each def.options as opt (opt.value)}
                <button
                  class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCodexConfigValue(def.key, def) === opt.value
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent hover:border-ring/30'}
                        {codexConfigWarning ? 'opacity-50 cursor-not-allowed' : ''}"
                  disabled={!!codexConfigWarning}
                  onclick={() => {
                    saveCodexConfigPatch(def.key, opt.value);
                    codexConfig = { ...codexConfig, [def.key]: opt.value };
                  }}
                >
                  {opt.label}
                </button>
              {/each}
              {#if unknownDisplay}
                <span class="text-[10px] text-amber-400 ml-1">{unknownDisplay}</span>
              {/if}
            </div>
          {:else if def.type === "string"}
            <input
              class="w-40 shrink-0 rounded-md border bg-transparent px-3 py-1.5 text-sm placeholder:text-muted-foreground focus:border-ring focus:outline-none
                    {codexConfigWarning ? 'opacity-50 cursor-not-allowed' : ''}"
              disabled={!!codexConfigWarning}
              value={getCodexConfigValue(def.key, def) ?? ""}
              placeholder={t("settings_codexConfig_modelPlaceholder")}
              onblur={(e) => {
                const val = (e.target as HTMLInputElement).value.trim();
                if (val) {
                  saveCodexConfigPatch(def.key, val);
                  codexConfig = { ...codexConfig, [def.key]: val };
                } else {
                  saveCodexConfigPatch(def.key, null);
                  const next = { ...codexConfig };
                  delete next[def.key];
                  codexConfig = next;
                }
              }}
            />
          {/if}
        </div>
      {/each}
    </Card>

    <!-- Codex footer note -->
    <p class="text-[10px] text-muted-foreground px-1">
      {t("settings_codexConfig_footer")}
    </p>

    <!-- Codex feature overrides belong with durable CLI configuration, not in the chat surface. -->
    <CodexFeaturesSettings config={codexConfig} projectConfig={projectCodexConfig} />

    <!-- Codex per-session flags (AgentCabin layer, overrides config.toml) -->
    {#if codexAgentSettings}
      <Card class="p-5 space-y-4">
        <div class="flex items-center justify-between gap-2">
          <div>
            <h3 class="text-sm font-semibold">
              {t("settings_codexFlags_title")}
            </h3>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_codexFlags_subtitle")}
            </p>
          </div>
        </div>

        <!-- Reasoning effort (per-session override) -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">{t("settings_codexFlags_effortLabel")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_codexFlags_effortDesc")}
            </p>
          </div>
          <div class="flex items-center gap-1.5 shrink-0 flex-wrap">
            {#each [{ val: "", labelKey: "settings_codexConfig_optInherit" }, { val: "low", labelKey: "settings_codexConfig_effortLight" }, { val: "medium", labelKey: "settings_codexConfig_effortMedium" }, { val: "high", labelKey: "settings_codexConfig_optHigh" }, { val: "xhigh", labelKey: "settings_codexConfig_optXHigh" }, { val: "max", labelKey: "settings_codexConfig_effortMax" }] as opt (opt.val)}
              <button
                class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {(codexAgentSettings.effort ?? '') === opt.val
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent hover:border-ring/30'}"
                onclick={() =>
                  saveCodexAgentPatch({
                    effort: opt.val === "" ? null : opt.val,
                  } as Partial<AgentSettings>)}
              >
                {t(opt.labelKey as Parameters<typeof t>[0])}
              </button>
            {/each}
          </div>
        </div>

        <!-- Profile -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">{t("settings_codexFlags_profileLabel")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_codexFlags_profileDesc")}
            </p>
          </div>
          <input
            class="w-40 shrink-0 rounded-md border bg-transparent px-3 py-1.5 text-sm placeholder:text-muted-foreground focus:border-ring focus:outline-none"
            value={codexAgentSettings.profile ?? ""}
            placeholder={t("settings_codexFlags_profilePlaceholder")}
            onblur={(e) => {
              const val = (e.target as HTMLInputElement).value.trim();
              saveCodexAgentPatch({ profile: val });
            }}
          />
        </div>

        <!-- Ephemeral toggle -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">{t("settings_codexFlags_ephemeralLabel")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_codexFlags_ephemeralDesc")}
            </p>
          </div>
          <button
            class="rounded-md border px-3 py-1.5 text-xs shrink-0 transition-all
                    {codexAgentSettings.ephemeral
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            onclick={() => saveCodexAgentPatch({ ephemeral: !codexAgentSettings?.ephemeral })}
          >
            {codexAgentSettings.ephemeral
              ? t("settings_codexFlags_on")
              : t("settings_codexFlags_off")}
          </button>
        </div>

        <!-- Ignore user config -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">
              {t("settings_codexFlags_ignoreUserConfigLabel")}
            </p>
            <p class="text-xs text-amber-400/80 mt-0.5">
              {t("settings_codexFlags_ignoreUserConfigDesc")}
            </p>
          </div>
          <button
            class="rounded-md border px-3 py-1.5 text-xs shrink-0 transition-all
                    {codexAgentSettings.ignore_user_config
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            onclick={() =>
              saveCodexAgentPatch({
                ignore_user_config: !codexAgentSettings?.ignore_user_config,
              })}
          >
            {codexAgentSettings.ignore_user_config
              ? t("settings_codexFlags_on")
              : t("settings_codexFlags_off")}
          </button>
        </div>

        <!-- Ignore rules -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">
              {t("settings_codexFlags_ignoreRulesLabel")}
            </p>
            <p class="text-xs text-amber-400/80 mt-0.5">
              {t("settings_codexFlags_ignoreRulesDesc")}
            </p>
          </div>
          <button
            class="rounded-md border px-3 py-1.5 text-xs shrink-0 transition-all
                    {codexAgentSettings.ignore_rules
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            onclick={() => saveCodexAgentPatch({ ignore_rules: !codexAgentSettings?.ignore_rules })}
          >
            {codexAgentSettings.ignore_rules
              ? t("settings_codexFlags_on")
              : t("settings_codexFlags_off")}
          </button>
        </div>

        <!-- Web search -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">{t("settings_codexFlags_webSearchLabel")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_codexFlags_webSearchDesc")}
            </p>
          </div>
          <button
            class="rounded-md border px-3 py-1.5 text-xs shrink-0 transition-all
                    {codexAgentSettings.web_search
              ? 'bg-primary text-primary-foreground'
              : 'hover:bg-accent hover:border-ring/30'}"
            onclick={() => saveCodexAgentPatch({ web_search: !codexAgentSettings?.web_search })}
          >
            {codexAgentSettings.web_search
              ? t("settings_codexFlags_on")
              : t("settings_codexFlags_off")}
          </button>
        </div>
      </Card>
    {/if}
  </div>

  <!-- ═══ 3. Runtime Provider: Claude Code ═══ -->
{/snippet}

{#snippet claudeSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="Claude Code"
      category={t("settings_nav_nativeAgents")}
      description={t("settings_scope_nativeClaudeDesc")}
      tag="Runtime Provider"
      tagColor="bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20"
    />

    <RuntimeProviderUnifiedCards
      providerId="claude"
      providerName="Claude Code"
      {settings}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <AgentCliStatusCard agent="claude" title="Claude" />
    <!-- Claude CLI launch command/path (#155) -->
    <Card class="p-6 space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          {t("settings_cliConfig_launch")}
        </h2>
        {#if claudePathSaved}
          <span class="text-xs text-emerald-500 flex items-center gap-1 animate-fade-in">
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
            {t("settings_general_saved")}
          </span>
        {/if}
      </div>
      <div>
        <p class="text-sm font-medium">{t("settings_cliConfig_claudePath")}</p>
        <p class="text-xs text-muted-foreground">
          {t("settings_cliConfig_claudePathDesc")}
        </p>
      </div>
      <input
        type="text"
        bind:value={claudePathInput}
        onblur={saveClaudePath}
        onkeydown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
        placeholder="claude"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        class="w-full rounded-md border bg-transparent px-3 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
      />
    </Card>
    <!-- Behavior -->
    <Card class="p-6 space-y-4">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
        {t("settings_cliConfig_behavior")}
      </h2>
      {#each behaviorSettings as def (def.key)}
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-sm font-medium">{def.label}</p>
              {#if isProjectOverride(def.key)}
                <span
                  class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                >
                  {t("settings_cliConfig_projectOverride")}
                </span>
              {/if}
            </div>
            <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
          </div>
          {#if def.type === "boolean"}
            <button
              aria-label={def.label}
              class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                def.key,
                def,
              ) === true
                ? 'bg-primary'
                : 'bg-neutral-700'}"
              onclick={() => {
                const current = getCliConfigValue(def.key, def);
                const next = current === true ? false : true;
                saveCliConfigPatch(def.key, next);
                cliConfig = { ...cliConfig, [def.key]: next };
              }}
            >
              <span
                class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                  def.key,
                  def,
                ) === true
                  ? 'translate-x-6'
                  : 'translate-x-1'}"
              ></span>
            </button>
          {:else if def.type === "enum" && def.options}
            <div class="flex gap-1.5 shrink-0">
              {#each def.options as opt (opt.value)}
                <button
                  class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent hover:border-ring/30'}"
                  onclick={() => {
                    saveCliConfigPatch(def.key, opt.value);
                    cliConfig = { ...cliConfig, [def.key]: opt.value };
                  }}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </Card>

    <!-- Appearance -->
    <Card class="p-6 space-y-4">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
        {t("settings_cliConfig_appearance")}
      </h2>
      {#each appearanceSettings as def (def.key)}
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-sm font-medium">{def.label}</p>
              {#if isProjectOverride(def.key)}
                <span
                  class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                >
                  {t("settings_cliConfig_projectOverride")}
                </span>
              {/if}
            </div>
            <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
          </div>
          {#if def.type === "boolean"}
            <button
              aria-label={def.label}
              class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                def.key,
                def,
              ) === true
                ? 'bg-primary'
                : 'bg-neutral-700'}"
              onclick={() => {
                const current = getCliConfigValue(def.key, def);
                const next = current === true ? false : true;
                saveCliConfigPatch(def.key, next);
                cliConfig = { ...cliConfig, [def.key]: next };
              }}
            >
              <span
                class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                  def.key,
                  def,
                ) === true
                  ? 'translate-x-6'
                  : 'translate-x-1'}"
              ></span>
            </button>
          {:else if def.type === "enum" && def.options}
            <div class="flex gap-1.5 shrink-0">
              {#each def.options as opt (opt.value)}
                <button
                  class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent hover:border-ring/30'}"
                  onclick={() => {
                    saveCliConfigPatch(def.key, opt.value);
                    cliConfig = { ...cliConfig, [def.key]: opt.value };
                  }}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          {:else if def.type === "string"}
            <input
              class="w-40 shrink-0 rounded-md border bg-transparent px-3 py-1.5 text-sm placeholder:text-muted-foreground focus:border-ring focus:outline-none"
              value={getCliConfigValue(def.key, def) ?? ""}
              placeholder={def.label}
              onblur={(e) => {
                const val = (e.target as HTMLInputElement).value.trim();
                if (val) {
                  saveCliConfigPatch(def.key, val);
                  cliConfig = { ...cliConfig, [def.key]: val };
                } else {
                  // Empty string → delete key (restore default)
                  saveCliConfigPatch(def.key, null);
                  const next = { ...cliConfig };
                  delete next[def.key];
                  cliConfig = next;
                }
              }}
            />
          {/if}
        </div>
      {/each}
    </Card>

    <!-- Advanced -->
    <Card class="p-6 space-y-4">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
        {t("settings_cliConfig_advanced")}
      </h2>
      {#each advancedSettings as def (def.key)}
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-sm font-medium">{def.label}</p>
              {#if isProjectOverride(def.key)}
                <span
                  class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                >
                  {t("settings_cliConfig_projectOverride")}
                </span>
              {/if}
            </div>
            <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
          </div>
          {#if def.type === "boolean"}
            <button
              aria-label={def.label}
              class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                def.key,
                def,
              ) === true
                ? 'bg-primary'
                : 'bg-neutral-700'}"
              onclick={() => {
                const current = getCliConfigValue(def.key, def);
                const next = current === true ? false : true;
                saveCliConfigPatch(def.key, next);
                cliConfig = { ...cliConfig, [def.key]: next };
              }}
            >
              <span
                class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                  def.key,
                  def,
                ) === true
                  ? 'translate-x-6'
                  : 'translate-x-1'}"
              ></span>
            </button>
          {:else if def.type === "enum" && def.options}
            <div class="flex gap-1.5 shrink-0">
              {#each def.options as opt (opt.value)}
                <button
                  class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent hover:border-ring/30'}"
                  onclick={() => {
                    saveCliConfigPatch(def.key, opt.value);
                    cliConfig = { ...cliConfig, [def.key]: opt.value };
                  }}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </Card>

    <!-- Footer note -->
    <p class="text-[10px] text-muted-foreground px-1">
      {t("settings_cliConfig_footer")}
    </p>
  </div>

  <!-- ═══ 4. Runtime Provider: Grok ═══ -->
{/snippet}

{#snippet grokSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="Grok"
      category={t("settings_nav_nativeAgents")}
      description={t("settings_scope_nativeGrokDesc")}
      tag="Runtime Provider"
      tagColor="bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20"
    />

    <RuntimeProviderUnifiedCards
      providerId="grok"
      providerName="Grok"
      {settings}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <AgentCliStatusCard agent="grok" title="Grok" />
    <Card class="space-y-3 p-6">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          {t("settings_cliConfig_launch")}
        </h2>
        {#if grokPathSaved}
          <span class="flex items-center gap-1 text-xs text-emerald-500 animate-fade-in">
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
            {t("settings_general_saved")}
          </span>
        {/if}
      </div>
      <div>
        <p class="text-sm font-medium">{t("settings_cliConfig_grokPath")}</p>
        <p class="text-xs text-muted-foreground">
          {t("settings_cliConfig_grokPathDesc")}
        </p>
      </div>
      <input
        type="text"
        bind:value={grokPathInput}
        onblur={saveGrokPath}
        onkeydown={(event) => {
          if (event.key === "Enter") (event.target as HTMLInputElement).blur();
        }}
        placeholder="grok"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        class="w-full rounded-md border bg-transparent px-3 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
      />
    </Card>
    <GrokCliConfigCard />
  </div>

  <!-- ═══ 4.1 Runtime Provider: DeepSeek Harness (DSH) ═══ -->
{/snippet}

{#snippet dshSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="DeepSeek Harness"
      category={t("settings_nav_nativeAgents")}
      description={t("settings_scope_nativeDshDesc")}
      tag="Runtime Provider"
      tagColor="bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20"
    />

    <RuntimeProviderUnifiedCards
      providerId="dsh"
      providerName="DeepSeek Harness (DSH)"
      {settings}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <AgentCliStatusCard agent="dsh" title="DeepSeek Harness" />

    <DshPluginPanel />

    <!-- 官方 CLI 安装向导卡片 -->
    <Card class="space-y-4 p-6">
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
            官方 CLI 安装
          </h2>
          <p class="mt-1 text-xs text-muted-foreground">
            DeepSeek Harness (DSH) 官方发布于 npm。请在终端执行以下命令进行全局安装：
          </p>
        </div>
      </div>

      <div class="flex items-center gap-2 rounded-lg border bg-muted/30 p-3 font-mono text-xs">
        <code class="flex-1 select-all text-foreground">npm install -g @deepseek-ai/dsh</code>
        <button
          type="button"
          class="rounded-md border border-border/80 bg-background px-2.5 py-1 text-xs font-medium text-foreground transition-all hover:bg-accent"
          onclick={() => {
            void navigator.clipboard.writeText("npm install -g @deepseek-ai/dsh");
            dshCmdCopied = true;
            setTimeout(() => (dshCmdCopied = false), 2000);
          }}
        >
          {dshCmdCopied ? "已复制 ✓" : "复制命令"}
        </button>
      </div>

      <div class="flex items-center gap-2 text-xs text-muted-foreground">
        <span class="h-1.5 w-1.5 rounded-full bg-cyan-500 shrink-0"></span>
        <span>环境要求：Node.js 18+。安装完成后请点击上方卡片的“刷新”按钮。</span>
      </div>
    </Card>

    <Card class="space-y-3 p-6">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          {t("settings_cliConfig_launch")}
        </h2>
        {#if dshPathSaved}
          <span class="flex items-center gap-1 text-xs text-emerald-500 animate-fade-in">
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
            {t("settings_general_saved")}
          </span>
        {/if}
      </div>
      <div>
        <p class="text-sm font-medium">{t("settings_cliConfig_dshPath")}</p>
        <p class="text-xs text-muted-foreground">
          {t("settings_cliConfig_dshPathDesc")}
        </p>
      </div>
      <input
        type="text"
        bind:value={dshPathInput}
        onblur={saveDshPath}
        onkeydown={(event) => {
          if (event.key === "Enter") (event.target as HTMLInputElement).blur();
        }}
        placeholder="dsh"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        class="w-full rounded-md border bg-transparent px-3 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
      />
    </Card>

    <!-- 托管 Provider 路由配置 -->
    {#if settings}
      <Card class="p-6 space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
              模型与 Provider 路由
            </h2>
            <p class="mt-1 text-xs text-muted-foreground">
              DSH 会话通过 AgentCabin 托管 Provider 协议（OpenAI / Anthropic
              兼容）接入模型，并动态注入到 DSH SDK 隔离环境中。
            </p>
          </div>
          {#if agentSaveNotice === "dsh"}
            <span class="flex items-center gap-1 text-xs text-emerald-500 animate-fade-in">
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
              >
              {t("settings_general_saved")}
            </span>
          {/if}
        </div>
      </Card>
    {/if}
  </div>

  <!-- ═══ 5. Runtime Provider: Pi Agent ═══ -->
{/snippet}

{#snippet codeSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="Code"
      category={t("settings_nav_piAgent")}
      description={t("settings_scope_piCodeDesc")}
      tag="工作载体"
      tagColor="bg-teal-500/10 text-teal-600 dark:text-teal-400 border-teal-500/20"
    />

    <HarnessRuntimeProviderSection
      mode="code"
      {settings}
      onSaveSettings={async (patch) => {
        settings = await api.updateUserSettings(patch as any);
      }}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <PiProfileConfigCard mode="code" />

    <!-- Pi Code 独立编程指令与全局规范 (AGENTS.md) -->
    <div class="mt-4">
      <PiCodeGlobalRulesPanel />
    </div>

    <!-- Isolated Worktrees Manager (Advanced / Collapsible) -->
    <Card class="overflow-hidden">
      <button
        type="button"
        class="flex w-full items-center justify-between p-5 text-left transition-colors hover:bg-muted/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary"
        onclick={() => (worktreeAdvancedOpen = !worktreeAdvancedOpen)}
        aria-expanded={worktreeAdvancedOpen}
      >
        <div class="flex items-start gap-3">
          <div
            class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-muted-foreground transition-transform duration-200 {worktreeAdvancedOpen
              ? 'rotate-90'
              : ''}"
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="m9 18 6-6-6-6" />
            </svg>
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
                {t("settings_worktrees_title")}
              </h2>
              <span
                class="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
              >
                {t("settings_worktrees_badge")}
              </span>
            </div>
            <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
              {t("settings_worktrees_desc")}
            </p>
          </div>
        </div>

        <div class="flex shrink-0 items-center gap-3">
          {#if worktreeSaved}
            <span class="flex shrink-0 items-center gap-1 text-xs text-emerald-500 animate-fade-in">
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"><path d="m5 12 4 4L19 6" /></svg
              >
              {t("settings_worktrees_saved")}
            </span>
          {/if}
          <span class="text-xs font-medium text-muted-foreground/80 hover:text-foreground">
            {worktreeAdvancedOpen
              ? t("settings_worktrees_collapse")
              : t("settings_worktrees_expand")}
          </span>
        </div>
      </button>

      {#if worktreeAdvancedOpen}
        <div class="space-y-5 border-t border-border/60 p-6 pt-5 animate-fade-in">
          <div class="space-y-4">
            <div class="flex flex-col gap-2">
              <label for="worktree-root" class="text-sm font-medium">
                {t("settings_worktrees_rootLabel")}
              </label>
              <p class="text-xs text-muted-foreground">
                {t("settings_worktrees_rootDesc")}
              </p>
              <input
                id="worktree-root"
                type="text"
                bind:value={worktreeRootInput}
                onblur={saveWorktreeRoot}
                onkeydown={(event) => {
                  if (event.key === "Enter") (event.target as HTMLInputElement).blur();
                }}
                placeholder={t("settings_worktrees_rootPlaceholder")}
                spellcheck="false"
                autocapitalize="off"
                autocomplete="off"
                class="w-full rounded-md border bg-transparent px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
              />
            </div>

            <div class="flex flex-col gap-2">
              <label for="worktree-branch-prefix" class="text-sm font-medium">
                {t("settings_worktrees_branchPrefixLabel")}
              </label>
              <p class="text-xs text-muted-foreground">
                {t("settings_worktrees_branchPrefixDesc")}
              </p>
              <input
                id="worktree-branch-prefix"
                type="text"
                bind:value={worktreeBranchPrefixInput}
                onblur={saveWorktreeBranchPrefix}
                onkeydown={(event) => {
                  if (event.key === "Enter") (event.target as HTMLInputElement).blur();
                }}
                placeholder={t("settings_worktrees_branchPrefixPlaceholder")}
                spellcheck="false"
                autocapitalize="off"
                autocomplete="off"
                class="w-full rounded-md border bg-transparent px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
              />
            </div>

            <div class="flex items-center justify-between gap-4 border-t border-border/60 pt-4">
              <div class="min-w-0">
                <p class="text-sm font-medium">
                  {t("settings_worktrees_autoCleanupLabel")}
                </p>
                <p class="mt-0.5 text-xs leading-5 text-muted-foreground">
                  {t("settings_worktrees_autoCleanupDesc")}
                </p>
              </div>
              <button
                type="button"
                aria-pressed={settings?.worktree_auto_cleanup ?? true}
                aria-label={t("settings_worktrees_autoCleanupLabel")}
                class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {(settings?.worktree_auto_cleanup ??
                true)
                  ? 'bg-primary'
                  : 'bg-neutral-700'}"
                onclick={() =>
                  void saveWorktreePatch({
                    worktree_auto_cleanup: !(settings?.worktree_auto_cleanup ?? true),
                  })}
              >
                <span
                  class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {(settings?.worktree_auto_cleanup ??
                  true)
                    ? 'translate-x-6'
                    : 'translate-x-1'}"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between gap-4">
              <div class="min-w-0">
                <label for="worktree-retention" class="text-sm font-medium">
                  {t("settings_worktrees_retentionLabel")}
                </label>
                <p class="mt-0.5 text-xs leading-5 text-muted-foreground">
                  {t("settings_worktrees_retentionDesc")}
                </p>
              </div>
              <input
                id="worktree-retention"
                type="number"
                min="1"
                max="1000"
                step="1"
                bind:value={worktreeCleanupLimitInput}
                onchange={saveWorktreeCleanupLimit}
                class="w-24 shrink-0 rounded-md border bg-transparent px-3 py-2 text-right text-sm tabular-nums focus:outline-none focus:ring-1 focus:ring-primary"
              />
            </div>
          </div>

          <p class="border-t border-border/60 pt-4 text-[11px] leading-5 text-muted-foreground">
            {t("settings_worktrees_cleanupSafety")}
          </p>
          {#if worktreeSaveError}
            <p class="text-xs text-red-400">{worktreeSaveError}</p>
          {/if}
        </div>
      {/if}
    </Card>
  </div>
{/snippet}

<!-- ═══ 6. Runtime Provider: Pi Agent ═══ -->
{#snippet piSnippet()}
  <div class="space-y-6">
    <ScopeBanner
      title="Pi Agent"
      category={t("settings_nav_nativeAgents")}
      description={t("settings_scope_piCommonDesc")}
      tag="Runtime Provider"
      tagColor="bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20"
    />

    <RuntimeProviderUnifiedCards
      providerId="pi"
      providerName="Pi Agent"
      {settings}
      onNavigate={(tab) => openSettingsTab(tab as SettingsTab)}
    />

    <AgentCliStatusCard agent="pi" title="Pi Agent" showConfig={false} />

    <!-- Pi CLI launch command/path -->
    <Card class="space-y-3 p-6">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          {t("settings_cliConfig_launch")}
        </h2>
        {#if piPathSaved}
          <span class="flex items-center gap-1 text-xs text-emerald-500 animate-fade-in">
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
            >
            {t("settings_general_saved")}
          </span>
        {/if}
      </div>
      <div>
        <p class="text-sm font-medium">{t("settings_cliConfig_piPath")}</p>
        <p class="text-xs text-muted-foreground">
          {t("settings_cliConfig_piPathDesc")}
        </p>
      </div>
      <input
        type="text"
        bind:value={piPathInput}
        onblur={savePiPath}
        onkeydown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
        placeholder="pi"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        class="w-full rounded-md border bg-transparent px-3 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
      />
    </Card>

    <!-- Pi 扩展与运行时功能中心 -->
    <PiExtensionsManager />

    <!-- Pi Agent runtime flags -->
    {#if piAgentSettings}
      <Card class="p-5 space-y-4">
        <div class="flex items-center justify-between gap-2">
          <div>
            <h3 class="text-sm font-semibold">
              {t("settings_piFlags_title")}
            </h3>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_piFlags_subtitle")}
            </p>
          </div>
        </div>

        <!-- Pi's default thinking level -->
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium">{t("settings_piFlags_effortLabel")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_piFlags_effortDesc")}
            </p>
          </div>
          <div class="flex items-center gap-1.5 shrink-0 flex-wrap">
            {#each PI_RUNTIME_THINKING_OPTIONS as opt (opt.val)}
              <button
                class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                          {(piAgentSettings.effort ?? '') === opt.val
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent hover:border-ring/30'}"
                onclick={() =>
                  savePiAgentPatch({
                    effort: opt.val === "" ? null : opt.val,
                  } as Partial<AgentSettings>)}
              >
                {t(opt.labelKey as Parameters<typeof t>[0])}
              </button>
            {/each}
          </div>
        </div>

        <!-- Session persistence belongs with Pi runtime settings -->
        <div class="border-t border-border/60 pt-2">
          <SettingToggle
            label={t("settings_pi_noPersistence")}
            description={t("settings_pi_noPersistenceDesc")}
            checked={piAgentSettings.no_session_persistence ?? false}
            onChange={(checked) =>
              savePiAgentPatch({
                no_session_persistence: checked,
              } as Partial<AgentSettings>)}
          />
        </div>
      </Card>
    {/if}
  </div>

  <!-- ═══ 6. 工作载体: Code ═══ -->
{/snippet}
