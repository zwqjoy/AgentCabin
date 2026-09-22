<script lang="ts">
  import type {
    PiFeatureCapabilities,
    PiGoalState,
    PiPermissionState,
    PiPlanState,
  } from "$lib/types";

  let {
    capabilities,
    plan,
    goal,
    permission,
    pending = null,
    busy = false,
    onPlan,
    onGoal,
    onPermission,
    onTree,
  }: {
    capabilities: PiFeatureCapabilities | null;
    plan: PiPlanState;
    goal: PiGoalState;
    permission: PiPermissionState;
    pending?:
      | "plan_entering"
      | "plan_exiting"
      | "goal_starting"
      | "goal_pausing"
      | "goal_resuming"
      | "goal_clearing"
      | null;
    busy?: boolean;
    onPlan?: (action: "enter" | "exit") => void;
    onGoal?: () => void;
    onPermission?: () => void;
    onTree?: () => void;
  } = $props();

  const planLabel = $derived(
    pending === "plan_entering"
      ? "Plan: entering…"
      : pending === "plan_exiting"
        ? "Plan: exiting…"
        : plan.phase === "active"
          ? "Plan: Active"
          : "Plan",
  );
  const goalLabel = $derived(
    pending === "goal_starting"
      ? "Goal: starting…"
      : pending === "goal_pausing"
        ? "Goal: pausing…"
        : pending === "goal_resuming"
          ? "Goal: resuming…"
          : pending === "goal_clearing"
            ? "Goal: clearing…"
            : goal.phase === "unavailable"
              ? "Goal unavailable"
              : `Goal: ${goal.phase}`,
  );
  const permissionLabel = $derived(
    permission.mode === "auto_approve"
      ? "Bypass"
      : permission.mode === "accept_edits"
        ? "Accept Edits"
        : permission.mode === "custom"
          ? "Custom"
          : "Ask",
  );
</script>

{#if capabilities}
  <div class="mx-auto flex w-full max-w-4xl flex-wrap items-center gap-1.5 px-4 py-1.5 text-[11px]">
    {#if capabilities.planAvailable}
      <button
        class="rounded-full border border-border/70 bg-card px-2.5 py-1 text-muted-foreground transition hover:border-primary/50 hover:text-foreground disabled:opacity-50"
        disabled={busy || pending !== null || plan.phase === "entering" || plan.phase === "exiting"}
        title={plan.detail ?? "Plan state comes from the Pi extension state entry"}
        onclick={() => onPlan?.(plan.phase === "active" ? "exit" : "enter")}>{planLabel}</button
      >
    {/if}
    {#if capabilities.goalAvailable}
      <button
        class="rounded-full border border-border/70 bg-card px-2.5 py-1 text-muted-foreground transition hover:border-primary/50 hover:text-foreground disabled:opacity-50"
        disabled={busy || pending !== null || goal.phase === "unavailable"}
        onclick={onGoal}>{goalLabel}</button
      >
    {/if}
    {#if capabilities.permissionAvailable}
      <button
        class="rounded-full border border-border/70 bg-card px-2.5 py-1 text-muted-foreground transition hover:border-primary/50 hover:text-foreground"
        title={permission.restartRequired ? "Restart Pi session to apply" : "Pi permission mode"}
        onclick={onPermission}>Permission: {permissionLabel}</button
      >
    {/if}
    {#if capabilities.sessionTreeAvailable}
      <button
        class="rounded-full border border-border/70 bg-card px-2.5 py-1 text-muted-foreground transition hover:border-primary/50 hover:text-foreground"
        onclick={onTree}>Tree</button
      >
    {/if}
  </div>
{/if}
