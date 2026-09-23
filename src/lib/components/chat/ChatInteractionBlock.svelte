<script lang="ts">
  import type { BusToolItem, TimelineEntry, PermissionSuggestion } from "$lib/types";
  import type { TaskNotificationItem } from "$lib/stores/session-store.svelte";
  import InlineToolCard from "$lib/components/InlineToolCard.svelte";

  import ApprovalCard from "./ApprovalCard.svelte";

  let {
    tool,
    subTimeline,
    runId = "",
    fetchToolResult,
    onAnswer,
    onApprove,
    onPermissionRespond,
    onExitPlanClearContext,
    taskNotifications,
    planContent,
    showPermissionInPanel = false,
    agentDisplayName,
    onPreviewFile,
  }: {
    tool: BusToolItem;
    subTimeline?: TimelineEntry[];
    runId?: string;
    fetchToolResult?: (runId: string, toolUseId: string) => Promise<Record<string, unknown> | null>;
    onAnswer?: (answer: string) => void;
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
    planContent?: { content: string; fileName: string } | null;
    showPermissionInPanel?: boolean;
    agentDisplayName?: string;
    onPreviewFile?: (path: string) => void;
  } = $props();

  // If this is a direct tool approval request (e.g. bash command permission or write permission) without questionnaire or complex plan
  let isPurePermission = $derived(
    Boolean(
      (tool.permission_request_id && onPermissionRespond) ||
      (onApprove &&
        !planContent &&
        tool.tool_name !== "ask_user_question" &&
        tool.tool_name !== "AskUserQuestion"),
    ),
  );

  let commandText = $derived.by(() => {
    if (typeof tool.tool_input === "object" && tool.tool_input !== null) {
      const input = tool.tool_input as Record<string, unknown>;
      if (typeof input.command === "string") return input.command;
      if (typeof input.cmd === "string") return input.cmd;
    }
    return "";
  });

  let rawArgs = $derived.by(() => {
    if (commandText) return "";
    if (tool.tool_input) {
      return JSON.stringify(tool.tool_input, null, 2);
    }
    return "";
  });

  function handleAllow() {
    if (tool.permission_request_id && onPermissionRespond) {
      void onPermissionRespond(tool.permission_request_id, "allow");
    } else if (onApprove) {
      onApprove(tool.tool_name);
    }
  }

  function handleDeny() {
    if (tool.permission_request_id && onPermissionRespond) {
      void onPermissionRespond(tool.permission_request_id, "deny");
    }
  }
</script>

{#if isPurePermission && !showPermissionInPanel}
  <ApprovalCard
    toolName={tool.tool_name}
    command={commandText}
    argsRaw={rawArgs}
    onAllow={handleAllow}
    onDeny={handleDeny}
  />
{:else}
  <div
    class="my-2 w-full rounded-xl border border-primary/20 bg-background/80 shadow-xs backdrop-blur-xs"
  >
    <InlineToolCard
      {tool}
      {subTimeline}
      {runId}
      {fetchToolResult}
      {onAnswer}
      {onApprove}
      {onPermissionRespond}
      {onExitPlanClearContext}
      {taskNotifications}
      {planContent}
      {showPermissionInPanel}
      {agentDisplayName}
      {onPreviewFile}
    />
  </div>
{/if}
