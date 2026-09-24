<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { PiExtensionWidget } from "$lib/types";

  type SnapshotNode = {
    id: string;
    label: string;
    state: string;
    activity?: {
      currentTool?: string;
      toolCount?: number;
      turnCount?: number;
    };
    children: SnapshotNode[];
  };

  type Snapshot = {
    runs: SnapshotNode[];
    omittedRuns: number;
    omittedChildren: number;
  };

  type WidgetContent =
    | { kind: "plain" }
    | { kind: "invalid" }
    | { kind: "snapshot"; value: Snapshot };

  let { widget }: { widget: PiExtensionWidget } = $props();

  const snapshotPrefix = "PI_SUBAGENT_ASYNC_JSON:";

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  function parseNode(value: unknown): SnapshotNode | null {
    if (
      !isRecord(value) ||
      typeof value.id !== "string" ||
      typeof value.label !== "string" ||
      typeof value.state !== "string"
    ) {
      return null;
    }

    const activity = isRecord(value.activity) ? value.activity : undefined;
    const children = Array.isArray(value.children)
      ? value.children.map(parseNode).filter((child): child is SnapshotNode => child !== null)
      : [];

    return {
      id: value.id,
      label: value.label,
      state: value.state,
      ...(activity
        ? {
            activity: {
              ...(typeof activity.currentTool === "string"
                ? { currentTool: activity.currentTool }
                : {}),
              ...(typeof activity.toolCount === "number" ? { toolCount: activity.toolCount } : {}),
              ...(typeof activity.turnCount === "number" ? { turnCount: activity.turnCount } : {}),
            },
          }
        : {}),
      children,
    };
  }

  function decodeWidget(lines: string[]): WidgetContent {
    const line = lines.find((item) => item.trimStart().startsWith(snapshotPrefix));
    if (!line) return { kind: "plain" };

    try {
      const payload = JSON.parse(line.trimStart().slice(snapshotPrefix.length)) as unknown;
      if (
        !isRecord(payload) ||
        payload.kind !== "pi-subagents.async-status-snapshot" ||
        payload.version !== 1 ||
        !Array.isArray(payload.runs)
      ) {
        return { kind: "invalid" };
      }

      const omitted = isRecord(payload.omitted) ? payload.omitted : {};
      return {
        kind: "snapshot",
        value: {
          runs: payload.runs.map(parseNode).filter((node): node is SnapshotNode => node !== null),
          omittedRuns: typeof omitted.runs === "number" ? omitted.runs : 0,
          omittedChildren: typeof omitted.children === "number" ? omitted.children : 0,
        },
      };
    } catch {
      return { kind: "invalid" };
    }
  }

  function flatten(nodes: SnapshotNode[], depth = 0): Array<{ node: SnapshotNode; depth: number }> {
    const rows: Array<{ node: SnapshotNode; depth: number }> = [];
    for (const node of nodes) {
      rows.push({ node, depth });
      if (rows.length < 100 && node.children.length > 0) {
        rows.push(...flatten(node.children, depth + 1).slice(0, 100 - rows.length));
      }
      if (rows.length >= 100) break;
    }
    return rows;
  }

  function statusLabel(state: string): string {
    const labels: Record<string, string> = {
      queued: t("pi_subagents_queued"),
      running: t("pi_subagents_running"),
      complete: t("pi_subagents_complete"),
      completed: t("pi_subagents_complete"),
      failed: t("pi_subagents_failed"),
      paused: t("pi_subagents_paused"),
      stopped: t("pi_subagents_stopped"),
      rejected: t("pi_subagents_rejected"),
    };
    return labels[state] ?? state;
  }

  function statusClasses(state: string): string {
    switch (state) {
      case "running":
        return "bg-blue-500/10 text-blue-700 dark:text-blue-300";
      case "complete":
      case "completed":
        return "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
      case "failed":
      case "rejected":
        return "bg-red-500/10 text-red-700 dark:text-red-300";
      case "paused":
        return "bg-amber-500/10 text-amber-700 dark:text-amber-300";
      default:
        return "bg-muted text-muted-foreground";
    }
  }

  function activityLabel(node: SnapshotNode): string {
    const parts: string[] = [];
    if (node.activity?.currentTool) parts.push(node.activity.currentTool);
    if (typeof node.activity?.toolCount === "number") {
      parts.push(t("pi_subagents_tool_count", { count: String(node.activity.toolCount) }));
    }
    if (typeof node.activity?.turnCount === "number") {
      parts.push(t("pi_subagents_turn_count", { count: String(node.activity.turnCount) }));
    }
    return parts.join(" · ");
  }

  const content = $derived(decodeWidget(widget.lines));
  const rows = $derived(content.kind === "snapshot" ? flatten(content.value.runs) : []);
</script>

{#if content.kind === "snapshot"}
  <div class="space-y-2">
    <div class="font-medium text-[11px] text-muted-foreground">{t("pi_subagents_status")}</div>
    {#if rows.length > 0}
      <div class="space-y-1" aria-live="polite">
        {#each rows as row, index (`${row.node.id}-${index}`)}
          <div
            class="flex min-w-0 items-center gap-2 rounded-md bg-background/45 px-2 py-1"
            style={`padding-left: ${8 + Math.min(row.depth, 5) * 14}px`}
          >
            <span class="min-w-0 truncate font-medium text-foreground/85">{row.node.label}</span>
            <span
              class="shrink-0 rounded px-1.5 py-0.5 text-[10px] {statusClasses(row.node.state)}"
            >
              {statusLabel(row.node.state)}
            </span>
            {#if activityLabel(row.node)}
              <span class="min-w-0 truncate text-muted-foreground">{activityLabel(row.node)}</span>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="text-muted-foreground">{t("pi_subagents_empty")}</div>
    {/if}
    {#if content.value.omittedRuns > 0 || content.value.omittedChildren > 0}
      <div class="text-[10px] text-muted-foreground">
        {t("pi_subagents_omitted", {
          count: String(content.value.omittedRuns + content.value.omittedChildren),
        })}
      </div>
    {/if}
  </div>
{:else if content.kind === "invalid"}
  <div class="text-muted-foreground">{t("pi_subagents_unavailable")}</div>
{:else}
  <pre
    class="max-h-32 overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-5 text-muted-foreground">{widget.lines.join(
      "\n",
    )}</pre>
{/if}
