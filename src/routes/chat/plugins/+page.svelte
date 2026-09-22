<script lang="ts">
  import PluginsPage from "$lib/components/PluginsPage.svelte";
</script>

<!-- Render the manager inside the chat shell so its new entry point keeps the conversation
     sidebar instead of switching to the legacy standalone extension layout. -->
<PluginsPage />
