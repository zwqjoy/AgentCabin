<script lang="ts">
  import type { UserSettings } from "$lib/types";
  import HarnessRuntimeProviderSection from "$lib/components/HarnessRuntimeProviderSection.svelte";
  import PiProfileConfigCard from "$lib/components/PiProfileConfigCard.svelte";
  import WorkGlobalRulesPanel from "$lib/components/work/WorkGlobalRulesPanel.svelte";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    settings: UserSettings;
    onSaveSettings: (patch: Partial<UserSettings>) => Promise<void>;
    onNavigate: (tab: string, section?: string) => void;
  }

  let { settings, onSaveSettings, onNavigate }: Props = $props();
</script>

<div class="space-y-8">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      Work 自主工作流模式
    </h2>
    <p class="mt-1 text-xs text-muted-foreground">
      专为自主长程任务打造的工作载体，支持 Pi Work 与 DSH Work
      智能调度、全局办公规则及工具能力投射。
    </p>
  </div>

  <!-- Harness Runtime Provider Section (Mode: Work) -->
  <HarnessRuntimeProviderSection mode="work" {settings} {onSaveSettings} {onNavigate} />

  <!-- Pi Work 专有 Profile 配置 -->
  <div class="space-y-4">
    <PiProfileConfigCard mode="work" />
  </div>

  <!-- Work 独立工作与办公规范 (AGENTS.md) -->
  <SettingsSection
    title="工作流规范与全局规则 (AGENTS.md)"
    description="定义自主工作流在分析、规划与执行阶段遵循的行为边界与安全守则"
  >
    <WorkGlobalRulesPanel />
  </SettingsSection>
</div>
