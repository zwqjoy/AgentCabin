<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import {
    listAgentPlugins,
    listPromptTemplates,
    listSkills,
    setAgentPluginBinding,
  } from "$lib/api";
  import {
    listWorkConnectorPackages as listWorkConnectorPackagesApi,
    enableWorkConnectorPackage as enableWorkConnectorPackageApi,
  } from "$lib/api/work";
  import type { AgentPluginSummary, PromptTemplate, StandaloneSkill } from "$lib/types";
  import type { ConnectorPackageSummary } from "$lib/types/work";

  export interface SelectedExpert {
    id: string;
    name: string;
    title: string;
    icon: string;
    isTeam?: boolean;
    description?: string;
    leadAgent?: string;
  }

  interface Props {
    open?: boolean;
    disabled?: boolean;
    planAvailable?: boolean;
    planActive?: boolean;
    goalAvailable?: boolean;
    goalActive?: boolean;
    selectedExpert?: SelectedExpert | null;
    onSelectExpert?: (expert: SelectedExpert | null) => void;
    onSelectSkill?: (skillName: string) => void;
    onSelectPromptTemplate?: (template: PromptTemplate) => void;
    onPlanToggle?: () => void;
    onGoalToggle?: () => void;
    onTriggerUpload?: () => void;
    onInsertAtMention?: () => void;
    onCaptureScreenshot?: () => void;
    onDismiss?: () => void;
  }

  let {
    open = $bindable(false),
    disabled = false,
    planAvailable = true,
    planActive = false,
    goalAvailable = false,
    goalActive = false,
    selectedExpert = null,
    onSelectExpert,
    onSelectSkill,
    onSelectPromptTemplate,
    onPlanToggle,
    onGoalToggle,
    onTriggerUpload,
    onInsertAtMention,
    onCaptureScreenshot,
    onDismiss,
  }: Props = $props();

  type PrimaryCategory = "prompts" | "files" | "modes" | "experts" | "skills" | "connectors";

  interface PrimaryItem {
    id: PrimaryCategory;
    label: string;
    /** Single-path SVG glyph. */
    path?: string;
    emoji?: string;
  }

  const PRIMARY_ITEMS: PrimaryItem[] = [
    {
      id: "prompts",
      label: "提示词",
      path: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M8 13h8M8 17h6",
    },
    {
      id: "files",
      label: "添加文件",
      path: "m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48",
    },
    { id: "modes", label: "模式", emoji: "🌱" },
    {
      id: "experts",
      label: "专家",
      path: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
    },
    {
      id: "skills",
      label: "技能",
      path: "m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3z",
    },
    {
      id: "connectors",
      label: "连接器",
      path: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
    },
  ];

  // Primary card geometry: 6px padding, 36px item, 2px gap, 6px padding.
  const PRIMARY_ITEM_TOP = 6;
  const PRIMARY_ITEM_STEP = 38;
  const PRIMARY_CARD_HEIGHT = 10 + PRIMARY_ITEM_STEP * PRIMARY_ITEMS.length;

  let activeCategory = $state<PrimaryCategory>("experts");
  let searchQuery = $state("");
  let buttonEl: HTMLButtonElement | undefined = $state();
  let menuContainerEl: HTMLDivElement | undefined = $state();

  let plugins = $state<AgentPluginSummary[]>([]);
  let connectors = $state<ConnectorPackageSummary[]>([]);
  let standaloneSkills = $state<StandaloneSkill[]>([]);
  let promptTemplates = $state<PromptTemplate[]>([]);
  let promptTemplatesLoading = $state(false);
  let loadingData = $state(false);

  // Position coordinates
  let basePos = $state<{ left: number; top: number }>({ left: 20, top: 200 });

  function updatePosition() {
    if (!buttonEl) return;
    const rect = buttonEl.getBoundingClientRect();
    // Primary card is positioned 6px above the plus button
    const top = Math.max(12, rect.top - 6 - PRIMARY_CARD_HEIGHT);
    const left = Math.max(12, rect.left);
    basePos = { left, top };
  }

  async function loadCapabilitiesData() {
    loadingData = true;
    promptTemplatesLoading = true;
    try {
      const [pluginsRes, connectorsRes, skillsRes, templatesRes] = await Promise.all([
        listAgentPlugins().catch(() => []),
        listWorkConnectorPackagesApi().catch(() => []),
        listSkills().catch(() => []),
        listPromptTemplates().catch(() => []),
      ]);
      plugins = pluginsRes;
      connectors = connectorsRes;
      standaloneSkills = skillsRes;
      promptTemplates = templatesRes;
    } catch {
      // Keep empty lists gracefully
    } finally {
      loadingData = false;
      promptTemplatesLoading = false;
    }
  }

  $effect(() => {
    if (open) {
      updatePosition();
      void loadCapabilitiesData();
    }
  });

  function avatarGlyph(name: string): string {
    const trimmed = (name || "").trim();
    return trimmed === "" ? "?" : [...trimmed][0]!;
  }

  function avatarTone(name: string): number {
    let hash = 0;
    for (let i = 0; i < (name || "").length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
    return hash % 8;
  }

  const TONE_COLORS = [
    "#2563eb",
    "#0d9488",
    "#d97706",
    "#dc2626",
    "#7c3aed",
    "#db2777",
    "#059669",
    "#4f46e5",
  ];

  const CONNECTOR_ICONS: Record<string, string> = {
    飞书: "🕊️",
    feishu: "🕊️",
    腾讯文档: "📄",
    通达信: "📈",
    QQ邮箱: "✉️",
    Notion: "📓",
    notion: "📓",
    GitHub: "🐙",
    github: "🐙",
    Slack: "💬",
    slack: "💬",
    Gmail: "📮",
    gmail: "📮",
  };

  // 专家过滤与展示列表
  let displayedExperts = $derived.by(() => {
    const list = plugins
      .filter((p) => p.packageFormat === "workbuddy")
      .map((p) => ({
        id: p.id,
        name: p.name,
        title: p.displayName || p.name,
        isTeam: p.expertKind === "expert-team",
        description: p.description,
        icon: p.expertKind === "expert-team" ? "👥" : "👤",
        skills: p.skills,
      }));

    const q = searchQuery.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (item) =>
        item.title.toLowerCase().includes(q) ||
        item.name.toLowerCase().includes(q) ||
        (item.description || "").toLowerCase().includes(q),
    );
  });

  // 技能过滤与展示列表
  let displayedSkills = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    // The public catalog owns visibility; runtime commands include package helpers.
    const genericSkills = standaloneSkills.filter((item) => item.enabled);
    if (!q) return genericSkills;
    return genericSkills.filter(
      (item) =>
        item.name.toLowerCase().includes(q) || (item.description || "").toLowerCase().includes(q),
    );
  });

  // 连接器过滤与展示列表
  let displayedConnectors = $derived.by(() => {
    const list = connectors.map((c) => ({
      id: c.manifest.id,
      name: c.manifest.displayName || c.manifest.id,
      description: c.manifest.description,
      enabled: c.state.enabled,
      trusted: c.state.trusted,
      authStatus: c.state.authStatus,
      icon: CONNECTOR_ICONS[c.manifest.id] || CONNECTOR_ICONS[c.manifest.displayName] || "🔗",
    }));

    const q = searchQuery.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (item) =>
        item.name.toLowerCase().includes(q) || (item.description || "").toLowerCase().includes(q),
    );
  });

  // 提示词模板过滤与展示列表
  let displayedPromptTemplates = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return promptTemplates;
    return promptTemplates.filter(
      (item) =>
        item.name.toLowerCase().includes(q) ||
        (item.description || "").toLowerCase().includes(q) ||
        item.content.toLowerCase().includes(q),
    );
  });

  function handleSelectExpert(exp: any) {
    if (selectedExpert?.id === exp.id) {
      onSelectExpert?.(null);
    } else {
      // 开启对应 plugin binding
      void setAgentPluginBinding(exp.id, true).catch(() => {});
      onSelectExpert?.({
        id: exp.id,
        name: exp.name,
        title: exp.title,
        icon: exp.icon,
        isTeam: exp.isTeam,
        description: exp.description,
      });
    }
    onDismiss?.();
  }

  function handleSelectSkill(skillName: string) {
    onSelectSkill?.(skillName);
    onDismiss?.();
  }

  function handleSelectPromptTemplate(template: PromptTemplate) {
    onSelectPromptTemplate?.(template);
    onDismiss?.();
  }

  async function handleToggleConnector(connectorId: string, currentEnabled: boolean) {
    try {
      await enableWorkConnectorPackageApi(connectorId, !currentEnabled);
      connectors = connectors.map((c) =>
        c.manifest.id === connectorId
          ? { ...c, state: { ...c.state, enabled: !currentEnabled } }
          : c,
      );
    } catch {
      // Error handled
    }
  }

  function navigateToCapabilityCenter(section: string) {
    onDismiss?.();
    goto(`/settings?tab=capability-center&section=${encodeURIComponent(section)}`);
  }

  onMount(() => {
    function onPointerDown(e: PointerEvent) {
      if (!open) return;
      const target = e.target as HTMLElement;
      if (
        menuContainerEl &&
        !menuContainerEl.contains(target) &&
        buttonEl &&
        !buttonEl.contains(target)
      ) {
        onDismiss?.();
      }
    }

    function onKeyDown(e: KeyboardEvent) {
      if (!open) return;
      if (e.key === "Escape") {
        e.preventDefault();
        onDismiss?.();
      }
    }

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", updatePosition);

    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", updatePosition);
    };
  });

  // Calculate secondary card style:
  // - files & modes: top aligned with category item offset
  // - prompts, experts, skills, connectors: bottom aligned with primary card bottom
  let secondaryStyle = $derived.by(() => {
    const left = basePos.left + 144 + 8;
    const isTopAligned = activeCategory === "files" || activeCategory === "modes";
    const index = Math.max(
      0,
      PRIMARY_ITEMS.findIndex((item) => item.id === activeCategory),
    );
    const offset = PRIMARY_ITEM_TOP + index * PRIMARY_ITEM_STEP;
    const viewportH = typeof window !== "undefined" ? window.innerHeight : 800;

    if (isTopAligned) {
      const top = Math.max(8, basePos.top + offset);
      const maxH = Math.min(360, Math.max(120, viewportH - top - 16));
      return `left: ${left}px; top: ${top}px; max-height: ${maxH}px;`;
    } else {
      const primaryBottom = basePos.top + PRIMARY_CARD_HEIGHT;
      const bottomFromViewport = Math.max(8, viewportH - primaryBottom);
      const maxH = Math.min(420, Math.max(260, primaryBottom - 16));
      return `left: ${left}px; bottom: ${bottomFromViewport}px; max-height: ${maxH}px;`;
    }
  });
</script>

<!-- Plus trigger button inside input action bar -->
<div class="relative shrink-0">
  <button
    bind:this={buttonEl}
    type="button"
    class="flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-all duration-150
      {open
      ? 'bg-accent text-foreground rotate-45'
      : 'hover:bg-accent hover:text-foreground rotate-0'}
      disabled:cursor-not-allowed disabled:opacity-40"
    onclick={() => {
      if (disabled) return;
      if (open) onDismiss?.();
      else {
        updatePosition();
        open = true;
      }
    }}
    {disabled}
    aria-label="打开能力与模式菜单"
    aria-haspopup="menu"
    aria-expanded={open}
    title="插入提示词、添加文件、切换模式、选择专家、技能与连接器"
  >
    <svg
      class="h-4 w-4"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      aria-hidden="true"
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  </button>
</div>

<!-- Cascading Menu Popover (Fixed Overlay) -->
{#if open}
  <div
    bind:this={menuContainerEl}
    class="fixed inset-0 z-50 pointer-events-none text-popover-foreground text-xs select-none"
  >
    <!-- Primary Menu -->
    <div
      class="pointer-events-auto fixed flex flex-col gap-0.5 rounded-2xl border border-border/80 bg-popover/95 p-1.5 shadow-2xl backdrop-blur-md transition-all animate-in fade-in zoom-in-95 duration-100"
      style="left: {basePos.left}px; top: {basePos.top}px; width: 144px; height: {PRIMARY_CARD_HEIGHT}px;"
      role="menu"
    >
      {#each PRIMARY_ITEMS as item (item.id)}
        <button
          type="button"
          class="flex w-full items-center justify-between rounded-xl px-2.5 py-1.5 text-left font-medium transition-colors {activeCategory ===
          item.id
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'}"
          onmouseenter={() => {
            activeCategory = item.id;
            searchQuery = "";
          }}
          onclick={() => {
            activeCategory = item.id;
            searchQuery = "";
          }}
        >
          <div class="flex items-center gap-2 truncate">
            {#if item.emoji}
              <span class="text-sm">{item.emoji}</span>
            {:else}
              <svg
                class="h-3.5 w-3.5 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d={item.path} />
              </svg>
            {/if}
            <span class="truncate">{item.label}</span>
          </div>
          <span class="text-[11px] text-muted-foreground/80">›</span>
        </button>
      {/each}
    </div>

    <!-- Secondary Submenu Card (Width 280px, Dynamic Height & Smart Alignment) -->
    <div
      class="pointer-events-auto fixed flex w-[280px] flex-col rounded-2xl border border-border/80 bg-popover/95 shadow-2xl backdrop-blur-md overflow-hidden transition-all animate-in fade-in duration-120"
      style={secondaryStyle}
    >
      <!-- Prompts Submenu -->
      {#if activeCategory === "prompts"}
        <div class="p-2 border-b border-border/50">
          <div class="relative flex items-center">
            <svg
              class="absolute left-2.5 h-3.5 w-3.5 text-muted-foreground pointer-events-none"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="text"
              class="w-full rounded-lg bg-muted/60 py-1.5 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground outline-none focus:bg-background focus:ring-1 focus:ring-ring"
              placeholder="搜索提示词模板"
              bind:value={searchQuery}
            />
          </div>
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto p-1.5 space-y-1">
          {#each displayedPromptTemplates as template (template.id)}
            <button
              type="button"
              class="flex w-full items-start gap-2.5 rounded-xl px-2.5 py-2 text-left hover:bg-accent/60 transition-colors"
              onclick={() => handleSelectPromptTemplate(template)}
            >
              <span
                class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded bg-muted/70 text-muted-foreground"
              >
                <svg
                  class="h-3 w-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <path d="M14 2v6h6M8 13h8M8 17h6" />
                </svg>
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  <span class="truncate font-mono font-medium text-foreground"
                    >/{template.name}</span
                  >
                  {#if template.builtin}
                    <span
                      class="shrink-0 rounded bg-purple-500/10 px-1 py-0.2 text-[9px] font-medium text-purple-600 dark:text-purple-400"
                      >内置</span
                    >
                  {/if}
                </div>
                {#if template.description}
                  <p class="truncate text-[10px] text-muted-foreground mt-0.5">
                    {template.description}
                  </p>
                {:else}
                  <p class="truncate font-mono text-[10px] text-muted-foreground mt-0.5">
                    {template.content.slice(0, 60)}
                  </p>
                {/if}
              </div>
            </button>
          {/each}

          {#if displayedPromptTemplates.length === 0}
            <div class="py-6 text-center text-xs text-muted-foreground">
              {promptTemplatesLoading ? "正在加载提示词…" : "未找到匹配的提示词"}
            </div>
          {/if}
        </div>

        <div class="border-t border-border/50 p-1.5 bg-muted/20">
          <button
            type="button"
            class="flex w-full items-center justify-center gap-1.5 rounded-lg py-1.5 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
            onclick={() => navigateToCapabilityCenter("prompts")}
          >
            <span>↗</span>
            <span>管理提示词模板</span>
          </button>
        </div>
      {/if}

      <!-- Experts Submenu -->
      {#if activeCategory === "experts"}
        <div class="p-2 border-b border-border/50">
          <div class="relative flex items-center">
            <svg
              class="absolute left-2.5 h-3.5 w-3.5 text-muted-foreground pointer-events-none"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="text"
              class="w-full rounded-lg bg-muted/60 py-1.5 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground outline-none focus:bg-background focus:ring-1 focus:ring-ring"
              placeholder="搜索专家与专家团"
              bind:value={searchQuery}
            />
          </div>
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto p-1.5 space-y-1">
          {#each displayedExperts as exp (exp.id)}
            {@const tone = avatarTone(exp.id || exp.title)}
            {@const isSelected = selectedExpert?.id === exp.id}
            <button
              type="button"
              class="flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left transition-colors {isSelected
                ? 'bg-accent/80 font-medium'
                : 'hover:bg-accent/60'}"
              onclick={() => handleSelectExpert(exp)}
            >
              <span
                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold text-white shadow-xs"
                style="background-color: {TONE_COLORS[tone]}"
              >
                {exp.icon && exp.icon !== "👤" && exp.icon !== "👥"
                  ? exp.icon
                  : avatarGlyph(exp.title)}
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  <span class="truncate font-medium text-foreground">{exp.title}</span>
                  {#if exp.isTeam}
                    <span
                      class="rounded bg-primary/10 px-1 py-0.2 text-[9px] font-medium text-primary"
                      >Team</span
                    >
                  {/if}
                </div>
                {#if exp.description}
                  <p class="truncate text-[10px] text-muted-foreground mt-0.5">{exp.description}</p>
                {/if}
              </div>
              {#if isSelected}
                <svg
                  class="h-3.5 w-3.5 text-primary shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.5"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              {/if}
            </button>
          {/each}

          {#if displayedExperts.length === 0}
            <div class="py-6 text-center text-xs text-muted-foreground">
              {loadingData ? "正在加载专家..." : "未找到匹配的专家"}
            </div>
          {/if}
        </div>

        <div class="border-t border-border/50 p-1.5 bg-muted/20">
          <button
            type="button"
            class="flex w-full items-center justify-center gap-1.5 rounded-lg py-1.5 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
            onclick={() => navigateToCapabilityCenter("experts")}
          >
            <span>↗</span>
            <span>召唤更多专家</span>
          </button>
        </div>
      {/if}

      <!-- Skills Submenu -->
      {#if activeCategory === "skills"}
        <div class="p-2 border-b border-border/50">
          <div class="relative flex items-center">
            <svg
              class="absolute left-2.5 h-3.5 w-3.5 text-muted-foreground pointer-events-none"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="text"
              class="w-full rounded-lg bg-muted/60 py-1.5 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground outline-none focus:bg-background focus:ring-1 focus:ring-ring"
              placeholder="搜索技能"
              bind:value={searchQuery}
            />
          </div>
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto p-1.5 space-y-1">
          {#each displayedSkills as sk (sk.name)}
            {@const tone = avatarTone(sk.name)}
            <button
              type="button"
              class="flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left hover:bg-accent/60 transition-colors"
              onclick={() => handleSelectSkill(sk.name)}
            >
              <span
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded text-[10px] font-bold text-white shadow-xs"
                style="background-color: {TONE_COLORS[tone]}"
              >
                {avatarGlyph(sk.name).toUpperCase()}
              </span>
              <div class="min-w-0 flex-1">
                <span class="block truncate font-medium text-foreground">{sk.name}</span>
                <span class="block truncate text-[10px] text-muted-foreground mt-0.5"
                  >{sk.description || "工作台自动化技能"}</span
                >
              </div>
            </button>
          {/each}

          {#if displayedSkills.length === 0}
            <div class="py-6 text-center text-xs text-muted-foreground">未找到相关技能</div>
          {/if}
        </div>

        <div
          class="border-t border-border/50 p-1.5 bg-muted/20 flex items-center justify-between gap-1"
        >
          <button
            type="button"
            class="flex-1 flex items-center justify-center gap-1 rounded-lg py-1.5 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
            onclick={() => navigateToCapabilityCenter("skills")}
          >
            <span>↑</span>
            <span>从本地添加</span>
          </button>
          <button
            type="button"
            class="flex-1 flex items-center justify-center gap-1 rounded-lg py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent transition-colors"
            onclick={() => navigateToCapabilityCenter("skills")}
          >
            <span>⚙️</span>
            <span>管理技能</span>
          </button>
        </div>
      {/if}

      <!-- Connectors Submenu -->
      {#if activeCategory === "connectors"}
        <div class="p-2 border-b border-border/50">
          <div class="relative flex items-center">
            <svg
              class="absolute left-2.5 h-3.5 w-3.5 text-muted-foreground pointer-events-none"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="text"
              class="w-full rounded-lg bg-muted/60 py-1.5 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground outline-none focus:bg-background focus:ring-1 focus:ring-ring"
              placeholder="搜索连接器"
              bind:value={searchQuery}
            />
          </div>
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto p-1.5 space-y-1">
          {#each displayedConnectors as conn (conn.id)}
            <div
              class="flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-2 text-left hover:bg-accent/40 transition-colors"
            >
              <div class="flex items-center gap-2.5 min-w-0 flex-1">
                <span class="text-base shrink-0">{conn.icon}</span>
                <div class="min-w-0 flex-1">
                  <span class="block truncate font-medium text-foreground">{conn.name}</span>
                  {#if conn.description}
                    <span class="block truncate text-[10px] text-muted-foreground mt-0.5"
                      >{conn.description}</span
                    >
                  {/if}
                </div>
              </div>

              <!-- Switch / Status -->
              <button
                type="button"
                class="shrink-0 relative inline-flex !h-4 !w-7 !min-h-0 cursor-pointer !rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none {conn.enabled
                  ? '!bg-emerald-500'
                  : '!bg-muted'}"
                role="switch"
                aria-checked={conn.enabled}
                onclick={() => handleToggleConnector(conn.id, conn.enabled)}
              >
                <span
                  class="pointer-events-none inline-block !h-3 !w-3 transform !rounded-full bg-white shadow-sm ring-0 transition duration-200 ease-in-out {conn.enabled
                    ? 'translate-x-3'
                    : 'translate-x-0'}"
                ></span>
              </button>
            </div>
          {/each}

          {#if displayedConnectors.length === 0}
            <div class="py-6 text-center text-xs text-muted-foreground">
              {loadingData ? "正在加载连接器..." : "未导入连接器"}
            </div>
          {/if}
        </div>

        <div class="border-t border-border/50 p-1.5 bg-muted/20">
          <button
            type="button"
            class="flex w-full items-center justify-center gap-1.5 rounded-lg py-1.5 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
            onclick={() => navigateToCapabilityCenter("connectors")}
          >
            <span>↗</span>
            <span>管理连接器</span>
          </button>
        </div>
      {/if}

      <!-- Files Submenu -->
      {#if activeCategory === "files"}
        <div class="p-2 space-y-1.5">
          <button
            type="button"
            class="flex w-full items-center gap-3 rounded-xl p-2.5 text-left hover:bg-accent/70 transition-colors"
            onclick={() => {
              onDismiss?.();
              onTriggerUpload?.();
            }}
          >
            <span class="text-lg">📁</span>
            <div class="min-w-0 flex-1">
              <span class="block font-medium text-foreground">上传本地文件</span>
              <span class="block text-[10px] text-muted-foreground mt-0.5"
                >支持图片、文档、表格、代码或压缩包</span
              >
            </div>
          </button>

          <button
            type="button"
            class="flex w-full items-center gap-3 rounded-xl p-2.5 text-left hover:bg-accent/70 transition-colors"
            onclick={() => {
              onDismiss?.();
              onInsertAtMention?.();
            }}
          >
            <span class="text-lg">📄</span>
            <div class="min-w-0 flex-1">
              <span class="block font-medium text-foreground">引用工作区文件</span>
              <span class="block text-[10px] text-muted-foreground mt-0.5"
                >键入 @ 快速检索当前项目代码与文档</span
              >
            </div>
          </button>

          {#if onCaptureScreenshot}
            <button
              type="button"
              class="flex w-full items-center gap-3 rounded-xl p-2.5 text-left hover:bg-accent/70 transition-colors"
              onclick={() => {
                onDismiss?.();
                onCaptureScreenshot?.();
              }}
            >
              <span class="text-lg">📸</span>
              <div class="min-w-0 flex-1">
                <span class="block font-medium text-foreground">截取屏幕</span>
                <span class="block text-[10px] text-muted-foreground mt-0.5"
                  >截取屏幕窗口并附加为图片</span
                >
              </div>
            </button>
          {/if}
        </div>
      {/if}

      <!-- Modes Submenu -->
      {#if activeCategory === "modes"}
        <div class="p-2 space-y-1.5">
          <!-- Standard Mode -->
          <button
            type="button"
            class="flex w-full items-center justify-between rounded-xl p-2.5 text-left transition-colors {!planActive &&
            !goalActive
              ? 'bg-accent/80 font-medium'
              : 'hover:bg-accent/60'}"
            onclick={() => {
              if (planActive) onPlanToggle?.();
              onDismiss?.();
            }}
          >
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <span class="text-lg">⚡</span>
              <div class="min-w-0 flex-1">
                <span class="block font-medium text-foreground">标准对话模式</span>
                <span class="block text-[10px] text-muted-foreground mt-0.5"
                  >自然语言直接对话与快速执行任务</span
                >
              </div>
            </div>
            {#if !planActive && !goalActive}
              <svg
                class="h-3.5 w-3.5 text-emerald-500 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
              >
                <path d="M20 6 9 17l-5-5" />
              </svg>
            {/if}
          </button>

          <!-- Plan Mode -->
          {#if planAvailable}
            <button
              type="button"
              class="flex w-full items-center justify-between rounded-xl p-2.5 text-left transition-colors {planActive
                ? 'bg-accent/80 font-medium'
                : 'hover:bg-accent/60'}"
              onclick={() => {
                onPlanToggle?.();
                onDismiss?.();
              }}
            >
              <div class="flex items-center gap-3 min-w-0 flex-1">
                <span class="text-lg">📝</span>
                <div class="min-w-0 flex-1">
                  <span class="block font-medium text-foreground">计划模式 (Plan Mode)</span>
                  <span class="block text-[10px] text-muted-foreground mt-0.5"
                    >先输出多步架构计划，确认后才执行修改</span
                  >
                </div>
              </div>
              {#if planActive}
                <svg
                  class="h-3.5 w-3.5 text-purple-500 shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.5"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              {/if}
            </button>
          {/if}

          <!-- Goal Mode (Optional if available) -->
          {#if goalAvailable}
            <button
              type="button"
              class="flex w-full items-center justify-between rounded-xl p-2.5 text-left transition-colors {goalActive
                ? 'bg-accent/80 font-medium'
                : 'hover:bg-accent/60'}"
              onclick={() => {
                onGoalToggle?.();
                onDismiss?.();
              }}
            >
              <div class="flex items-center gap-3 min-w-0 flex-1">
                <span class="text-lg">🎯</span>
                <div class="min-w-0 flex-1">
                  <span class="block font-medium text-foreground">目标模式 (Goal Mode)</span>
                  <span class="block text-[10px] text-muted-foreground mt-0.5"
                    >持续推进长周期复合任务直到目标达成</span
                  >
                </div>
              </div>
              {#if goalActive}
                <svg
                  class="h-3.5 w-3.5 text-emerald-500 shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.5"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              {/if}
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}
