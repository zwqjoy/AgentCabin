import type { SessionPhase } from "$lib/stores/types";

export type PetMode = "idle" | "waiting" | "running" | "review" | "failed";

export interface PetAggregateState {
  mode: PetMode;
  runningCount: number;
  errorCount: number;
  activeRunId: string | null;
  timestamp: number;
}

export interface PetNotification {
  type: "done" | "error" | "info";
  text: string;
  agentId?: string;
  timestamp: number;
}

export interface PetSettingsPayload {
  petEnabled: boolean;
  petScale: number;
  petAlwaysOnTop: boolean;
  petId: string;
  petPatrolEnabled: boolean;
  petPatrolPauseMin: number;
  petSnapToEdge: boolean;
  petClickInteractionEnabled: boolean;
  petGrokbotColor: string;
  petGrokbotShape: string;
  petGrokbotParts: string[];
  petGrokbotAccessories: string[];
}

export interface PetManifest {
  id: string;
  displayName: string;
  description: string;
  spritesheetPath?: string;
  spritesheetUrl?: string;
  gridCols?: number;
  gridRows?: number;
  cellWidth?: number;
  cellHeight?: number;
  singleFrame?: boolean;
  animationUrls?: Record<PetMode, string>;
  custom?: boolean;
}

export const PET_EVENTS = {
  READY: "pet://ready",
  STATE: "pet://state",
  NOTIFICATION: "pet://notification",
  SETTINGS: "pet://settings",
} as const;

export const SESSION_PHASE_TO_PET_MODE: Record<SessionPhase, PetMode> = {
  empty: "idle",
  ready: "idle",
  idle: "idle",
  stopped: "idle",
  loading: "waiting",
  spawning: "waiting",
  running: "running",
  completed: "review",
  failed: "failed",
};
