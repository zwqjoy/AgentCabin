import { describe, expect, it, vi } from "vitest";
import records from "./__fixtures__/work-subagents.json";
import { areSubagentRecordsEqual, projectWorkSubagent } from "./work-subagents";
import { listWorkSubagents } from "$lib/api/work";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("$lib/transport", () => ({ getTransport: () => ({ invoke }) }));

describe("Work subagent host contract", () => {
  it("preserves three distinct render keys through the host API and projection", async () => {
    // The Rust serialization test verifies this same fixture against the host DTO.
    invoke.mockResolvedValue(structuredClone(records));
    const result = await listWorkSubagents("parent-run");
    const summaries = result.map((record) => projectWorkSubagent(record, []));
    expect(summaries.map((item) => item.id)).toEqual(["agent-0", "agent-1", "agent-2"]);
    expect(summaries.every((item) => item.agentId === item.id)).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("work_list_subagents", {
      parentScope: "parent-run",
      taskId: null,
    });
  });

  it("detects identity replacement and result-only updates", () => {
    expect(areSubagentRecordsEqual(records, structuredClone(records))).toBe(true);
    expect(
      areSubagentRecordsEqual(
        records,
        records.map((r) => ({ ...r, agentId: `${r.agentId}-new` })),
      ),
    ).toBe(false);
    expect(
      areSubagentRecordsEqual(
        records,
        records.map((r) => ({ ...r, resultSummary: "调研完成" })),
      ),
    ).toBe(false);
    expect(
      projectWorkSubagent({ ...records[0], status: "completed", resultSummary: "调研完成" }, []),
    ).toMatchObject({
      id: "agent-0",
      status: "completed",
      resultSummary: "调研完成",
    });
  });

  it.each([null, [null], [{ agent_id: "legacy-id" }], [{ agentId: "" }], [records[0], records[0]]])(
    "rejects invalid identities before they can reach a keyed render: %j",
    async (payload) => {
      invoke.mockResolvedValue(payload);
      await expect(listWorkSubagents("parent-run")).rejects.toThrow();
    },
  );
});
