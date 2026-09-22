<script lang="ts">
  import type { Snippet } from "svelte";
  import type { UserSettings } from "$lib/types";

  interface Props {
    settings: UserSettings;
    onUpdateSettings?: (patch: Partial<UserSettings>) => Promise<void>;
    codeContent?: Snippet;
  }

  let { settings, onUpdateSettings, codeContent }: Props = $props();
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
      Code 极客编程模式
    </h2>
    <p class="mt-1 text-xs text-muted-foreground">
      面向全栈极客与开发者打造的高频编程工作载体，配置默认 Runtime Provider、工作载体
      Profile、全局规则与隔离工作区。
    </p>
  </div>

  {#if codeContent}
    {@render codeContent()}
  {/if}
</div>
