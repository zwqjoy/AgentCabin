<script lang="ts">
  import { onMount } from "svelte";
  import { KeybindingStore, formatKeyDisplay } from "$lib/stores/keybindings.svelte";
  import KeybindingEditor from "$lib/components/KeybindingEditor.svelte";
  import SettingsCard from "../SettingsCard.svelte";
  import SettingsRow from "../SettingsRow.svelte";
  import SettingsSection from "../SettingsSection.svelte";

  let keybindingStore = $state(new KeybindingStore());
  let recordingConflict = $state("");

  let editableBindings = $derived(keybindingStore.resolved.filter((b) => b.editable));
  let fixedBindings = $derived(
    keybindingStore.resolved.filter((b) => !b.editable && b.source === "app"),
  );
  let cliBindings = $derived(keybindingStore.resolved.filter((b) => b.source === "cli"));

  onMount(async () => {
    keybindingStore = new KeybindingStore();
    await keybindingStore.loadOverrides();
    await keybindingStore.loadCliBindings();
  });
</script>

<div class="space-y-8">
  <!-- 1. 可自定义快捷键 -->
  <SettingsSection
    title="全局与模式快捷键"
    description="点击快捷键胶囊录制自定义按键，支持 Esc 取消与重置回默认设置"
  >
    {#snippet action()}
      <button
        type="button"
        class="rounded-lg border border-border/70 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
        onclick={() => void keybindingStore.resetAll()}
      >
        恢复所有默认
      </button>
    {/snippet}

    <SettingsCard>
      {#each editableBindings as binding (binding.command)}
        <div class="px-5 py-3 flex items-center justify-between gap-4">
          <div class="min-w-0">
            <div class="text-[13px] font-medium text-foreground">{binding.label}</div>
            <div class="text-[11px] text-muted-foreground">{binding.command}</div>
          </div>
          <div class="shrink-0">
            <KeybindingEditor
              {binding}
              isOverridden={keybindingStore.overrides.some((o) => o.command === binding.command)}
              conflictWarning={recordingConflict}
              onSave={(key) => {
                const conflict = keybindingStore.findConflict(
                  key,
                  binding.context,
                  binding.command,
                );
                if (conflict) {
                  recordingConflict = conflict.label;
                } else {
                  recordingConflict = "";
                  void keybindingStore.setOverride(binding.command, key);
                }
              }}
              onReset={keybindingStore.overrides.some((o) => o.command === binding.command)
                ? () => void keybindingStore.resetBinding(binding.command)
                : undefined}
            />
          </div>
        </div>
      {/each}
    </SettingsCard>
  </SettingsSection>

  <!-- 2. 系统固定输入快捷键 -->
  <SettingsSection
    title="输入框固定快捷键"
    description="与系统原生文本编辑保持一致的默认键盘行为（只读）"
  >
    <SettingsCard>
      {#each fixedBindings as binding (binding.command)}
        <SettingsRow title={binding.label} description={binding.command}>
          <kbd
            class="rounded-md border border-border/80 bg-muted/50 px-2.5 py-1 font-mono text-xs text-muted-foreground"
          >
            {formatKeyDisplay(binding.key)}
          </kbd>
        </SettingsRow>
      {/each}
    </SettingsCard>
  </SettingsSection>

  <!-- 3. CLI 终端快捷键 -->
  <SettingsSection title="CLI 交互快捷键" description="终端控制台常用的中断与流控快捷键">
    <SettingsCard>
      {#each cliBindings as binding (binding.command)}
        <SettingsRow title={binding.label} description={binding.command}>
          <kbd
            class="rounded-md border border-border/80 bg-muted/50 px-2.5 py-1 font-mono text-xs text-muted-foreground"
          >
            {formatKeyDisplay(binding.key)}
          </kbd>
        </SettingsRow>
      {/each}
    </SettingsCard>
  </SettingsSection>
</div>
