<script lang="ts">
  import HookManager from "$lib/components/HookManager.svelte";
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      钩子系统 (Hooks)
    </h2>
    <p class="mt-1 text-xs text-muted-foreground">
      监听与拦截 Agent 运行生命周期事件（如 PreToolUse,
      PostToolUse），执行外部脚本、安全审批或自定义逻辑。
    </p>
  </div>

  <HookManager />
</div>
