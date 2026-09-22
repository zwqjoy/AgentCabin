<script lang="ts">
  let {
    title,
    category,
    description,
    tag,
    tagColor = "bg-primary/10 text-primary border-primary/20",
  }: {
    title: string;
    category?: string;
    description?: string;
    tag?: string;
    tagColor?: string;
  } = $props();
</script>

<div class="mb-6 rounded-xl border border-border/50 bg-card/60 p-4 shadow-sm backdrop-blur-sm">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="space-y-1 min-w-0">
      {#if category}
        <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span>{category}</span>
          <span>/</span>
          <span class="font-medium text-foreground">{title}</span>
        </div>
      {/if}
      <h1 class="text-xl font-bold tracking-tight text-foreground">{title}</h1>
      {#if description}
        <p class="text-xs text-muted-foreground">{description}</p>
      {/if}
    </div>

    {#if tag}
      <span
        class="inline-flex items-center rounded-md border px-2.5 py-1 text-xs font-semibold tracking-wide {tagColor}"
      >
        {tag}
      </span>
    {/if}
  </div>
</div>
