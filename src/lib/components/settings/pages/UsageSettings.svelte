<script lang="ts">
  import UsagePage from "../../../../routes/usage/+page.svelte";
</script>

<div class="usage-settings-wrapper">
  <UsagePage />
</div>

<style>
  .usage-settings-wrapper :global(.usage-container) {
    padding: 0;
    max-width: 100%;
  }
</style>
