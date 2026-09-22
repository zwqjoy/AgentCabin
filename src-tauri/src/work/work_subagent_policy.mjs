import { Type } from "typebox";

// Work owns these roles and limits. A provider may translate the resulting
// capability ceiling, but it must not redefine the Work policy.
export const SUBAGENT_ROLES = Object.freeze(["researcher", "worker", "reviewer"]);

export const CANONICAL_AGENT_NAMES = Object.freeze({
  researcher: "agentcabin-researcher",
  worker: "agentcabin-worker",
  reviewer: "agentcabin-reviewer",
});

export const SUBAGENT_WEB_TOOLS = Object.freeze([
  "web_search",
  "web_open",
  "web_extract",
  "web_cite",
]);

export const ALLOWED_TOOLS_BY_ROLE = Object.freeze({
  researcher: Object.freeze(["read", ...SUBAGENT_WEB_TOOLS]),
  reviewer: Object.freeze(["read"]),
  worker: Object.freeze(["read", "write", "edit", "bash"]),
});

export const ALL_SUBAGENT_TOOLS = Object.freeze([
  ...new Set(Object.values(ALLOWED_TOOLS_BY_ROLE).flat()),
]);

export const WORK_SUBAGENT_LIMITS = Object.freeze({
  maxConcurrentChildren: 3,
  maxConcurrentWorkers: 1,
  maxChildrenPerTurn: 3,
  maxTotalChildrenPerRun: 8,
});

export const TERMINAL_CHILD_STATUSES = new Set([
  "completed",
  "failed",
  "stopped",
  "interrupted",
]);

export const DelegateSchema = Type.Object({
  role: Type.Union([
    Type.Literal("researcher"),
    Type.Literal("worker"),
    Type.Literal("reviewer"),
  ], { description: "Subagent role: researcher (read-only local and public-web investigation), worker (implementation/validation), or reviewer (code review/read-only)." }),
  task: Type.String({ description: "Specific, self-contained description of the task for the subagent." }),
});

export const ResearchSwarmSchema = Type.Object({
  objective: Type.String({ description: "Overall investigation objective to be decomposed and researched across parallel dimensions." }),
  tasks: Type.Array(
    Type.Object({
      label: Type.String({ description: "Unique short dimension label (e.g. 'architecture', 'security', 'runtime')." }),
      task: Type.String({ description: "Specific, self-contained research question/task for this dimension." }),
    }),
    { description: "2 to 3 independent, orthogonal research dimensions." },
  ),
  timeout_seconds: Type.Optional(
    Type.Integer({ description: "Maximum seconds to wait for all researchers to complete (default 180, range 1-600).", minimum: 1, maximum: 600 }),
  ),
});

export const ImplementReviewFixSchema = Type.Object({
  task: Type.String({ description: "Implementation task description for the worker subagent." }),
  review_focus: Type.Optional(Type.String({ description: "Optional specific areas, requirements, or invariants to emphasize during review." })),
  timeout_seconds: Type.Optional(
    Type.Integer({ description: "Maximum seconds to wait per stage (default 180, range 1-600).", minimum: 1, maximum: 600 }),
  ),
});

export const AgentStatusSchema = Type.Object({
  agent_id: Type.Optional(Type.String({ description: "Optional agent ID to inspect. If omitted, returns active subagents." })),
});

export const AgentWaitSchema = Type.Object({
  agent_id: Type.Optional(Type.String({ description: "Agent ID to wait on. If omitted, waits on the current active subagent." })),
  timeout_seconds: Type.Optional(Type.Integer({ description: "Maximum seconds to wait (default 120).", minimum: 1, maximum: 600 })),
});

export const AgentSteerSchema = Type.Object({
  agent_id: Type.String({ description: "Agent ID of the running subagent to steer." }),
  message: Type.String({ description: "Steering instruction or additional context for the running subagent." }),
});

export const AgentStopSchema = Type.Object({
  agent_id: Type.String({ description: "Agent ID of the running subagent to stop." }),
});

export function roleTools(role) {
  return ALLOWED_TOOLS_BY_ROLE[role] ? [...ALLOWED_TOOLS_BY_ROLE[role]] : [];
}

export function activeChildrenDetails(activeChildren) {
  const children = Array.from(activeChildren.entries()).map(([id, info]) => ({
    id,
    role: info.role,
  }));
  return {
    children,
    list: children.map(({ id, role }) => `${id} (${role})`).join(", "),
  };
}

export function validateDelegateRequest(role, task) {
  if (!SUBAGENT_ROLES.includes(role)) {
    return {
      ok: false,
      message: `Invalid role '${role}'. Allowed roles: ${SUBAGENT_ROLES.join(", ")}.`,
    };
  }
  if (!task) {
    return { ok: false, message: "Task description cannot be empty." };
  }
  return { ok: true };
}

export function evaluateDelegateAdmission({ role, activeChildren, turnSpawnCount, totalSpawnCount }) {
  const { children, list } = activeChildrenDetails(activeChildren);
  if (activeChildren.size >= WORK_SUBAGENT_LIMITS.maxConcurrentChildren) {
    return {
      ok: false,
      message: `Subagents limit reached (${WORK_SUBAGENT_LIMITS.maxConcurrentChildren} active: [${list}]). Call work_agent_wait or work_agent_stop before delegating a new task.`,
      details: { active_children: children },
    };
  }

  if (role === "worker") {
    const activeWorker = Array.from(activeChildren.entries()).find(([, info]) => info.role === "worker");
    if (activeWorker) {
      return {
        ok: false,
        message: `Worker subagent '${activeWorker[0]}' is currently running. Only ${WORK_SUBAGENT_LIMITS.maxConcurrentWorkers} concurrent worker is allowed. Wait for it to finish before delegating another worker task.`,
        details: { active_worker_id: activeWorker[0] },
      };
    }
  }

  if (turnSpawnCount >= WORK_SUBAGENT_LIMITS.maxChildrenPerTurn) {
    return {
      ok: false,
      message: `Exceeded max child spawns per turn (${WORK_SUBAGENT_LIMITS.maxChildrenPerTurn}).`,
    };
  }
  if (totalSpawnCount >= WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun) {
    return {
      ok: false,
      message: `Exceeded total max child spawns per run (${WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun}).`,
    };
  }
  return { ok: true };
}

export function evaluateResearchSwarmAdmission({ count, activeChildren, turnSpawnCount, totalSpawnCount }) {
  const { children, list } = activeChildrenDetails(activeChildren);
  if (activeChildren.size + count > WORK_SUBAGENT_LIMITS.maxConcurrentChildren) {
    return {
      ok: false,
      message: `Cannot launch swarm of ${count} researchers: would exceed MAX_CONCURRENT_CHILDREN (${WORK_SUBAGENT_LIMITS.maxConcurrentChildren}). Currently active: [${list}].`,
      details: { active_children: children },
    };
  }
  if (turnSpawnCount + count > WORK_SUBAGENT_LIMITS.maxChildrenPerTurn) {
    return {
      ok: false,
      message: `Launching swarm would exceed max child spawns per turn (${WORK_SUBAGENT_LIMITS.maxChildrenPerTurn}).`,
    };
  }
  if (totalSpawnCount + count > WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun) {
    return {
      ok: false,
      message: `Launching swarm would exceed total max child spawns per run (${WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun}).`,
    };
  }
  return { ok: true };
}

export function evaluateImplementReviewFixAdmission({ activeChildren, turnSpawnCount, totalSpawnCount }) {
  const { children, list } = activeChildrenDetails(activeChildren);
  if (activeChildren.size > 0) {
    return {
      ok: false,
      message: `Cannot start Implement → Review → Fix while other subagents are active (${activeChildren.size} active: [${list}]). Call work_agent_wait or work_agent_stop before starting.`,
      details: { active_children: children },
    };
  }
  if (turnSpawnCount > 0) {
    return {
      ok: false,
      message: "Cannot start Implement → Review → Fix when subagents have already been spawned in this turn.",
    };
  }

  const requiredSpawns = 4;
  if (totalSpawnCount + requiredSpawns > WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun) {
    return {
      ok: false,
      message: `Cannot start Implement → Review → Fix: insufficient total run spawn budget (requires ${requiredSpawns}, available ${WORK_SUBAGENT_LIMITS.maxTotalChildrenPerRun - totalSpawnCount}).`,
    };
  }
  return { ok: true };
}

export function parseReviewVerdict(text) {
  if (typeof text !== "string") return null;
  const lines = text.split("\n");
  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) continue;
    if (/^VERDICT:\s*PASS$/i.test(line)) return "pass";
    if (/^VERDICT:\s*NEEDS_CHANGES$/i.test(line)) return "needs_changes";
  }
  const match = text.match(/\bVERDICT:\s*(PASS|NEEDS_CHANGES)\b/i);
  if (match) return match[1].toUpperCase() === "PASS" ? "pass" : "needs_changes";
  return null;
}
