<script lang="ts">
  import type { CodeAsideTabType } from "$lib/types/code-aside";
  import { CODE_ASIDE_SHORTCUTS } from "$lib/types/code-aside";

  interface Props {
    onSelect: (type: CodeAsideTabType) => void;
  }

  let { onSelect }: Props = $props();

  const options: { type: CodeAsideTabType; label: string; shortcut: string }[] = [
    { type: "review", label: "审查", shortcut: CODE_ASIDE_SHORTCUTS.review },
    { type: "terminal", label: "终端", shortcut: CODE_ASIDE_SHORTCUTS.terminal },
    { type: "browser", label: "浏览器", shortcut: CODE_ASIDE_SHORTCUTS.browser },
    { type: "file", label: "文件", shortcut: CODE_ASIDE_SHORTCUTS.file },
  ];
</script>

<div class="flex h-full flex-col items-center justify-center p-6 select-none bg-background">
  <div
    class="w-full max-w-[360px] space-y-1.5 rounded-2xl border border-border/70 bg-card/50 p-2.5 shadow-sm backdrop-blur-sm"
  >
    {#each options as opt}
      <button
        type="button"
        class="group flex w-full items-center justify-between rounded-xl px-4 py-3 text-left text-sm font-medium text-foreground transition-all hover:bg-accent hover:text-accent-foreground active:scale-[0.99]"
        onclick={() => onSelect(opt.type)}
      >
        <div class="flex items-center gap-3">
          {#if opt.type === "review"}
            <svg
              class="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect x="3" y="3" width="18" height="18" rx="2" />
              <path d="M9 3v18" />
              <path d="M14 9h5" />
              <path d="M14 15h5" />
            </svg>
          {:else if opt.type === "terminal"}
            <svg
              class="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <polyline points="4 17 10 11 4 5" />
              <line x1="12" y1="19" x2="20" y2="19" />
            </svg>
          {:else if opt.type === "browser"}
            <svg
              class="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="2" y1="12" x2="22" y2="12" />
              <path
                d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
              />
            </svg>
          {:else}
            <svg
              class="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <path d="M14 2v6h6" />
            </svg>
          {/if}
          <span>{opt.label}</span>
        </div>

        <kbd
          class="rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] text-muted-foreground border border-border/50 shadow-2xs"
        >
          {opt.shortcut}
        </kbd>
      </button>
    {/each}
  </div>
</div>
