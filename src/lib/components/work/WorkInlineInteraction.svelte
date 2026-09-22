<script lang="ts">
  import type { InboxItem, InboxItemStatus } from "$lib/types/work";
  import WorkPendingActionItem from "./WorkPendingActionItem.svelte";

  interface Props {
    item: InboxItem;
    readOnly?: boolean;
    onResolve?: (item: InboxItem, status: InboxItemStatus, response?: unknown) => Promise<void>;
  }

  let { item, readOnly = false, onResolve }: Props = $props();
</script>

<div class="chat-content-width my-2">
  <WorkPendingActionItem {item} {readOnly} {onResolve} />
</div>
