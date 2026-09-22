<script lang="ts">
  import {
    getAppearance,
    setAppearance,
    BUILTIN_THEMES,
    UI_FONT_SIZE_LIMITS,
    CODE_FONT_SIZE_LIMITS,
    importThemeJson,
    exportThemeJson,
    type ThemeMode,
    type ColorScheme,
    type ReduceMotion,
    type DiffMarkerStyle,
  } from "$lib/stores/appearance.svelte";
  import SettingsCard from "../SettingsCard.svelte";
  import SettingsRow from "../SettingsRow.svelte";
  import SettingsToggleRow from "../SettingsToggleRow.svelte";
  import SettingsSelectRow from "../SettingsSelectRow.svelte";
  import SettingsSection from "../SettingsSection.svelte";
  import ColorInput from "$lib/components/appearance/ColorInput.svelte";

  // Reactive appearance state
  let appearance = $state(getAppearance());

  function update(patch: Parameters<typeof setAppearance>[0]) {
    setAppearance(patch);
    appearance = getAppearance();
  }

  // Theme Import/Export feedback
  let themeFeedback = $state<string | null>(null);
  let isFeedbackError = $state(false);

  function handleExport() {
    try {
      const json = exportThemeJson();
      navigator.clipboard.writeText(json);
      themeFeedback = "主题配置已复制到剪贴板！";
      isFeedbackError = false;
      setTimeout(() => (themeFeedback = null), 2000);
    } catch (e) {
      themeFeedback = "导出失败: " + String(e);
      isFeedbackError = true;
    }
  }

  function handleImport() {
    const raw = prompt("请粘贴主题 JSON 配置：");
    if (!raw) return;
    try {
      const err = importThemeJson(raw);
      if (!err) {
        appearance = getAppearance();
        themeFeedback = "主题配置导入成功！";
        isFeedbackError = false;
      } else {
        themeFeedback = "导入失败: " + err;
        isFeedbackError = true;
      }
      setTimeout(() => (themeFeedback = null), 2500);
    } catch (e) {
      themeFeedback = "导入失败: " + String(e);
      isFeedbackError = true;
    }
  }

  const ACCENT_SWATCHES = [
    { label: "绿色", value: "#20b982" },
    { label: "蓝色", value: "#3b82f6" },
    { label: "紫色", value: "#8b5cf6" },
    { label: "橙色", value: "#f97316" },
    { label: "粉红", value: "#ec4899" },
    { label: "灰色", value: "#71717a" },
  ];
</script>

<div class="space-y-8">
  <!-- 1. 主题模式 -->
  <SettingsSection title="主题模式" description="选择跟随系统外观，或强制使用明亮 / 暗色模式">
    {#snippet action()}
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="rounded-lg border border-border/70 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          onclick={handleImport}
        >
          导入主题
        </button>
        <button
          type="button"
          class="rounded-lg border border-border/70 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          onclick={handleExport}
        >
          导出主题
        </button>
      </div>
    {/snippet}

    {#if themeFeedback}
      <div
        class="rounded-lg p-2.5 text-xs {isFeedbackError
          ? 'bg-rose-500/10 text-rose-600 dark:text-rose-400'
          : 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'}"
      >
        {themeFeedback}
      </div>
    {/if}

    <!-- 3 Theme Cards Grid (System, Light, Dark) matching Codex design -->
    <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
      <button
        type="button"
        class="rounded-xl border p-3.5 text-left transition-all {appearance.themeMode === 'system'
          ? 'border-primary bg-primary/[0.06] shadow-xs'
          : 'border-border/70 bg-card hover:border-primary/40'}"
        onclick={() => update({ themeMode: "system" })}
      >
        <div
          class="mb-3 h-14 overflow-hidden rounded-lg border border-border/60 bg-gradient-to-r from-slate-200 via-slate-100 to-slate-800 p-2"
        >
          <div class="h-full w-full rounded bg-background/80 shadow-2xs"></div>
        </div>
        <div class="font-medium text-xs text-foreground">跟随系统 (System)</div>
        <div class="mt-0.5 text-[11px] text-muted-foreground">根据 macOS 系统外观动态切换</div>
      </button>

      <button
        type="button"
        class="rounded-xl border p-3.5 text-left transition-all {appearance.themeMode === 'light'
          ? 'border-primary bg-primary/[0.06] shadow-xs'
          : 'border-border/70 bg-card hover:border-primary/40'}"
        onclick={() => update({ themeMode: "light" })}
      >
        <div class="mb-3 h-14 overflow-hidden rounded-lg border border-border/60 bg-slate-100 p-2">
          <div class="h-full w-full rounded bg-white shadow-2xs"></div>
        </div>
        <div class="font-medium text-xs text-foreground">浅色模式 (Light)</div>
        <div class="mt-0.5 text-[11px] text-muted-foreground">清爽明快的白昼视觉</div>
      </button>

      <button
        type="button"
        class="rounded-xl border p-3.5 text-left transition-all {appearance.themeMode === 'dark'
          ? 'border-primary bg-primary/[0.06] shadow-xs'
          : 'border-border/70 bg-card hover:border-primary/40'}"
        onclick={() => update({ themeMode: "dark" })}
      >
        <div class="mb-3 h-14 overflow-hidden rounded-lg border border-border/60 bg-zinc-900 p-2">
          <div class="h-full w-full rounded bg-zinc-800 shadow-2xs"></div>
        </div>
        <div class="font-medium text-xs text-foreground">深色模式 (Dark)</div>
        <div class="mt-0.5 text-[11px] text-muted-foreground">高对比度与低眩光的暗夜视觉</div>
      </button>
    </div>
  </SettingsSection>

  <!-- 2. 预设主题与调色盘 -->
  <SettingsSection title="配色方案与强调色" description="自定义全局基准色系与控件高亮强调色">
    <SettingsCard>
      <SettingsSelectRow
        id="builtin-theme-select"
        title="预设设计方案"
        description="选用精心微调的内置调色板"
        value={appearance.builtinTheme}
        options={BUILTIN_THEMES.map((t) => ({
          value: t.id,
          label: t.name,
        }))}
        onchange={(val) => update({ builtinTheme: val })}
      />

      <SettingsSelectRow
        id="color-scheme-select"
        title="灰色基底色系 (Color Scheme)"
        description="控制界面的中性灰调冷暖感"
        value={appearance.colorScheme}
        options={[
          { value: "neutral", label: "中性 (Neutral)" },
          { value: "zinc", label: "锌灰 (Zinc)" },
          { value: "slate", label: "板岩 (Slate)" },
          { value: "stone", label: "暖石 (Stone)" },
          { value: "gray", label: "冷灰 (Gray)" },
        ]}
        onchange={(val) => update({ colorScheme: val as ColorScheme })}
      />

      <SettingsRow title="强调色 (Accent Color)" description="按钮、激活开关与指示器的焦点色彩">
        <div class="flex items-center gap-3">
          <div class="flex items-center gap-1.5">
            {#each ACCENT_SWATCHES as swatch}
              <button
                type="button"
                class="h-5 w-5 rounded-full border-2 transition-transform hover:scale-110 {appearance.accentColor ===
                swatch.value
                  ? 'border-foreground scale-110'
                  : 'border-transparent'}"
                style="background-color: {swatch.value};"
                title={swatch.label}
                onclick={() => update({ accentColor: swatch.value })}
              ></button>
            {/each}
          </div>
          <div class="h-4 w-px bg-border"></div>
          <ColorInput
            value={appearance.accentColor || "#20b982"}
            onChange={(val: string) => update({ accentColor: val })}
          />
        </div>
      </SettingsRow>
    </SettingsCard>
  </SettingsSection>

  <!-- 3. 字体与排版 -->
  <SettingsSection title="字体与排版" description="调整界面文本与代码块的字体系列及基准字号">
    <SettingsCard>
      <SettingsRow
        title="界面字号 (UI Font Size)"
        description="当前基准字号: {appearance.uiFontSize}px (范围: {UI_FONT_SIZE_LIMITS.min} - {UI_FONT_SIZE_LIMITS.max}px)"
      >
        <div class="flex items-center gap-3 w-44">
          <input
            type="range"
            min={UI_FONT_SIZE_LIMITS.min}
            max={UI_FONT_SIZE_LIMITS.max}
            step="1"
            class="w-full accent-primary"
            value={appearance.uiFontSize}
            oninput={(e) => update({ uiFontSize: Number((e.target as HTMLInputElement).value) })}
          />
          <span class="w-8 text-right font-mono text-xs text-muted-foreground"
            >{appearance.uiFontSize}px</span
          >
        </div>
      </SettingsRow>

      <SettingsRow
        title="代码字号 (Code Font Size)"
        description="当前代码字号: {appearance.codeFontSize}px (范围: {CODE_FONT_SIZE_LIMITS.min} - {CODE_FONT_SIZE_LIMITS.max}px)"
      >
        <div class="flex items-center gap-3 w-44">
          <input
            type="range"
            min={CODE_FONT_SIZE_LIMITS.min}
            max={CODE_FONT_SIZE_LIMITS.max}
            step="1"
            class="w-full accent-primary"
            value={appearance.codeFontSize}
            oninput={(e) => update({ codeFontSize: Number((e.target as HTMLInputElement).value) })}
          />
          <span class="w-8 text-right font-mono text-xs text-muted-foreground"
            >{appearance.codeFontSize}px</span
          >
        </div>
      </SettingsRow>

      <SettingsToggleRow
        id="font-smoothing"
        title="字体平滑渲染 (Font Smoothing)"
        description="在 macOS 与 Windows 上启用亚像素抗锯齿提升字体边缘清晰度"
        checked={appearance.fontSmoothing}
        onchange={(checked) => update({ fontSmoothing: checked })}
      />
    </SettingsCard>
  </SettingsSection>

  <!-- 4. 动效与高级视觉 -->
  <SettingsSection title="动效与辅助视觉" description="毛玻璃质感、系统动效减弱与差异对比标记风格">
    <SettingsCard>
      <SettingsToggleRow
        id="sidebar-translucent"
        title="侧边栏半透明效果 (Translucency)"
        description="在支持的原生桌面端启用窗口背衬毛玻璃透光"
        checked={appearance.translucentSidebar}
        onchange={(checked) => update({ translucentSidebar: checked })}
      />

      <SettingsSelectRow
        id="reduce-motion"
        title="减弱动效 (Reduce Motion)"
        description="降低过渡动画和页面跳动频率"
        value={appearance.reduceMotion}
        options={[
          { value: "system", label: "跟随系统设置 (System)" },
          { value: "on", label: "开启减弱动画 (Reduce Motion)" },
          { value: "off", label: "完整动态过渡 (Full Motion)" },
        ]}
        onchange={(val) => update({ reduceMotion: val as ReduceMotion })}
      />

      <SettingsSelectRow
        id="diff-marker-style"
        title="代码差异高亮标记风格 (Diff Style)"
        description="选择 Diff 审查面板中的颜色块或符号标记"
        value={appearance.diffMarkerStyle}
        options={[
          { value: "color", label: "纯色彩高亮 (Color Highlight)" },
          { value: "symbols", label: "符号与对比 (Symbols +/-)" },
        ]}
        onchange={(val) => update({ diffMarkerStyle: val as DiffMarkerStyle })}
      />

      <SettingsToggleRow
        id="pointer-cursor"
        title="自定义鼠标指针样式"
        description="在可点击元素上呈现极客光标指针"
        checked={appearance.pointerCursor}
        onchange={(checked) => update({ pointerCursor: checked })}
      />
    </SettingsCard>
  </SettingsSection>
</div>
