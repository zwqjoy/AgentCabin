import type { StandaloneSkill } from "$lib/types";

export interface ChatSkillItem {
  name: string;
  description: string;
}

export interface CodexChatSkillItem extends ChatSkillItem {
  path: string;
}

/**
 * Return only skills enabled by the global Capability Center binding.
 *
 * A disabled entry must never leak into the pre-session UI for either runtime.
 */
export function getEnabledCapabilitySkills(skills: StandaloneSkill[]): StandaloneSkill[] {
  return skills.filter((skill) => skill.enabled === true);
}

export function toCapabilitySkillItems(skills: StandaloneSkill[]): ChatSkillItem[] {
  return getEnabledCapabilitySkills(skills).map((skill) => ({
    name: skill.name,
    description: skill.description ?? "",
  }));
}

export function toCodexCapabilitySkillItems(skills: StandaloneSkill[]): CodexChatSkillItem[] {
  return getEnabledCapabilitySkills(skills)
    .filter((skill) => skill.path.trim().length > 0)
    .map((skill) => ({
      name: skill.name,
      path: skill.path,
      description: skill.description ?? "",
    }));
}
