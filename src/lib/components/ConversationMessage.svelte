<script lang="ts">
  import WorkConversationMessage from "$lib/components/work/WorkConversationMessage.svelte";
  import type { Attachment, ChatMessage } from "$lib/types";

  type Props = {
    message: ChatMessage;
    attachments?: Attachment[];
    thinkingText?: string;
    onRewind?: () => void;
    onContinueFromMessage?: () => void;
    onExport?: () => void;
    onEdit?: (newContent: string) => void | Promise<void>;
    agent?: string;
    isRunning?: boolean;
    showHeader?: boolean;
  };

  let props: Props = $props();
</script>

<WorkConversationMessage {...props} />
