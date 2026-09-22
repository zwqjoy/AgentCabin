<script lang="ts">
  import DesktopUsePanel from "$lib/components/DesktopUsePanel.svelte";
  import type { DesktopUseStatus } from "$lib/types/work";

  interface Props {
    status: DesktopUseStatus | null;
    loading: boolean;
    onToggle: (enabled: boolean) => Promise<void>;
    onRefresh: () => Promise<void>;
    onRequestPermissions: () => Promise<void>;
    onOpenPermissionPane: (kind: "accessibility" | "screen-recording") => Promise<void>;
  }

  let { status, loading, onToggle, onRefresh, onRequestPermissions, onOpenPermissionPane }: Props =
    $props();
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      电脑控制 (Desktop Use)
    </h2>
    <p class="mt-1 text-xs text-muted-foreground">
      配置桌面操作系统级别的观察和交互能力，包括屏幕截图、鼠标点击与键盘按键合成。
    </p>
  </div>

  <DesktopUsePanel
    {status}
    {loading}
    {onToggle}
    {onRefresh}
    {onRequestPermissions}
    {onOpenPermissionPane}
  />
</div>
