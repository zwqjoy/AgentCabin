<script lang="ts">
  let { class: className = "h-4 w-4" }: { class?: string } = $props();
</script>

<svg
  class={className}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d="m12 3 9 5-9 5-9-5 9-5Z" />
  <path d="m3 12 9 5 9-5" />
  <path d="m3 16 9 5 9-5" />
</svg>
