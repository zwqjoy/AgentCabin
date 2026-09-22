<script lang="ts">
  import SettingsSearch from "./SettingsSearch.svelte";

  export interface SidebarItem {
    id: string;
    label: string;
    icon: string;
    badge?: string;
  }

  export interface SidebarGroup {
    title: string;
    items: SidebarItem[];
  }

  interface Props {
    activeTab: string;
    onSelectTab: (tab: string, section?: string) => void;
    onBack: () => void;
    searchQuery?: string;
    onSearchChange?: (query: string) => void;
  }

  let {
    activeTab,
    onSelectTab,
    onBack,
    searchQuery = "",
    onSearchChange = () => {},
  }: Props = $props();

  const GROUPS: SidebarGroup[] = [
    {
      title: "个人",
      items: [
        { id: "general", label: "常规", icon: "general" },
        { id: "appearance", label: "外观", icon: "appearance" },
        { id: "keybindings", label: "键盘快捷键", icon: "keybindings" },
        { id: "usage", label: "使用情况和计费", icon: "usage" },
        { id: "pet", label: "桌面宠物", icon: "pet" },
      ],
    },
    {
      title: "能力中枢与模式",
      items: [
        { id: "capability-center", label: "能力中心", icon: "capability-center" },
        { id: "code", label: "Code 极客编程模式", icon: "code" },
        { id: "work", label: "Work 自主工作流模式", icon: "work" },
        { id: "runtimes", label: "Runtime 运行时", icon: "runtimes" },
        { id: "models", label: "模型与提供商", icon: "models" },
        { id: "doctor", label: "CLI 引擎检测", icon: "doctor" },
      ],
    },
    {
      title: "工具与运行时能力",
      items: [
        { id: "web-access", label: "网络访问", icon: "web-access" },
        { id: "browser-use", label: "浏览器自动化", icon: "browser-use" },
        { id: "desktop-use", label: "电脑控制", icon: "desktop-use" },
      ],
    },
  ];

  function itemMatchesSearch(item: SidebarItem, query: string): boolean {
    if (!query) return true;
    const q = query.trim().toLowerCase();
    if (item.label.toLowerCase().includes(q)) return true;
    if (item.id.toLowerCase().includes(q)) return true;
    return false;
  }
</script>

<aside
  class="flex h-full w-[250px] shrink-0 flex-col border-r border-border/60 bg-sidebar-background/65 backdrop-blur-md select-none"
>
  <!-- macOS window controls area clearance (traffic lights) -->
  <div class="h-9 shrink-0 pt-3 px-3"></div>

  <!-- Header: Back button & Search input -->
  <div class="px-3 pb-3 space-y-2.5">
    <button
      type="button"
      class="flex h-[34px] w-full items-center gap-2 rounded-lg px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground active:bg-accent"
      onclick={onBack}
      title="返回工作区"
    >
      <svg
        class="h-3.5 w-3.5 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M19 12H5M12 19l-7-7 7-7" />
      </svg>
      <span>返回应用</span>
    </button>

    <SettingsSearch
      {searchQuery}
      {onSearchChange}
      onSelectResult={(tab, section) => onSelectTab(tab, section)}
    />
  </div>

  <!-- Navigation items -->
  <div class="flex-1 overflow-y-auto px-3 py-1 space-y-4 scrollbar-thin">
    {#each GROUPS as group (group.title)}
      {@const visibleItems = group.items.filter((item) => itemMatchesSearch(item, searchQuery))}
      {#if visibleItems.length > 0}
        <div>
          <div class="px-2 pb-1 text-[11px] font-semibold text-muted-foreground/60 tracking-wider">
            {group.title}
          </div>
          <div class="space-y-0.5">
            {#each visibleItems as item (item.id)}
              {@const isActive = activeTab === item.id}
              <button
                type="button"
                class="flex h-[36px] w-full items-center justify-between gap-2.5 rounded-lg px-2.5 text-xs font-medium transition-all {isActive
                  ? 'bg-accent font-semibold text-accent-foreground shadow-2xs'
                  : 'text-muted-foreground hover:bg-accent/40 hover:text-foreground'}"
                onclick={() => onSelectTab(item.id)}
              >
                <div class="flex items-center gap-2.5 min-w-0">
                  <span
                    class="shrink-0 flex items-center justify-center text-muted-foreground {isActive
                      ? 'text-foreground'
                      : ''}"
                  >
                    {#if item.icon === "general"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><path
                          d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
                        /><circle cx="12" cy="12" r="3" /></svg
                      >
                    {:else if item.icon === "appearance"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><circle cx="12" cy="12" r="4" /><path
                          d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
                        /></svg
                      >
                    {:else if item.icon === "keybindings"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><rect width="20" height="16" x="2" y="4" rx="2" /><path
                          d="M6 8h.001M10 8h.001M14 8h.001M18 8h.001M8 12h.001M12 12h.001M16 12h.001M7 16h10"
                        /></svg
                      >
                    {:else if item.icon === "usage"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><path d="M3 3v18h18" /><path d="m19 9-5 5-4-4-3 3" /></svg
                      >
                    {:else if item.icon === "pet"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><path
                          d="M12 5c.67 0 1.35.09 2 .26 1.78-2 5.03-2.84 6.42-2.26 1.4.58 1.4 3.84-.2 6.06.52 1.07.78 2.29.78 3.54 0 5.52-4.03 8.4-9 8.4s-9-2.88-9-8.4c0-1.25.26-2.47.78-3.54-1.6-2.22-1.6-5.48-.2-6.06 1.39-.58 4.64.26 6.42 2.26.65-.17 1.33-.26 2-.26z"
                        /><path d="M9 13v.01M15 13v.01" /></svg
                      >
                    {:else if item.icon === "capability-center"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><path
                          d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"
                        /></svg
                      >
                    {:else if item.icon === "code"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><polyline points="16 18 22 12 16 6" /><polyline
                          points="8 6 2 12 8 18"
                        /></svg
                      >
                    {:else if item.icon === "work"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" /></svg
                      >
                    {:else if item.icon === "runtimes"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <rect width="16" height="16" x="4" y="4" rx="2" />
                        <rect width="6" height="6" x="9" y="9" rx="1" />
                        <path d="M15 2v2" />
                        <path d="M15 20v2" />
                        <path d="M2 15h2" />
                        <path d="M2 9h2" />
                        <path d="M20 15h2" />
                        <path d="M20 9h2" />
                        <path d="M9 2v2" />
                        <path d="M9 20v2" />
                      </svg>
                    {:else if item.icon === "models"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><circle cx="12" cy="12" r="10" /><line
                          x1="2"
                          y1="12"
                          x2="22"
                          y2="12"
                        /><path
                          d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
                        /></svg
                      >
                    {:else if item.icon === "doctor"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2" /></svg
                      >
                    {:else if item.icon === "web-access"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><circle cx="12" cy="12" r="10" /><path
                          d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"
                        /></svg
                      >
                    {:else if item.icon === "browser-use"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><rect width="18" height="18" x="3" y="3" rx="2" /><path
                          d="M3 9h18M9 21V9"
                        /></svg
                      >
                    {:else if item.icon === "desktop-use"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><rect width="20" height="14" x="2" y="3" rx="2" /><line
                          x1="8"
                          y1="21"
                          x2="16"
                          y2="21"
                        /><line x1="12" y1="17" x2="12" y2="21" /></svg
                      >
                    {:else if item.icon === "hooks"}
                      <svg
                        class="h-4 w-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><circle cx="12" cy="6" r="3" /><path d="M12 9v7a4 4 0 0 1-8 0" /></svg
                      >
                    {/if}
                  </span>
                  <span class="truncate">{item.label}</span>
                </div>
                {#if item.badge}
                  <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    {item.badge}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/each}
  </div>
</aside>
