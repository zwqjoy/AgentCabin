<script lang="ts">
  import MarkdownContent from "./MarkdownContent.svelte";
  import ConversationHtmlPreview from "./ConversationHtmlPreview.svelte";
  import { conversationParts } from "$lib/utils/conversation-html";
  let {
    text = "",
    streaming = false,
    basePath = "",
    workspaceId = "",
    lazy = true,
  }: {
    text?: string;
    streaming?: boolean;
    basePath?: string;
    workspaceId?: string;
    /** Forwarded to MarkdownContent's viewport gate; pass false for always-on-screen blocks. */
    lazy?: boolean;
  } = $props();
  let snapshot = $state("");
  let latest = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  import { onDestroy } from "svelte";
  $effect(() => {
    latest = text;
    if (!streaming) {
      clearTimeout(timer);
      timer = undefined;
      snapshot = text;
    } else if (!timer) {
      timer = setTimeout(() => {
        snapshot = latest;
        timer = undefined;
      }, 100);
    }
  });
  onDestroy(() => clearTimeout(timer));
  const parts = $derived(conversationParts(snapshot, streaming));
</script>

{#each parts as part, index (index)}
  {#if part.kind === "html"}
    <ConversationHtmlPreview content={part.content} pending={part.pending} />
  {:else}
    <MarkdownContent text={part.content} {streaming} {basePath} {workspaceId} {lazy} />
  {/if}
{/each}
