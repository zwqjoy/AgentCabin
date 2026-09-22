<script lang="ts">
  interface Props {
    status?: "ok" | "warning" | "error" | "info" | "neutral";
    text: string;
    dot?: boolean;
    class?: string;
  }

  let { status = "neutral", text, dot = true, class: className = "" }: Props = $props();

  const STATUS_CLASSES = {
    ok: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
    warning: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
    error: "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20",
    info: "bg-sky-500/10 text-sky-600 dark:text-sky-400 border-sky-500/20",
    neutral: "bg-muted text-muted-foreground border-border/60",
  };

  const DOT_CLASSES = {
    ok: "bg-emerald-500",
    warning: "bg-amber-500",
    error: "bg-rose-500",
    info: "bg-sky-500",
    neutral: "bg-muted-foreground/60",
  };
</script>

<span
  class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-[11px] font-medium border {STATUS_CLASSES[
    status
  ]} {className}"
>
  {#if dot}
    <span class="h-1.5 w-1.5 rounded-full {DOT_CLASSES[status]} shrink-0"></span>
  {/if}
  <span>{text}</span>
</span>
