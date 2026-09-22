<script lang="ts">
  interface Props {
    label: string;
    description?: string;
    checked: boolean;
    disabled?: boolean;
    onChange: (checked: boolean) => void;
  }

  let { label, description = "", checked, disabled = false, onChange }: Props = $props();
</script>

<div
  class="flex items-center justify-between gap-4 py-3 {disabled
    ? 'cursor-not-allowed opacity-50'
    : 'cursor-pointer'}"
  onclick={() => !disabled && onChange(!checked)}
  role="presentation"
>
  <div class="min-w-0 flex-1">
    <p class="text-sm font-medium text-foreground">{label}</p>
    {#if description}
      <p class="mt-1 text-xs leading-relaxed text-muted-foreground">{description}</p>
    {/if}
  </div>
  <button
    type="button"
    class="ui-switch relative inline-flex shrink-0 items-center transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
    class:opacity-50={disabled}
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    onclick={(e) => {
      e.stopPropagation();
      onChange(!checked);
    }}
  >
    <span
      class="inline-block rounded-full bg-white shadow transition-transform {checked
        ? 'translate-x-6'
        : 'translate-x-1'}"
    ></span>
  </button>
</div>
