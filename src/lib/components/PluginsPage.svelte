<script lang="ts">
  import { flushSync, onMount, getContext } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { platform } from "$lib/platform";
  import {
    listStandaloneSkills,
    getSkillContent,
    createSkill,
    updateSkill,
    deleteSkill,
    installPlugin,
    uninstallPlugin,
    enablePlugin,
    disablePlugin,
    updatePlugin,
    addMarketplace,
    removeMarketplace,
    updateMarketplace,
    checkCommunityHealth,
    searchCommunitySkills,
    getCommunitySkillDetail,
    installCommunitySkill,
    importSkillZip,
    listClaudeInstalledPlugins,
    listClaudeAvailablePlugins,
    listClaudeMarketplaces,
    ensureClaudeOfficialMarketplace,
    syncClaudeOfficialMarketplace,
    listCodexSkills,
    listGrokSkills,
    createCodexSkill,
    createGrokSkill,
    deleteCodexSkill,
    toggleCodexSkill,
    listCodexInstalledPlugins,
    listCodexAvailablePlugins,
    listCodexMarketplaces,
    addCodexMarketplace,
    removeCodexMarketplace,
    upgradeCodexMarketplace,
    installCodexPlugin,
    uninstallCodexPlugin,
    toggleCodexPlugin,
    listPiInstalledPlugins,
    listPiSharedExtensions,
    installPiSharedExtension,
    uninstallPiSharedExtension,
    updatePiSharedExtension,
    togglePiSharedExtension,
    installPiExtension,
    uninstallPiProfileExtension,
    uninstallPiExtension,
    updatePiProfileExtension,
    updatePiExtension,
    togglePiProfileExtension,
    togglePiExtension,
    listSkills,
    createSharedSkill,
    deleteSharedSkill,
    getSkillBindings,
    toggleSkillBinding,
    listConnectorCatalog,
    getConnectorBindings,
    setConnectorBinding,
    getWebAccessBinding,
    setWebAccessBinding,
    getBrowserConfig,
    saveBrowserConfig,
    testBrowser,
    getBrowserUseBinding,
    setBrowserUseBinding,
    prepareBrowserRuntime as prepareBrowserRuntimeApi,
    getAgentSettings,
    updateAgentSettings,
  } from "$lib/api";
  import type {
    ConnectorBinding,
    ConnectorCatalogItem as RuntimeConnectorCatalogItem,
  } from "$lib/api";
  import {
    listWorkConnectors as listWorkConnectorsApi,
    listWorkConnectorPackages as listWorkConnectorPackagesApi,
    listWorkConnectorCatalog as listWorkConnectorCatalogApi,
    listWorkResources as listWorkResourcesApi,
    installWorkConnectorPackage as installWorkConnectorPackageApi,
    trustWorkConnectorPackage as trustWorkConnectorPackageApi,
    enableWorkConnectorPackage as enableWorkConnectorPackageApi,
    uninstallWorkConnectorPackage as uninstallWorkConnectorPackageApi,
    removeWorkConnector as removeWorkConnectorApi,
    saveWorkConnector as saveWorkConnectorApi,
    setWorkResourceEnabled as setWorkResourceEnabledApi,
    testWorkConnector as testWorkConnectorApi,
    toggleWorkConnector as toggleWorkConnectorApi,
    listWorkAppsCatalog as listWorkAppsCatalogApi,
    listWorkAppsConnections as listWorkAppsConnectionsApi,
    authorizeWorkApp as authorizeWorkAppApi,
    startWorkConnectorAuth as startWorkConnectorAuthApi,
    configureWorkConnectorToken as configureWorkConnectorTokenApi,
    startLarkAuth as startLarkAuthApi,
    getLarkAuthStatus as getLarkAuthStatusApi,
    getWorkAppStatus as getWorkAppStatusApi,
    disconnectWorkApp as disconnectWorkAppApi,
    setWorkAppDefaultAccount as setWorkAppDefaultAccountApi,
  } from "$lib/api/work";
  import { formatInstallCount, relativeTime } from "$lib/utils/format";
  import { renderMarkdown } from "$lib/utils/markdown";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { ALL_RUNTIME_PROVIDERS, RUNTIME_PROVIDERS_CONFIG } from "$lib/utils/agent-metadata";
  import { getSavedProjectCwd } from "$lib/utils/project-cwd";
  import McpDiscoverPanel from "$lib/components/McpDiscoverPanel.svelte";
  import McpConfiguredPanel from "$lib/components/McpConfiguredPanel.svelte";
  import PromptTemplatesPanel from "$lib/components/PromptTemplatesPanel.svelte";
  import WorkConnectorPanel from "$lib/components/work/WorkConnectorPanel.svelte";
  import WorkConnectorPackagesPanel from "$lib/components/work/WorkConnectorPackagesPanel.svelte";
  import WorkAppsPanel from "$lib/components/work/WorkAppsPanel.svelte";
  import WorkResourcePanel from "$lib/components/work/WorkResourcePanel.svelte";
  import PiSharedExtensionPanel from "$lib/components/PiSharedExtensionPanel.svelte";
  import SharedConnectorPanel from "$lib/components/SharedConnectorPanel.svelte";
  import WorkSkillDiscoverPanel from "$lib/components/work/WorkSkillDiscoverPanel.svelte";
  import WorkMcpDiscoverPanel from "$lib/components/work/WorkMcpDiscoverPanel.svelte";
  import WebAccessPanel from "$lib/components/WebAccessPanel.svelte";
  import BrowserUsePanel from "$lib/components/BrowserUsePanel.svelte";
  import AgentPluginsPanel from "$lib/components/AgentPluginsPanel.svelte";
  import CodeGlobalRulesPanel from "$lib/components/CodeGlobalRulesPanel.svelte";
  import CapabilityCenterIcon from "$lib/components/CapabilityCenterIcon.svelte";
  import CapabilityOverview from "$lib/components/capabilities/CapabilityOverview.svelte";
  import CapabilityItemCard from "$lib/components/capabilities/CapabilityItemCard.svelte";
  import CapabilityDetailDrawer from "$lib/components/capabilities/CapabilityDetailDrawer.svelte";
  import { getCapabilityCenterProjection, searchCapabilities } from "$lib/api/work";
  import type { CapabilityCenterProjection, CapabilityCenterItem } from "$lib/types/work";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";

  const runtimeProviderSummary = ALL_RUNTIME_PROVIDERS.map(
    (provider) => RUNTIME_PROVIDERS_CONFIG[provider].name,
  ).join(" · ");
  import type {
    MarketplacePlugin,
    StandaloneSkill,
    MarketplaceInfo,
    InstalledPlugin,
    CommunitySkillResult,
    CommunitySkillDetail,
    ProviderHealth,
    AgentSettings,
    CodexPluginInfo,
    CodexMarketplace,
  } from "$lib/types";
  import type {
    AppCatalogItem,
    AppConnection,
    ConnectorCatalogItem,
    ConnectorPackageSummary,
    WorkConnectorHealth,
    WorkConnectorSummary,
    WorkResourceSummary,
    WorkBrowserHealth,
    WorkBrowserSummary,
  } from "$lib/types/work";

  let {
    embeddedAgent = null,
    embeddedProfile = null,
    capabilityCenter = false,
  }: {
    embeddedAgent?: "codex" | "claude" | null;
    embeddedProfile?: "code" | "work" | null;
    capabilityCenter?: boolean;
  } = $props();

  type PluginSection =
    | "skills"
    | "mcp"
    | "connectors"
    | "browser"
    | "rules"
    | "prompts"
    | "hooks"
    | "claude-plugins"
    | "codex-plugins"
    | "pi-extensions";
  type PluginAgent = "claude" | "codex";
  type PluginPageMode = "catalog" | "config";

  // Active section driven by layout sidebar context
  const sectionCtx = getContext<{ active: string }>("pluginSection");
  const pluginBasePath = "/chat/plugins";
  let embeddedSection = $state<PluginSection>("skills");
  let activeTab = $derived(
    (embeddedProfile ? embeddedSection : (sectionCtx?.active ?? "skills")) as PluginSection,
  );
  type PluginScope = "global" | "native" | "pi-code" | "work" | "code";
  type WorkCapabilitySection =
    | "skills"
    | "mcp"
    | "experts"
    | "expert-teams"
    | "connectors"
    | "apps"
    | "web"
    | "browser"
    | "runtime"
    | "rules";
  type WorkCatalogSource = "discover" | "enabled";
  let pluginScope = $state<PluginScope>("native");
  let pageMode = $state<PluginPageMode>("catalog");
  let isCapabilityCenterPage = $state(false);
  let workCapabilitySection = $state<WorkCapabilitySection>("skills");
  let workCatalogSource = $state<WorkCatalogSource>("discover");
  let isSettingsPage = $derived($page.url.pathname.startsWith("/settings"));
  const workTransportSupported = getTransport().isDesktop();

  // Skills section: Discover vs Installed toggle
  let skillsSource = $state<"discover" | "installed">("discover");

  // Plugins section: Marketplace vs Installed toggle
  let pluginsSource = $state<"marketplace" | "installed">("marketplace");

  // Prompts section: Discover vs Installed toggle
  let promptsSource = $state<"discover" | "installed">("discover");

  // Codex Plugins section: Marketplace vs Installed toggle
  let codexPluginsSource = $state<"marketplace" | "installed">("marketplace");

  // Pi Extensions section: Marketplace vs Installed toggle
  let piExtensionsSource = $state<"marketplace" | "installed">("marketplace");

  // Capability Center: Scope toggle (native agents stay isolated from Pi profiles)
  type NativeAgent = "claude" | "codex" | "grok" | "dsh";
  type CapabilityScope = "all" | NativeAgent | "pi";
  let capabilityScope = $state<CapabilityScope>("all");

  function isNativeAgent(value: string): value is NativeAgent {
    return value === "claude" || value === "codex" || value === "grok" || value === "dsh";
  }

  function nativeAgentLabel(agent: NativeAgent): string {
    return agent === "claude"
      ? "Claude Code"
      : agent === "codex"
        ? "Codex"
        : agent === "grok"
          ? "Grok"
          : "DeepSeek Harness (DSH)";
  }

  function capabilityLabel(scope: CapabilityScope): string {
    if (scope === "all") return "全部 Runtime Provider";
    if (scope === "pi") return "Pi";
    return nativeAgentLabel(scope);
  }

  // ── Capability Center 2.0 States & Actions ──
  let capabilityProjection = $state<CapabilityCenterProjection | null>(null);
  let loadingCapabilityProjection = $state(false);
  let selectedCapabilityItem = $state<CapabilityCenterItem | null>(null);
  let intentSearchQuery = $state("");
  let intentSearchResults = $state<CapabilityCenterItem[]>([]);
  let searchingIntent = $state(false);
  let readinessFilter = $state<string | null>(null);

  const CAPABILITY_LOAD_TIMEOUT_MS = 8_000;
  function withLoadTimeout<T>(promise: Promise<T>, fallback: T): Promise<T> {
    return Promise.race([
      promise.catch(() => fallback),
      new Promise<T>((resolve) => {
        setTimeout(() => resolve(fallback), CAPABILITY_LOAD_TIMEOUT_MS);
      }),
    ]);
  }

  async function loadCapabilityProjection(scope: CapabilityScope = capabilityScope) {
    loadingCapabilityProjection = true;
    try {
      const target = scope === "all" ? undefined : scope;
      capabilityProjection = await withLoadTimeout(getCapabilityCenterProjection(target), null);
    } catch (e) {
      console.error("Failed to load capability projection:", e);
    } finally {
      loadingCapabilityProjection = false;
    }
  }

  let intentSearchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  function handleIntentSearchInput() {
    if (intentSearchDebounceTimer) clearTimeout(intentSearchDebounceTimer);
    intentSearchDebounceTimer = setTimeout(() => {
      void executeIntentSearch();
    }, 250);
  }

  async function executeIntentSearch() {
    const q = intentSearchQuery.trim();
    if (!q) {
      intentSearchResults = [];
      return;
    }
    searchingIntent = true;
    try {
      intentSearchResults = await searchCapabilities(q, 12);
    } catch (e) {
      console.error("Failed to search capabilities:", e);
    } finally {
      searchingIntent = false;
    }
  }

  function clearIntentSearch() {
    intentSearchQuery = "";
    intentSearchResults = [];
  }

  function handleFilterReadiness(r: string | null) {
    readinessFilter = r;
  }

  async function handleCapabilityAction(actionType: string, item: CapabilityCenterItem) {
    try {
      if (actionType === "enable") {
        if (item.category === "skill") {
          await setWorkResourceEnabledApi(item.id, true);
        } else if (item.category === "connector") {
          await enableWorkConnectorPackageApi(item.id, true);
        } else if (item.category === "browser") {
          await setWebAccessBinding(true);
        }
        showToast("已启用", "success");
      } else if (actionType === "disable") {
        if (item.category === "skill") {
          await setWorkResourceEnabledApi(item.id, false);
        } else if (item.category === "connector") {
          await enableWorkConnectorPackageApi(item.id, false);
        } else if (item.category === "browser") {
          await setWebAccessBinding(false);
        }
        showToast("已停用", "success");
      } else if (actionType === "connect") {
        if (item.category === "app") {
          await authorizeWorkAppApi(item.id);
        } else if (item.category === "connector") {
          await startWorkConnectorAuthApi(item.id);
        }
      } else if (actionType === "install_chromium") {
        await prepareBrowserRuntimeApi();
        showToast("已安装浏览器运行时", "success");
      } else if (actionType === "test") {
        if (item.category === "connector") {
          await testWorkConnectorApi(item.id);
          showToast("连接测试完成", "success");
        } else if (item.category === "browser") {
          await testBrowser();
          showToast("浏览器测试完成", "success");
        }
      } else if (actionType === "install") {
        if (item.category === "connector") {
          await installWorkConnectorPackageApi(item.id);
          showToast("已安装连接器", "success");
        }
      }
      await loadCapabilityProjection();
    } catch (err: any) {
      showToast(err?.message || "操作失败", "error");
    }
  }

  // Sync state from URL query parameters (?scope=global|code|work|native|pi-code&section=skills|mcp|...&tab=discover|installed)
  $effect(() => {
    if (embeddedProfile) {
      pageMode = "config";
      isCapabilityCenterPage = false;
      pluginScope = embeddedProfile === "work" ? "work" : "pi-code";
      workCatalogSource = "enabled";
      workMcpSource = "configured";
      skillsSource = "installed";
      mcpSource = "configured";
      piExtensionsSource = "installed";
      return;
    }
    const params = $page.url.searchParams;
    const capabilityCenterRoute =
      capabilityCenter || (isSettingsPage && params.get("tab") === "capability-center");
    pageMode = capabilityCenterRoute
      ? "catalog"
      : params.get("mode") === "config"
        ? "config"
        : "catalog";
    isCapabilityCenterPage = capabilityCenterRoute;
    const urlTab = params.get("tab");
    if (urlTab === "discover" || urlTab === "installed") {
      skillsSource = urlTab;
      promptsSource = urlTab;
    }
    if (urlTab === "marketplace" || urlTab === "installed") {
      pluginsSource = urlTab;
      codexPluginsSource = urlTab;
      piExtensionsSource = urlTab;
    }
    const urlSource = params.get("source");
    if (urlSource === "discover" || urlSource === "configured") {
      workMcpSource = urlSource;
    }
    if (capabilityCenter && !params.get("category")) {
      workCapabilitySection = "skills";
    }
    if (capabilityCenterRoute && pageMode === "catalog") {
      workCatalogSource = urlSource === "enabled" ? "enabled" : "discover";
      workMcpSource = urlSource === "configured" ? "configured" : "discover";
    }
    const urlScope = params.get("agent");
    if (
      urlScope === "all" ||
      urlScope === "claude" ||
      urlScope === "codex" ||
      urlScope === "grok" ||
      urlScope === "pi" ||
      urlScope === "dsh"
    ) {
      capabilityScope = urlScope;
    }
    const urlSection = params.get("section");
    const normalizedSection = urlSection === "plugins" ? "claude-plugins" : urlSection;
    const rawScope = params.get("scope");
    const urlAgent = params.get("agent");
    if (
      urlAgent === "all" ||
      urlAgent === "claude" ||
      urlAgent === "codex" ||
      urlAgent === "grok" ||
      urlAgent === "pi" ||
      urlAgent === "dsh"
    ) {
      capabilityScope = urlAgent;
    }
    pluginScope = capabilityCenterRoute
      ? "work"
      : rawScope === "global"
        ? "global"
        : rawScope === "work" || rawScope === "pi-work"
          ? "work"
          : rawScope === "pi-code"
            ? "pi-code"
            : isSettingsPage && !rawScope
              ? "pi-code"
              : "native";
    const workSection = params.get("category");
    if (
      workSection === "skills" ||
      workSection === "mcp" ||
      workSection === "experts" ||
      workSection === "expert-teams" ||
      workSection === "connectors" ||
      workSection === "web" ||
      workSection === "browser" ||
      workSection === "runtime" ||
      workSection === "rules"
    ) {
      workCapabilitySection = workSection;
    } else if (workSection === "apps") {
      workCapabilitySection = "connectors";
    } else if (pluginScope === "work" && normalizedSection === "mcp") {
      workCapabilitySection = "mcp";
    } else if (pluginScope === "work" && normalizedSection === "pi-extensions") {
      workCapabilitySection = "runtime";
    }
    if (
      normalizedSection &&
      [
        "skills",
        "mcp",
        "connectors",
        "browser",
        "rules",
        "hooks",
        "claude-plugins",
        "codex-plugins",
        "pi-extensions",
        "prompts",
      ].includes(normalizedSection)
    ) {
      if (sectionCtx) sectionCtx.active = normalizedSection;
    }
    if (pageMode === "config") {
      if (pluginScope === "work") {
        workCatalogSource = "enabled";
        workMcpSource = "configured";
      } else if (pluginScope === "pi-code") {
        skillsSource = "installed";
        mcpSource = "configured";
        piExtensionsSource = "installed";
      }
    }
  });

  // Plugin agent is selected by the sidebar section, not an inner tab.
  let pluginsAgent = $derived<PluginAgent>(activeTab === "codex-plugins" ? "codex" : "claude");

  // MCP section: toggle
  let mcpSource = $state<"discover" | "configured">("discover");
  let workMcpSource = $state<"discover" | "configured">("discover");

  let plugins = $state<MarketplacePlugin[]>([]);
  let installedPlugins = $state<InstalledPlugin[]>([]);
  let claudeAvailablePlugins = $state<MarketplacePlugin[]>([]);
  let claudeMarketplaces = $state<MarketplaceInfo[]>([]);
  let codexInstalledPlugins = $state<InstalledPlugin[]>([]);
  let codexAvailablePlugins = $state<CodexPluginInfo[]>([]);
  let codexMarketplaces = $state<CodexMarketplace[]>([]);
  let piInstalledPlugins = $state<InstalledPlugin[]>([]);
  let connectorCatalog = $state<RuntimeConnectorCatalogItem[]>([]);
  let skillBindingsMap = $state<Record<string, boolean>>({});
  let connectorBindings = $state<ConnectorBinding[]>([]);
  let webAccessEnabled = $state(true);
  let browserUseEnabled = $state(true);
  let piAgentSettings = $state<AgentSettings | null>(null);
  let grokAgentSettings = $state<AgentSettings | null>(null);
  let workResources = $state<WorkResourceSummary[]>([]);
  let workConnectors = $state<WorkConnectorSummary[]>([]);
  let workConnectorPackages = $state<ConnectorPackageSummary[]>([]);
  let workConnectorCatalog = $state<ConnectorCatalogItem[]>([]);
  let workAppsCatalog = $state<AppCatalogItem[]>([]);
  let workAppsConnections = $state<AppConnection[]>([]);
  let workBrowserConfig = $state<WorkBrowserSummary | null>(null);
  let workResourcesLoading = $state(false);
  let workResourcesLoaded = $state(false);
  let workResourcesError = $state("");

  // Per-recommended-extension install loading key ("pi:rec:<pkg>")
  let piRecInstalling = $state<string | null>(null);
  function getPiInstalledPluginBySource(sourceName: string) {
    return (
      piInstalledPlugins.find(
        (p) =>
          p.name === sourceName ||
          p.name === `npm:${sourceName}` ||
          p.pluginId === sourceName ||
          p.pluginId === `npm:${sourceName}` ||
          (p.extra as Record<string, unknown> | undefined)?.source === sourceName ||
          (p.extra as Record<string, unknown> | undefined)?.source === `npm:${sourceName}`,
      ) ?? null
    );
  }

  function isSharedPiExtension(plugin: InstalledPlugin): boolean {
    return (
      plugin.agent === "pi" &&
      (plugin.scope === "shared" ||
        (plugin.extra as Record<string, unknown> | undefined)?.shared === true)
    );
  }

  function sharedPiExtensionId(plugin: InstalledPlugin): string {
    return plugin.pluginId ?? plugin.name;
  }

  function isPiCodeProfilePlugin(plugin: InstalledPlugin): boolean {
    return plugin.agent === "pi" && plugin.profile === "code";
  }

  function listPiExtensionsForCurrentScope(): Promise<InstalledPlugin[]> {
    if (pluginScope === "pi-code") return listPiSharedExtensions("code");
    if (pluginScope === "work") return Promise.resolve([]);
    return listPiInstalledPlugins(projectCwd || undefined);
  }

  function isPiProfileScope(scope: PluginScope = pluginScope): boolean {
    return scope === "pi-code" || scope === "work";
  }

  function selectNativeAgent(agent: CapabilityScope) {
    capabilityScope = agent;
    skillsAgentFilter = "all";
    if (sectionCtx) sectionCtx.active = "skills";
    syncUrl("skills");
  }

  function piExtensionDescription(key: string): string {
    return t(key as Parameters<typeof t>[0]);
  }

  const RECOMMENDED_PI_EXTENSIONS = [
    {
      name: "@ff-labs/pi-fff",
      pkgSource: "npm:@ff-labs/pi-fff",
      badge: "模糊文件搜索",
      badgeColor: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
      descKey: "piExt_fffCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@ff-labs/pi-fff" }],
    },
    // 长会话与工作区包 (Long Context & Workspace)
    {
      name: "pi-context-usage",
      pkgSource: "npm:pi-context-usage",
      badge: "上下文诊断",
      badgeColor: "bg-violet-500/10 text-violet-600 dark:text-violet-400",
      descKey: "piExt_contextUsageCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/pi-context-usage" }],
    },
    {
      name: "pi-cache-graph",
      pkgSource: "npm:pi-cache-graph",
      badge: "缓存诊断",
      badgeColor: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400",
      descKey: "piExt_cacheGraphCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/pi-cache-graph" }],
    },
    {
      name: "@narumitw/pi-caffeinate",
      pkgSource: "npm:@narumitw/pi-caffeinate",
      badge: "防休眠",
      badgeColor: "bg-amber-600/10 text-amber-700 dark:text-amber-300",
      descKey: "piExt_caffeinateCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@narumitw/pi-caffeinate" }],
    },
    // 高级研究 (Advanced Research)
    {
      name: "@narumitw/pi-chrome-devtools",
      pkgSource: "npm:@narumitw/pi-chrome-devtools",
      badge: "DevTools 调试",
      badgeColor: "bg-emerald-600/10 text-emerald-700 dark:text-emerald-300",
      descKey: "piExt_chromeDevtoolsCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@narumitw/pi-chrome-devtools" }],
    },
  ];

  let installedPiExtList = $derived(
    RECOMMENDED_PI_EXTENSIONS.filter((ext) => getPiInstalledPluginBySource(ext.name) !== null),
  );
  let recommendedPiExtList = $derived(
    RECOMMENDED_PI_EXTENSIONS.filter((ext) => getPiInstalledPluginBySource(ext.name) === null),
  );
  let skills = $state<StandaloneSkill[]>([]);
  let piCodeSkillResources = $derived(
    skills
      .filter((skill) => skill.agent === "pi")
      .map(
        (skill): WorkResourceSummary => ({
          id: skill.name,
          name: skill.name,
          description: skill.description,
          kind: "skill",
          origin: skill.source_kind === "bundled" ? "builtin" : "user",
          entry: skill.path,
          permissions: [],
          enabled: skill.enabled !== false,
          active: true,
          runtimeAvailable: true,
          discovery: {
            aliases: [],
            domains: [],
            verbs: [],
            nouns: [],
            keywords: [],
            guidance: [],
            examples: [],
          },
        }),
      ),
  );
  let marketplaces = $state<MarketplaceInfo[]>([]);
  let loading = $state(true);
  let loadError = $state(false);
  let loadWarnings = $state<string[]>([]);
  let selectedCategory = $state<string | null>(null);
  let selectedSkillKey = $state<string | null>(null);

  // Skill editor state
  let editorMode = $state<null | "new" | "edit">(null);
  let editorName = $state("");
  let editorDescription = $state("");
  let editorContent = $state("");
  let editorScope = $state<"user" | "project">("user");
  let editorPath = $state("");
  let editorSaving = $state(false);
  let editorAgent = $state<NativeAgent | "pi">("claude");
  let skillsAgentFilter = $state<"all" | "universal" | NativeAgent | "pi">("all");

  // Read-only detail state
  let readonlyContent = $state<string | null>(null);
  let readonlyLoading = $state(false);

  // Project CWD for project-scope skills
  let projectCwd = $state("");

  // Operation state
  let operationLoading = $state<string | null>(null);
  let toastMessage = $state<string | null>(null);
  let toastType = $state<"success" | "error">("success");
  let toastTimeout: ReturnType<typeof setTimeout> | null = null;

  // Scope selector for install operations
  let installScope = $state<"user" | "project" | "local">("user");

  // Confirmation dialog
  let confirmAction = $state<{
    title: string;
    message: string;
    onConfirm: () => void;
  } | null>(null);

  // Marketplace add input
  let newMarketplaceSource = $state("");
  let claudeSearchQuery = $state("");
  let newCodexMarketplaceSource = $state("");
  let codexSearchQuery = $state("");

  // Community tab state
  let communityQuery = $state("");
  let communityResults = $state<CommunitySkillResult[]>([]);
  let communityPopular = $state<CommunitySkillResult[]>([]);
  let communitySearching = $state(false);
  let communityScope = $state<"user" | "project">("user");
  let communityHealth = $state<ProviderHealth | null>(null);
  let communityDetail = $state<CommunitySkillDetail | null>(null);
  let communityDetailLoading = $state(false);
  let communityDetailError = $state<string | null>(null);
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  let communityDisplayResults = $derived(
    communityQuery.trim().length >= 2 ? communityResults : communityPopular,
  );

  const categoryColors: Record<string, string> = {
    development: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    productivity: "bg-teal-500/10 text-teal-600 dark:text-teal-400",
    security: "bg-red-500/10 text-red-600 dark:text-red-400",
    testing: "bg-amber-500/10 text-amber-600 dark:text-amber-400",
    learning: "bg-purple-500/10 text-purple-600 dark:text-purple-400",
    database: "bg-green-500/10 text-green-600 dark:text-green-400",
    monitoring: "bg-orange-500/10 text-orange-600 dark:text-orange-400",
    deployment: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400",
    design: "bg-pink-500/10 text-pink-600 dark:text-pink-400",
  };

  const componentBadges: {
    key: keyof MarketplacePlugin["components"];
    label: () => string;
    color: string;
  }[] = [
    {
      key: "skills",
      label: () => t("plugin_badgeSkills"),
      color: "bg-rose-500/10 text-rose-600 dark:text-rose-400",
    },
    {
      key: "commands",
      label: () => t("plugin_badgeCommands"),
      color: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    },
    {
      key: "agents",
      label: () => t("plugin_badgeAgents"),
      color: "bg-purple-500/10 text-purple-600 dark:text-purple-400",
    },
    {
      key: "hooks",
      label: () => t("plugin_badgeHooks"),
      color: "bg-amber-500/10 text-amber-600 dark:text-amber-400",
    },
    {
      key: "mcp_servers",
      label: () => t("plugin_badgeMcp"),
      color: "bg-teal-500/10 text-teal-600 dark:text-teal-400",
    },
    {
      key: "lsp_servers",
      label: () => t("plugin_badgeLsp"),
      color: "bg-green-500/10 text-green-600 dark:text-green-400",
    },
  ];

  // ── Skill permission & deduplication helpers ──

  function canEditSkill(s: StandaloneSkill): boolean {
    const normPath = s.path.replace(/\\/g, "/").toLowerCase();
    if (normPath.includes(".agents/skills")) return true;
    return s.agent === "codex" ? s.can_edit === true : s.can_edit !== false;
  }
  function canDeleteSkill(s: StandaloneSkill): boolean {
    const normPath = s.path.replace(/\\/g, "/").toLowerCase();
    if (normPath.includes(".agents/skills")) return true;
    return s.agent === "codex" ? s.can_delete === true : s.can_delete !== false;
  }
  function canToggleSkill(s: StandaloneSkill): boolean {
    if (s.agent === "grok") return false;
    return s.agent === "codex" ? s.can_toggle === true : s.can_toggle !== false;
  }

  function dedupeSkills(rawSkills: StandaloneSkill[]): StandaloneSkill[] {
    const map = new Map<string, StandaloneSkill>();
    for (const s of rawSkills) {
      const normPath = s.path.replace(/\\/g, "/").toLowerCase();
      const existing = map.get(normPath);
      if (!existing) {
        const copy = { ...s };
        if (normPath.includes(".agents/skills")) {
          copy.can_edit = true;
          copy.can_delete = true;
          copy.can_toggle = true;
        }
        map.set(normPath, copy);
      } else {
        if (!existing.can_edit && s.can_edit) {
          existing.can_edit = true;
          existing.can_delete = s.can_delete;
          existing.can_toggle = s.can_toggle;
          existing.agent = s.agent;
        }
      }
    }
    return Array.from(map.values());
  }

  function skillKey(s: StandaloneSkill): string {
    return `${s.agent ?? "claude"}:${s.path}`;
  }

  async function loadCombinedSkills(cwd?: string): Promise<StandaloneSkill[]> {
    if (pluginScope === "pi-code" || pluginScope === "work") {
      const skills = await listSkills(cwd).catch(() => []);
      return dedupeSkills(skills);
    }
    const [claude, codex, grok] = await Promise.allSettled([
      listStandaloneSkills(cwd),
      listCodexSkills(cwd),
      listGrokSkills(cwd),
    ]);
    const raw: StandaloneSkill[] = [
      ...(claude.status === "fulfilled" ? claude.value : []),
      ...(codex.status === "fulfilled" ? codex.value : []),
      ...(grok.status === "fulfilled" ? grok.value : []),
    ];
    return dedupeSkills(raw);
  }

  // ── Plugin key for unique operation tracking (avoids same-name Claude+Codex collisions) ──

  function pluginOpKey(p: InstalledPlugin): string {
    return `${p.agent ?? "claude"}:${p.pluginId ?? p.name}:${p.scope ?? "user"}`;
  }

  let visibleInstalledPlugins = $derived(
    pluginsAgent === "codex" ? codexInstalledPlugins : installedPlugins,
  );

  // ── Derived values ──

  function getSkillAgentCategory(skill: StandaloneSkill): "universal" | NativeAgent | "pi" {
    const p = skill.path.toLowerCase();
    if (p.includes(".agents/skills") || p.includes(".agents\\skills")) {
      return "universal";
    }
    if (skill.agent === "codex" || p.includes(".codex")) {
      return "codex";
    }
    if (skill.agent === "grok" || p.includes(".grok") || p.includes("/grok-home/")) {
      return "grok";
    }
    if (
      skill.agent === "dsh" ||
      p.includes(".dsh") ||
      p.includes("/dsh-home/") ||
      p.includes("dsh/skills")
    ) {
      return "dsh";
    }
    if (skill.agent === "pi" || p.includes(".pi")) {
      return "pi";
    }
    return "claude";
  }

  let capabilitySkills = $derived(
    pluginScope === "native"
      ? skills.filter((s) => {
          const category = getSkillAgentCategory(s);
          if (isNativeAgent(capabilityScope)) {
            return category === "universal" || category === capabilityScope;
          }
          return category === "universal" || isNativeAgent(category);
        })
      : skills,
  );

  let displayedSkills = $derived(
    capabilitySkills.filter(
      (s) => skillsAgentFilter === "all" || getSkillAgentCategory(s) === skillsAgentFilter,
    ),
  );

  let claudeCategories = $derived([
    ...new Set(claudeAvailablePlugins.map((p) => p.category).filter(Boolean)),
  ] as string[]);

  let filteredClaudePlugins = $derived(
    claudeAvailablePlugins.filter((p) => {
      if (selectedCategory && p.category !== selectedCategory) return false;
      if (!claudeSearchQuery.trim()) return true;
      const q = claudeSearchQuery.trim().toLowerCase();
      return (
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        (p.author?.name ?? "").toLowerCase().includes(q) ||
        (p.marketplace_name ?? "").toLowerCase().includes(q)
      );
    }),
  );

  let filteredCodexPlugins = $derived(
    codexAvailablePlugins.filter((plugin) => {
      if (!codexSearchQuery.trim()) return true;
      const query = codexSearchQuery.trim().toLowerCase();
      return [plugin.name, plugin.pluginId, plugin.marketplaceName ?? "", plugin.description].some(
        (value) => value.toLowerCase().includes(query),
      );
    }),
  );

  // Installed skill slugs (for community install status check)
  // Extract parent directory name from skill.path as the installed slug
  // path format: "~/.claude/skills/react-components/SKILL.md"
  function installedSlug(skillPath: string): string {
    const parts = skillPath.replace(/\\/g, "/").split("/");
    return parts.length >= 2 ? parts[parts.length - 2].toLowerCase() : "";
  }
  // Match backend to_local_slug: rsplit('/') → colon→hyphen → keep alphanumeric/-/_
  function toLocalSlug(s: string): string {
    const base = s.split("/").pop() ?? s;
    return base
      .replace(/:/g, "-")
      .replace(/[^a-zA-Z0-9_-]/g, "")
      .toLowerCase();
  }
  interface InstalledSkillLocation {
    scope: string;
    agent: "universal" | NativeAgent | "pi";
  }

  let installedSkillsMap = $derived.by(() => {
    const map = new Map<string, InstalledSkillLocation[]>();
    for (const s of capabilitySkills) {
      const slug = installedSlug(s.path);
      if (!slug) continue;
      const pathLower = s.path.toLowerCase();
      let agentType: "universal" | NativeAgent | "pi" = "universal";
      if (pathLower.includes(".agents/skills") || pathLower.includes(".agents\\skills")) {
        agentType = "universal";
      } else if (s.agent === "codex" || pathLower.includes(".codex")) {
        agentType = "codex";
      } else if (
        s.agent === "grok" ||
        pathLower.includes(".grok") ||
        pathLower.includes("/grok-home/")
      ) {
        agentType = "grok";
      } else if (s.agent === "pi" || pathLower.includes(".pi")) {
        agentType = "pi";
      } else {
        agentType = "claude";
      }
      const scope = s.scope || "user";
      const list = map.get(slug) || [];
      if (!list.some((loc) => loc.scope === scope && loc.agent === agentType)) {
        list.push({ scope, agent: agentType });
      }
      map.set(slug, list);
    }
    return map;
  });

  function formatInstalledLocationsText(locs: InstalledSkillLocation[]): string {
    if (locs.length === 0) return "";
    const universalLocs = locs.filter((l) => l.agent === "universal");
    if (universalLocs.length > 0) {
      const hasUser = universalLocs.some((l) => l.scope === "user");
      const hasProj = universalLocs.some((l) => l.scope === "project");
      if (hasUser && hasProj) return "通用规范 (~/.agents 与项目)";
      if (hasProj) return "通用规范 (项目 .agents)";
      return "通用规范 (全局 ~/.agents)";
    }
    const agents = Array.from(
      new Set(
        locs.map((l) =>
          l.agent === "claude"
            ? "Claude 专属"
            : l.agent === "codex"
              ? "Codex 专属"
              : l.agent === "grok"
                ? "Grok 专属"
                : "Pi 专属",
        ),
      ),
    );
    return agents.join(", ") + " (非 Universal 规范)";
  }

  // Community Install Modal state
  let installModalSkill = $state<CommunitySkillResult | null>(null);
  let installModalScope = $state<"user" | "project">("user");
  let installModalTargetAgent = $state<"universal" | NativeAgent | "pi">("universal");

  // Import Zip Modal state
  let importZipModalOpen = $state(false);
  let importZipPath = $state("");
  let importZipSlug = $state("");
  let importZipScope = $state<"user" | "project">("user");
  let importZipTargetAgent = $state<"universal" | NativeAgent | "pi">("universal");
  let importZipLoading = $state(false);

  async function handleOpenImportZipDialog() {
    try {
      const { open: openDialog } = await import("$lib/platform/dialog");
      const selected = await openDialog({
        title: "选择技能 ZIP 压缩包",
        multiple: false,
        directory: false,
        filters: [{ name: "Zip Archive", extensions: ["zip"] }],
      });
      if (selected && typeof selected === "string") {
        importZipPath = selected;
        const filename = selected.split(/[/\\\\]/).pop() || "";
        importZipSlug = filename
          .replace(/\.zip$/i, "")
          .toLowerCase()
          .replace(/[^a-z0-9_-]/g, "-");
        importZipScope = "user";
        importZipTargetAgent = "universal";
        importZipModalOpen = true;
      }
    } catch (err) {
      showToast(String(err), "error");
    }
  }

  async function handleExecImportZip() {
    if (!importZipPath || !importZipSlug.trim()) return;
    importZipLoading = true;
    try {
      const result = await importSkillZip(
        importZipPath,
        importZipSlug.trim(),
        importZipScope,
        importZipTargetAgent,
        projectCwd || undefined,
      );
      showToast(
        result.success
          ? `技能 '${importZipSlug}' 导入成功！` + " " + t("plugin_skillRestartHint")
          : result.message,
        result.success ? "success" : "error",
      );
      if (result.success) {
        importZipModalOpen = false;
        importZipPath = "";
        importZipSlug = "";
        await refreshSkills();
        await refreshPluginData();
      }
    } catch (e) {
      showToast((e as Error)?.message || String(e), "error");
    } finally {
      importZipLoading = false;
    }
  }

  // MCP: installed plugins that declare mcp_servers

  // ── Lifecycle ──

  onMount(async () => {
    // Initialize from URL params
    const params = $page.url.searchParams;
    const capabilityCenterRoute =
      !embeddedProfile &&
      (capabilityCenter || (isSettingsPage && params.get("tab") === "capability-center"));
    pageMode = embeddedProfile
      ? "config"
      : capabilityCenterRoute
        ? "catalog"
        : params.get("mode") === "config"
          ? "config"
          : "catalog";
    isCapabilityCenterPage = capabilityCenterRoute;
    const urlSection = params.get("section");
    const normalizedSection = urlSection === "plugins" ? "claude-plugins" : urlSection;
    const rawScope = params.get("scope");
    pluginScope = capabilityCenterRoute
      ? "work"
      : embeddedProfile
        ? embeddedProfile === "work"
          ? "work"
          : "pi-code"
        : rawScope === "global"
          ? "global"
          : rawScope === "work" || rawScope === "pi-work"
            ? "work"
            : rawScope === "pi-code"
              ? "pi-code"
              : isCapabilityCenterPage
                ? "work"
                : isSettingsPage && !rawScope
                  ? "pi-code"
                  : "native";
    const workSection = params.get("category");
    if (embeddedProfile) {
      workCapabilitySection = "skills";
      embeddedSection = "skills";
    } else if (
      workSection === "skills" ||
      workSection === "mcp" ||
      workSection === "experts" ||
      workSection === "expert-teams" ||
      workSection === "connectors" ||
      workSection === "web" ||
      workSection === "browser" ||
      workSection === "runtime" ||
      workSection === "rules"
    ) {
      workCapabilitySection = workSection;
    } else if (workSection === "apps") {
      workCapabilitySection = "connectors";
    } else if (pluginScope === "work" && normalizedSection === "mcp") {
      workCapabilitySection = "mcp";
    } else if (pluginScope === "work" && normalizedSection === "pi-extensions") {
      workCapabilitySection = "runtime";
    }
    const urlSource = params.get("source");
    if (urlSource === "enabled") workCatalogSource = "enabled";
    if (urlSource === "discover" || urlSource === "configured") {
      workMcpSource = urlSource;
    }
    if (capabilityCenter && !params.get("category")) {
      workCapabilitySection = "skills";
    }
    if (capabilityCenterRoute && pageMode === "catalog") {
      workCatalogSource = urlSource === "enabled" ? "enabled" : "discover";
      workMcpSource = urlSource === "configured" ? "configured" : "discover";
    }
    if (
      !embeddedProfile &&
      normalizedSection &&
      [
        "skills",
        "mcp",
        "hooks",
        "claude-plugins",
        "codex-plugins",
        "pi-extensions",
        "prompts",
      ].includes(normalizedSection)
    ) {
      if (sectionCtx) sectionCtx.active = normalizedSection;
    }
    if (urlSection === "skills" && (urlSource === "discover" || urlSource === "installed")) {
      skillsSource = urlSource;
    }
    if (
      (normalizedSection === "claude-plugins" || normalizedSection === "codex-plugins") &&
      (urlSource === "marketplace" || urlSource === "installed")
    ) {
      pluginsSource = urlSource;
    }
    if (urlSection === "mcp" && (urlSource === "discover" || urlSource === "configured")) {
      mcpSource = urlSource;
    }
    if (pageMode === "config") {
      if (pluginScope === "work") {
        workCatalogSource = "enabled";
        workMcpSource = "configured";
      } else if (pluginScope === "pi-code") {
        skillsSource = "installed";
        mcpSource = "configured";
        piExtensionsSource = "installed";
      }
    }

    projectCwd = getSavedProjectCwd(pluginScope === "pi-code" ? "pi" : "native") ?? "";

    if (capabilityCenterRoute) {
      // Do not make the whole Capability Center depend on every desktop API
      // completing. A stale/unavailable Work request must not leave the page
      // as an otherwise empty, indefinite spinner; each section owns its
      // loading/error state below.
      loading = false;
      void Promise.allSettled([
        loadCapabilityProjection(),
        loadWorkPluginData(),
        loadConnectorData(),
      ]).then((results) => {
        for (const result of results) {
          if (result.status === "rejected") {
            dbgWarn("plugins", "capability center load error", result.reason);
          }
        }
      });
      return;
    }

    if (isPiProfileScope() || pluginScope === "work") {
      void loadCapabilityProjection();
    }

    if (pluginScope === "work" || pluginScope === "pi-code") {
      void loadWorkPluginData();
      void loadConnectorData();
    }
    loading = true;
    const warnings: string[] = [];
    try {
      // Claude Code normally bootstraps the official marketplace on first
      // interactive launch. Keep the desktop page equally useful when the
      // CLI has not materialized it yet.
      if (normalizedSection === "claude-plugins") {
        try {
          const syncResult = await ensureClaudeOfficialMarketplace();
          if (!syncResult.success) {
            dbgWarn("plugins", "Claude official marketplace sync failed", syncResult.message);
          }
        } catch (error) {
          dbgWarn("plugins", "Claude official marketplace sync unavailable", error);
        }
      }

      const results = await Promise.allSettled([
        listClaudeAvailablePlugins(),
        listClaudeInstalledPlugins(),
        listStandaloneSkills(projectCwd || undefined),
        listClaudeMarketplaces(),
        listCodexSkills(projectCwd || undefined),
        listCodexInstalledPlugins(),
        listPiExtensionsForCurrentScope(),
        listSkills(projectCwd || undefined),
        getAgentSettings("pi"),
        listCodexAvailablePlugins(),
        listCodexMarketplaces(),
        getAgentSettings("grok"),
        listGrokSkills(projectCwd || undefined),
      ]);

      if (results[0].status === "fulfilled") {
        plugins = results[0].value;
        claudeAvailablePlugins = results[0].value;
      } else {
        dbgWarn("plugins", "Claude marketplace load error", results[0].reason);
        warnings.push("Claude marketplace plugins");
      }

      if (results[1].status === "fulfilled") {
        installedPlugins = results[1].value;
      } else {
        dbgWarn("plugins", "Claude installed plugins load error", results[1].reason);
        warnings.push("Claude installed plugins");
      }

      let rawSkills: StandaloneSkill[] = [];
      if (results[2].status === "fulfilled") {
        rawSkills = [...rawSkills, ...results[2].value];
      } else {
        dbgWarn("plugins", "skills load error", results[2].reason);
        warnings.push("standalone skills");
      }

      if (results[3].status === "fulfilled") {
        marketplaces = results[3].value;
        claudeMarketplaces = results[3].value;
      } else {
        dbgWarn("plugins", "Claude marketplaces load error", results[3].reason);
        warnings.push("Claude marketplaces");
      }

      if (results[4].status === "fulfilled") {
        rawSkills = [...rawSkills, ...results[4].value];
      } else {
        dbgWarn("plugins", "codex skills load error", results[4].reason);
        warnings.push("codex skills");
      }

      if (results[5].status === "fulfilled") {
        codexInstalledPlugins = results[5].value;
      } else {
        dbgWarn("plugins", "codex installed plugins load error", results[5].reason);
        warnings.push("codex plugins");
      }

      if (results[6].status === "fulfilled") {
        piInstalledPlugins = results[6].value;
      } else {
        dbgWarn("plugins", "pi installed extensions load error", results[6].reason);
        warnings.push("pi extensions");
      }

      if (results[7].status === "fulfilled") {
        rawSkills = [...rawSkills, ...results[7].value];
      }

      if (results[8].status === "fulfilled") {
        piAgentSettings = results[8].value;
      }

      if (results[9].status === "fulfilled") {
        codexAvailablePlugins = results[9].value;
      } else {
        dbgWarn("plugins", "codex available plugins load error", results[9].reason);
        warnings.push("Codex marketplace plugins");
      }

      if (results[10].status === "fulfilled") {
        codexMarketplaces = results[10].value;
      } else {
        dbgWarn("plugins", "codex marketplaces load error", results[10].reason);
        warnings.push("Codex marketplaces");
      }

      if (results[11].status === "fulfilled") {
        grokAgentSettings = results[11].value;
      } else {
        dbgWarn("plugins", "Grok plugin settings load error", results[11].reason);
      }

      if (results[12].status === "fulfilled") {
        rawSkills = [...rawSkills, ...results[12].value];
      } else {
        dbgWarn("plugins", "Grok skills load error", results[12].reason);
      }

      skills = dedupeSkills(rawSkills);

      loadWarnings = warnings;
      loadError = warnings.length >= 9;

      dbg("plugins", "loaded", {
        marketplace: plugins.length,
        installed: installedPlugins.length,
        claudeMarketplaces: claudeMarketplaces.length,
        codexAvailable: codexAvailablePlugins.length,
        codexInstalled: codexInstalledPlugins.length,
        codexMarketplaces: codexMarketplaces.length,
        skills: skills.length,
        marketplaces: marketplaces.length,
        warnings: warnings.length,
      });
    } catch (e) {
      dbgWarn("plugins", "load error", e);
      loadError = true;
    } finally {
      loading = false;
    }

    // Load community health + popular list (non-blocking)
    checkCommunityHealth()
      .then((h) => {
        communityHealth = h;
      })
      .catch(() => {});
    searchCommunitySkills("skill", 20)
      .then((r) => {
        communityPopular = r;
      })
      .catch(() => {});
  });

  // Sync project cwd when user switches projects (like Memory page)
  onMount(() => {
    function onProjectChanged(e: Event) {
      const cwd = (e as CustomEvent).detail?.cwd ?? "";
      if (cwd === projectCwd) return;
      dbg("plugins", "project-changed", { old: projectCwd, new: cwd });
      projectCwd = cwd;
      // Re-fetch skills for the new project context
      loadCombinedSkills(projectCwd || undefined)
        .then((s) => {
          skills = s;
        })
        .catch((err) => {
          dbgWarn("plugins", "skills reload on project-change failed", err);
        });
    }
    window.addEventListener("agentcabin:project-changed", onProjectChanged);
    return () => window.removeEventListener("agentcabin:project-changed", onProjectChanged);
  });

  // ── Helpers ──

  function hasComponent(
    components: MarketplacePlugin["components"],
    key: keyof MarketplacePlugin["components"],
  ): boolean {
    const val = components[key];
    if (typeof val === "boolean") return val;
    if (Array.isArray(val)) return val.length > 0;
    return false;
  }

  function componentCount(
    components: MarketplacePlugin["components"],
    key: keyof MarketplacePlugin["components"],
  ): number {
    const val = components[key];
    if (Array.isArray(val)) return val.length;
    return 0;
  }

  function getCategoryColor(category: string): string {
    return categoryColors[category.toLowerCase()] ?? "bg-muted text-muted-foreground";
  }

  // ── URL sync ──

  function syncUrl(sectionOverride?: PluginSection) {
    flushSync();
    const section = sectionOverride ?? activeTab;
    if (embeddedProfile) {
      embeddedSection = section;
      return;
    }
    const embeddedInSettings = $page.url.pathname.startsWith("/settings");
    const capabilityCenterRoute =
      isCapabilityCenterPage ||
      (embeddedInSettings && $page.url.searchParams.get("tab") === "capability-center");
    let url = embeddedInSettings
      ? capabilityCenterRoute
        ? `/settings?tab=capability-center&view=plugins&section=${section}`
        : embeddedAgent
          ? `/settings?tab=native-${embeddedAgent}&view=plugins&section=${section}`
          : `/settings?view=plugins&section=${section}`
      : `${pluginBasePath}?section=${section}`;
    if (pluginScope === "global") url += "&scope=global";
    else if (pluginScope === "work") url += "&scope=work";
    else if (pluginScope === "pi-code") url += "&scope=pi-code";
    if (pageMode === "config") url += "&mode=config";
    if (pluginScope === "native" && capabilityScope !== "all") {
      url += `&agent=${capabilityScope}`;
    }
    if (pluginScope === "work") {
      url += `&category=${workCapabilitySection}&source=${
        workCapabilitySection === "mcp" ? workMcpSource : workCatalogSource
      }`;
    } else if (section === "skills") url += `&source=${skillsSource}`;
    else if (section === "claude-plugins") url += `&source=${pluginsSource}`;
    else if (section === "codex-plugins") url += `&source=${pluginsSource}`;
    else if (section === "mcp") url += `&source=${mcpSource}`;
    // hooks has no sub-source
    void goto(url, { replaceState: true, noScroll: true });
  }

  async function loadConnectorData() {
    try {
      const catalog = await withLoadTimeout(listConnectorCatalog(), []);
      connectorCatalog = catalog;
    } catch (cause) {
      dbgWarn("plugins", "connector data load failed", cause);
      connectorCatalog = [];
    }
  }

  function selectSection(section: PluginSection) {
    if (embeddedProfile) embeddedSection = section;
    if (sectionCtx) sectionCtx.active = section;
    syncUrl(section);
  }

  async function loadWorkPluginData(force = false) {
    if (workResourcesLoading || (workResourcesLoaded && !force)) return;
    workResourcesLoading = true;
    workResourcesError = "";
    if (!workTransportSupported) {
      workResourcesError = "Work Mode 当前仅支持桌面 App 运行环境。";
      workResourcesLoading = false;
      return;
    }
    try {
      const [
        resources,
        connectors,
        connectorPackages,
        connectorCatalog,
        appsCatalog,
        appsConnections,
        browserConfig,
        globalSkills,
        globalConn,
        globalWeb,
        globalBrowserUse,
      ] = await Promise.all([
        withLoadTimeout(listWorkResourcesApi(), []),
        withLoadTimeout(listWorkConnectorsApi(), []),
        withLoadTimeout(listWorkConnectorPackagesApi(), []),
        withLoadTimeout(listWorkConnectorCatalogApi(), []),
        withLoadTimeout(listWorkAppsCatalogApi(), []),
        withLoadTimeout(listWorkAppsConnectionsApi(), []),
        withLoadTimeout(getBrowserConfig(), null),
        withLoadTimeout(getSkillBindings(), []),
        withLoadTimeout(getConnectorBindings(), []),
        withLoadTimeout(getWebAccessBinding(), true),
        withLoadTimeout(getBrowserUseBinding(), true),
      ]);
      workResources = Array.isArray(resources) ? resources : [];
      workConnectors = Array.isArray(connectors) ? connectors : [];
      workConnectorPackages = Array.isArray(connectorPackages) ? connectorPackages : [];
      workConnectorCatalog = Array.isArray(connectorCatalog) ? connectorCatalog : [];
      workAppsCatalog = Array.isArray(appsCatalog) ? appsCatalog : [];
      workAppsConnections = Array.isArray(appsConnections) ? appsConnections : [];
      workBrowserConfig = browserConfig;
      skillBindingsMap = Array.isArray(globalSkills)
        ? Object.fromEntries(
            globalSkills.filter((b) => b && b.skillId).map((b) => [b.skillId, b.enabled]),
          )
        : {};
      connectorBindings = Array.isArray(globalConn) ? globalConn : [];
      webAccessEnabled = globalWeb;
      browserUseEnabled = globalBrowserUse;
      workResourcesLoaded = true;
    } catch (cause) {
      workResourcesError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      workResourcesLoading = false;
    }
  }

  function selectWorkCapabilitySection(section: WorkCapabilitySection) {
    workCapabilitySection = section;
    syncUrl("skills");
  }

  function selectWorkCatalogSource(source: WorkCatalogSource) {
    workCatalogSource = source;
    syncUrl("skills");
  }

  function selectWorkMcpSource(source: "discover" | "configured") {
    workMcpSource = source;
    syncUrl("skills");
  }

  async function toggleWorkResource(resource: WorkResourceSummary) {
    operationLoading = `work-resource:${resource.id}`;
    try {
      const updated = await setWorkResourceEnabledApi(resource.id, !resource.enabled);
      workResources = workResources.map((item) => (item.id === updated.id ? updated : item));
      showToast(
        updated.enabled ? `已启用 Work 技能“${updated.name}”` : `已停用 Work 技能“${updated.name}”`,
        "success",
      );
    } catch (cause) {
      showToast(String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function saveWorkConnector(input: {
    name: string;
    transport: string;
    command: string | null;
    args: string[];
    url: string | null;
    envVars: Record<string, string>;
    headers: Record<string, string>;
  }) {
    const saved = await saveWorkConnectorApi(input);
    workConnectors = [
      saved,
      ...workConnectors.filter((connector) => connector.name !== saved.name),
    ];
  }

  async function toggleWorkConnector(name: string, enabled: boolean) {
    const updated = await toggleWorkConnectorApi(name, enabled);
    workConnectors = workConnectors.map((connector) =>
      connector.name === name ? updated : connector,
    );
  }

  async function removeWorkConnector(name: string) {
    await removeWorkConnectorApi(name);
    workConnectors = workConnectors.filter((connector) => connector.name !== name);
  }

  async function installWorkConnectorPackage(source: string) {
    const installed = await installWorkConnectorPackageApi(source);
    workConnectorPackages = [
      installed,
      ...workConnectorPackages.filter((item) => item.manifest.id !== installed.manifest.id),
    ];
    showToast(`已导入 WorkBuddy 连接器“${installed.manifest.displayName}”，等待信任`, "success");
  }

  async function trustWorkConnectorPackage(packageId: string, trusted: boolean) {
    const updated = await trustWorkConnectorPackageApi(packageId, trusted);
    workConnectorPackages = workConnectorPackages.map((item) =>
      item.manifest.id === updated.manifest.id ? updated : item,
    );
    showToast(trusted ? "已信任 WorkBuddy 连接器" : "已撤销连接器信任", "success");
  }

  async function enableWorkConnectorPackage(packageId: string, enabled: boolean) {
    const updated = await enableWorkConnectorPackageApi(packageId, enabled);
    workConnectorPackages = workConnectorPackages.map((item) =>
      item.manifest.id === updated.manifest.id ? updated : item,
    );
    showToast(enabled ? "已启用 WorkBuddy 连接器" : "已停用 WorkBuddy 连接器", "success");
  }

  async function uninstallWorkConnectorPackage(packageId: string) {
    await uninstallWorkConnectorPackageApi(packageId);
    workConnectorPackages = workConnectorPackages.filter((item) => item.manifest.id !== packageId);
    showToast("已卸载 WorkBuddy 连接器", "success");
  }

  async function handleConnectApp(appId: string) {
    operationLoading = `work-app:${appId}`;
    try {
      workAppsConnections = await listWorkAppsConnectionsApi();
      workConnectorCatalog = await listWorkConnectorCatalogApi();
      showToast(`已成功连接 ${appId}`, "success");
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleDisconnectApp(appId: string) {
    operationLoading = `work-app:${appId}`;
    try {
      await disconnectWorkAppApi(appId);
      workAppsConnections = await listWorkAppsConnectionsApi();
      workConnectorCatalog = await listWorkConnectorCatalogApi();
      showToast(`已断开应用连接`, "success");
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleAuthorizeConnectorPackage(packageId: string) {
    operationLoading = `connector-package-auth:${packageId}`;
    try {
      if (packageId.trim().toLowerCase() === "feishu") {
        let latest = await startLarkAuthApi();
        if (latest.status !== "authenticated") {
          const taskId = latest.taskId;
          if (!taskId) throw new Error("飞书授权任务未返回 taskId。");
          for (let attempt = 0; attempt < 610; attempt += 1) {
            await new Promise((resolve) => setTimeout(resolve, 1000));
            latest = await getLarkAuthStatusApi(taskId);
            if (latest.status === "authenticated" || latest.status === "error") break;
          }
        }
        if (latest.status === "error") {
          throw new Error(latest.message || "飞书浏览器授权失败。");
        }
        workConnectorPackages = workConnectorPackages.map((item) =>
          item.manifest.id === packageId
            ? { ...item, state: { ...item.state, authStatus: "authenticated" } }
            : item,
        );
        workAppsConnections = await listWorkAppsConnectionsApi();
        workConnectorCatalog = await listWorkConnectorCatalogApi();
        showToast("飞书已完成浏览器授权", "success");
        return;
      }
      const auth = await startWorkConnectorAuthApi(packageId);
      if (auth.authorizationUrl) {
        try {
          await platform.shell.openExternal(auth.authorizationUrl);
        } catch {
          window.open(auth.authorizationUrl, "_blank");
        }
      }
      showToast("已打开连接器授权页面，正在等待授权完成…", "success");

      let latestStatus = await getWorkAppStatusApi(packageId, auth.connectionId);
      for (
        let attempt = 0;
        attempt < 30 &&
        latestStatus !== "connected" &&
        latestStatus !== "expired" &&
        latestStatus !== "error";
        attempt += 1
      ) {
        await new Promise((resolve) => setTimeout(resolve, 2000));
        latestStatus = await getWorkAppStatusApi(packageId, auth.connectionId);
      }

      const authStatus =
        latestStatus === "connected"
          ? "authenticated"
          : latestStatus === "pending"
            ? "pending"
            : latestStatus === "expired"
              ? "expired"
              : latestStatus === "error"
                ? "error"
                : "not_authenticated";
      workConnectorPackages = workConnectorPackages.map((item) =>
        item.manifest.id === packageId ? { ...item, state: { ...item.state, authStatus } } : item,
      );
      workAppsConnections = await listWorkAppsConnectionsApi();
      if (latestStatus === "connected") {
        showToast("连接器已完成认证", "success");
      } else if (latestStatus === "expired" || latestStatus === "error") {
        showToast("连接器授权未完成，请重试", "error");
      } else {
        showToast("授权仍在处理中，可稍后继续任务", "success");
      }
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function configureWorkConnectorToken(packageId: string, values: Record<string, string>) {
    const updated = await configureWorkConnectorTokenApi(packageId, values);
    workConnectorPackages = workConnectorPackages.map((item) =>
      item.manifest.id === updated.manifest.id ? updated : item,
    );
    showToast("连接器凭据已保存", "success");
  }

  async function handleDisconnectAppAccount(appId: string, accountId: string) {
    operationLoading = `work-app:${appId}:${accountId}`;
    try {
      await disconnectWorkAppApi(appId, accountId);
      workAppsConnections = await listWorkAppsConnectionsApi();
      showToast(`已断开指定账号`, "success");
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleSetDefaultAppAccount(appId: string, accountId: string) {
    operationLoading = `work-app-default:${appId}:${accountId}`;
    try {
      await setWorkAppDefaultAccountApi("", appId, accountId);
      workAppsConnections = await listWorkAppsConnectionsApi();
      showToast(`已设为默认账号`, "success");
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause), "error");
    } finally {
      operationLoading = null;
    }
  }

  function addInstalledWorkResource(resource: WorkResourceSummary) {
    workResources = [
      resource,
      ...workResources.filter((candidate) => candidate.id !== resource.id),
    ];
  }

  async function testWorkConnection(name: string): Promise<WorkConnectorHealth> {
    return testWorkConnectorApi(name);
  }

  async function saveWorkBrowser(input: {
    provider?: string;
    enabled: boolean;
    maxResults: number;
    apiKey: string | null;
    endpointUrl?: string | null;
    allowedHosts?: string[] | null;
  }) {
    workBrowserConfig = await saveBrowserConfig(input);
  }

  async function togglePiCodeSkill(id: string, enabled: boolean) {
    await toggleSkillBinding(id, enabled);
    await refreshSkills();
    showToast(enabled ? `已全局启用技能“${id}”` : `已全局停用技能“${id}”`, "success");
  }

  async function deletePiCodeSkill(id: string) {
    const skill = skills.find((item) => item.agent === "pi" && item.name === id);
    if (!skill) return;
    await deleteSharedSkill(skill.path);
    await refreshSkills();
    showToast(`已卸载 Code 技能“${id}”`, "success");
  }

  async function testWorkBrowser(): Promise<WorkBrowserHealth> {
    return testBrowser();
  }

  // ── Skill CRUD ──

  function startNewSkill() {
    editorMode = "new";
    editorName = "";
    editorDescription = "Brief description";
    editorContent = "# New Skill\n\nInstructions for Claude...";
    editorScope = "user";
    editorAgent =
      pluginScope === "pi-code"
        ? "pi"
        : pluginScope === "native" && isNativeAgent(capabilityScope)
          ? capabilityScope
          : "claude";
    editorPath = "";
    selectedSkillKey = null;
  }

  function startEditSkill(skill: StandaloneSkill) {
    editorMode = "edit";
    editorName = skill.name;
    editorDescription = skill.description;
    editorPath = skill.path;
    editorScope = (skill.scope as "user" | "project") ?? "user";
    getSkillContent(skill.path, projectCwd || undefined)
      .then((raw) => {
        editorContent = raw;
        selectedSkillKey = skillKey(skill);
      })
      .catch((e) => {
        editorContent = t("plugin_loadFailedContent");
        dbgWarn("plugins", "edit load error", e);
      });
  }

  function cancelEditor() {
    editorMode = null;
    editorName = "";
    editorDescription = "";
    editorContent = "";
    editorPath = "";
  }

  async function loadReadonlyContent(skill: StandaloneSkill) {
    readonlyContent = null;
    readonlyLoading = true;
    try {
      readonlyContent = await getSkillContent(skill.path, projectCwd || undefined);
    } catch {
      readonlyContent = null;
    } finally {
      readonlyLoading = false;
    }
  }

  async function handleCreateSkill() {
    const name = editorName.trim();
    if (!name) {
      showToast(t("plugin_skillNameRequired"), "error");
      return;
    }
    editorSaving = true;
    dbg("plugins", "createSkill", { name, scope: editorScope, agent: editorAgent });
    try {
      let skill: StandaloneSkill;
      if (editorAgent === "pi") {
        skill = await createSharedSkill(
          name,
          editorDescription.trim(),
          editorContent,
          editorScope,
          projectCwd || undefined,
        );
      } else if (editorAgent === "codex") {
        skill = await createCodexSkill(
          name,
          editorDescription.trim(),
          editorContent,
          editorScope,
          projectCwd || undefined,
        );
      } else if (editorAgent === "grok") {
        skill = await createGrokSkill(
          name,
          editorDescription.trim(),
          editorContent,
          editorScope,
          projectCwd || undefined,
        );
      } else {
        skill = await createSkill(
          name,
          editorDescription.trim(),
          editorContent,
          editorScope,
          projectCwd || undefined,
        );
      }
      showToast(t("plugin_createdSkill", { name: skill.name }), "success");
      cancelEditor();
      await refreshSkills();
    } catch (e) {
      showToast(t("plugin_failedCreateSkill", { error: String(e) }), "error");
    } finally {
      editorSaving = false;
    }
  }

  async function handleSaveSkill() {
    editorSaving = true;
    dbg("plugins", "updateSkill", { path: editorPath });
    try {
      await updateSkill(editorPath, editorContent, projectCwd || undefined);
      showToast(t("plugin_skillSaved"), "success");
      cancelEditor();
      await refreshSkills();
    } catch (e) {
      showToast(t("plugin_failedSaveSkill", { error: String(e) }), "error");
    } finally {
      editorSaving = false;
    }
  }

  function handleDeleteSkill(skill: StandaloneSkill) {
    confirmAction = {
      title: t("plugin_deleteSkillTitle"),
      message: t("plugin_deleteSkillMsg", { name: skill.name }),
      onConfirm: async () => {
        operationLoading = skillKey(skill);
        dbg("plugins", "deleteSkill", { path: skill.path, agent: skill.agent });
        try {
          if (skill.agent === "pi") {
            await deleteSharedSkill(skill.path);
          } else if (skill.agent === "codex") {
            await deleteCodexSkill(skill.path, projectCwd || undefined);
          } else {
            await deleteSkill(skill.path, projectCwd || undefined);
          }
          showToast(t("plugin_deletedSkill", { name: skill.name }), "success");
          if (selectedSkillKey === skillKey(skill)) {
            selectedSkillKey = null;
          }
          cancelEditor();
          await refreshSkills();
        } catch (e) {
          showToast(t("plugin_failedDeleteSkill", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  async function refreshSkills() {
    try {
      skills = await loadCombinedSkills(projectCwd || undefined);
    } catch (e) {
      dbgWarn("plugins", "refresh skills error", e);
    }
  }

  async function handleToggleSkill(skill: StandaloneSkill) {
    if (skill.agent === "grok") {
      showToast("Grok 原生 Skill 不支持启用状态切换。", "error");
      return;
    }
    const newEnabled = skill.enabled === false;
    operationLoading = skillKey(skill);
    try {
      if (skill.agent === "pi") {
        await toggleSkillBinding(skill.name, newEnabled);
      } else {
        await toggleCodexSkill(skill.path, newEnabled, projectCwd || undefined);
      }
      showToast(
        newEnabled
          ? t("plugin_skillEnabled", { name: skill.name })
          : t("plugin_skillDisabled", { name: skill.name }),
        "success",
      );
      await refreshSkills();
    } catch (e) {
      showToast(t("plugin_failedToggleSkill", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  // ── Community skill handlers ──

  let communityRefreshing = $state(false);

  async function refreshCommunity() {
    communityRefreshing = true;
    try {
      const [h, r] = await Promise.all([
        checkCommunityHealth(),
        searchCommunitySkills("skill", 20),
      ]);
      communityHealth = h;
      communityPopular = r;
      dbg("plugins", "community refreshed", { health: h.available, popular: r.length });
    } catch (e) {
      dbgWarn("plugins", "community refresh error", e);
    } finally {
      communityRefreshing = false;
    }
  }

  function handleCommunitySearch() {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    const q = communityQuery.trim();
    if (q.length < 2) {
      communityResults = [];
      return;
    }
    searchDebounceTimer = setTimeout(async () => {
      communitySearching = true;
      try {
        communityResults = await searchCommunitySkills(q, 30);
      } catch (e) {
        showToast(t("plugin_searchFailed", { error: String(e) }), "error");
      } finally {
        communitySearching = false;
      }
    }, 300);
  }

  async function handleCommunityDetail(skill: CommunitySkillResult) {
    communityDetailLoading = true;
    communityDetail = null;
    communityDetailError = null;
    try {
      communityDetail = await getCommunitySkillDetail(skill.source, skill.skill_id);
    } catch (e) {
      communityDetailError = String(e);
    } finally {
      communityDetailLoading = false;
    }
  }

  function openCommunityInstallModal(skill: CommunitySkillResult) {
    installModalSkill = skill;
    installModalScope = communityScope;
    installModalTargetAgent = "universal";
  }

  async function handleCommunityInstallExec() {
    if (!installModalSkill) return;
    const skill = installModalSkill;
    operationLoading = skill.id;
    try {
      const result = await installCommunitySkill(
        skill.source,
        skill.skill_id,
        installModalScope,
        installModalTargetAgent,
        projectCwd || undefined,
      );
      showToast(
        result.success
          ? t("plugin_installedSkill", { name: skill.name }) + " " + t("plugin_skillRestartHint")
          : result.message,
        result.success ? "success" : "error",
      );
      if (result.success) {
        installModalSkill = null;
        await refreshSkills();
        await refreshPluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  function setCommunityFilter(q: string) {
    communityQuery = q;
    handleCommunitySearch();
  }

  // ── Toast & refresh helpers ──

  function showToast(message: string, type: "success" | "error") {
    toastMessage = message;
    toastType = type;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(() => {
      toastMessage = null;
    }, 4000);
  }

  async function refreshPluginData() {
    const results = await Promise.allSettled([
      listClaudeAvailablePlugins(),
      listClaudeInstalledPlugins(),
      listCodexInstalledPlugins(),
      listPiExtensionsForCurrentScope(),
      listClaudeMarketplaces(),
      listCodexAvailablePlugins(),
      listCodexMarketplaces(),
      getAgentSettings("grok"),
    ]);
    if (results[0].status === "fulfilled") {
      plugins = results[0].value;
      claudeAvailablePlugins = results[0].value;
    } else dbgWarn("plugins", "refresh Claude marketplace error", results[0].reason);
    if (results[1].status === "fulfilled") installedPlugins = results[1].value;
    else dbgWarn("plugins", "refresh Claude installed error", results[1].reason);
    if (results[2].status === "fulfilled") codexInstalledPlugins = results[2].value;
    else dbgWarn("plugins", "refresh codex plugins error", results[2].reason);
    if (results[3].status === "fulfilled") piInstalledPlugins = results[3].value;
    else dbgWarn("plugins", "refresh pi extensions error", results[3].reason);
    if (results[4].status === "fulfilled") {
      marketplaces = results[4].value;
      claudeMarketplaces = results[4].value;
    } else dbgWarn("plugins", "refresh Claude marketplaces error", results[4].reason);
    if (results[5].status === "fulfilled") codexAvailablePlugins = results[5].value;
    else dbgWarn("plugins", "refresh codex available plugins error", results[5].reason);
    if (results[6].status === "fulfilled") codexMarketplaces = results[6].value;
    else dbgWarn("plugins", "refresh codex marketplaces error", results[6].reason);
    if (results[7].status === "fulfilled") grokAgentSettings = results[7].value;
    else dbgWarn("plugins", "refresh Grok plugin settings error", results[7].reason);
  }

  async function refreshClaudePluginData() {
    const results = await Promise.allSettled([
      listClaudeAvailablePlugins(),
      listClaudeInstalledPlugins(),
      listClaudeMarketplaces(),
      getAgentSettings("grok"),
    ]);
    if (results[0].status === "fulfilled") {
      plugins = results[0].value;
      claudeAvailablePlugins = results[0].value;
    } else dbgWarn("plugins", "refresh Claude marketplace error", results[0].reason);
    if (results[1].status === "fulfilled") installedPlugins = results[1].value;
    else dbgWarn("plugins", "refresh Claude installed error", results[1].reason);
    if (results[2].status === "fulfilled") {
      marketplaces = results[2].value;
      claudeMarketplaces = results[2].value;
    } else dbgWarn("plugins", "refresh Claude marketplaces error", results[2].reason);
    if (results[3].status === "fulfilled") grokAgentSettings = results[3].value;
    else dbgWarn("plugins", "refresh Grok plugin settings error", results[3].reason);
  }

  async function refreshCodexPluginData() {
    const results = await Promise.allSettled([
      listCodexInstalledPlugins(),
      listCodexAvailablePlugins(),
      listCodexMarketplaces(),
    ]);
    if (results[0].status === "fulfilled") codexInstalledPlugins = results[0].value;
    else dbgWarn("plugins", "refresh codex installed plugins error", results[0].reason);
    if (results[1].status === "fulfilled") codexAvailablePlugins = results[1].value;
    else dbgWarn("plugins", "refresh codex available plugins error", results[1].reason);
    if (results[2].status === "fulfilled") codexMarketplaces = results[2].value;
    else dbgWarn("plugins", "refresh codex marketplaces error", results[2].reason);
  }

  /** Install a recommended Pi extension by its npm source string. */
  async function handleInstallRecommendedPiExt(pkgSource: string) {
    if (piRecInstalling) return;
    piRecInstalling = pkgSource;
    try {
      const result =
        pluginScope === "pi-code"
          ? await installPiSharedExtension(pkgSource)
          : await installPiExtension(pkgSource, "user", undefined);
      showToast(
        result.success
          ? t("piExt_installSuccess", { name: pkgSource })
          : result.message || `Installed ${pkgSource}`,
        result.success ? "success" : "error",
      );
      if (result.success) await refreshPluginData();
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      piRecInstalling = null;
    }
  }

  async function installSharedPiExtension(source: string): Promise<void> {
    operationLoading = "pi:shared:install";
    try {
      const result = await installPiSharedExtension(source);
      if (!result.success) throw new Error(result.message || `安装失败：${source}`);
      showToast(t("piExt_installSuccess", { name: source }), "success");
      await refreshPluginData();
    } finally {
      operationLoading = null;
    }
  }

  async function toggleSharedPiExtension(
    mode: "code" | "work",
    plugin: InstalledPlugin,
    enabled: boolean,
  ): Promise<void> {
    const id = sharedPiExtensionId(plugin);
    operationLoading = `pi:shared:${mode}:${id}`;
    try {
      await togglePiSharedExtension(mode, id, enabled);
      showToast(t(enabled ? "plugins_enabled" : "plugins_disabled"), "success");
      if (mode === "work") await loadWorkPluginData(true);
      else await refreshPluginData();
    } finally {
      operationLoading = null;
    }
  }

  async function handleToggleGlobalSkill(skillId: string, enabled: boolean): Promise<void> {
    operationLoading = `skill:${skillId}`;
    try {
      await toggleSkillBinding(skillId, enabled);
      skillBindingsMap = { ...skillBindingsMap, [skillId]: enabled };
      showToast(`已全局${enabled ? "启用" : "停用"}技能`, "success");
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleToggleConnector(
    item: RuntimeConnectorCatalogItem,
    enabled: boolean,
  ): Promise<void> {
    operationLoading = `connector:${item.id}`;
    try {
      await setConnectorBinding(item.id, enabled);
      const existing = connectorBindings.find(
        (binding) => binding.connectorId.toLowerCase() === item.id.toLowerCase(),
      );
      connectorBindings = existing
        ? connectorBindings.map((binding) =>
            binding.connectorId.toLowerCase() === item.id.toLowerCase()
              ? { ...binding, enabled }
              : binding,
          )
        : [
            ...connectorBindings,
            { connectorId: item.id, enabled, permissionProfile: "interactive" },
          ];
      showToast(`已全局${enabled ? "启用" : "停用"}连接器`, "success");
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleToggleWebAccess(enabled: boolean): Promise<void> {
    operationLoading = "web-access";
    try {
      await setWebAccessBinding(enabled);
      webAccessEnabled = enabled;
      showToast(`已全局${enabled ? "启用" : "停用"}网络访问`, "success");
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleToggleBrowserUse(enabled: boolean): Promise<void> {
    operationLoading = "browser-use";
    try {
      await setBrowserUseBinding(enabled);
      showToast(`已全局${enabled ? "启用" : "停用"}浏览器操作`, "success");
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
      throw e;
    } finally {
      operationLoading = null;
    }
  }

  async function prepareBrowserRuntime(): Promise<WorkBrowserSummary> {
    workBrowserConfig = await prepareBrowserRuntimeApi();
    return workBrowserConfig;
  }

  async function updateSharedPiExtension(plugin: InstalledPlugin): Promise<void> {
    const id = sharedPiExtensionId(plugin);
    operationLoading = `pi:shared:update:${id}`;
    try {
      const result = await updatePiSharedExtension(id);
      if (!result.success) throw new Error(result.message || `更新失败：${plugin.name}`);
      showToast(t("plugin_updatedName", { name: plugin.name }), "success");
      await refreshPluginData();
      if (pluginScope === "work") await loadWorkPluginData(true);
    } finally {
      operationLoading = null;
    }
  }

  const PI_NATIVE_FEATURES: Array<{
    label: string;
    description: string;
  }> = [
    {
      label: "Permission system",
      description: "Pi 权限控制",
    },
    {
      label: "Plan mode",
      description: "实施前的只读方案设计",
    },
    {
      label: "Todo progress",
      description: "实施过程中的分阶段进度（已有 todo 扩展时保持关闭）",
    },
    {
      label: "Goal tracking",
      description: "跨回合自主运行与目标达成",
    },
    {
      label: "Subagents delegation",
      description: "多智能体委派与后台协作",
    },
    {
      label: "LSP code intelligence",
      description: "语言服务器代码智能 (LSP)",
    },
    {
      label: "Context prune",
      description: "长任务旧工具输出剪枝（默认关闭）",
    },
    {
      label: "Multi edit",
      description: "多文件、多位置批量 Patch 编辑",
    },
  ];

  // ── Plugin operation handlers ──

  function needsCwd(scope: string): boolean {
    return scope === "project" || scope === "local";
  }

  /** Resolve the correct cwd for a plugin operation.
   *  Prefers the plugin's own projectPath (from CLI metadata) over the page-level projectCwd.
   *  This prevents "not installed in project scope" when the plugin belongs to a different project. */
  function resolvePluginCwd(plugin: InstalledPlugin): string | undefined {
    const scope = (plugin.scope as string) ?? "user";
    if (!needsCwd(scope)) return undefined;
    return plugin.projectPath || projectCwd || undefined;
  }

  function claudePluginReference(plugin: MarketplacePlugin): string {
    if (plugin.name.includes("@") || !plugin.marketplace_name) return plugin.name;
    return `${plugin.name}@${plugin.marketplace_name}`;
  }

  function marketplaceSourceLabel(marketplace: MarketplaceInfo): string {
    if (marketplace.install_location) return marketplace.install_location;
    if (typeof marketplace.source === "string") return marketplace.source;
    if (marketplace.source && typeof marketplace.source === "object") {
      const source = marketplace.source as Record<string, unknown>;
      for (const key of ["repo", "url", "path", "source"]) {
        if (typeof source[key] === "string") return source[key] as string;
      }
    }
    return marketplace.name;
  }

  function installedClaudePluginReference(plugin: InstalledPlugin): string {
    if (plugin.pluginId) return plugin.pluginId;
    if (plugin.marketplace && !plugin.name.includes("@")) {
      return `${plugin.name}@${plugin.marketplace}`;
    }
    return plugin.name;
  }

  function grokPluginDir(plugin: InstalledPlugin): string | undefined {
    const candidates = [plugin.installLocation, plugin.installPath, plugin.path, plugin.root];
    return candidates.find((value) => typeof value === "string" && value.trim().length > 0)?.trim();
  }

  function samePluginPath(left: string, right: string): boolean {
    return (
      left.replaceAll("\\", "/").replace(/\/+$/, "") ===
      right.replaceAll("\\", "/").replace(/\/+$/, "")
    );
  }

  function isGrokPluginEnabled(plugin: InstalledPlugin): boolean {
    const path = grokPluginDir(plugin);
    return (
      !!path &&
      (grokAgentSettings?.grok_plugin_dirs ?? []).some((configured) =>
        samePluginPath(configured, path),
      )
    );
  }

  function grokPluginOpKey(plugin: InstalledPlugin): string {
    return `grok:${pluginOpKey(plugin)}`;
  }

  async function handleToggleGrokPlugin(plugin: InstalledPlugin) {
    const path = grokPluginDir(plugin);
    if (!path) {
      showToast(t("plugin_grokNoPath"), "error");
      return;
    }

    const current = grokAgentSettings?.grok_plugin_dirs ?? [];
    const enabled = current.some((configured) => samePluginPath(configured, path));
    const next = enabled
      ? current.filter((configured) => !samePluginPath(configured, path))
      : [...current, path];
    operationLoading = grokPluginOpKey(plugin);
    try {
      grokAgentSettings = await updateAgentSettings("grok", {
        grok_plugin_dirs: next,
      });
      showToast(t("plugin_grokSelectionSaved"), "success");
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleInstall(plugin: MarketplacePlugin) {
    const pluginReference = claudePluginReference(plugin);
    operationLoading = pluginReference;
    dbg("plugins", "install", { name: pluginReference, scope: installScope });
    try {
      const result = await installPlugin(
        pluginReference,
        installScope,
        needsCwd(installScope) ? projectCwd : undefined,
      );
      dbg("plugins", "install result", result);
      showToast(
        result.success
          ? t("plugin_installedPlugin", { name: plugin.name })
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        pluginsSource = "installed";
        syncUrl("claude-plugins");
        await refreshClaudePluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorInstalling", { name: plugin.name, error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleInstallCodexPlugin(plugin: CodexPluginInfo) {
    operationLoading = `codex:add:${plugin.pluginId}`;
    dbg("plugins", "install codex plugin", {
      pluginId: plugin.pluginId,
      marketplace: plugin.marketplaceName,
    });
    try {
      const result = await installCodexPlugin(plugin.pluginId, plugin.marketplaceName);
      showToast(
        result.success
          ? t("plugin_installedPlugin", { name: plugin.name })
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        pluginsSource = "installed";
        syncUrl("codex-plugins");
        await refreshCodexPluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorInstalling", { name: plugin.name, error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleUninstall(plugin: InstalledPlugin) {
    const scope = (plugin.scope as string) ?? "user";
    const cwd = resolvePluginCwd(plugin);
    confirmAction = {
      title: t("plugin_uninstallTitle"),
      message: t("plugin_uninstallMsg", { name: plugin.name }),
      onConfirm: async () => {
        operationLoading = pluginOpKey(plugin);
        dbg("plugins", "uninstall", { name: plugin.name, scope, cwd });
        try {
          const result =
            plugin.agent === "pi"
              ? isSharedPiExtension(plugin)
                ? await uninstallPiSharedExtension(sharedPiExtensionId(plugin))
                : isPiCodeProfilePlugin(plugin)
                  ? await uninstallPiProfileExtension(
                      "code",
                      plugin.pluginId ?? plugin.name,
                      scope as "user" | "project",
                      cwd,
                    )
                  : await uninstallPiExtension(
                      plugin.pluginId ?? plugin.name,
                      scope as "user" | "project",
                      cwd,
                    )
              : plugin.agent === "codex"
                ? await uninstallCodexPlugin(plugin.pluginId ?? plugin.name, plugin.marketplace)
                : await uninstallPlugin(installedClaudePluginReference(plugin), scope, cwd);
          dbg("plugins", "uninstall result", result);
          showToast(
            result.success
              ? t("plugin_uninstalledPlugin", { name: plugin.name })
              : t("plugin_failedOp", { error: result.message }),
            result.success ? "success" : "error",
          );
          if (result.success) {
            if (plugin.agent === "codex") await refreshCodexPluginData();
            else if (plugin.agent === "claude") await refreshClaudePluginData();
            else await refreshPluginData();
          }
        } catch (e) {
          showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  async function handleToggleEnabled(plugin: InstalledPlugin) {
    operationLoading = pluginOpKey(plugin);
    try {
      if (plugin.agent === "pi") {
        const newEnabled = plugin.enabled === false;
        if (isSharedPiExtension(plugin)) {
          const profile =
            ((plugin.extra as Record<string, unknown> | undefined)?.profile as string) === "work"
              ? "work"
              : "code";
          await togglePiSharedExtension(profile, sharedPiExtensionId(plugin), newEnabled);
          showToast(t(newEnabled ? "plugins_enabled" : "plugins_disabled"), "success");
          if (profile === "work") await loadWorkPluginData(true);
          else await refreshPluginData();
          return;
        }
        const scope = ((plugin.scope as string) ?? "user") as "user" | "project";
        const cwd = resolvePluginCwd(plugin);
        if (isPiCodeProfilePlugin(plugin)) {
          await togglePiProfileExtension(
            "code",
            plugin.pluginId ?? plugin.name,
            scope,
            newEnabled,
            cwd,
          );
        } else {
          await togglePiExtension(plugin.pluginId ?? plugin.name, scope, newEnabled, cwd);
        }
        showToast(t(newEnabled ? "plugins_enabled" : "plugins_disabled"), "success");
        await refreshPluginData();
      } else if (plugin.agent === "codex") {
        if (!plugin.pluginId) {
          showToast("Missing plugin ID — cannot toggle", "error");
          return;
        }
        const newEnabled = plugin.enabled === false;
        await toggleCodexPlugin(plugin.pluginId, newEnabled);
        showToast(t(newEnabled ? "plugins_enabled" : "plugins_disabled"), "success");
        await refreshCodexPluginData();
      } else {
        const action = plugin.enabled !== false ? "disable" : "enable";
        const scope = (plugin.scope as string) ?? "user";
        const cwd = resolvePluginCwd(plugin);
        const pluginName =
          plugin.agent === "claude" ? installedClaudePluginReference(plugin) : plugin.name;
        dbg("plugins", action, { name: pluginName, scope, cwd });
        const fn = plugin.enabled !== false ? disablePlugin : enablePlugin;
        const result = await fn(pluginName, scope, cwd);
        dbg("plugins", `${action} result`, result);
        showToast(
          result.success
            ? plugin.enabled !== false
              ? t("plugin_disabledPlugin", { name: plugin.name })
              : t("plugin_enabledPlugin", { name: plugin.name })
            : t("plugin_failedOp", { error: result.message }),
          result.success ? "success" : "error",
        );
        if (result.success) {
          if (plugin.agent === "claude") await refreshClaudePluginData();
          else await refreshPluginData();
        }
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleUpdate(plugin: InstalledPlugin) {
    const scope = (plugin.scope as string) ?? "user";
    const cwd = resolvePluginCwd(plugin);
    const pluginName =
      plugin.agent === "claude" ? installedClaudePluginReference(plugin) : plugin.name;
    operationLoading = pluginOpKey(plugin);
    dbg("plugins", "update", { name: pluginName, scope, cwd });
    try {
      const result =
        plugin.agent === "pi"
          ? isSharedPiExtension(plugin)
            ? await updatePiSharedExtension(sharedPiExtensionId(plugin))
            : isPiCodeProfilePlugin(plugin)
              ? await updatePiProfileExtension(
                  "code",
                  plugin.pluginId ?? plugin.name,
                  scope as "user" | "project",
                  cwd,
                )
              : await updatePiExtension(
                  plugin.pluginId ?? plugin.name,
                  scope as "user" | "project",
                  cwd,
                )
          : await updatePlugin(pluginName, scope, cwd);
      dbg("plugins", "update result", result);
      showToast(
        result.success
          ? t("plugin_updatedName", { name: plugin.name })
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        if (plugin.agent === "claude") await refreshClaudePluginData();
        else await refreshPluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  // ── Marketplace operation handlers ──

  async function handleAddMarketplace() {
    const source = newMarketplaceSource.trim();
    if (!source) return;
    operationLoading = "__marketplace_add";
    dbg("plugins", "addMarketplace", { source });
    try {
      const result = await addMarketplace(source);
      dbg("plugins", "addMarketplace result", result);
      showToast(
        result.success
          ? t("plugin_addedMarketplace")
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        newMarketplaceSource = "";
        await refreshClaudePluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleRemoveMarketplace(name: string) {
    confirmAction = {
      title: t("plugin_removeMarketplaceTitle"),
      message: t("plugin_removeMarketplaceMsg", { name }),
      onConfirm: async () => {
        operationLoading = `__mp_${name}`;
        dbg("plugins", "removeMarketplace", { name });
        try {
          const result = await removeMarketplace(name);
          dbg("plugins", "removeMarketplace result", result);
          showToast(
            result.success
              ? t("plugin_removedMarketplace", { name })
              : t("plugin_failedOp", { error: result.message }),
            result.success ? "success" : "error",
          );
          if (result.success) {
            await refreshClaudePluginData();
          }
        } catch (e) {
          showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  async function handleUpdateMarketplace(name: string) {
    operationLoading = `__mp_${name}`;
    dbg("plugins", "updateMarketplace", { name });
    try {
      const result = await updateMarketplace(name);
      dbg("plugins", "updateMarketplace result", result);
      showToast(
        result.success
          ? t("plugin_updatedName", { name })
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        await refreshClaudePluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleSyncClaudeOfficialMarketplace() {
    operationLoading = "__claude_official_sync";
    try {
      const result = await syncClaudeOfficialMarketplace();
      showToast(
        result.success
          ? t("plugin_claudeOfficialSynced")
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) await refreshClaudePluginData();
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  // ── Codex marketplace operation handlers ──

  async function handleAddCodexMarketplace() {
    const source = newCodexMarketplaceSource.trim();
    if (!source) return;
    operationLoading = "__codex_marketplace_add";
    dbg("plugins", "add codex marketplace", { source });
    try {
      const result = await addCodexMarketplace(source);
      showToast(
        result.success
          ? t("plugin_codexMarketplaceAdded")
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) {
        newCodexMarketplaceSource = "";
        await refreshCodexPluginData();
      }
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }

  async function handleRemoveCodexMarketplace(name: string) {
    confirmAction = {
      title: t("plugin_codexMarketplaceRemoveTitle"),
      message: t("plugin_codexMarketplaceRemoveMsg", { name }),
      onConfirm: async () => {
        operationLoading = `__codex_mp_${name}`;
        try {
          const result = await removeCodexMarketplace(name);
          showToast(
            result.success
              ? t("plugin_codexMarketplaceRemoved", { name })
              : t("plugin_failedOp", { error: result.message }),
            result.success ? "success" : "error",
          );
          if (result.success) await refreshCodexPluginData();
        } catch (e) {
          showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  async function handleUpgradeCodexMarketplace(name?: string) {
    const key = name ? `__codex_mp_${name}` : "__codex_marketplace_upgrade";
    operationLoading = key;
    try {
      const result = await upgradeCodexMarketplace(name);
      showToast(
        result.success
          ? t("plugin_codexMarketplaceRefreshed")
          : t("plugin_failedOp", { error: result.message }),
        result.success ? "success" : "error",
      );
      if (result.success) await refreshCodexPluginData();
    } catch (e) {
      showToast(t("plugin_errorGeneric", { error: String(e) }), "error");
    } finally {
      operationLoading = null;
    }
  }
</script>

<!-- Toast notification -->
{#if toastMessage}
  <div
    class="fixed top-4 right-4 z-50 rounded-lg border px-4 py-2 text-sm shadow-lg transition-opacity {toastType ===
    'success'
      ? 'border-green-500/30 bg-green-500/10 text-green-600 dark:text-green-400'
      : 'border-destructive/30 bg-destructive/10 text-destructive'}"
  >
    {toastMessage}
  </div>
{/if}

<!-- Confirmation dialog -->
{#if confirmAction}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    onclick={() => (confirmAction = null)}
  >
    <div
      class="rounded-lg border border-border bg-background p-6 shadow-xl max-w-sm"
      onclick={(e) => e.stopPropagation()}
    >
      <h3 class="text-sm font-semibold text-foreground mb-2">{confirmAction.title}</h3>
      <p class="text-xs text-muted-foreground mb-4">{confirmAction.message}</p>
      <div class="flex justify-end gap-2">
        <button
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
          onclick={() => (confirmAction = null)}>{t("common_cancel")}</button
        >
        <button
          class="rounded-md bg-destructive px-3 py-1.5 text-xs text-destructive-foreground hover:bg-destructive/90"
          onclick={() => {
            confirmAction?.onConfirm();
            confirmAction = null;
          }}>{t("plugin_confirm")}</button
        >
      </div>
    </div>
  </div>
{/if}

<div
  class:px-6={!embeddedProfile}
  class:py-5={!embeddedProfile}
  class:h-full={!embeddedProfile}
  class:overflow-y-auto={!embeddedProfile}
>
  {#if embeddedAgent}
    <div class="mb-4 flex items-center justify-between gap-3">
      <button
        type="button"
        class="inline-flex min-h-9 items-center gap-1.5 rounded-lg px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => goto(`/settings?tab=native-${embeddedAgent}`)}
        aria-label={`返回 ${embeddedAgent === "codex" ? "Codex" : "Claude Code"}`}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="m15 18-6-6 6-6" />
        </svg>
        返回 {embeddedAgent === "codex" ? "Codex" : "Claude Code"}
      </button>
      <span class="text-xs text-muted-foreground">Runtime Provider 扩展配置</span>
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center justify-center py-16">
      <div
        class="h-5 w-5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
      ></div>
    </div>
  {:else if loadError}
    <div class="flex flex-col items-center justify-center py-16 text-center">
      <p class="text-sm text-destructive">
        {t("plugin_loadFailed")}
      </p>
    </div>
  {:else}
    {#if isPiProfileScope() || isCapabilityCenterPage || pluginScope === "work"}
      <div class="mb-6 space-y-4">
        <CapabilityOverview
          projection={capabilityProjection}
          loading={loadingCapabilityProjection}
          onAction={handleCapabilityAction}
          onSelectItem={(item) => (selectedCapabilityItem = item)}
          onFilterReadiness={handleFilterReadiness}
          activeFilter={readinessFilter}
        />

        <!-- Intent Search Input -->
        <div
          class="flex items-center gap-2.5 rounded-xl border border-border/70 bg-card/60 px-3.5 py-2.5 shadow-sm"
        >
          <svg
            class="h-4 w-4 text-muted-foreground shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="你想让 AgentCabin 做什么？(例如: excel 分析、飞书文档、网页抓取...)"
            bind:value={intentSearchQuery}
            oninput={handleIntentSearchInput}
            class="w-full bg-transparent text-xs text-foreground placeholder:text-muted-foreground outline-none"
          />
          {#if searchingIntent}
            <span
              class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-primary border-t-transparent shrink-0"
            ></span>
          {/if}
          {#if intentSearchQuery}
            <button
              type="button"
              class="text-xs text-muted-foreground hover:text-foreground transition-colors shrink-0 px-1"
              onclick={clearIntentSearch}
            >
              清除
            </button>
          {/if}
        </div>

        <!-- Intent Search Results -->
        {#if intentSearchResults.length > 0}
          <div class="rounded-xl border border-primary/20 bg-primary/5 p-4">
            <div class="flex items-center justify-between mb-3">
              <span class="text-xs font-semibold text-foreground"
                >意图匹配能力 ({intentSearchResults.length})</span
              >
              <span class="text-[11px] text-muted-foreground">Intent-based Search Results</span>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {#each intentSearchResults as item (item.id)}
                <CapabilityItemCard
                  {item}
                  onAction={handleCapabilityAction}
                  onSelect={(i) => (selectedCapabilityItem = i)}
                />
              {/each}
            </div>
          </div>
        {:else if intentSearchQuery && !searchingIntent}
          <div
            class="rounded-xl border border-dashed border-border p-4 text-center text-xs text-muted-foreground"
          >
            未找到与 "{intentSearchQuery}" 匹配的能力
          </div>
        {/if}

        <!-- Readiness Filtered Results (when clicking Ready / Needs Setup / etc.) -->
        {#if readinessFilter && capabilityProjection}
          {@const filteredItems = capabilityProjection.items.filter((item) => {
            if (readinessFilter === "ready") return item.readiness === "ready";
            if (readinessFilter === "needs_auth") return item.readiness === "needs_auth";
            if (readinessFilter === "needs_setup")
              return (
                item.readiness === "missing_dependency" ||
                item.readiness === "incompatible" ||
                item.readiness === "not_installed"
              );
            if (readinessFilter === "unavailable")
              return item.readiness === "disabled" || item.readiness === "unhealthy";
            return true;
          })}
          <div class="rounded-xl border border-border/70 bg-card/40 p-4">
            <div class="flex items-center justify-between mb-3">
              <span class="text-xs font-semibold text-foreground">
                筛选状态: <span class="capitalize font-mono text-primary">{readinessFilter}</span>
              </span>
              <button
                type="button"
                class="text-xs text-muted-foreground hover:text-foreground"
                onclick={() => (readinessFilter = null)}
              >
                重置筛选
              </button>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {#each filteredItems as item (item.id)}
                <CapabilityItemCard
                  {item}
                  onAction={handleCapabilityAction}
                  onSelect={(i) => (selectedCapabilityItem = i)}
                />
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Partial load warning -->
    {#if activeTab === "codex-plugins" && loadWarnings.length > 0}
      <div
        class="rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-2 text-xs text-amber-600 dark:text-amber-400 mb-4"
      >
        {t("plugin_couldNotLoad", { items: loadWarnings.join(", ") })}
      </div>
    {/if}

    {#if pluginScope === "pi-code"}
      <div
        class="mb-5 flex w-fit max-w-full items-center gap-1 overflow-x-auto rounded-lg border border-border bg-muted/20 p-1"
        aria-label="Pi 能力类型"
      >
        {#each [{ id: "skills", label: "Skill" }, { id: "mcp", label: "MCP" }, { id: "connectors", label: "连接器" }, { id: "browser", label: "网络访问" }] as section}
          <button
            type="button"
            class="shrink-0 rounded-md px-5 py-1.5 text-sm font-medium transition-colors {activeTab ===
            section.id
              ? 'bg-primary text-primary-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'}"
            aria-pressed={activeTab === section.id}
            onclick={() => selectSection(section.id as PluginSection)}
          >
            {section.label}
          </button>
        {/each}
      </div>
    {/if}

    {#if pluginScope === "pi-code" && activeTab === "skills"}
      <WorkResourcePanel
        resources={piCodeSkillResources}
        source="enabled"
        showAll={true}
        canToggle={true}
        profileLabel="Code"
        onToggle={togglePiCodeSkill}
        onDelete={deletePiCodeSkill}
        title="已安装技能"
        description="Skill 定义 Code 的工作方法；安装实体与启用状态和 Work 全局共用。"
        emptyText="请先在能力中心安装 Skill。"
      />
    {/if}

    {#if pluginScope === "work"}
      <div class="space-y-5">
        {#if workResourcesLoading && !workResourcesLoaded && !isCapabilityCenterPage}
          <div class="flex items-center gap-2 py-8 text-sm text-muted-foreground">
            <span
              class="h-4 w-4 animate-spin rounded-full border-2 border-primary/25 border-t-primary"
            ></span>
            正在加载全局资源…
          </div>
        {:else if workResourcesError}
          <div
            class="rounded-lg border border-red-500/30 bg-red-500/5 px-4 py-3 text-xs text-red-500"
            role="alert"
          >
            {workResourcesError}
            <button class="ml-2 underline" onclick={() => void loadWorkPluginData(true)}
              >重试</button
            >
          </div>
        {:else}
          {#if pageMode === "catalog"}
            <header
              class="overflow-hidden rounded-2xl border border-border/70 bg-gradient-to-br from-card via-card to-emerald-500/5 shadow-sm mb-5"
              aria-label="能力中心"
            >
              <div class="px-4 pb-4 pt-5 sm:px-5">
                <div class="flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <div class="flex items-center gap-2">
                      <span
                        class="flex h-8 w-8 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                      >
                        <CapabilityCenterIcon class="h-4 w-4" />
                      </span>
                      <div>
                        <h1 class="text-base font-semibold text-foreground">能力中心</h1>
                        <p class="mt-0.5 text-xs text-muted-foreground">
                          统一管理 WorkBuddy 技能、专家、专家团、连接器以及
                          MCP；网络、浏览器和电脑控制位于一级工具设置。
                        </p>
                      </div>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    <span
                      class="rounded-lg border border-emerald-500/25 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-700 dark:text-emerald-300"
                      >全局开关 · 全部运行时</span
                    >
                  </div>
                </div>

                <div
                  id="capability-mode-help"
                  class="mt-3 rounded-xl border border-border/60 bg-background/45 px-3 py-2.5 text-[11px] leading-5 text-muted-foreground"
                >
                  <span class="font-semibold text-foreground">全局配置：</span>
                  一个开关同时影响所有对话与任务。安装实体、认证与公共配置共用；不同运行时仍保留各自的权限、审批、沙箱与交付边界。切换不会影响正在进行的会话。
                </div>

                <!-- 来源与支持运行时徽标 -->
                <div
                  class="mt-4 flex flex-wrap items-center gap-2 text-[10px] text-muted-foreground border-t border-border/40 pt-3"
                >
                  <span
                    class="rounded-md bg-background/80 border border-border/50 px-2 py-1 font-mono"
                  >
                    配置来源：~/.agentcabin
                  </span>
                  <span class="rounded-md bg-background/80 border border-border/50 px-2 py-1">
                    已登记的 Runtime Provider：{runtimeProviderSummary}
                  </span>
                </div>
              </div>

              <div class="border-t border-border/60 bg-background/30 px-2 pt-2 sm:px-3">
                <nav class="flex min-w-0 gap-1 overflow-x-auto" aria-label="能力类型">
                  {#each [{ id: "skills", label: "Skills", hint: "技能" }, { id: "experts", label: "专家", hint: "WorkBuddy Expert" }, { id: "expert-teams", label: "专家团", hint: "WorkBuddy Team" }, { id: "connectors", label: "连接器", hint: "WorkBuddy Connector" }, { id: "mcp", label: "MCP", hint: "Model Context Protocol" }] as section}
                    <button
                      type="button"
                      class="group relative min-h-14 shrink-0 rounded-t-xl px-4 pb-3 pt-2 text-left transition-colors {workCapabilitySection ===
                      section.id
                        ? 'bg-card text-foreground'
                        : 'text-muted-foreground hover:bg-card/50 hover:text-foreground'}"
                      aria-pressed={workCapabilitySection === section.id}
                      onclick={() =>
                        selectWorkCapabilitySection(section.id as WorkCapabilitySection)}
                    >
                      <span class="block text-xs font-semibold">{section.label}</span>
                      <span class="mt-0.5 block text-[9px] text-muted-foreground"
                        >{section.hint}</span
                      >
                      {#if workCapabilitySection === section.id}
                        <span class="absolute inset-x-4 bottom-0 h-0.5 rounded-full bg-emerald-500"
                        ></span>
                      {/if}
                    </button>
                  {/each}
                </nav>
              </div>
            </header>
          {:else}
            <header class="mb-5">
              <div
                class="flex w-fit max-w-full items-center gap-1 overflow-x-auto rounded-lg border border-border bg-muted/20 p-1"
              >
                <nav class="flex min-w-0 gap-1 overflow-x-auto" aria-label="Work 配置类型">
                  {#each [{ id: "skills", label: "Skill" }, { id: "experts", label: "专家" }, { id: "expert-teams", label: "专家团" }, { id: "connectors", label: "连接器" }, { id: "mcp", label: "MCP" }] as section}
                    <button
                      type="button"
                      class="shrink-0 rounded-md px-5 py-1.5 text-sm font-medium transition-colors {workCapabilitySection ===
                      section.id
                        ? 'bg-primary text-primary-foreground shadow-sm'
                        : 'text-muted-foreground hover:text-foreground'}"
                      aria-pressed={workCapabilitySection === section.id}
                      onclick={() =>
                        selectWorkCapabilitySection(section.id as WorkCapabilitySection)}
                    >
                      {section.label}
                    </button>
                  {/each}
                </nav>
              </div>
            </header>
          {/if}

          {#if pageMode === "catalog" && workCapabilitySection !== "web" && workCapabilitySection !== "browser" && workCapabilitySection !== "mcp" && workCapabilitySection !== "experts" && workCapabilitySection !== "expert-teams"}
            <div class="flex items-center justify-between gap-3">
              <div
                class="flex w-fit items-center gap-1 rounded-lg border border-border bg-muted/20 p-1"
                aria-label="能力范围"
              >
                <button
                  type="button"
                  class="min-h-9 rounded-md px-4 text-xs font-medium transition-colors {workCatalogSource ===
                  'discover'
                    ? 'bg-foreground text-background shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'}"
                  aria-pressed={workCatalogSource === "discover"}
                  onclick={() => selectWorkCatalogSource("discover")}
                >
                  发现
                </button>
                <button
                  type="button"
                  class="min-h-9 rounded-md px-4 text-xs font-medium transition-colors {workCatalogSource ===
                  'enabled'
                    ? 'bg-foreground text-background shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'}"
                  aria-pressed={workCatalogSource === "enabled"}
                  onclick={() => selectWorkCatalogSource("enabled")}
                >
                  已安装
                </button>
              </div>
              <p class="hidden text-[10px] text-muted-foreground sm:block">
                安装实体与启用状态全局共用；这里的开关会同时影响所有运行时
              </p>
            </div>
          {/if}

          {#if workCapabilitySection === "experts"}
            <AgentPluginsPanel canManage={pageMode === "catalog"} expertKind="expert" />
          {:else if workCapabilitySection === "expert-teams"}
            <AgentPluginsPanel canManage={pageMode === "catalog"} expertKind="expert-team" />
          {:else if workCapabilitySection === "skills"}
            {#if workCatalogSource === "discover"}
              <WorkSkillDiscoverPanel
                installed={workResources.filter((resource) => resource.kind === "skill")}
                onInstalled={addInstalledWorkResource}
              />
            {:else}
              <WorkResourcePanel
                resources={workResources.filter((resource) =>
                  ["skill", "capability", "artifact_tool"].includes(resource.kind),
                )}
                source={workCatalogSource}
                showAll={true}
                canToggle={true}
                bindings={skillBindingsMap}
                onToggle={handleToggleGlobalSkill}
                onDelete={async (id) => {
                  const resource = workResources.find((item) => item.id === id);
                  if (!resource) return;
                  await deleteSharedSkill(resource.entry);
                  await loadWorkPluginData(true);
                }}
                title="已安装技能"
                description="Skill 定义工作方法，本机能力提供基础操作，交付工具负责生成可验收产物。"
                emptyText="目前没有已安装的技能。"
              />
            {/if}
          {:else if workCapabilitySection === "mcp"}
            <section
              class="space-y-4 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5"
            >
              <div>
                <h2 class="text-sm font-semibold text-foreground">MCP</h2>
                <p class="mt-1 text-xs leading-5 text-muted-foreground">
                  MCP 安装实体与启用状态全局共用；这里的开关会同时影响所有运行时。
                </p>
              </div>
              {#if pageMode === "catalog"}
                <div class="flex w-fit items-center gap-1 rounded-lg border border-border p-0.5">
                  <button
                    type="button"
                    class="rounded-md px-3 py-1 text-xs font-medium {workMcpSource === 'discover'
                      ? 'bg-primary text-primary-foreground'
                      : 'text-muted-foreground hover:text-foreground'}"
                    onclick={() => selectWorkMcpSource("discover")}
                  >
                    发现
                  </button>
                  <button
                    type="button"
                    class="rounded-md px-3 py-1 text-xs font-medium {workMcpSource === 'configured'
                      ? 'bg-primary text-primary-foreground'
                      : 'text-muted-foreground hover:text-foreground'}"
                    onclick={() => selectWorkMcpSource("configured")}
                  >
                    已配置
                  </button>
                </div>
              {/if}
              {#if workMcpSource === "discover" && pageMode === "catalog"}
                <McpDiscoverPanel
                  {projectCwd}
                  visible={true}
                  targetRealm="work"
                  enableOnInstall={false}
                  bind:operationLoading
                  {showToast}
                />
              {:else}
                <McpConfiguredPanel
                  {projectCwd}
                  visible={true}
                  targetRealm="work"
                  canToggle={true}
                  bind:operationLoading
                  {showToast}
                  bind:confirmAction
                />
              {/if}
              {#if pageMode === "catalog" && workMcpSource === "discover"}
                <WorkMcpDiscoverPanel connectors={workConnectors} onSave={saveWorkConnector} />
              {/if}
              <WorkConnectorPanel
                connectors={workConnectors}
                source={pageMode === "config" || workMcpSource === "configured"
                  ? "enabled"
                  : "discover"}
                showAll={pageMode === "config" || workMcpSource === "configured"}
                canToggle={pageMode === "config"}
                onSave={saveWorkConnector}
                onToggle={toggleWorkConnector}
                onRemove={removeWorkConnector}
                onTest={testWorkConnection}
              />
            </section>
          {:else if workCapabilitySection === "connectors"}
            <WorkConnectorPackagesPanel
              packages={workConnectorPackages}
              source={workCatalogSource}
              showAll={pageMode === "config"}
              canEnable={true}
              onInstall={installWorkConnectorPackage}
              onTrust={trustWorkConnectorPackage}
              onEnable={enableWorkConnectorPackage}
              onUninstall={uninstallWorkConnectorPackage}
              onAuthorize={handleAuthorizeConnectorPackage}
              onConfigureToken={configureWorkConnectorToken}
            />
          {:else if workCapabilitySection === "apps"}
            <WorkAppsPanel
              catalog={workAppsCatalog}
              connections={workAppsConnections}
              source={workCatalogSource}
              onConnect={handleConnectApp}
              onDisconnect={handleDisconnectApp}
              onDisconnectAccount={handleDisconnectAppAccount}
              onSetDefaultAccount={handleSetDefaultAppAccount}
            />
          {:else if workCapabilitySection === "web"}
            <div class="space-y-5">
              <WebAccessPanel
                config={workBrowserConfig}
                enabled={webAccessEnabled}
                showSharedConfiguration={true}
                canToggle={true}
                onSave={saveWorkBrowser}
                onToggle={handleToggleWebAccess}
                onTest={testWorkBrowser}
              />
            </div>
          {:else if workCapabilitySection === "browser"}
            <BrowserUsePanel
              runtime={workBrowserConfig}
              enabled={browserUseEnabled}
              onToggle={handleToggleBrowserUse}
              onPrepare={prepareBrowserRuntime}
            />
          {/if}
        {/if}
      </div>
    {/if}

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Skills Section                                         -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div
      class="space-y-4"
      class:hidden={activeTab !== "skills" || pluginScope === "work" || pluginScope === "pi-code"}
    >
      <div>
        <h2 class="text-sm font-semibold text-foreground">{t("plugin_title")}</h2>
        <p class="text-xs text-muted-foreground">
          {t("plugin_desc")}
        </p>
      </div>

      <!-- Source toggle + Create Skill -->
      <div class="flex items-center gap-3">
        <div
          class="flex gap-1 rounded-lg border border-border p-0.5 w-fit"
          class:hidden={pageMode === "config"}
        >
          <button
            class="rounded-md px-3 py-1 text-xs font-medium transition-colors {skillsSource ===
            'discover'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => {
              skillsSource = "discover";
              syncUrl();
            }}>{t("plugin_discover")}</button
          >
          <button
            class="rounded-md px-3 py-1 text-xs font-medium transition-colors {skillsSource ===
            'installed'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => {
              skillsSource = "installed";
              syncUrl();
            }}>{t("plugin_installed")}</button
          >
        </div>
        {#if skillsSource === "installed"}
          <div class="flex rounded-md border border-border p-0.5 shrink-0">
            <button
              class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
              'all'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "all")}>All (全部)</button
            >
            <button
              class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
              'universal'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "universal")}>通用 (.agents)</button
            >
            {#if pluginScope !== "pi-code"}
              <button
                class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
                'claude'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => (skillsAgentFilter = "claude")}
                >{t("extend_agentBadge_claude")}</button
              >
              <button
                class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
                'codex'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => (skillsAgentFilter = "codex")}>{t("extend_agentBadge_codex")}</button
              >
              <button
                class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
                'grok'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => (skillsAgentFilter = "grok")}>Grok</button
              >
              <button
                class="rounded px-2.5 py-1 text-xs font-medium transition-colors {skillsAgentFilter ===
                'dsh'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => (skillsAgentFilter = "dsh")}>DSH</button
              >
            {/if}
          </div>
        {/if}
        <button
          class:hidden={pageMode === "config"}
          class="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm"
          onclick={handleOpenImportZipDialog}
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline
              points="17 8 12 3 7 8"
            /><line x1="12" x2="12" y1="3" y2="15" /></svg
          >
          <span>导入技能 (.zip)</span>
        </button>
      </div>

      <!-- Create Skill editor (shown inline, hides sub-views) -->
      {#if editorMode === "new"}
        <div class="rounded-lg border border-border/50 bg-muted/20 px-4 py-4 space-y-3">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-medium text-foreground">{t("plugin_createSkill")}</h3>
            <button
              class="text-xs text-muted-foreground hover:text-foreground"
              onclick={cancelEditor}>{t("common_cancel")}</button
            >
          </div>

          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1"
              >{t("plugin_editorName")}</label
            >
            <input
              type="text"
              placeholder="my-skill-name"
              class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={editorName}
            />
          </div>

          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1"
              >{t("plugin_editorDescription")}</label
            >
            <input
              type="text"
              placeholder="Brief description of what this skill does"
              class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={editorDescription}
            />
          </div>

          <!-- Agent selector -->
          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1"
              >{t("plugin_editorAgent")}</label
            >
            <div class="flex rounded-md border border-border p-0.5 w-fit">
              {#if pluginScope === "pi-code"}
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors {editorAgent ===
                  'pi'
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (editorAgent = "pi")}>Pi</button
                >
              {:else if pluginScope === "native" && isNativeAgent(capabilityScope)}
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors bg-primary text-primary-foreground"
                  aria-pressed="true"
                  disabled
                >
                  {nativeAgentLabel(capabilityScope)}
                </button>
              {:else}
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors {editorAgent ===
                  'claude'
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (editorAgent = "claude")}>{t("extend_agentBadge_claude")}</button
                >
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors {editorAgent ===
                  'codex'
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (editorAgent = "codex")}>{t("extend_agentBadge_codex")}</button
                >
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors {editorAgent ===
                  'grok'
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (editorAgent = "grok")}>Grok</button
                >
                <button
                  class="rounded px-2.5 py-1 text-xs font-medium transition-colors {editorAgent ===
                  'dsh'
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                  onclick={() => (editorAgent = "dsh")}>DSH</button
                >
              {/if}
            </div>
          </div>

          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1"
              >{t("plugin_editorScope")}</label
            >
            <select
              class="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={editorScope}
            >
              <option value="user">{t("plugin_editorScopeUser")}</option>
              <option value="project" disabled={!projectCwd}>
                {t("plugin_editorScopeProject")}
                {projectCwd ? "" : t("plugin_editorNoProject")}
              </option>
            </select>
          </div>

          <div>
            <label class="block text-xs font-medium text-muted-foreground mb-1"
              >{t("plugin_editorContent")}</label
            >
            <textarea
              class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring resize-y"
              rows="16"
              placeholder="# Skill Title&#10;&#10;Instructions for Claude..."
              bind:value={editorContent}
            ></textarea>
          </div>

          <div class="flex justify-end gap-2">
            <button
              class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
              onclick={cancelEditor}>{t("common_cancel")}</button
            >
            <button
              class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
              onclick={handleCreateSkill}
              disabled={editorSaving || !editorName.trim()}
            >
              {editorSaving ? t("plugin_saving") : t("plugin_createSkill")}
            </button>
          </div>
        </div>
      {/if}

      <!-- Discover sub-view (community skills) -->
      <div class:hidden={skillsSource !== "discover" || editorMode === "new"}>
        <!-- Health badge + search + scope -->
        <div class="flex items-center gap-3 mb-4">
          <!-- Health indicator + refresh -->
          <div class="flex items-center gap-1 shrink-0">
            <div class="flex items-center gap-1.5" title={communityHealth?.reason ?? ""}>
              <span
                class="inline-block h-2 w-2 rounded-full {communityHealth === null
                  ? 'bg-muted-foreground/40'
                  : communityHealth.available
                    ? 'bg-green-500'
                    : 'bg-red-500'}"
              ></span>
              <span class="text-[10px] text-muted-foreground">skills.sh</span>
            </div>
            <button
              class="rounded p-1 text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-40"
              title="Refresh"
              disabled={communityRefreshing}
              onclick={refreshCommunity}
            >
              <svg
                class="h-3 w-3 {communityRefreshing ? 'animate-spin' : ''}"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" /><path
                  d="M21 3v5h-5"
                /></svg
              >
            </button>
          </div>

          <!-- Search input -->
          <div class="relative flex-1">
            <svg
              class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" /></svg
            >
            <input
              type="text"
              placeholder={t("plugin_searchCommunity")}
              class="w-full rounded-md border border-border bg-background pl-8 pr-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={communityQuery}
              oninput={handleCommunitySearch}
            />
          </div>

          <!-- Scope selector -->
          <div class="flex rounded-md border border-border p-0.5 shrink-0">
            <button
              class="rounded px-2 py-1 text-xs font-medium transition-colors {communityScope ===
              'user'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (communityScope = "user")}>{t("plugin_scopeUser")}</button
            >
            <button
              class="rounded px-2 py-1 text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed {communityScope ===
              'project'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              disabled={!projectCwd}
              onclick={() => (communityScope = "project")}>{t("plugin_scopeProject")}</button
            >
          </div>
        </div>

        <!-- Quick filters -->
        <div class="flex flex-wrap gap-1.5 mb-4">
          {#each ["react", "python", "security", "testing", "devops", "best practices"] as filter}
            <button
              class="rounded-full border border-border px-2.5 py-0.5 text-[11px] text-muted-foreground hover:text-foreground hover:border-foreground/30 transition-colors {communityQuery ===
              filter
                ? 'bg-primary/10 border-primary/30 text-foreground'
                : ''}"
              onclick={() => setCommunityFilter(filter)}
            >
              {filter}
            </button>
          {/each}
        </div>

        <!-- Loading spinner for search -->
        {#if communitySearching}
          <div class="flex items-center justify-center py-4">
            <div
              class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
            ></div>
            <span class="ml-2 text-xs text-muted-foreground">{t("plugin_searching")}</span>
          </div>
        {/if}

        <!-- Results / Popular -->
        {#if !communitySearching}
          <div>
            <div class="text-xs font-medium text-muted-foreground mb-2">
              {communityQuery.trim().length >= 2
                ? t("plugin_nResults", { count: String(communityResults.length) })
                : t("plugin_popularSkills")}
            </div>

            {#if communityDisplayResults.length === 0}
              <div class="flex flex-col items-center justify-center py-12 text-center gap-2">
                <p class="text-xs text-muted-foreground">
                  {communityQuery.trim().length >= 2
                    ? t("plugin_noSkillsFound")
                    : communityRefreshing
                      ? t("plugin_loadingPopular")
                      : t("plugin_couldNotLoadPopular")}
                </p>
                {#if communityQuery.trim().length < 2 && !communityRefreshing}
                  <button
                    class="rounded-md border border-border px-3 py-1.5 text-xs text-foreground hover:bg-muted transition-colors"
                    onclick={refreshCommunity}
                  >
                    {t("common_retry")}
                  </button>
                {/if}
              </div>
            {:else}
              <!-- Side-by-side: skill list (left) + preview (right) -->
              <div class="flex gap-3" style="height: calc(100vh - 320px); min-height: 300px;">
                <!-- Left: scrollable skill list -->
                <div class="w-[280px] shrink-0 overflow-y-auto space-y-1.5 pr-1">
                  {#each communityDisplayResults as skill}
                    {@const installedLocs =
                      installedSkillsMap.get(toLocalSlug(skill.skill_id)) ?? []}
                    {@const isInstalledInCurrentScope = installedLocs.some(
                      (l) => l.scope === communityScope && l.agent === "universal",
                    )}
                    <div
                      class="w-full text-left rounded-lg border px-3 py-2 transition-colors cursor-pointer {communityDetail?.id ===
                      skill.id
                        ? 'border-primary/50 bg-primary/5'
                        : 'border-border/50 bg-muted/30 hover:bg-muted/50'}"
                      onclick={() => handleCommunityDetail(skill)}
                      onkeydown={(e) => {
                        if (e.key === "Enter") handleCommunityDetail(skill);
                      }}
                      role="button"
                      tabindex="0"
                    >
                      <div class="flex items-center justify-between gap-2">
                        <div class="flex-1 min-w-0">
                          <span class="text-sm font-medium text-foreground truncate block"
                            >{skill.name}</span
                          >
                          <div class="flex items-center gap-2 mt-0.5">
                            {#if skill.installs > 0}
                              <span class="text-[11px] text-muted-foreground"
                                >{formatInstallCount(skill.installs)}</span
                              >
                            {/if}
                            <span class="text-[10px] text-muted-foreground truncate"
                              >{skill.source}</span
                            >
                          </div>
                        </div>
                        <button
                          class="rounded-md px-2 py-1 text-xs font-medium transition-colors disabled:opacity-50 shrink-0 {isInstalledInCurrentScope
                            ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 hover:bg-emerald-500/20'
                            : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
                          onclick={(e) => {
                            e.stopPropagation();
                            openCommunityInstallModal(skill);
                          }}
                          disabled={operationLoading === skill.id}
                        >
                          {#if operationLoading === skill.id}
                            ...
                          {:else if isInstalledInCurrentScope}
                            {t("plugin_installed")}
                          {:else}
                            {t("plugin_install")}
                          {/if}
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>

                <!-- Right: preview panel (sticky) -->
                <div class="flex-1 min-w-0 overflow-y-auto">
                  {#if communityDetailLoading}
                    <div
                      class="rounded-lg border border-border/50 bg-muted/20 p-6 flex items-center justify-center h-full"
                    >
                      <div
                        class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                      ></div>
                      <span class="ml-2 text-xs text-muted-foreground"
                        >{t("plugin_loadingPreview")}</span
                      >
                    </div>
                  {:else if communityDetailError}
                    <div
                      class="rounded-lg border border-destructive/30 bg-destructive/5 p-6 flex flex-col items-center justify-center h-full gap-2 text-center"
                    >
                      <span class="text-2xl">⚠</span>
                      <p class="text-xs font-medium text-destructive">
                        {t("plugin_skillUnavailable")}
                      </p>
                      <p class="text-[10px] text-muted-foreground max-w-[280px] leading-relaxed">
                        {communityDetailError}
                      </p>
                    </div>
                  {:else if communityDetail}
                    {@const detailLocs =
                      installedSkillsMap.get(toLocalSlug(communityDetail.id)) ?? []}
                    <div class="rounded-lg border border-border/50 bg-muted/20 p-4 space-y-3">
                      <!-- Header -->
                      <div class="flex items-start justify-between gap-2">
                        <div class="flex-1 min-w-0">
                          <h3 class="text-sm font-semibold text-foreground truncate">
                            {communityDetail.name}
                          </h3>
                          <div class="flex items-center gap-2 mt-1 flex-wrap">
                            {#if communityDetail.installs > 0}
                              <span class="text-[10px] text-muted-foreground"
                                >{formatInstallCount(communityDetail.installs)}
                                {t("plugin_installs")}</span
                              >
                            {/if}
                            <span class="text-[10px] text-muted-foreground/60"
                              >{communityDetail.source}</span
                            >
                            {#if communityDetail.github_url}
                              <a
                                href={communityDetail.github_url}
                                target="_blank"
                                rel="noopener noreferrer"
                                class="text-[10px] text-muted-foreground hover:text-foreground underline"
                                >GitHub</a
                              >
                            {/if}
                            {#if communityDetail.skills_sh_url}
                              <a
                                href={communityDetail.skills_sh_url}
                                target="_blank"
                                rel="noopener noreferrer"
                                class="text-[10px] text-muted-foreground hover:text-foreground underline"
                                >skills.sh</a
                              >
                            {/if}
                          </div>
                          {#if communityDetail.description}
                            <p class="text-xs text-muted-foreground mt-1.5">
                              {communityDetail.description}
                            </p>
                          {/if}
                          {#if detailLocs.length > 0}
                            <div class="mt-2.5 flex items-center gap-1.5 flex-wrap">
                              <span class="text-[11px] font-medium text-muted-foreground"
                                >已安装于:</span
                              >
                              {#each detailLocs as loc}
                                <span
                                  class="rounded-md px-2 py-0.5 text-[10px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                                >
                                  {loc.scope === "user" ? "全局" : "项目"} · {loc.agent ===
                                  "universal"
                                    ? "Universal (.agents)"
                                    : loc.agent === "claude"
                                      ? "Claude"
                                      : loc.agent === "codex"
                                        ? "Codex"
                                        : "Pi"}
                                </span>
                              {/each}
                            </div>
                          {/if}
                        </div>
                        <button
                          class="shrink-0 text-muted-foreground hover:text-foreground"
                          onclick={() => (communityDetail = null)}
                          title={t("plugin_closePreview")}
                        >
                          <svg
                            class="h-3.5 w-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            ><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg
                          >
                        </button>
                      </div>

                      <!-- SKILL.md content -->
                      {#if communityDetail.content}
                        <div class="border-t border-border pt-3">
                          <div class="prose prose-sm dark:prose-invert max-w-none">
                            {@html renderMarkdown(communityDetail.content)}
                          </div>
                        </div>
                      {:else}
                        <div class="border-t border-border pt-3">
                          <p class="text-xs text-muted-foreground italic">
                            {t("plugin_noContentPreview")}
                          </p>
                        </div>
                      {/if}
                    </div>
                  {:else}
                    <div
                      class="rounded-lg border border-dashed border-border/50 p-6 flex items-center justify-center h-full"
                    >
                      <p class="text-xs text-muted-foreground">{t("plugin_clickToPreview")}</p>
                    </div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Installed sub-view (standalone skills) -->
      <div class:hidden={skillsSource !== "installed" || editorMode === "new"}>
        <div class="mb-4 flex items-center justify-between">
          <h3 class="text-xs font-medium text-muted-foreground">
            {t("plugin_standaloneSkills")}
          </h3>
          <div class="flex rounded-md border border-border p-0.5 shrink-0">
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'all'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "all")}>All (全部)</button
            >
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'universal'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "universal")}>通用 (.agents)</button
            >
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'claude'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "claude")}>{t("extend_agentBadge_claude")}</button
            >
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'codex'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "codex")}>{t("extend_agentBadge_codex")}</button
            >
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'grok'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "grok")}>Grok</button
            >
            <button
              class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors {skillsAgentFilter ===
              'dsh'
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (skillsAgentFilter = "dsh")}>DSH</button
            >
          </div>
        </div>

        {#if capabilitySkills.length === 0 && !editorMode}
          <div class="flex flex-col items-center justify-center py-16 text-center">
            <div
              class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
            >
              <svg
                class="h-6 w-6 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" /></svg
              >
            </div>
            <h2 class="text-sm font-medium text-foreground mb-1">
              {t("plugin_noStandaloneSkills")}
            </h2>
            <p class="text-xs text-muted-foreground max-w-sm mb-3">
              {t("plugin_skillsEmptyDesc")}
            </p>
            <button
              class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
              onclick={startNewSkill}
            >
              {t("plugin_createFirstSkill")}
            </button>
          </div>
        {:else}
          <div class="flex gap-3" style="height: calc(100vh - 320px); min-height: 300px;">
            <!-- Left: scrollable skill list -->
            <div class="w-[280px] shrink-0 overflow-y-auto space-y-1.5 pr-1">
              {#each displayedSkills as skill}
                {@const cat = getSkillAgentCategory(skill)}
                <div
                  class="w-full text-left rounded-lg border px-3 py-2 transition-colors cursor-pointer {skill.enabled ===
                  false
                    ? 'opacity-50'
                    : ''} {selectedSkillKey === skillKey(skill) && !editorMode
                    ? 'border-primary/50 bg-primary/5'
                    : 'border-border/50 bg-muted/30 hover:bg-muted/50'}"
                  onclick={() => {
                    if (canEditSkill(skill)) {
                      startEditSkill(skill);
                    } else {
                      cancelEditor();
                      selectedSkillKey = skillKey(skill);
                      loadReadonlyContent(skill);
                    }
                  }}
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      if (canEditSkill(skill)) startEditSkill(skill);
                      else {
                        cancelEditor();
                        selectedSkillKey = skillKey(skill);
                        loadReadonlyContent(skill);
                      }
                    }
                  }}
                  role="button"
                  tabindex="0"
                >
                  <div class="flex items-center justify-between gap-2">
                    <div class="flex-1 min-w-0">
                      <span class="text-sm font-medium text-foreground truncate block"
                        >{skill.name}</span
                      >
                      <div class="flex items-center gap-1.5 mt-0.5 flex-wrap">
                        {#if skill.scope}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {skill.scope ===
                            'project'
                              ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                              : 'bg-muted text-muted-foreground'}">{skill.scope}</span
                          >
                        {/if}
                        {#if cat === "universal"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-cyan-500/10 text-cyan-600 dark:text-cyan-400"
                          >
                            Universal
                          </span>
                        {:else if cat === "codex"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                          >
                            {t("extend_agentBadge_codex")}
                          </span>
                        {:else if cat === "pi"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-purple-500/10 text-purple-600 dark:text-purple-400"
                          >
                            Pi
                          </span>
                        {/if}
                        {#if skill.source_kind === "bundled"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-violet-500/10 text-violet-600 dark:text-violet-400"
                          >
                            {t("extend_skills_sourceKind_bundled")}
                          </span>
                        {:else if skill.source_kind === "legacy"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400"
                          >
                            {t("extend_skills_sourceKind_legacy")}
                          </span>
                        {:else if skill.source_kind === "project-codex"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-cyan-500/10 text-cyan-600 dark:text-cyan-400"
                          >
                            {t("extend_skills_sourceKind_projectCodex")}
                          </span>
                        {:else if skill.source_kind === "project-agents"}
                          <span
                            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-green-500/10 text-green-600 dark:text-green-400"
                          >
                            {t("extend_skills_sourceKind_projectAgents")}
                          </span>
                        {/if}
                        <span class="text-[11px] text-muted-foreground truncate"
                          >{skill.description}</span
                        >
                      </div>
                    </div>
                    <div class="flex items-center gap-1 shrink-0">
                      {#if (skill.agent === "codex" || skill.agent === "pi") && canToggleSkill(skill)}
                        <button
                          class="rounded p-1 text-muted-foreground hover:text-foreground transition-colors"
                          onclick={(e) => {
                            e.stopPropagation();
                            handleToggleSkill(skill);
                          }}
                          title={skill.enabled === false
                            ? t("plugin_skillToggleEnable")
                            : t("plugin_skillToggleDisable")}
                          disabled={operationLoading === skillKey(skill)}
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
                            {#if skill.enabled === false}
                              <path
                                d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"
                              /><path
                                d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"
                              /><line x1="1" x2="23" y1="1" y2="23" />
                            {:else}
                              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle
                                cx="12"
                                cy="12"
                                r="3"
                              />
                            {/if}
                          </svg>
                        </button>
                      {:else if (skill.agent === "codex" || skill.agent === "pi") && !canToggleSkill(skill)}
                        <span
                          class="rounded p-1 text-muted-foreground/50"
                          title={skill.disabled_by ?? t("plugin_skillCannotToggle")}
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
                            <rect width="18" height="11" x="3" y="11" rx="2" ry="2" /><path
                              d="M7 11V7a5 5 0 0 1 10 0v4"
                            />
                          </svg>
                        </span>
                      {/if}
                      {#if canDeleteSkill(skill)}
                        <button
                          class="rounded p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                          onclick={(e) => {
                            e.stopPropagation();
                            handleDeleteSkill(skill);
                          }}
                          title={t("plugin_deleteSkillTooltip")}
                          disabled={operationLoading === skillKey(skill)}
                        >
                          <svg
                            class="h-3.5 w-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            ><path d="M3 6h18" /><path
                              d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"
                            /><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /></svg
                          >
                        </button>
                      {/if}
                    </div>
                  </div>
                </div>
              {/each}
            </div>

            <!-- Right: Edit editor / read-only detail / placeholder -->
            <div class="flex-1 min-w-0 overflow-y-auto">
              {#if editorMode === "edit"}
                <!-- Skill edit editor -->
                <div class="rounded-lg border border-border/50 bg-muted/20 px-4 py-4 space-y-3">
                  <div class="flex items-center justify-between">
                    <h3 class="text-sm font-medium text-foreground">
                      {t("plugin_editSkillHeader", { name: editorName })}
                    </h3>
                    <button
                      class="text-xs text-muted-foreground hover:text-foreground"
                      onclick={cancelEditor}>{t("common_cancel")}</button
                    >
                  </div>

                  <div>
                    <label class="block text-xs font-medium text-muted-foreground mb-1"
                      >{t("plugin_skillMdContent")}</label
                    >
                    <textarea
                      class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring resize-y"
                      rows="16"
                      placeholder="# Skill Title&#10;&#10;Instructions for Claude..."
                      bind:value={editorContent}
                    ></textarea>
                  </div>

                  <div class="flex justify-end gap-2">
                    <button
                      class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                      onclick={cancelEditor}>{t("common_cancel")}</button
                    >
                    <button
                      class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
                      onclick={handleSaveSkill}
                      disabled={editorSaving}
                    >
                      {editorSaving ? t("plugin_saving") : t("plugin_saveChanges")}
                    </button>
                  </div>
                </div>
              {:else if selectedSkillKey && !editorMode}
                {@const skill = skills.find((s) => skillKey(s) === selectedSkillKey)}
                {#if skill}
                  <div class="rounded-lg border border-border/50 bg-muted/20 p-4 space-y-3">
                    <div class="flex items-start justify-between">
                      <div>
                        <h3 class="text-sm font-semibold">{skill.name}</h3>
                        <p class="text-[11px] text-muted-foreground mt-1">{skill.description}</p>
                        <div class="flex items-center gap-1.5 mt-1.5 flex-wrap">
                          {#if skill.scope}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {skill.scope ===
                              'project'
                                ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                                : 'bg-muted text-muted-foreground'}">{skill.scope}</span
                            >
                          {/if}
                          {#if skill.agent === "codex"}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                              >{t("extend_agentBadge_codex")}</span
                            >
                          {/if}
                          {#if skill.source_kind === "bundled"}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-violet-500/10 text-violet-600 dark:text-violet-400"
                              >{t("extend_skills_sourceKind_bundled")}</span
                            >
                          {:else if skill.source_kind === "legacy"}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400"
                              >{t("extend_skills_sourceKind_legacy")}</span
                            >
                          {:else if skill.source_kind === "project-codex"}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-cyan-500/10 text-cyan-600 dark:text-cyan-400"
                              >{t("extend_skills_sourceKind_projectCodex")}</span
                            >
                          {:else if skill.source_kind === "project-agents"}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-green-500/10 text-green-600 dark:text-green-400"
                              >{t("extend_skills_sourceKind_projectAgents")}</span
                            >
                          {/if}
                          {#if skill.enabled === false}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400"
                              >{t("plugin_skillStatusDisabled")}</span
                            >
                          {/if}
                        </div>
                      </div>
                      <button
                        class="shrink-0 text-muted-foreground hover:text-foreground"
                        onclick={() => (selectedSkillKey = null)}
                        title={t("common_close")}
                      >
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          ><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg
                        >
                      </button>
                    </div>
                    <div
                      class="text-[11px] font-mono text-muted-foreground border-t border-border pt-2"
                    >
                      {skill.path}
                    </div>
                    <!-- Read-only SKILL.md content -->
                    {#if readonlyLoading}
                      <div class="border-t border-border pt-3 flex items-center gap-2">
                        <div
                          class="h-3.5 w-3.5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                        ></div>
                        <span class="text-[11px] text-muted-foreground"
                          >{t("plugin_skillLoadingContent")}</span
                        >
                      </div>
                    {:else if readonlyContent}
                      <div class="border-t border-border pt-3">
                        <div class="prose prose-sm dark:prose-invert max-w-none text-xs">
                          {@html renderMarkdown(readonlyContent)}
                        </div>
                      </div>
                    {/if}
                    {#if canDeleteSkill(skill)}
                      <div class="border-t border-border pt-3">
                        <button
                          class="rounded-md border border-destructive/30 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                          onclick={() => handleDeleteSkill(skill)}
                          disabled={operationLoading === skillKey(skill)}
                        >
                          {t("plugin_deleteSkillTitle")}
                        </button>
                      </div>
                    {/if}
                  </div>
                {/if}
              {:else}
                <div
                  class="rounded-lg border border-dashed border-border/50 p-6 flex items-center justify-center h-full"
                >
                  <p class="text-xs text-muted-foreground">{t("plugin_selectSkillToEdit")}</p>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- MCP Servers Section                                    -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-4" class:hidden={activeTab !== "mcp" || pluginScope === "work"}>
      <div>
        <h2 class="text-sm font-semibold text-foreground">{t("plugin_mcpTitle")}</h2>
        <p class="text-xs text-muted-foreground">
          {t("plugin_mcpDesc")}
        </p>
      </div>

      <!-- Source toggle -->
      <div
        class="flex gap-1 rounded-lg border border-border p-0.5 w-fit"
        class:hidden={pageMode === "config"}
      >
        <button
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mcpSource ===
          'discover'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => {
            mcpSource = "discover";
            syncUrl();
          }}>{t("plugin_discover")}</button
        >
        <button
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors {mcpSource ===
          'configured'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => {
            mcpSource = "configured";
            syncUrl();
          }}>{t("plugin_configured")}</button
        >
      </div>

      <!-- Discover sub-view -->
      <div class:hidden={mcpSource !== "discover" || pageMode === "config"}>
        <McpDiscoverPanel
          {projectCwd}
          visible={mcpSource === "discover"}
          targetRealm={pluginScope === "pi-code" ? "pi" : "native"}
          targetAgent={pluginScope === "native" &&
          (capabilityScope === "claude" ||
            capabilityScope === "codex" ||
            capabilityScope === "grok")
            ? capabilityScope
            : "all"}
          bind:operationLoading
          {showToast}
        />
      </div>

      <!-- Configured sub-view -->
      <div class:hidden={mcpSource !== "configured"}>
        <McpConfiguredPanel
          {projectCwd}
          visible={mcpSource === "configured"}
          targetRealm={pluginScope === "pi-code" ? "pi" : "native"}
          targetAgent={pluginScope === "native" &&
          (capabilityScope === "claude" ||
            capabilityScope === "codex" ||
            capabilityScope === "grok")
            ? capabilityScope
            : "all"}
          bind:operationLoading
          {showToast}
          bind:confirmAction
        />
      </div>
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Global Rules Section (Code Mode)                       -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-4" class:hidden={activeTab !== "rules" || pluginScope === "work"}>
      <CodeGlobalRulesPanel
        {showToast}
        initialAgent={pluginScope === "pi-code"
          ? "pi-code"
          : capabilityScope === "all" || capabilityScope === "dsh"
            ? "all"
            : capabilityScope}
        nativeOnly={pluginScope === "native"}
      />
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Shared Prompt Templates Section                          -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-4" class:hidden={activeTab !== "prompts" || pluginScope === "work"}>
      <PromptTemplatesPanel visible={activeTab === "prompts"} {showToast} bind:confirmAction />
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Plugins Section                                        -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div
      class="space-y-4"
      class:hidden={pluginScope === "work" ||
        pluginScope === "pi-code" ||
        (activeTab !== "claude-plugins" && activeTab !== "codex-plugins")}
    >
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">
            {pluginsAgent === "claude"
              ? t("plugin_claudePluginsTitle")
              : t("plugin_codexPluginsTitle")}
          </h2>
          <span
            class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {pluginsAgent === 'claude'
              ? 'bg-violet-500/10 text-violet-600 dark:text-violet-400'
              : 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'}"
          >
            {pluginsAgent === "claude" ? t("plugin_agentClaude") : t("plugin_agentCodex")}
          </span>
        </div>
        <p class="text-xs text-muted-foreground">
          {pluginsAgent === "claude" ? t("plugin_claudePluginsDesc") : t("plugin_codexPluginsDesc")}
        </p>
      </div>

      <!-- Each agent has its own independent plugin page; only the source
           within that page changes between available and installed entries. -->
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex gap-1 rounded-lg border border-border p-0.5 w-fit">
          <button
            type="button"
            class="rounded-md px-3 py-1 text-xs font-medium transition-colors {pluginsSource ===
            'marketplace'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => {
              pluginsSource = "marketplace";
              syncUrl();
            }}
            >{pluginsAgent === "codex"
              ? t("plugin_codexAvailableCount", { count: String(codexAvailablePlugins.length) })
              : t("plugin_claudeAvailableCount", {
                  count: String(claudeAvailablePlugins.length),
                })}</button
          >
          <button
            type="button"
            class="rounded-md px-3 py-1 text-xs font-medium transition-colors {pluginsSource ===
            'installed'
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => {
              pluginsSource = "installed";
              syncUrl();
            }}
            >{t("plugin_installedCount", {
              count: String(
                pluginsAgent === "codex" ? codexInstalledPlugins.length : installedPlugins.length,
              ),
            })}</button
          >
        </div>
        {#if pluginsAgent === "codex"}
          <span class="text-[11px] text-muted-foreground">{t("plugin_codexGlobalScope")}</span>
        {/if}
      </div>

      {#if pluginsAgent === "codex"}
        <div
          class="flex items-start gap-2 rounded-lg border border-emerald-500/20 bg-emerald-500/5 px-3 py-2.5 text-xs text-muted-foreground"
        >
          <svg
            class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><circle cx="12" cy="12" r="9" /><path d="M12 8v4l2.5 2.5" /></svg
          >
          <span>{t("plugin_codexManagedHint")}</span>
        </div>
      {/if}

      <!-- Marketplace sub-view -->
      {#if pluginsAgent === "claude"}
        <div class:hidden={pluginsSource !== "marketplace"} class="space-y-4">
          <div class="rounded-xl border border-violet-500/20 bg-violet-500/5 p-4">
            <div class="flex items-start justify-between gap-3">
              <div>
                <h3 class="text-sm font-medium text-foreground">
                  {t("plugin_claudeMarketplaceTitle")}
                </h3>
                <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
                  {t("plugin_claudeMarketplaceDesc")}
                </p>
              </div>
              <button
                type="button"
                class="shrink-0 rounded-md border border-border px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                onclick={() => handleSyncClaudeOfficialMarketplace()}
                disabled={operationLoading === "__claude_official_sync"}
              >
                {operationLoading === "__claude_official_sync"
                  ? t("plugin_updating")
                  : t("plugin_refresh")}
              </button>
            </div>

            <form
              class="mt-4 flex flex-col gap-2 sm:flex-row"
              onsubmit={(event) => {
                event.preventDefault();
                handleAddMarketplace();
              }}
            >
              <input
                type="text"
                placeholder={t("plugin_claudeMarketplacePlaceholder")}
                aria-label={t("plugin_claudeMarketplacePlaceholder")}
                class="min-h-9 flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                bind:value={newMarketplaceSource}
              />
              <button
                type="submit"
                class="min-h-9 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
                disabled={!newMarketplaceSource.trim() || operationLoading === "__marketplace_add"}
              >
                {operationLoading === "__marketplace_add" ? t("plugin_adding") : t("plugin_add")}
              </button>
            </form>

            {#if claudeMarketplaces.length === 0}
              <p class="mt-3 text-xs text-muted-foreground">{t("plugin_noMarketplaces")}</p>
            {:else}
              <div class="mt-4 grid gap-2 md:grid-cols-2">
                {#each claudeMarketplaces as marketplace}
                  {@const marketplaceLoadingKey = marketplace.builtin
                    ? "__claude_official_sync"
                    : "__mp_" + marketplace.name}
                  <div
                    class="flex min-w-0 items-center justify-between gap-3 rounded-lg border border-border/60 bg-background/70 px-3 py-2.5"
                  >
                    <div class="min-w-0">
                      <div class="flex items-center gap-2">
                        <p class="truncate text-xs font-medium text-foreground">
                          {marketplace.display_name ?? marketplace.name}
                        </p>
                        {#if marketplace.builtin}
                          <span
                            class="shrink-0 rounded-full bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-600 dark:text-violet-400"
                            >{t("plugin_claudeBuiltin")}</span
                          >
                        {/if}
                      </div>
                      <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
                        {marketplace.builtin
                          ? t("plugin_claudeOfficialSource")
                          : marketplaceSourceLabel(marketplace)}
                      </p>
                      {#if marketplace.plugin_count > 0}
                        <p class="mt-0.5 text-[10px] text-muted-foreground">
                          {t("plugin_pluginCount", { count: String(marketplace.plugin_count) })}
                        </p>
                      {/if}
                    </div>
                    <div class="flex shrink-0 items-center gap-1.5">
                      <button
                        type="button"
                        class="rounded-md border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                        onclick={() =>
                          marketplace.builtin
                            ? handleSyncClaudeOfficialMarketplace()
                            : handleUpdateMarketplace(marketplace.name)}
                        disabled={operationLoading === marketplaceLoadingKey}
                      >
                        {operationLoading === marketplaceLoadingKey
                          ? t("plugin_updating")
                          : t("plugin_refresh")}
                      </button>
                      {#if !marketplace.builtin}
                        <button
                          type="button"
                          class="rounded-md border border-destructive/30 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                          onclick={() => handleRemoveMarketplace(marketplace.name)}
                          disabled={operationLoading === marketplaceLoadingKey}
                        >
                          {t("plugin_remove")}
                        </button>
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <div class="flex flex-col gap-2 sm:flex-row">
            <div class="relative flex-1">
              <svg
                class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" /></svg
              >
              <input
                type="search"
                placeholder={t("plugin_claudeSearchPlaceholder")}
                aria-label={t("plugin_claudeSearchPlaceholder")}
                class="w-full rounded-md border border-border bg-background py-2 pl-8 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                bind:value={claudeSearchQuery}
              />
            </div>
            <select
              class="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              value={selectedCategory ?? ""}
              onchange={(event) => {
                const value = (event.target as HTMLSelectElement).value;
                selectedCategory = value || null;
              }}
            >
              <option value="">{t("plugin_allCategories")}</option>
              {#each claudeCategories as category}
                <option value={category}>{category}</option>
              {/each}
            </select>
            <div class="flex shrink-0 rounded-md border border-border p-0.5">
              <button
                type="button"
                class="rounded px-2 py-1 text-xs font-medium transition-colors {installScope ===
                'user'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => (installScope = "user")}>{t("plugin_scopeUser")}</button
              >
              <button
                type="button"
                class="rounded px-2 py-1 text-xs font-medium transition-colors {installScope ===
                'project'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'} {!projectCwd
                  ? 'cursor-not-allowed opacity-40'
                  : ''}"
                disabled={!projectCwd}
                onclick={() => (installScope = "project")}>{t("plugin_scopeProject")}</button
              >
            </div>
          </div>

          {#if filteredClaudePlugins.length === 0}
            <div
              class="flex flex-col items-center justify-center rounded-xl border border-dashed border-border py-14 text-center"
            >
              <div
                class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
              >
                <svg
                  class="h-6 w-6 text-muted-foreground"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M12 3v18M3 12h18" /></svg
                >
              </div>
              <h2 class="mb-1 text-sm font-medium text-foreground">
                {t("plugin_claudeNoAvailablePlugins")}
              </h2>
              <p class="max-w-sm text-xs text-muted-foreground">
                {claudeSearchQuery.trim() || selectedCategory
                  ? t("plugin_claudeNoPluginsMatch")
                  : t("plugin_claudeAddMarketplaceHint")}
              </p>
            </div>
          {:else}
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {#each filteredClaudePlugins as plugin}
                {@const pluginReference = claudePluginReference(plugin)}
                {@const isInstalled = installedPlugins.some(
                  (installed) => installedClaudePluginReference(installed) === pluginReference,
                )}
                <div
                  class="flex flex-col gap-3 rounded-xl border border-border/60 bg-muted/20 px-4 py-3.5 transition-colors hover:border-border hover:bg-muted/35"
                >
                  <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                      <h3 class="truncate text-sm font-medium text-foreground">{plugin.name}</h3>
                      <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
                        {plugin.marketplace_name ?? t("plugin_claudeOfficialMarketplace")}
                        {#if plugin.version}
                          · v{plugin.version}{/if}
                      </p>
                      {#if plugin.author}
                        <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
                          {plugin.author.name}
                        </p>
                      {/if}
                    </div>
                    <span
                      class="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium {isInstalled
                        ? 'bg-violet-500/10 text-violet-600 dark:text-violet-400'
                        : 'bg-muted text-muted-foreground'}"
                    >
                      {isInstalled ? t("plugin_installed") : t("plugin_available")}
                    </span>
                  </div>

                  <p class="min-h-9 line-clamp-2 text-xs leading-5 text-muted-foreground">
                    {plugin.description || t("plugin_claudeNoDescription")}
                  </p>

                  <div class="flex flex-wrap gap-1.5">
                    {#if plugin.category}
                      <span
                        class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {getCategoryColor(
                          plugin.category,
                        )}">{plugin.category}</span
                      >
                    {/if}
                    {#each plugin.tags as tag}
                      <span
                        class="rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                        >{tag}</span
                      >
                    {/each}
                  </div>

                  <button
                    type="button"
                    class="mt-auto min-h-9 rounded-md px-3 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 {isInstalled
                      ? 'border border-border text-muted-foreground'
                      : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
                    onclick={() => handleInstall(plugin)}
                    disabled={isInstalled || operationLoading === pluginReference}
                  >
                    {#if operationLoading === pluginReference}
                      {t("plugin_installing")}
                    {:else if isInstalled}
                      {t("plugin_installed")}
                    {:else}
                      {t("plugin_install")}
                    {/if}
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class:hidden={pluginsSource !== "marketplace"} class="space-y-4">
          <div class="rounded-xl border border-emerald-500/20 bg-emerald-500/5 p-4">
            <div class="flex items-start justify-between gap-3">
              <div>
                <h3 class="text-sm font-medium text-foreground">
                  {t("plugin_codexMarketplaceTitle")}
                </h3>
                <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
                  {t("plugin_codexMarketplaceDesc")}
                </p>
              </div>
              <button
                type="button"
                class="shrink-0 rounded-md border border-border px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                onclick={() => handleUpgradeCodexMarketplace()}
                disabled={operationLoading === "__codex_marketplace_upgrade"}
              >
                {operationLoading === "__codex_marketplace_upgrade"
                  ? t("plugin_updating")
                  : t("plugin_refresh")}
              </button>
            </div>

            <form
              class="mt-4 flex flex-col gap-2 sm:flex-row"
              onsubmit={(event) => {
                event.preventDefault();
                handleAddCodexMarketplace();
              }}
            >
              <input
                type="text"
                placeholder={t("plugin_codexMarketplacePlaceholder")}
                aria-label={t("plugin_codexMarketplacePlaceholder")}
                class="min-h-9 flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                bind:value={newCodexMarketplaceSource}
              />
              <button
                type="submit"
                class="min-h-9 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
                disabled={!newCodexMarketplaceSource.trim() ||
                  operationLoading === "__codex_marketplace_add"}
              >
                {operationLoading === "__codex_marketplace_add"
                  ? t("plugin_adding")
                  : t("plugin_add")}
              </button>
            </form>

            {#if codexMarketplaces.length === 0}
              <p class="mt-3 text-xs text-muted-foreground">{t("plugin_codexNoMarketplaces")}</p>
            {:else}
              <div class="mt-4 grid gap-2 md:grid-cols-2">
                {#each codexMarketplaces as marketplace}
                  <div
                    class="flex min-w-0 items-center justify-between gap-3 rounded-lg border border-border/60 bg-background/70 px-3 py-2.5"
                  >
                    <div class="min-w-0">
                      <p class="truncate text-xs font-medium text-foreground">{marketplace.name}</p>
                      {#if marketplace.root}
                        <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
                          {marketplace.root}
                        </p>
                      {/if}
                    </div>
                    <div class="flex shrink-0 items-center gap-1.5">
                      <button
                        type="button"
                        class="rounded-md border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                        onclick={() => handleUpgradeCodexMarketplace(marketplace.name)}
                        disabled={operationLoading === `__codex_mp_${marketplace.name}`}
                      >
                        {t("plugin_refresh")}
                      </button>
                      <button
                        type="button"
                        class="rounded-md border border-destructive/30 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                        onclick={() => handleRemoveCodexMarketplace(marketplace.name)}
                        disabled={operationLoading === `__codex_mp_${marketplace.name}`}
                      >
                        {t("plugin_remove")}
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <div class="relative">
            <svg
              class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" /></svg
            >
            <input
              type="search"
              placeholder={t("plugin_codexSearchPlaceholder")}
              aria-label={t("plugin_codexSearchPlaceholder")}
              class="w-full rounded-md border border-border bg-background py-2 pl-8 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              bind:value={codexSearchQuery}
            />
          </div>

          {#if filteredCodexPlugins.length === 0}
            <div
              class="flex flex-col items-center justify-center rounded-xl border border-dashed border-border py-14 text-center"
            >
              <div
                class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
              >
                <svg
                  class="h-6 w-6 text-muted-foreground"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M12 3v18M3 12h18" /></svg
                >
              </div>
              <h2 class="mb-1 text-sm font-medium text-foreground">
                {t("plugin_noCodexAvailablePlugins")}
              </h2>
              <p class="max-w-sm text-xs text-muted-foreground">
                {codexSearchQuery.trim()
                  ? t("plugin_noCodexPluginsMatch")
                  : t("plugin_codexAddMarketplaceHint")}
              </p>
            </div>
          {:else}
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {#each filteredCodexPlugins as plugin}
                {@const isInstalled =
                  plugin.installed ||
                  codexInstalledPlugins.some((installed) => installed.pluginId === plugin.pluginId)}
                <div
                  class="flex flex-col gap-3 rounded-xl border border-border/60 bg-muted/20 px-4 py-3.5 transition-colors hover:border-border hover:bg-muted/35"
                >
                  <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                      <h3 class="truncate text-sm font-medium text-foreground">{plugin.name}</h3>
                      <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
                        {plugin.marketplaceName ?? plugin.pluginId}
                        {#if plugin.version}
                          · v{plugin.version}{/if}
                      </p>
                    </div>
                    <span
                      class="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium {isInstalled
                        ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                        : 'bg-muted text-muted-foreground'}"
                    >
                      {isInstalled ? t("plugin_installed") : t("plugin_available")}
                    </span>
                  </div>

                  <p class="min-h-9 line-clamp-2 text-xs leading-5 text-muted-foreground">
                    {plugin.description || t("plugin_codexNoDescription")}
                  </p>

                  <div class="flex flex-wrap gap-1.5">
                    {#if plugin.authPolicy}
                      <span
                        class="rounded-full bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-600 dark:text-amber-400"
                      >
                        {plugin.authPolicy}
                      </span>
                    {/if}
                    {#if plugin.installPolicy}
                      <span
                        class="rounded-full bg-blue-500/10 px-1.5 py-0.5 text-[10px] text-blue-600 dark:text-blue-400"
                      >
                        {plugin.installPolicy}
                      </span>
                    {/if}
                  </div>

                  <button
                    type="button"
                    class="mt-auto min-h-9 rounded-md px-3 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-60 {isInstalled
                      ? 'border border-border text-muted-foreground'
                      : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
                    onclick={() => handleInstallCodexPlugin(plugin)}
                    disabled={isInstalled || operationLoading === `codex:add:${plugin.pluginId}`}
                  >
                    {#if operationLoading === `codex:add:${plugin.pluginId}`}
                      {t("plugin_installing")}
                    {:else if isInstalled}
                      {t("plugin_installed")}
                    {:else}
                      {t("plugin_install")}
                    {/if}
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- Installed sub-view -->
      <div class:hidden={pluginsSource !== "installed"}>
        {#if visibleInstalledPlugins.length === 0}
          <div class="flex flex-col items-center justify-center py-16 text-center">
            <div
              class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
            >
              <svg
                class="h-6 w-6 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path
                  d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"
                /></svg
              >
            </div>
            <h2 class="text-sm font-medium text-foreground mb-1">
              {pluginsAgent === "codex"
                ? t("plugin_noCodexPlugins")
                : t("plugin_noInstalledPlugins")}
            </h2>
            <p class="text-xs text-muted-foreground max-w-sm">
              {pluginsAgent === "codex"
                ? t("plugin_codexManagedHint")
                : t("plugin_installFromMarketplace")}
            </p>
          </div>
        {:else}
          <div class="space-y-2">
            {#each visibleInstalledPlugins as plugin}
              <div
                class="rounded-lg border border-border/50 bg-muted/30 px-4 py-3 flex items-center justify-between gap-4"
              >
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium text-foreground">{plugin.name}</span>
                    {#if plugin.version}
                      <span class="text-[11px] text-muted-foreground">v{plugin.version}</span>
                    {/if}
                    {#if plugin.scope}
                      <span
                        class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-muted text-muted-foreground"
                        >{plugin.scope}</span
                      >
                    {/if}
                  </div>
                  {#if plugin.description}
                    <p class="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                      {plugin.description}
                    </p>
                  {/if}
                  <!-- Component badges for installed plugins (Claude only) -->
                  {#if pluginsAgent === "claude" && plugins.find((p) => p.name === plugin.name)}
                    {@const mpMatch = plugins.find((p) => p.name === plugin.name)}
                    {#if mpMatch}
                      <div class="flex flex-wrap gap-1 mt-1">
                        {#each componentBadges as badge}
                          {#if hasComponent(mpMatch.components, badge.key)}
                            <span
                              class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {badge.color}"
                            >
                              {badge.label()}
                            </span>
                          {/if}
                        {/each}
                      </div>
                    {/if}
                  {/if}
                </div>
                <div class="flex items-center gap-2 shrink-0">
                  <!-- Enable/Disable toggle -->
                  <button
                    class="rounded-md border border-border px-2 py-1 text-xs {plugin.enabled !==
                    false
                      ? 'text-green-600 dark:text-green-400 border-green-500/30'
                      : 'text-muted-foreground'} hover:bg-muted transition-colors disabled:opacity-50"
                    onclick={() => handleToggleEnabled(plugin)}
                    disabled={operationLoading === pluginOpKey(plugin)}
                  >
                    {plugin.enabled !== false ? t("plugin_enabled") : t("plugin_disabled")}
                  </button>
                  {#if pluginsAgent === "claude"}
                    <!-- Explicitly opt this Claude plugin into Grok's session-scoped pluginDirs. -->
                    <button
                      class="rounded-md border border-border px-2 py-1 text-xs {isGrokPluginEnabled(
                        plugin,
                      )
                        ? 'text-orange-600 dark:text-orange-400 border-orange-500/30'
                        : 'text-muted-foreground'} hover:bg-muted transition-colors disabled:opacity-50"
                      onclick={() => handleToggleGrokPlugin(plugin)}
                      disabled={!grokPluginDir(plugin) ||
                        operationLoading === grokPluginOpKey(plugin)}
                      title={!grokPluginDir(plugin) ? t("plugin_grokNoPath") : undefined}
                    >
                      {isGrokPluginEnabled(plugin)
                        ? t("plugin_grokEnabled")
                        : t("plugin_grokEnable")}
                    </button>
                    <!-- Update button -->
                    <button
                      class="rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
                      onclick={() => handleUpdate(plugin)}
                      disabled={operationLoading === pluginOpKey(plugin)}
                      title="Update plugin"
                    >
                      {t("plugin_update")}
                    </button>
                    <!-- Uninstall button -->
                    <button
                      class="rounded-md border border-destructive/30 px-2 py-1 text-xs text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                      onclick={() => handleUninstall(plugin)}
                      disabled={operationLoading === pluginOpKey(plugin)}
                    >
                      {t("plugin_uninstall")}
                    </button>
                  {:else}
                    <!-- Codex uses the CLI's remove command; updates happen through marketplace refresh. -->
                    <button
                      class="rounded-md border border-destructive/30 px-2 py-1 text-xs text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                      onclick={() => handleUninstall(plugin)}
                      disabled={operationLoading === pluginOpKey(plugin)}
                    >
                      {t("plugin_uninstall")}
                    </button>
                  {/if}
                  <!-- Loading spinner -->
                  {#if operationLoading === pluginOpKey(plugin)}
                    <div
                      class="h-3.5 w-3.5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                    ></div>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Pi Extensions Section                                   -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-6" class:hidden={activeTab !== "pi-extensions" || pluginScope === "work"}>
      <!-- Header -->
      <div>
        <h2 class="text-sm font-semibold text-foreground">{t("piExt_title")}</h2>
        <p class="text-xs text-muted-foreground mt-0.5">{t("piExt_desc")}</p>
      </div>

      <PiSharedExtensionPanel
        mode="code"
        items={piInstalledPlugins}
        canInstall={pageMode === "catalog"}
        canManagePackage={pageMode === "catalog"}
        canToggle={pageMode === "config"}
        onInstall={installSharedPiExtension}
        onToggle={(item, enabled) => toggleSharedPiExtension("code", item, enabled)}
        onUpdate={updateSharedPiExtension}
        onUninstall={(item) => {
          handleUninstall(item);
          return Promise.resolve();
        }}
      />

      {#if pageMode === "catalog"}
        <div class="space-y-3">
          <div class="flex items-center justify-between gap-3">
            <h3 class="text-sm font-semibold text-foreground">{t("piExt_recommendedSection")}</h3>
            <span class="text-[11px] text-muted-foreground">Pi Code profile</span>
          </div>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            {#each RECOMMENDED_PI_EXTENSIONS as extension}
              {@const installed = getPiInstalledPluginBySource(extension.name)}
              <div class="flex flex-col gap-3 rounded-xl border border-border/60 bg-card p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <h4 class="truncate text-sm font-medium text-foreground">{extension.name}</h4>
                    <span
                      class="mt-1 inline-flex rounded-full px-1.5 py-0.5 text-[10px] {extension.badgeColor}"
                      >{extension.badge}</span
                    >
                  </div>
                  <span
                    class="shrink-0 rounded-full px-2 py-0.5 text-[10px] {installed
                      ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                      : 'bg-muted text-muted-foreground'}"
                    >{installed ? t("piExt_installed") : "未安装"}</span
                  >
                </div>
                <p class="min-h-10 text-xs leading-5 text-muted-foreground">
                  {piExtensionDescription(extension.descKey)}
                </p>
                <div class="mt-auto flex items-center justify-between gap-2">
                  <div class="flex flex-wrap gap-2">
                    {#each extension.links as link}
                      <a
                        href={link.url}
                        target="_blank"
                        rel="noreferrer"
                        class="text-[10px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                        >{link.label}</a
                      >
                    {/each}
                  </div>
                  <button
                    type="button"
                    class="rounded-md px-2.5 py-1.5 text-[11px] font-medium transition-colors disabled:opacity-50 {installed
                      ? 'border border-border text-muted-foreground'
                      : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
                    onclick={() => void handleInstallRecommendedPiExt(extension.pkgSource)}
                    disabled={Boolean(installed) || piRecInstalling === extension.pkgSource}
                  >
                    {piRecInstalling === extension.pkgSource
                      ? "安装中…"
                      : installed
                        ? t("piExt_installed")
                        : t("piExt_install")}
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="rounded-xl border border-border/80 bg-card p-4 space-y-3">
        <div>
          <p class="text-xs font-semibold text-foreground">Native Pi 原生能力（Code 模式）</p>
          <p class="mt-0.5 text-[11px] text-muted-foreground">
            这 8 项能力为 AgentCabin 基础扩展能力，Pi Code 启动时自动注入加载。其中代码智能 (LSP)
            默认禁用以保证极速启动，可前往 Pi Agent
            设置按需启用；其余核心能力系统内置常驻。请勿重复手动安装同名扩展以防冲突。
          </p>
        </div>
        <div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {#each PI_NATIVE_FEATURES as feature}
            <div
              class="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2"
            >
              <div class="min-w-0">
                <span class="block text-xs text-foreground">{feature.label}</span>
                <span class="block truncate text-[10px] text-muted-foreground"
                  >{feature.description}</span
                >
              </div>
              <span
                class="shrink-0 inline-flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400 select-none"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="h-3 w-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                系统内置
              </span>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Pi Code Connectors Section                              -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-6" class:hidden={activeTab !== "connectors" || pluginScope === "work"}>
      <div>
        <h2 class="text-sm font-semibold text-foreground">连接器</h2>
        <p class="text-xs text-muted-foreground mt-0.5">
          共享安装的连接器包；统一由能力中心全局开关控制，所有运行时共用
        </p>
      </div>

      <SharedConnectorPanel
        items={connectorCatalog}
        bindings={connectorBindings}
        canToggle={true}
        onToggle={handleToggleConnector}
      />
    </div>

    <!-- ═══════════════════════════════════════════════════════ -->
    <!-- Pi Code Web Access Section                              -->
    <!-- ═══════════════════════════════════════════════════════ -->
    <div class="space-y-6" class:hidden={activeTab !== "browser" || pluginScope === "work"}>
      <WebAccessPanel
        config={workBrowserConfig}
        enabled={webAccessEnabled}
        showSharedConfiguration={true}
        canToggle={true}
        onSave={saveWorkBrowser}
        onToggle={handleToggleWebAccess}
        onTest={testWorkBrowser}
      />
    </div>
  {/if}
</div>

<!-- Install Community Skill Modal -->
{#if installModalSkill}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
    <div class="w-full max-w-md rounded-xl border border-border bg-card p-5 shadow-xl space-y-4">
      <div class="flex items-center justify-between border-b border-border pb-3">
        <h3 class="text-sm font-semibold text-foreground">安装社区技能 / Install Skill</h3>
        <button
          class="text-muted-foreground hover:text-foreground"
          onclick={() => (installModalSkill = null)}
        >
          ✕
        </button>
      </div>

      <div class="space-y-3 text-xs">
        <div>
          <span class="font-medium text-foreground text-sm block">{installModalSkill.name}</span>
          <span class="text-muted-foreground font-mono text-[11px] block mt-0.5"
            >{installModalSkill.source}</span
          >
        </div>

        <div>
          <label class="block font-medium text-foreground mb-1">Scope (作用域)</label>
          <div class="flex gap-3">
            <label class="flex items-center gap-1.5 cursor-pointer">
              <input type="radio" value="user" bind:group={installModalScope} />
              <span>User (全局)</span>
            </label>
            {#if projectCwd}
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input type="radio" value="project" bind:group={installModalScope} />
                <span>Project (当前项目)</span>
              </label>
            {/if}
          </div>
        </div>

        <div>
          <label class="block font-medium text-foreground mb-1"
            >Target Agent / Standard (目标目录与规范)</label
          >
          <div
            class="grid {pluginScope === 'pi-code'
              ? 'grid-cols-2'
              : 'grid-cols-2 md:grid-cols-4'} gap-2"
          >
            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
              'universal'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (installModalTargetAgent = "universal")}
            >
              <span class="font-semibold text-xs">Universal (通用) ⭐</span>
              <span class="text-[10px] opacity-80 mt-0.5">.agents/skills</span>
            </button>

            {#if pluginScope === "pi-code"}
              <button
                class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
                'pi'
                  ? 'border-primary bg-primary/10 text-foreground font-medium'
                  : 'border-border text-muted-foreground hover:text-foreground'}"
                onclick={() => (installModalTargetAgent = "pi")}
              >
                <span class="font-semibold text-xs">Pi Only</span>
                <span class="text-[10px] opacity-80 mt-0.5">.pi/skills</span>
              </button>
            {:else}
              <button
                class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
                'claude'
                  ? 'border-primary bg-primary/10 text-foreground font-medium'
                  : 'border-border text-muted-foreground hover:text-foreground'}"
                onclick={() => (installModalTargetAgent = "claude")}
              >
                <span class="font-semibold text-xs">Claude Only</span>
                <span class="text-[10px] opacity-80 mt-0.5">.claude/skills</span>
              </button>

              <button
                class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
                'codex'
                  ? 'border-primary bg-primary/10 text-foreground font-medium'
                  : 'border-border text-muted-foreground hover:text-foreground'}"
                onclick={() => (installModalTargetAgent = "codex")}
              >
                <span class="font-semibold text-xs">Codex Only</span>
                <span class="text-[10px] opacity-80 mt-0.5">.codex/skills</span>
              </button>
              <button
                class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
                'grok'
                  ? 'border-primary bg-primary/10 text-foreground font-medium'
                  : 'border-border text-muted-foreground hover:text-foreground'}"
                onclick={() => (installModalTargetAgent = "grok")}
              >
                <span class="font-semibold text-xs">Grok Only</span>
                <span class="text-[10px] opacity-80 mt-0.5">.grok/skills</span>
              </button>
              <button
                class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {installModalTargetAgent ===
                'dsh'
                  ? 'border-primary bg-primary/10 text-foreground font-medium'
                  : 'border-border text-muted-foreground hover:text-foreground'}"
                onclick={() => (installModalTargetAgent = "dsh")}
              >
                <span class="font-semibold text-xs">DSH Only</span>
                <span class="text-[10px] opacity-80 mt-0.5">.dsh/skills</span>
              </button>
            {/if}
          </div>

          {#if installModalTargetAgent === "universal"}
            <div
              class="mt-2.5 rounded-lg border border-blue-500/30 bg-blue-500/10 p-2.5 text-[11px] text-blue-700 dark:text-blue-300 space-y-1"
            >
              <p class="font-medium flex items-center gap-1">
                <span>💡</span>
                <span>将自动为 Claude Code 建立软链接兼容</span>
              </p>
              <p class="opacity-90 leading-relaxed">
                技能将存储于通用源 <code class="font-mono rounded bg-blue-500/20 px-1 text-[10px]"
                  >.agents/skills</code
                >，并自动在
                <code class="font-mono rounded bg-blue-500/20 px-1 text-[10px]">.claude/skills</code
                > 下创建软链接。Claude Code 斜杠指令及所有 Agent 均可完美支持！
              </p>
            </div>
          {/if}
        </div>
      </div>

      <div class="flex justify-end gap-2 border-t border-border pt-3">
        <button
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
          onclick={() => (installModalSkill = null)}
        >
          取消
        </button>
        <button
          class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          disabled={operationLoading === installModalSkill.id}
          onclick={handleCommunityInstallExec}
        >
          {operationLoading === installModalSkill.id ? "正在安装..." : "确认安装"}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Import Skill ZIP Modal -->
{#if importZipModalOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
    <div class="w-full max-w-md rounded-xl border border-border bg-card p-5 shadow-xl space-y-4">
      <div class="flex items-center justify-between border-b border-border pb-3">
        <h3 class="text-sm font-semibold text-foreground">导入本地技能 (.zip) / Import Skill</h3>
        <button
          class="text-muted-foreground hover:text-foreground"
          onclick={() => (importZipModalOpen = false)}
        >
          ✕
        </button>
      </div>

      <div class="space-y-3 text-xs">
        <div>
          <label class="block font-medium text-foreground mb-1">ZIP 文件路径</label>
          <div
            class="truncate rounded-md border border-border bg-muted/40 px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground"
            title={importZipPath}
          >
            {importZipPath}
          </div>
        </div>

        <div>
          <label class="block font-medium text-foreground mb-1">技能标识 / 名称 (Skill Slug)</label>
          <input
            type="text"
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            placeholder="my-custom-skill"
            bind:value={importZipSlug}
          />
        </div>

        <div>
          <label class="block font-medium text-foreground mb-1">Scope (安装作用域)</label>
          <div class="flex gap-3">
            <label class="flex items-center gap-1.5 cursor-pointer">
              <input type="radio" value="user" bind:group={importZipScope} />
              <span>User (全局)</span>
            </label>
            {#if projectCwd}
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input type="radio" value="project" bind:group={importZipScope} />
                <span>Project (当前项目)</span>
              </label>
            {/if}
          </div>
        </div>

        <div>
          <label class="block font-medium text-foreground mb-1"
            >Target Agent / Standard (目标 Agent 与规范)</label
          >
          <div class="grid grid-cols-2 md:grid-cols-3 gap-2">
            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'universal'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "universal")}
            >
              <span class="font-semibold text-xs">Universal (通用) ⭐</span>
              <span class="text-[10px] opacity-80 mt-0.5">.agents/skills (所有 Agent 共享)</span>
            </button>

            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'claude'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "claude")}
            >
              <span class="font-semibold text-xs">Claude Only</span>
              <span class="text-[10px] opacity-80 mt-0.5">.claude/skills</span>
            </button>

            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'codex'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "codex")}
            >
              <span class="font-semibold text-xs">Codex Only</span>
              <span class="text-[10px] opacity-80 mt-0.5">.codex/skills</span>
            </button>

            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'grok'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "grok")}
            >
              <span class="font-semibold text-xs">Grok Only</span>
              <span class="text-[10px] opacity-80 mt-0.5">.grok/skills</span>
            </button>

            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'dsh'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "dsh")}
            >
              <span class="font-semibold text-xs">DSH Only</span>
              <span class="text-[10px] opacity-80 mt-0.5">.dsh/skills</span>
            </button>

            <button
              class="flex flex-col items-start p-2.5 rounded-lg border text-left transition-colors {importZipTargetAgent ===
              'pi'
                ? 'border-primary bg-primary/10 text-foreground font-medium'
                : 'border-border text-muted-foreground hover:text-foreground'}"
              onclick={() => (importZipTargetAgent = "pi")}
            >
              <span class="font-semibold text-xs">Pi Only</span>
              <span class="text-[10px] opacity-80 mt-0.5">.pi/skills</span>
            </button>
          </div>

          {#if importZipTargetAgent === "universal"}
            <div
              class="mt-2.5 rounded-lg border border-blue-500/30 bg-blue-500/10 p-2.5 text-[11px] text-blue-700 dark:text-blue-300 space-y-1"
            >
              <p class="font-medium flex items-center gap-1">
                <span>💡</span>
                <span>将自动为 Claude Code 建立软链接兼容</span>
              </p>
              <p class="opacity-90 leading-relaxed">
                技能将解压保存于通用源 <code
                  class="font-mono rounded bg-blue-500/20 px-1 text-[10px]">.agents/skills</code
                >，并自动在
                <code class="font-mono rounded bg-blue-500/20 px-1 text-[10px]">.claude/skills</code
                > 下创建软链接。Claude Code 斜杠指令及所有 Agent 均可完美支持！
              </p>
            </div>
          {/if}
        </div>
      </div>

      <div class="flex justify-end gap-2 border-t border-border pt-3">
        <button
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
          onclick={() => (importZipModalOpen = false)}
        >
          取消
        </button>
        <button
          class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          disabled={importZipLoading || !importZipSlug.trim()}
          onclick={handleExecImportZip}
        >
          {importZipLoading ? "正在导入解压..." : "确认导入"}
        </button>
      </div>
    </div>
  </div>
{/if}

<CapabilityDetailDrawer
  item={selectedCapabilityItem}
  open={Boolean(selectedCapabilityItem)}
  onClose={() => (selectedCapabilityItem = null)}
  onAction={handleCapabilityAction}
/>
