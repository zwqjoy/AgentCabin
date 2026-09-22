import type { UserSettings } from "$lib/types";
import {
  COLORS,
  GROKBOT_ACCESSORIES,
  GROKBOT_PARTS,
  SHAPES,
  type ColorId,
  type GrokBotAccessoryId,
  type GrokBotPartId,
  type ShapeId,
} from "./grokbot-data";

export const PET_SCALE_MIN = 0.3;
export const PET_SCALE_MAX = 2;
export const PET_PATROL_PAUSE_MIN = 1;
export const PET_PATROL_PAUSE_MAX = 30;

export interface BuiltinPetOption {
  id: string;
  displayName: string;
  description: string;
  assetPath: string;
  singleFrame?: boolean;
}

/**
 * Built-in pets are shipped with the frontend so the independent Tauri window
 * can load them without a filesystem bridge. The PiDeck spritesheets are
 * redistributed under the MIT notice in static/pets/PIDeck-LICENSE.txt.
 */
export const BUILTIN_PET_OPTIONS: readonly BuiltinPetOption[] = [
  {
    id: "agentcabin-bot",
    displayName: "AgentCabin Bot",
    description: "The little AgentCabin mascot from the app splash screen.",
    assetPath: "/logo.png",
    singleFrame: true,
  },
  {
    id: "clawd",
    displayName: "Clawd",
    description: "A tiny pixel Clawd companion made from sticker GIFs.",
    assetPath: "/pets/clawd-3/spritesheet.webp",
  },
  {
    id: "cache-capy",
    displayName: "Cache Capy",
    description: "A calm capybara carrying a tiny cache box for patient builds.",
    assetPath: "/pets/cache-capy/spritesheet.webp",
  },
  {
    id: "grokbot",
    displayName: "GrokBot",
    description:
      "来自 LaoA-GrokBot 的果冻感 SVG 宠物 — 25 套表情、8 种形态、6 种弹性动作，与 AI 状态实时联动。",
    assetPath: "",
    singleFrame: true, // no spritesheet; rendered by GrokBotPet.svelte
  },
] as const;

export const DEFAULT_PET_ID = "agentcabin-bot";

export interface PetSettings {
  enabled: boolean;
  scale: number;
  alwaysOnTop: boolean;
  id: string;
  patrolEnabled: boolean;
  patrolPauseMin: number;
  snapToEdge: boolean;
  clickInteractionEnabled: boolean;
  grokbotColor: ColorId;
  grokbotShape: ShapeId;
  grokbotParts: GrokBotPartId[];
  grokbotAccessories: GrokBotAccessoryId[];
}

export const DEFAULT_PET_SETTINGS: PetSettings = {
  enabled: false,
  scale: 1,
  alwaysOnTop: true,
  id: DEFAULT_PET_ID,
  patrolEnabled: true,
  patrolPauseMin: 3,
  snapToEdge: false,
  clickInteractionEnabled: true,
  grokbotColor: "blue",
  grokbotShape: "blob",
  grokbotParts: [],
  grokbotAccessories: [],
};

function isColorId(value: unknown): value is ColorId {
  return COLORS.some((color) => color.id === value);
}

function isShapeId(value: unknown): value is ShapeId {
  return SHAPES.some((shape) => shape.id === value);
}

function normalizeGrokbotParts(value: unknown): GrokBotPartId[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is GrokBotPartId =>
    GROKBOT_PARTS.some((part) => part.id === item),
  );
}

function normalizeGrokbotAccessories(value: unknown): GrokBotAccessoryId[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is GrokBotAccessoryId =>
    GROKBOT_ACCESSORIES.some((accessory) => accessory.id === item),
  );
}

export function normalizePetScale(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 1;
  return Math.min(PET_SCALE_MAX, Math.max(PET_SCALE_MIN, value));
}

export function normalizePetPatrolPause(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return DEFAULT_PET_SETTINGS.patrolPauseMin;
  }
  return Math.round(Math.min(PET_PATROL_PAUSE_MAX, Math.max(PET_PATROL_PAUSE_MIN, value)));
}

export function resolvePetSettings(
  settings?: Pick<
    UserSettings,
    | "pet_enabled"
    | "pet_scale"
    | "pet_always_on_top"
    | "pet_id"
    | "pet_patrol_enabled"
    | "pet_patrol_pause_min"
    | "pet_snap_to_edge"
    | "pet_click_interaction_enabled"
    | "pet_grokbot_color"
    | "pet_grokbot_shape"
    | "pet_grokbot_parts"
    | "pet_grokbot_accessories"
  > | null,
): PetSettings {
  return {
    enabled: settings?.pet_enabled ?? DEFAULT_PET_SETTINGS.enabled,
    scale: normalizePetScale(settings?.pet_scale),
    alwaysOnTop: settings?.pet_always_on_top ?? DEFAULT_PET_SETTINGS.alwaysOnTop,
    id: settings?.pet_id || DEFAULT_PET_SETTINGS.id,
    patrolEnabled: settings?.pet_patrol_enabled ?? DEFAULT_PET_SETTINGS.patrolEnabled,
    patrolPauseMin: normalizePetPatrolPause(settings?.pet_patrol_pause_min),
    snapToEdge: settings?.pet_snap_to_edge ?? DEFAULT_PET_SETTINGS.snapToEdge,
    clickInteractionEnabled:
      settings?.pet_click_interaction_enabled ?? DEFAULT_PET_SETTINGS.clickInteractionEnabled,
    grokbotColor: isColorId(settings?.pet_grokbot_color)
      ? settings.pet_grokbot_color
      : DEFAULT_PET_SETTINGS.grokbotColor,
    grokbotShape: isShapeId(settings?.pet_grokbot_shape)
      ? settings.pet_grokbot_shape
      : DEFAULT_PET_SETTINGS.grokbotShape,
    grokbotParts: normalizeGrokbotParts(settings?.pet_grokbot_parts),
    grokbotAccessories: normalizeGrokbotAccessories(settings?.pet_grokbot_accessories),
  };
}

export function petSettingsPatch(
  settings: PetSettings,
): Pick<
  UserSettings,
  | "pet_enabled"
  | "pet_scale"
  | "pet_always_on_top"
  | "pet_id"
  | "pet_patrol_enabled"
  | "pet_patrol_pause_min"
  | "pet_snap_to_edge"
  | "pet_click_interaction_enabled"
  | "pet_grokbot_color"
  | "pet_grokbot_shape"
  | "pet_grokbot_parts"
  | "pet_grokbot_accessories"
> {
  return {
    pet_enabled: settings.enabled,
    pet_scale: normalizePetScale(settings.scale),
    pet_always_on_top: settings.alwaysOnTop,
    pet_id: settings.id,
    pet_patrol_enabled: settings.patrolEnabled,
    pet_patrol_pause_min: normalizePetPatrolPause(settings.patrolPauseMin),
    pet_snap_to_edge: settings.snapToEdge,
    pet_click_interaction_enabled: settings.clickInteractionEnabled,
    pet_grokbot_color: settings.grokbotColor,
    pet_grokbot_shape: settings.grokbotShape,
    pet_grokbot_parts: settings.grokbotParts,
    pet_grokbot_accessories: settings.grokbotAccessories,
  };
}
