<script lang="ts">
  import type { ChatPresentationTurn } from "$lib/utils/chat-presentation";
  import type { TaskNotificationItem } from "$lib/stores/session-store.svelte";
  import type { PermissionSuggestion, BusToolItem } from "$lib/types";
  import ChatActivityItem from "./ChatActivityItem.svelte";
  import ChatReasoningBlock from "./ChatReasoningBlock.svelte";
  import ChatInteractionBlock from "./ChatInteractionBlock.svelte";
  import TurnStatus from "$lib/dsh-ui/TurnStatus.svelte";
  import DshTurnErrorCard from "$lib/dsh-ui/DshTurnErrorCard.svelte";
  import DshMaxTokensCard from "$lib/dsh-ui/DshMaxTokensCard.svelte";
  import DshRetryCard from "$lib/dsh-ui/DshRetryCard.svelte";

  let {
    turn,
    runId = "",
    fetchToolResult,
    onAnswer,
    onApprove,
    onPermissionRespond,
    onExitPlanClearContext,
    taskNotifications,
    showPermissionInPanel = false,
    agentDisplayName,
    onPreviewFile,
    onToggleCollapse,
    error,
    maxTokens = false,
    retryInfo,
    onRetry,
    onContinue,
    onOpenDetails,
    renderCustomInteraction,
    renderCustomActivity,
  }: {
    turn: ChatPresentationTurn;
    runId?: string;
    fetchToolResult?: (runId: string, toolUseId: string) => Promise<Record<string, unknown> | null>;
    onAnswer?: (itemId: string, answer: string) => void;
    onApprove?: (toolName: string) => void;
    onPermissionRespond?: (
      requestId: string,
      behavior: "allow" | "deny",
      updatedPermissions?: PermissionSuggestion[],
      updatedInput?: Record<string, unknown>,
      denyMessage?: string,
      interrupt?: boolean,
    ) => void | Promise<void>;
    onExitPlanClearContext?: () => void | Promise<void>;
    taskNotifications?: Map<string, TaskNotificationItem>;
    showPermissionInPanel?: boolean;
    agentDisplayName?: string;
    onPreviewFile?: (path: string) => void;
    onToggleCollapse?: () => void;
    error?: { message?: string; code?: string | number } | null;
    maxTokens?: boolean;
    retryInfo?: {
      retry?: number;
      maximum?: number;
      delayMs?: number;
      seconds?: number;
      failure?: string;
      active?: boolean;
    } | null;
    onRetry?: () => void;
    onContinue?: () => void;
    onOpenDetails?: (
      toolName: string,
      args: unknown,
      result: unknown,
      isError: boolean,
      isRunning: boolean,
    ) => void;
    renderCustomInteraction?: import("svelte").Snippet<
      [tool: BusToolItem, item: import("$lib/utils/chat-presentation").ChatInteractionItem]
    >;
    renderCustomActivity?: import("svelte").Snippet<
      [activity: import("$lib/utils/tool-activity-adapter").ActivityItem]
    >;
  } = $props();

  const hasProcessContent = $derived(turn.processBlocks.length > 0);
  const hasInteractions = $derived(turn.interactionBlocks.length > 0);
  const hasInStreamInteractions = $derived(
    turn.processBlocks.some((b) => b.type === "interaction"),
  );

  // DSH TurnProcess counts & label
  const dshProcessSummary = $derived.by(() => {
    let toolCallCount = 0;
    let messageCount = 0;
    let subagentCount = 0;
    let hasReasoning = false;

    for (const block of turn.processBlocks) {
      if (block.type === "activity-group") {
        toolCallCount += block.activities.length;
        for (const act of block.activities) {
          if (act.iconKind === "bot") subagentCount++;
        }
      } else if (block.type === "narration") {
        messageCount++;
      } else if (block.type === "reasoning") {
        hasReasoning = true;
      }
    }

    const parts: string[] = [];
    if (toolCallCount > 0) parts.push(`${toolCallCount} 次工具调用`);
    if (messageCount > 0) parts.push(`${messageCount} 条消息`);
    if (subagentCount > 0) parts.push(`${subagentCount} 个 subagent`);

    if (parts.length === 0 && hasReasoning) {
      return "已思考";
    }
    return parts.join(" · ") || "已思考";
  });
</script>

<div class="w-full py-1">
  <!-- DSH TurnProcessNodeView: full-width subtle disclosure header without dividing line -->
  {#if hasProcessContent || turn.isRunning}
    <div class="w-full">
      <button
        type="button"
        class="w-full h-[30px] flex items-center text-left cursor-pointer text-[13px] text-muted-foreground/60 hover:text-foreground transition-colors group select-none py-0"
        aria-expanded={!turn.isCollapsed}
        onclick={() => onToggleCollapse?.()}
      >
        <span class="truncate">{dshProcessSummary}</span>

        <!-- Chevron (DSH: 16px, rotate-0 when open, -rotate-90 when closed) -->
        <svg
          class="h-4 w-4 text-muted-foreground/45 shrink-0 ml-1.5 transition-transform duration-150 {!turn.isCollapsed
            ? 'rotate-0'
            : '-rotate-90'}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
    </div>
  {/if}

  <!-- DSH Process Stream: clean flat linear stream without heavy left borders -->
  {#if !turn.isCollapsed && (hasProcessContent || turn.isRunning)}
    <div class="mt-2 space-y-1 animate-fade-in">
      {#if turn.processBlocks.length === 0 && turn.isRunning}
        <TurnStatus />
      {/if}
      {#each turn.processBlocks as block (block.id)}
        {#if block.type === "reasoning"}
          <ChatReasoningBlock content={block.content} isStreaming={block.isStreaming} />
        {:else if block.type === "narration"}
          <ChatReasoningBlock
            content={block.content}
            isStreaming={block.isStreaming}
            isNarration={true}
          />
        {:else if block.type === "activity-group"}
          <!-- DSH: Flat individual activity rows -->
          {#each block.activities as activity (activity.id)}
            {#if renderCustomActivity}
              {@render renderCustomActivity(activity)}
            {:else}
              <ChatActivityItem
                {activity}
                {runId}
                {fetchToolResult}
                {onPreviewFile}
                {onOpenDetails}
              />
            {/if}
          {/each}
        {:else if block.type === "interaction"}
          {#if renderCustomInteraction}
            {@render renderCustomInteraction(block.item.tool, block.item)}
          {:else}
            <ChatInteractionBlock
              tool={block.item.tool}
              subTimeline={block.item.subTimeline}
              {runId}
              {fetchToolResult}
              onAnswer={block.item.type === "ask_user" && onAnswer
                ? (answer) => onAnswer(block.item.id, answer)
                : undefined}
              {onApprove}
              {onPermissionRespond}
              {onExitPlanClearContext}
              {taskNotifications}
              planContent={block.item.planContent}
              {showPermissionInPanel}
              {agentDisplayName}
              {onPreviewFile}
            />
          {/if}
        {/if}
      {/each}
    </div>
  {/if}

  <!-- Fallback for interactive blocks not already rendered in-stream -->
  {#if !hasInStreamInteractions && hasInteractions}
    <div class="mt-2 space-y-2">
      {#each turn.interactionBlocks as item (item.id)}
        {#if renderCustomInteraction}
          {@render renderCustomInteraction(item.tool, item)}
        {:else}
          <ChatInteractionBlock
            tool={item.tool}
            subTimeline={item.subTimeline}
            {runId}
            {fetchToolResult}
            onAnswer={item.type === "ask_user" && onAnswer
              ? (answer) => onAnswer(item.id, answer)
              : undefined}
            {onApprove}
            {onPermissionRespond}
            {onExitPlanClearContext}
            {taskNotifications}
            planContent={item.planContent}
            {showPermissionInPanel}
            {agentDisplayName}
            {onPreviewFile}
          />
        {/if}
      {/each}
    </div>
  {/if}

  <!-- DSH Retry State Row -->
  {#if retryInfo}
    <div class="mt-2">
      <DshRetryCard
        retry={retryInfo.retry}
        maximum={retryInfo.maximum}
        delayMs={retryInfo.delayMs}
        seconds={retryInfo.seconds}
        failure={retryInfo.failure}
        active={retryInfo.active}
      />
    </div>
  {/if}

  <!-- DSH Turn Error Card -->
  {#if error}
    <div class="mt-2">
      <DshTurnErrorCard message={error.message} code={error.code} {onRetry} />
    </div>
  {/if}

  <!-- DSH Max Tokens Truncation Card -->
  {#if maxTokens}
    <div class="mt-2">
      <DshMaxTokensCard {onContinue} />
    </div>
  {/if}
</div>
