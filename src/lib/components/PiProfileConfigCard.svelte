<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Card from "$lib/components/Card.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { PiProfileInfo } from "$lib/types";

  let {
    mode,
    title,
    description,
  }: {
    mode: "code" | "work";
    title?: string;
    description?: string;
  } = $props();

  let profile = $state<PiProfileInfo | null>(null);
  let loading = $state(false);
  let error = $state("");
  let copiedKey = $state<string | null>(null);

  async function loadProfile() {
    if (loading) return;
    loading = true;
    error = "";
    try {
      profile = await api.getPiProfileInfo(mode);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function copyPath(key: string, path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copiedKey = key;
      setTimeout(() => {
        if (copiedKey === key) copiedKey = null;
      }, 1500);
    } catch {
      // ignore
    }
  }

  onMount(() => {
    void loadProfile();
  });
</script>

<Card class="space-y-4 p-5">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h3 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        {title ?? (mode === "code" ? "Code 工作载体 Profile" : "Work 工作载体 Profile")}
      </h3>
      <p class="mt-1 text-xs text-muted-foreground">
        {description ??
          (mode === "code" ? t("settings_scope_piCodeDesc") : t("settings_scope_piWorkDesc"))}
      </p>
    </div>
    <button
      class="shrink-0 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
      disabled={loading}
      onclick={() => void loadProfile()}
    >
      {loading ? t("settings_pi_checking") : t("settings_codex_refresh")}
    </button>
  </div>

  {#if error}
    <div class="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-500">
      {error}
    </div>
  {/if}

  {#if loading && !profile}
    <div class="flex items-center gap-2 text-sm text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"
      ></span>
      {t("settings_pi_checking")}
    </div>
  {:else if profile}
    <div class="grid gap-4 rounded-lg border border-border/60 p-4 text-sm sm:grid-cols-2">
      <div class="min-w-0">
        <span class="text-xs text-muted-foreground"
          >{t("settings_pi_profileConfig")} (settings.json)</span
        >
        <div
          class="mt-1.5 flex items-center justify-between gap-2 rounded-md border border-border/40 bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          <span class="truncate text-foreground select-all">{profile.settingsPath}</span>
          <button
            class="shrink-0 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            onclick={() => copyPath("settings", profile!.settingsPath)}
            title="复制路径"
          >
            {copiedKey === "settings" ? "已复制" : "复制"}
          </button>
        </div>
      </div>

      <div class="min-w-0">
        <span class="text-xs text-muted-foreground">Profile 隔离目录</span>
        <div
          class="mt-1.5 flex items-center justify-between gap-2 rounded-md border border-border/40 bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          <span class="truncate text-foreground select-all">{profile.profileDir}</span>
          <button
            class="shrink-0 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            onclick={() => copyPath("dir", profile!.profileDir)}
            title="复制路径"
          >
            {copiedKey === "dir" ? "已复制" : "复制"}
          </button>
        </div>
      </div>

      <div class="min-w-0">
        <span class="text-xs text-muted-foreground">规则文件 (AGENTS.md)</span>
        <div
          class="mt-1.5 flex items-center justify-between gap-2 rounded-md border border-border/40 bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          <span class="truncate text-foreground select-all">{profile.rulesPath}</span>
          <button
            class="shrink-0 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            onclick={() => copyPath("rules", profile!.rulesPath)}
            title="复制路径"
          >
            {copiedKey === "rules" ? "已复制" : "复制"}
          </button>
        </div>
      </div>

      <div class="min-w-0">
        <span class="text-xs text-muted-foreground">扩展目录 (Extensions)</span>
        <div
          class="mt-1.5 flex items-center justify-between gap-2 rounded-md border border-border/40 bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          <span class="truncate text-foreground select-all">{profile.extensionsDir}</span>
          <button
            class="shrink-0 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            onclick={() => copyPath("ext", profile!.extensionsDir)}
            title="复制路径"
          >
            {copiedKey === "ext" ? "已复制" : "复制"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</Card>
