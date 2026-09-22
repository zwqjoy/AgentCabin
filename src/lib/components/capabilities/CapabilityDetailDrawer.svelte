<script lang="ts">
  import type { CapabilityCenterItem } from "$lib/types/work";
  import CapabilityReadinessBadge from "./CapabilityReadinessBadge.svelte";

  interface Props {
    item: CapabilityCenterItem | null;
    open: boolean;
    onClose: () => void;
    onAction?: (actionType: string, item: CapabilityCenterItem) => void;
  }

  let { item, open, onClose, onAction }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && item}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm transition-opacity"
    onclick={onClose}
    role="presentation"
  ></div>

  <!-- Drawer -->
  <div
    class="fixed inset-y-0 right-0 z-50 flex w-full max-w-md flex-col bg-background p-6 shadow-2xl border-l border-border transition-transform"
    role="dialog"
    aria-modal="true"
    aria-labelledby="drawer-title"
  >
    <!-- Header -->
    <div class="flex items-start justify-between gap-4 pb-4 border-b border-border">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2 mb-1">
          <span
            class="rounded bg-muted px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wider text-muted-foreground"
          >
            {item.category}
          </span>
          <CapabilityReadinessBadge readiness={item.readiness} size="sm" />
        </div>
        <h2 id="drawer-title" class="text-lg font-bold text-foreground truncate">
          {item.name}
        </h2>
        <p class="text-xs text-muted-foreground mt-0.5">
          {item.readinessReason}
        </p>
      </div>

      <button
        type="button"
        class="rounded-lg p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={onClose}
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto py-4 space-y-5 text-xs">
      <!-- Description -->
      <div>
        <h3 class="font-semibold text-foreground mb-1.5">能力介绍</h3>
        <p
          class="text-muted-foreground leading-relaxed bg-muted/30 p-3 rounded-lg border border-border/50"
        >
          {item.description || "暂无描述"}
        </p>
      </div>

      <!-- Runtime Availability Matrix -->
      <div>
        <h3 class="font-semibold text-foreground mb-2">运行时兼容矩阵 (Available In)</h3>
        <div class="rounded-lg border border-border overflow-hidden">
          <table class="w-full text-left">
            <thead class="bg-muted/60 text-[11px] font-medium text-muted-foreground">
              <tr>
                <th class="px-3 py-2">Runtime Provider</th>
                <th class="px-3 py-2">Work Mode</th>
                <th class="px-3 py-2">Code Mode</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border/50 text-[11px]">
              {#each item.runtimeAvailability as scope}
                {@const workAvailable =
                  scope.workAvailable ?? (item.scopes.includes("work") && scope.available)}
                {@const codeAvailable =
                  scope.codeAvailable ?? (item.scopes.includes("code") && scope.available)}
                <tr class="hover:bg-muted/20">
                  <td class="px-3 py-2 font-mono font-medium capitalize">{scope.provider}</td>
                  <td class="px-3 py-2">
                    {#if workAvailable}
                      <span class="text-emerald-600 dark:text-emerald-400 font-semibold"
                        >✓ 可用</span
                      >
                    {:else if scope.reason}
                      <span class="text-muted-foreground text-[10px]">{scope.reason}</span>
                    {:else}
                      <span class="text-zinc-400">-</span>
                    {/if}
                  </td>
                  <td class="px-3 py-2">
                    {#if codeAvailable}
                      <span class="text-emerald-600 dark:text-emerald-400 font-semibold"
                        >✓ 可用</span
                      >
                    {:else}
                      <span class="text-zinc-400">-</span>
                    {/if}
                  </td>
                </tr>
              {/each}
              {#if !item.runtimeAvailability.some((r) => r.provider === "claude" || r.provider === "codex")}
                <tr class="text-muted-foreground/60 text-[10px]">
                  <td class="px-3 py-2 font-mono">Claude / Codex</td>
                  <td class="px-3 py-2 italic" colspan="2">未确定的第三方运行时 (Unknown)</td>
                </tr>
              {/if}
            </tbody>
          </table>
        </div>
      </div>

      <!-- Account & Auth Details -->
      {#if item.auth}
        <div>
          <h3 class="font-semibold text-foreground mb-1.5">认证信息 (Authentication)</h3>
          <div class="rounded-lg bg-muted/40 p-3 border border-border/50 space-y-1.5 text-xs">
            <div class="flex justify-between">
              <span class="text-muted-foreground">认证状态:</span>
              <span class="font-mono font-medium capitalize">{item.auth.status}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">绑定账号数:</span>
              <span class="font-mono">{item.auth.accountCount}</span>
            </div>
            {#if item.auth.accounts && item.auth.accounts.length > 0}
              <div class="pt-1.5 border-t border-border/40">
                <span class="text-muted-foreground block mb-1">账号列表:</span>
                <div class="space-y-1">
                  {#each item.auth.accounts as acc}
                    <div class="flex items-center gap-1.5 text-foreground font-mono text-[11px]">
                      <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
                      <span>{acc}</span>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Permissions & Capabilities -->
      {#if (item.permissions && item.permissions.length > 0) || (item.capabilities && item.capabilities.length > 0)}
        <div>
          <h3 class="font-semibold text-foreground mb-1.5">权限与能力标识</h3>
          <div class="flex flex-wrap gap-1.5">
            {#each item.permissions || [] as perm}
              <span class="rounded bg-primary/10 px-2 py-0.5 text-[11px] font-mono text-primary">
                perm:{perm}
              </span>
            {/each}
            {#each item.capabilities || [] as cap}
              <span
                class="rounded bg-muted px-2 py-0.5 text-[11px] font-mono text-muted-foreground"
              >
                #{cap}
              </span>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Footer Actions -->
    <div class="pt-4 border-t border-border flex items-center justify-end gap-2">
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-foreground hover:bg-muted transition-colors"
        onclick={onClose}
      >
        关闭
      </button>

      {#if item.actions.length > 0}
        {#each item.actions as act}
          <button
            type="button"
            class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
            onclick={() => {
              onAction?.(act.actionType, item);
              onClose();
            }}
          >
            {act.label}
          </button>
        {/each}
      {/if}
    </div>
  </div>
{/if}
