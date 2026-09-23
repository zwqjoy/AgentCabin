<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { formatRunDuration } from "$lib/utils/chat-format";
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
  let labelText = $derived(text ?? (isEn ? "Thinking…" : "正在深度思考…"));

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
  <span
    class="text-sm font-medium whitespace-nowrap bg-clip-text text-transparent bg-[length:250%_100%] animate-[pulse_2s_cubic-bezier(0.4,0,0.6,1)_infinite]"
    style="background-image: linear-gradient(90deg, #8b5cf6 0%, #8b5cf6 40%, #c4b5fd 50%, #8b5cf6 60%, #8b5cf6 100%);"
  >
    {labelText}
  </span>

  {#if elapsedMs >= 15000}
    <span class="ml-2 text-xs font-mono text-muted-foreground/70 font-normal">
      {formatRunDuration(elapsedMs, isEn)}
    </span>
  {/if}
</div>
