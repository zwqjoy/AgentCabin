<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  type AuthMode = "cli" | "app";

  let {
    value,
    onModeChange,
  }: {
    value: AuthMode;
    onModeChange: (mode: AuthMode) => void;
  } = $props();
</script>

<div>
  <span class="mb-2 block text-sm font-medium">{t("settings_auth_modeLabel")}</span>
  <div class="mt-1 grid grid-cols-1 gap-3 sm:grid-cols-2">
    <button
      type="button"
      class="flex min-h-32 flex-col items-center justify-center gap-2 rounded-lg border p-4 text-sm transition-all duration-150
        {value === 'cli'
        ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
        : 'hover:border-ring/30 hover:bg-accent'}"
      aria-pressed={value === "cli"}
      onclick={() => onModeChange("cli")}
    >
      <span class="flex h-10 w-10 items-center justify-center rounded-full bg-emerald-500/10">
        <svg
          class="h-5 w-5 text-emerald-400"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
      </span>
      <span class="font-medium">{t("auth_cliAuth")}</span>
      <span class="text-center text-[10px] text-muted-foreground">
        {t("settings_auth_modeCliDesc")}
      </span>
    </button>

    <button
      type="button"
      class="flex min-h-32 flex-col items-center justify-center gap-2 rounded-lg border p-4 text-sm transition-all duration-150
        {value === 'app'
        ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
        : 'hover:border-ring/30 hover:bg-accent'}"
      aria-pressed={value === "app"}
      onclick={() => onModeChange("app")}
    >
      <span class="flex h-10 w-10 items-center justify-center rounded-full bg-blue-500/10">
        <svg
          class="h-5 w-5 text-blue-400"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path
            d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"
          />
        </svg>
      </span>
      <span class="font-medium">{t("settings_managedByAgentCabin")}</span>
      <span class="text-center text-[10px] text-muted-foreground">
        {t("settings_managedByAgentCabinDesc")}
      </span>
    </button>
  </div>
</div>
