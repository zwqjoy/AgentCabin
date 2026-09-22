<script lang="ts">
  import Card from "./Card.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { SettingsScope } from "$lib/types";

  let { scope }: { scope: SettingsScope } = $props();

  interface CapabilityItem {
    title: string;
    description: string;
    href: string;
    icon: string;
    tag?: string;
  }

  let items = $derived.by<CapabilityItem[]>(() => {
    switch (scope) {
      case "native_codex":
        return [
          {
            title: t("sidebar_codexPlugins"),
            description: "Codex 原生扩展与集成插件",
            href: "/settings?tab=native-codex&view=plugins&scope=native&agent=codex&section=codex-plugins",
            icon: "package",
          },
        ];
      case "native_claude":
        return [
          {
            title: t("sidebar_claudePlugins"),
            description: "Claude Code 扩展插件与工具库",
            href: "/settings?tab=native-claude&view=plugins&scope=native&agent=claude&section=claude-plugins",
            icon: "package",
          },
        ];
      case "native_grok":
        return [];
      case "pi_common":
        return [];
      case "pi_code":
        return [
          {
            title: "Pi 扩展",
            description: "共享安装的 Pi 扩展与 Pi Code 运行时加载状态",
            href: "/settings?view=plugins&scope=pi-code&section=pi-extensions",
            icon: "pi-ext",
          },
          {
            title: "全局配置",
            description: "Pi Code 独立编程规范与 AGENTS.md",
            href: "/settings?view=plugins&scope=pi-code&section=rules",
            icon: "scroll",
          },
        ];
      case "pi_work":
        return [
          {
            title: "Runtime 能力",
            description: "共享安装的技能、MCP 与连接器，启用状态由能力中心全局管理",
            href: "/settings?view=plugins&scope=work&category=runtime&section=skills",
            icon: "package",
          },
          {
            title: "全局配置",
            description: "Work 独立办公与分析规范 (AGENTS.md)",
            href: "/settings?tab=pi-work",
            icon: "scroll",
          },
        ];
      default:
        return [];
    }
  });

  let helperText = $derived(
    scope === "native_codex" || scope === "native_claude"
      ? "点击在当前 Agent 菜单内管理扩展，返回按钮可回到 Agent 设置"
      : scope === "pi_code"
        ? "技能、MCP/连接器请在能力中心统一管理；Pi 扩展按运行时加载"
        : scope === "pi_work"
          ? "技能、MCP/连接器与 Runtime 能力请在能力中心统一管理"
          : "点击进入能力中心管理对应作用域的组件",
  );
</script>

{#if items.length > 0}
  <Card class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
        {t("settings_nav_integrations")} / 能力与扩展
      </h2>
      <span class="text-xs text-muted-foreground">{helperText}</span>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
      {#each items as item}
        <a
          href={item.href}
          class="flex flex-col justify-between p-3.5 rounded-xl border border-border/60 bg-muted/20 hover:bg-accent/50 hover:border-border transition-all duration-150 no-underline group"
        >
          <div class="space-y-1">
            <div class="flex items-center justify-between">
              <span
                class="text-xs font-semibold text-foreground group-hover:text-primary transition-colors"
              >
                {item.title}
              </span>
              <svg
                class="h-3.5 w-3.5 text-muted-foreground/50 group-hover:text-primary group-hover:translate-x-0.5 transition-all"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="9 18 15 12 9 6" />
              </svg>
            </div>
            <p class="text-[11px] text-muted-foreground line-clamp-2">
              {item.description}
            </p>
          </div>
        </a>
      {/each}
    </div>
  </Card>
{/if}
