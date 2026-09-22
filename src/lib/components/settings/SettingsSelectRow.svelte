<script lang="ts">
  import type { Snippet } from "svelte";
  import SettingsRow from "./SettingsRow.svelte";

  interface SelectOption {
    value: string | number;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    title: string;
    description?: string;
    value: string | number;
    options: SelectOption[];
    disabled?: boolean;
    onchange: (value: string) => void;
    id?: string;
    icon?: Snippet;
    badge?: Snippet;
  }

  let {
    title,
    description = "",
    value,
    options,
    disabled = false,
    onchange,
    id,
    icon,
    badge,
  }: Props = $props();
</script>

<SettingsRow {title} {description} {disabled} {id} {icon} {badge}>
  <div class="relative inline-block">
    <select
      {disabled}
      class="h-8 rounded-lg border border-border/70 bg-background/80 px-3 pr-8 text-xs font-medium text-foreground transition-colors hover:bg-accent/40 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary appearance-none cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
      value={String(value)}
      onchange={(e) => onchange((e.target as HTMLSelectElement).value)}
    >
      {#each options as opt}
        <option value={String(opt.value)} disabled={opt.disabled}>
          {opt.label}
        </option>
      {/each}
    </select>
    <div
      class="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-muted-foreground"
    >
      <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
        <path
          fill-rule="evenodd"
          d="M5.23 7.21a.75.75 0 011.06.02L10 10.94l3.71-3.71a.75.75 0 111.06 1.06l-4.25 4.25a.75.75 0 01-1.06 0L5.21 8.27a.75.75 0 01.02-1.06z"
          clip-rule="evenodd"
        />
      </svg>
    </div>
  </div>
</SettingsRow>
