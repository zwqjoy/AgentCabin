<script lang="ts">
  type Variant = "default" | "subtle" | "highlighted";

  let {
    variant = "default",
    class: className = "",
    children,
  }: {
    variant?: Variant;
    class?: string;
    children?: import("svelte").Snippet;
  } = $props();

  const variants: Record<Variant, string> = {
    default: "ui-card",
    subtle: "ui-card border-transparent bg-muted/50 hover:bg-muted/70",
    highlighted: "ui-card border-2 border-primary/20 hover:border-primary/40",
  };
</script>

<div class="{variants[variant]} {className}">
  {#if children}
    {@render children()}
  {/if}
</div>
