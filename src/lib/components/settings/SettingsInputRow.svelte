<script lang="ts">
  import type { Snippet } from "svelte";
  import SettingsRow from "./SettingsRow.svelte";

  interface Props {
    title: string;
    description?: string;
    value: string;
    placeholder?: string;
    disabled?: boolean;
    type?: string;
    oninput?: (value: string) => void;
    onchange?: (value: string) => void;
    id?: string;
    icon?: Snippet;
    badge?: Snippet;
    actionButton?: Snippet;
    inputClass?: string;
  }

  let {
    title,
    description = "",
    value,
    placeholder = "",
    disabled = false,
    type = "text",
    oninput,
    onchange,
    id,
    icon,
    badge,
    actionButton,
    inputClass = "",
  }: Props = $props();
</script>

<SettingsRow {title} {description} {disabled} {id} {icon} {badge}>
  <div class="flex items-center gap-2">
    <input
      {type}
      {value}
      {placeholder}
      {disabled}
      class="h-8 rounded-lg border border-border/70 bg-background/80 px-3 text-xs text-foreground placeholder:text-muted-foreground/60 transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary disabled:cursor-not-allowed disabled:opacity-50 {inputClass}"
      oninput={(e) => oninput?.((e.target as HTMLInputElement).value)}
      onchange={(e) => onchange?.((e.target as HTMLInputElement).value)}
    />
    {#if actionButton}
      {@render actionButton()}
    {/if}
  </div>
</SettingsRow>
