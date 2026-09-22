import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GrokBotEngine } from "./GrokBotEngine";
import {
  COLORS,
  EXPRESSIONS,
  GROKBOT_ACCESSORIES,
  GROKBOT_PARTS,
  GROKBOT_STATE_GROUPS,
  PET_MODE_TO_GROKBOT_STATE,
  POOLS,
  SHAPES,
} from "./grokbot-data";
import type { PetMode } from "./types";

describe("GrokBotEngine", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("initializes with 2 rings of expressions", () => {
    expect(EXPRESSIONS.length).toBe(25);
    expect(EXPRESSIONS[0].length).toBe(2);
    expect(EXPRESSIONS[0][0].length).toBe(48);
  });

  it("keeps the upstream customization catalog complete", () => {
    expect(SHAPES).toHaveLength(8);
    expect(COLORS).toHaveLength(10);
    expect(GROKBOT_PARTS.map((part) => part.id)).toEqual(["hands", "feet", "tail", "antenna"]);
    expect(GROKBOT_ACCESSORIES.map((accessory) => accessory.id)).toEqual([
      "straw-hat",
      "glasses",
      "bowtie",
      "cape",
    ]);
    expect(GROKBOT_STATE_GROUPS.flatMap((group) => group.states)).toHaveLength(39);
  });

  it("maps PetModes to appropriate GrokBot state keys", () => {
    const modes: PetMode[] = ["idle", "waiting", "running", "review", "failed"];
    for (const mode of modes) {
      const stateKey = PET_MODE_TO_GROKBOT_STATE[mode];
      expect(stateKey).toBeDefined();
      expect(POOLS[stateKey]).toBeDefined();
    }
  });

  it("dispatches frames to callback and manages lifecycle", () => {
    const engine = new GrokBotEngine(() => {});

    // Advance animation frame
    vi.advanceTimersByTime(100);

    engine.setPetMode("running");
    engine.setGaze(0.5, -0.3);
    engine.clearGaze();

    engine.destroy();
  });

  it("triggers quick actions on host element", () => {
    const classes = new Set<string>();
    const mockEl = {
      classList: {
        add: (c: string) => classes.add(c),
        remove: (c: string) => classes.delete(c),
        contains: (c: string) => classes.has(c),
      },
      offsetWidth: 100,
    } as unknown as HTMLElement;

    const engine = new GrokBotEngine(() => {}, mockEl);

    engine.triggerAction("bounce");
    expect(mockEl.classList.contains("grokbot-quick-bounce")).toBe(true);

    vi.advanceTimersByTime(1000);
    expect(mockEl.classList.contains("grokbot-quick-bounce")).toBe(false);

    engine.destroy();
  });

  it("does not repeat the same random action consecutively", () => {
    const classes = new Set<string>();
    const mockEl = {
      classList: {
        add: (c: string) => classes.add(c),
        remove: (c: string) => classes.delete(c),
        contains: (c: string) => classes.has(c),
      },
      offsetWidth: 100,
    } as unknown as HTMLElement;
    const engine = new GrokBotEngine(() => {}, mockEl);

    const first = engine.triggerRandomAction();
    const second = engine.triggerRandomAction();
    expect(first).not.toBe(second);
    expect([...classes].some((name) => name.startsWith("grokbot-quick-"))).toBe(true);

    engine.destroy();
  });
});
