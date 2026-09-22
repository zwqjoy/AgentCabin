<script lang="ts">
  import type { UserSettings, CodexAuthResult } from "$lib/types";
  import GlobalProviderSettings from "$lib/components/GlobalProviderSettings.svelte";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    settings: UserSettings;
    onSaved: (next: UserSettings) => void;
    codexAuth?: CodexAuthResult | null;
    codexInstalled?: boolean | null;
    codexLoginLoading?: boolean;
    codexLoginError?: string;
    onOpenCodexLogin?: () => void;
    onReopenCodexLogin?: () => void;
    onLogoutCodexSubscription?: () => void;
  }

  let {
    settings,
    onSaved,
    codexAuth = null,
    codexInstalled = true,
    codexLoginLoading = false,
    codexLoginError = "",
    onOpenCodexLogin = () => {},
    onReopenCodexLogin = () => {},
    onLogoutCodexSubscription = () => {},
  }: Props = $props();
</script>

<div class="space-y-6">
  <GlobalProviderSettings
    initialSettings={settings}
    {onSaved}
    {codexAuth}
    {codexInstalled}
    {codexLoginLoading}
    {codexLoginError}
    {onOpenCodexLogin}
    {onReopenCodexLogin}
    {onLogoutCodexSubscription}
  />
</div>
