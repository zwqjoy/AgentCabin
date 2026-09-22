<script lang="ts">
  import type { Snippet } from "svelte";
  import SettingsRow from "./SettingsRow.svelte";

  interface Props {
    title: string;
    description?: string;
    checked: boolean;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
    id?: string;
    badge?: Snippet;
    icon?: Snippet;
  }

  let {
    title,
    description = "",
    checked,
    disabled = false,
    onchange,
    id,
    badge,
    icon,
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    onchange(!checked);
  }
</script>

<SettingsRow
  {title}
  {description}
  {disabled}
  {id}
  clickable={!disabled}
  onclick={toggle}
  {badge}
  {icon}
>
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={title}
    {disabled}
    onclick={(e) => {
      e.stopPropagation();
      toggle();
    }}
    class="relative inline-flex h-[22px] w-[38px] shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-150 ease-in-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background {checked
      ? 'bg-primary'
      : 'bg-muted-foreground/25 dark:bg-muted-foreground/30'} {disabled
      ? 'cursor-not-allowed opacity-50'
      : ''}"
  >
    <span
      class="pointer-events-none inline-block h-[18px] w-[18px] transform rounded-full bg-white shadow-xs ring-0 transition duration-150 ease-in-out {checked
        ? 'translate-x-4'
        : 'translate-x-0'}"
    ></span>
  </button>
</SettingsRow>
