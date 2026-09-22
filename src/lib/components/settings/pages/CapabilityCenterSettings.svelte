<script lang="ts">
  import PluginsPage from "$lib/components/PluginsPage.svelte";
</script>

<div class="capability-center-wrapper">
  <PluginsPage capabilityCenter={true} />
</div>

<style>
  .capability-center-wrapper :global(.plugins-page-container) {
    padding: 0;
  }
</style>
