import { describe, expect, it } from "vitest";
import {
  BUILTIN_PET_OPTIONS,
  DEFAULT_PET_SETTINGS,
  normalizePetScale,
  petSettingsPatch,
  resolvePetSettings,
} from "./pet-settings";

describe("desktop pet settings", () => {
  it("exposes the built-in pet choices", () => {
    expect(BUILTIN_PET_OPTIONS.map((pet) => pet.id)).toEqual([
      "agentcabin-bot",
      "clawd",
      "cache-capy",
      "grokbot",
    ]);
  });

  it("uses the MVP defaults when settings are absent", () => {
    expect(resolvePetSettings()).toEqual(DEFAULT_PET_SETTINGS);
    expect(DEFAULT_PET_SETTINGS).toEqual({
      enabled: false,
      scale: 1,
      alwaysOnTop: true,
      id: "agentcabin-bot",
      patrolEnabled: true,
      patrolPauseMin: 3,
      snapToEdge: false,
      clickInteractionEnabled: true,
      grokbotColor: "blue",
      grokbotShape: "blob",
      grokbotParts: [],
      grokbotAccessories: [],
    });
  });

  it("normalizes persisted scale values to the supported range", () => {
    expect(normalizePetScale(0.2)).toBe(0.3);
    expect(normalizePetScale(3)).toBe(2);
    expect(normalizePetScale(Number.NaN)).toBe(1);
    expect(
      resolvePetSettings({
        pet_enabled: true,
        pet_scale: 1.25,
        pet_always_on_top: false,
        pet_id: "clawd",
        pet_patrol_enabled: false,
        pet_patrol_pause_min: 8,
        pet_snap_to_edge: true,
        pet_click_interaction_enabled: false,
        pet_grokbot_color: "pink",
        pet_grokbot_shape: "cloud",
        pet_grokbot_parts: ["antenna"],
        pet_grokbot_accessories: ["glasses"],
      }),
    ).toEqual({
      enabled: true,
      scale: 1.25,
      alwaysOnTop: false,
      id: "clawd",
      patrolEnabled: false,
      patrolPauseMin: 8,
      snapToEdge: true,
      clickInteractionEnabled: false,
      grokbotColor: "pink",
      grokbotShape: "cloud",
      grokbotParts: ["antenna"],
      grokbotAccessories: ["glasses"],
    });
  });

  it("builds a persistent settings patch with the Rust field names", () => {
    expect(
      petSettingsPatch({
        enabled: true,
        scale: 1.25,
        alwaysOnTop: false,
        id: "clawd",
        patrolEnabled: true,
        patrolPauseMin: 6,
        snapToEdge: true,
        clickInteractionEnabled: false,
        grokbotColor: "pink",
        grokbotShape: "cloud",
        grokbotParts: ["antenna"],
        grokbotAccessories: ["glasses"],
      }),
    ).toEqual({
      pet_enabled: true,
      pet_scale: 1.25,
      pet_always_on_top: false,
      pet_id: "clawd",
      pet_patrol_enabled: true,
      pet_patrol_pause_min: 6,
      pet_snap_to_edge: true,
      pet_click_interaction_enabled: false,
      pet_grokbot_color: "pink",
      pet_grokbot_shape: "cloud",
      pet_grokbot_parts: ["antenna"],
      pet_grokbot_accessories: ["glasses"],
    });
  });
});
