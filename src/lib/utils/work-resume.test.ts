import { describe, expect, it } from "vitest";
import { canResumeWorkSession } from "$lib/utils/work-resume";

describe("canResumeWorkSession", () => {
  const run = { id: "run-1", status: "stopped" as const };

  it.each(["idle", "running", "completed", "failed", "stopped"] as const)(
    "allows %s runs to resume when the actor is gone",
    (status) => {
      expect(canResumeWorkSession({ ...run, status }, false, true, false)).toBe(true);
    },
  );

  it.each(["pending", "cancelled"] as const)("does not resume %s runs", (status) => {
    expect(canResumeWorkSession({ ...run, status }, false, true, false)).toBe(false);
  });

  it("keeps live, unsupported, read-only, and missing runs out of the resume path", () => {
    expect(canResumeWorkSession(run, true, true, false)).toBe(false);
    expect(canResumeWorkSession(run, false, false, false)).toBe(false);
    expect(canResumeWorkSession(run, false, true, true)).toBe(false);
    expect(canResumeWorkSession(null, false, true, false)).toBe(false);
  });
});
