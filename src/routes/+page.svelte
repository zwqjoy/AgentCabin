<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import {
    getSavedAppMode,
    getSavedRealm,
    getSavedPiSubMode,
    getRealmHref,
    getModeHref,
  } from "$lib/stores/app-mode.svelte";
  import { getUserSettings } from "$lib/api";
  import { getTransport } from "$lib/transport";

  onMount(async () => {
    const appMode = getSavedAppMode();
    const realm = getSavedRealm();
    const subMode = getSavedPiSubMode();
    const isWork = appMode === "work" || (realm === "pi" && subMode === "work");

    if (isWork) {
      try {
        const settings = await getUserSettings();
        const workEnabled = settings?.work_mode_enabled !== false && getTransport().isDesktop();
        if (!workEnabled) {
          void goto(getRealmHref(realm === "pi" ? "pi" : "native", "code"), { replaceState: true });
          return;
        }
      } catch {
        // ignore fetch error
      }
      void goto(getModeHref("work"), { replaceState: true });
      return;
    }

    void goto(getRealmHref(realm, "code"), { replaceState: true });
  });
</script>

<div class="flex h-full items-center justify-center">
  <p class="text-muted-foreground">{t("common_redirecting")}</p>
</div>
