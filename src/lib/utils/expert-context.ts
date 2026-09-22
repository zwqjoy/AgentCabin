/**
 * WorkBuddy expert context utilities for extracting, cleaning, and formatting
 * expert information associated with chat turns and task runs.
 */

export interface ActiveExpertContext {
  id?: string;
  title: string;
  isTeam?: boolean;
  avatarChar?: string;
  avatarBg?: string;
}

const EXPERT_TAG_REGEX = /^\[当前协作专家:\s*([^\]]+)\]\s*\n?/;

/**
 * Parses out any leading `[当前协作专家: ...]` tag from text,
 * returning the clean user-facing text and structured expert info.
 */
export function parseExpertFromText(text?: string | null): {
  cleanText: string;
  expert: ActiveExpertContext | null;
} {
  if (!text) return { cleanText: "", expert: null };
  const match = text.match(EXPERT_TAG_REGEX);
  if (!match) return { cleanText: text, expert: null };

  const rawTitle = match[1].trim();
  const isTeam = rawTitle.includes("专家团队") || rawTitle.includes("专家团");
  const cleanTitle = rawTitle.replace(/\s*\(专家团队\)|\s*\(专家团\)/g, "").trim();
  const avatarChar = cleanTitle.slice(0, 1) || "专";

  return {
    cleanText: text.replace(EXPERT_TAG_REGEX, "").trim(),
    expert: {
      title: cleanTitle,
      isTeam,
      avatarChar,
      avatarBg: isTeam ? "bg-indigo-600" : "bg-rose-500",
    },
  };
}

/**
 * Strips any leading `[当前协作专家: ...]` tag from user prompt/title.
 */
export function stripExpertTag(text?: string | null): string {
  if (!text) return "";
  return text.replace(EXPERT_TAG_REGEX, "").trim();
}

/**
 * Determines whether a skill is an internal WorkBuddy expert/expert-team synthetic skill.
 * Expert skills are bound to specific experts and should not appear in the generic "Skills" menu.
 */
export function isExpertSkill(
  skill?: { name?: string; id?: string; description?: string } | null,
): boolean {
  if (!skill) return false;
  const name = skill.name || "";
  const id = skill.id || "";
  const desc = skill.description || "";

  if (
    name.startsWith("agent-plugin--") &&
    (name.includes("--expert--") || name.includes("--expert-team--"))
  ) {
    return true;
  }
  if (
    id.startsWith("agent-plugin--") &&
    (id.includes("--expert--") || id.includes("--expert-team--"))
  ) {
    return true;
  }
  if (desc.startsWith("WorkBuddy expert:") || desc.startsWith("WorkBuddy expert team:")) {
    return true;
  }
  return false;
}

/**
 * Return the public DSH skill name for a WorkBuddy expert plugin id.
 *
 * AgentCabin's persisted component ids are intentionally namespaced with
 * `--`, but DSH accepts only kebab-case skill names. Keep this mapping in one
 * place so the Code composer can explicitly invoke the selected expert.
 */
export function dshExpertSkillName(pluginId?: string | null): string {
  const normalized = (pluginId || "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return normalized ? `agentcabin-expert-${normalized}` : "agentcabin-expert";
}
