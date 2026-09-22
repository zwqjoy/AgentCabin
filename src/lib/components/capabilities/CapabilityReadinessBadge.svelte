<script lang="ts">
  import type { CapabilityReadiness } from "$lib/types/work";

  interface Props {
    readiness: CapabilityReadiness;
    size?: "sm" | "md";
    showLabel?: boolean;
  }

  let { readiness, size = "md", showLabel = true }: Props = $props();

  const config = $derived.by(() => {
    switch (readiness) {
      case "ready":
        return {
          label: "Ready",
          bg: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
          dot: "bg-emerald-500",
        };
      case "needs_auth":
        return {
          label: "Needs Login",
          bg: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
          dot: "bg-amber-500",
        };
      case "missing_dependency":
        return {
          label: "Missing Dependency",
          bg: "bg-orange-500/10 text-orange-600 dark:text-orange-400 border-orange-500/20",
          dot: "bg-orange-500",
        };
      case "disabled":
        return {
          label: "Disabled",
          bg: "bg-zinc-500/10 text-zinc-500 dark:text-zinc-400 border-zinc-500/20",
          dot: "bg-zinc-400 dark:bg-zinc-500",
        };
      case "unhealthy":
        return {
          label: "Unhealthy",
          bg: "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20",
          dot: "bg-rose-500",
        };
      case "incompatible":
        return {
          label: "Incompatible",
          bg: "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20",
          dot: "bg-purple-500",
        };
      case "not_installed":
      default:
        return {
          label: "Not Installed",
          bg: "bg-zinc-500/10 text-zinc-400 dark:text-zinc-500 border-zinc-500/20",
          dot: "bg-zinc-300 dark:bg-zinc-600",
        };
    }
  });
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full border font-medium transition-colors {config.bg} {size ===
  'sm'
    ? 'px-2 py-0.5 text-[11px]'
    : 'px-2.5 py-1 text-xs'}"
>
  <span class="h-1.5 w-1.5 rounded-full {config.dot}"></span>
  {#if showLabel}
    <span>{config.label}</span>
  {/if}
</span>
