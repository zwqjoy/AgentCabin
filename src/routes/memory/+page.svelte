<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  onMount(() => {
    goto("/chat/plugins?scope=code&section=rules", { replaceState: true });
  });
</script>
