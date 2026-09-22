<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { formatRunDuration } from "$lib/utils/dsh-format";
  import { currentLocale } from "$lib/i18n/index.svelte";

  let {
    startTime = Date.now(),
    text,
  }: {
    startTime?: number;
    text?: string;
  } = $props();

  let elapsedMs = $state(0);
  let timerId: ReturnType<typeof setInterval> | undefined;

  let isEn = $derived(Boolean(currentLocale() && currentLocale().startsWith("en")));
  let labelText = $derived(text ?? (isEn ? "Deep diving…" : "正在深度思考…"));

  onMount(() => {
    const update = () => {
      elapsedMs = Math.max(0, Date.now() - startTime);
    };
    update();
    timerId = setInterval(update, 1000);
  });

  onDestroy(() => {
    clearInterval(timerId);
  });
</script>

<div class="flex items-center py-2 select-none" role="status" aria-live="polite">
  <!-- DSH Shimmer Text -->
  <span
    class="text-sm font-medium whitespace-nowrap bg-clip-text text-transparent bg-[length:250%_100%] animate-[dsh-turn-status-shimmer_1.8s_linear_infinite]"
    style="background-image: linear-gradient(90deg, #4d6bfe 0%, #4d6bfe 40%, #a8b9ff 50%, #4d6bfe 60%, #4d6bfe 100%);"
  >
    {labelText}
  </span>

  <!-- DSH TurnStatusClock (> 15s) -->
  {#if elapsedMs >= 15000}
    <span class="ml-2 text-xs font-mono text-muted-foreground/70 font-normal">
      {formatRunDuration(elapsedMs, isEn)}
    </span>
  {/if}
</div>
