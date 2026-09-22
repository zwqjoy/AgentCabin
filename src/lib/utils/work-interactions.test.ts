import { describe, expect, it } from "vitest";
import type { TaskRun } from "$lib/types";
import type { InboxItem } from "$lib/types/work";
import {
  buildStandingRuleFromProposal,
  filterCurrentRunInteractions,
  formatDirectoryPath,
  getDirectoryDisplayName,
  getDirectoryPathFromItem,
  getInteractionDescription,
  getInteractionTitle,
  isAccessRootRequest,
  isQuestionInteraction,
  excludeInlineInteractions,
  riskClassForTool,
} from "./work-interactions";

function makeRun(id: string, overrides: Partial<TaskRun> = {}): TaskRun {
  return {
    id,
    prompt: "test",
    cwd: "/project",
    agent: "pi",
    auth_mode: "cli",
    status: "running",
    started_at: "2026-08-15T00:00:00Z",
    execution_path: "session_actor",
    workspace_id: "ws-1",
    work_task_id: "task-1",
    work_run_id: "run-work-1",
    ...overrides,
  };
}

function makeInboxItem(id: string, overrides: Partial<InboxItem> = {}): InboxItem {
  return {
    id,
    taskId: "task-1",
    runId: "run-work-1",
    workspaceId: "ws-1",
    itemType: "access_root_request",
    status: "pending",
    title: "Approval Needed: Access Directory",
    description: "Execution is paused pending human approval.",
    payload: {
      toolName: "work_request_directory_access",
      path: "/Users/cengwenqi/crm",
      purpose: "读取客户清单以更新报告",
    },
    createdAt: "2026-08-15T01:00:00Z",
    ...overrides,
  };
}

describe("work-interactions", () => {
  describe("isQuestionInteraction", () => {
    it("recognizes question elicitation items and ask_questions payloads", () => {
      expect(
        isQuestionInteraction(
          makeInboxItem("question-1", {
            itemType: "question_elicitation",
            payload: { question: "下一步做什么？" },
          }),
        ),
      ).toBe(true);
      expect(
        isQuestionInteraction(
          makeInboxItem("question-2", {
            payload: { toolName: "ask_questions" },
          }),
        ),
      ).toBe(true);
    });

    it("does not classify approval items as questions", () => {
      expect(isQuestionInteraction(makeInboxItem("approval-1"))).toBe(false);
    });
  });

  describe("filterCurrentRunInteractions", () => {
    it("returns empty array when run is null or undefined", () => {
      const item = makeInboxItem("item-1");
      expect(filterCurrentRunInteractions([item], null)).toEqual([]);
      expect(filterCurrentRunInteractions([item], undefined)).toEqual([]);
    });

    it("filters items belonging to the current work run", () => {
      const run = makeRun("session-1", {
        workspace_id: "ws-1",
        work_task_id: "task-1",
        work_run_id: "run-work-1",
      });

      const currentItem = makeInboxItem("item-1", { runId: "run-work-1" });
      const otherRunItem = makeInboxItem("item-2", { runId: "run-work-2" });
      const otherWsItem = makeInboxItem("item-3", {
        workspaceId: "ws-2",
        runId: "run-work-1",
      });

      const result = filterCurrentRunInteractions(
        [currentItem, otherRunItem, otherWsItem],
        run,
        true,
      );
      expect(result).toHaveLength(1);
      expect(result[0].id).toBe("item-1");
    });

    it("filters only pending items when onlyPending is true", () => {
      const run = makeRun("session-1");
      const pendingItem = makeInboxItem("item-pending", {
        status: "pending",
        createdAt: "2026-08-15T01:00:00Z",
      });
      const approvedItem = makeInboxItem("item-approved", {
        status: "approved",
        createdAt: "2026-08-15T01:01:00Z",
      });

      expect(filterCurrentRunInteractions([pendingItem, approvedItem], run, true)).toEqual([
        pendingItem,
      ]);
      expect(filterCurrentRunInteractions([pendingItem, approvedItem], run, false)).toEqual([
        pendingItem,
        approvedItem,
      ]);
    });

    it("matches session run id if item.runId refers to session id", () => {
      const run = makeRun("session-123", {
        work_run_id: undefined,
        work_task_id: undefined,
      });
      const item = makeInboxItem("item-1", { runId: "session-123" });

      expect(filterCurrentRunInteractions([item], run, true)).toEqual([item]);
    });

    it("removes only items already rendered inline in the current conversation", () => {
      const inlinePermission = makeInboxItem("inline-permission", {
        itemType: "permission_request",
        payload: {
          toolName: "work_run_connector_cli",
          toolCallId: "tool-call-1",
          parameters: { operation: "sendMessage" },
        },
      });
      const standaloneQuestion = makeInboxItem("standalone-question", {
        itemType: "question_elicitation",
        payload: { question: "请确认收件人" },
      });

      expect(
        excludeInlineInteractions(
          [inlinePermission, standaloneQuestion],
          new Set([inlinePermission.id]),
        ),
      ).toEqual([standaloneQuestion]);
    });
  });

  describe("formatDirectoryPath", () => {
    it("replaces macOS /Users/username with ~", () => {
      expect(formatDirectoryPath("/Users/cengwenqi/crm")).toBe("~/crm");
      expect(formatDirectoryPath("/Users/alex/projects/foo")).toBe("~/projects/foo");
    });

    it("replaces Linux /home/username with ~", () => {
      expect(formatDirectoryPath("/home/user/data")).toBe("~/data");
    });

    it("handles empty or other paths correctly", () => {
      expect(formatDirectoryPath("")).toBe("");
      expect(formatDirectoryPath("/var/data")).toBe("/var/data");
    });
  });

  describe("getDirectoryDisplayName", () => {
    it("returns friendly names for common directories", () => {
      expect(getDirectoryDisplayName("/Users/test/crm")).toBe("客户数据目录");
      expect(getDirectoryDisplayName("/Users/test/sales")).toBe("销售数据目录");
      expect(getDirectoryDisplayName("/Users/test/docs")).toBe("文档目录");
      expect(getDirectoryDisplayName("/Users/test/custom_folder")).toBe("custom_folder 目录");
    });
  });

  describe("isAccessRootRequest & getDirectoryPathFromItem", () => {
    it("identifies access root request items and extracts path", () => {
      const item1 = makeInboxItem("item-1", {
        payload: { path: "/Users/cengwenqi/crm" },
      });
      expect(isAccessRootRequest(item1)).toBe(true);
      expect(getDirectoryPathFromItem(item1)).toBe("/Users/cengwenqi/crm");

      const item2 = makeInboxItem("item-2", {
        itemType: "permission_request",
        payload: {
          parameters: { target: "/Users/cengwenqi/docs" },
        },
      });
      expect(getDirectoryPathFromItem(item2)).toBe("/Users/cengwenqi/docs");
    });

    it("does not classify a file permission request as directory access", () => {
      const item = makeInboxItem("item-file", {
        itemType: "permission_request",
        payload: {
          toolName: "work_read_file",
          parameters: { path: "input/Q2-sales.xlsx" },
        },
      });

      expect(isAccessRootRequest(item)).toBe(false);
      expect(getInteractionTitle(item)).toBe("读取文件（input/Q2-sales.xlsx）");
      expect(getInteractionDescription(item)).toBe("Agent 请求执行此操作以继续当前任务。");
    });

    it("renders semantic title for work_execute permission requests", () => {
      const item = makeInboxItem("item-exec", {
        itemType: "permission_request",
        payload: {
          toolName: "work_execute",
          parameters: {
            action: "export_table",
            resource_id: "work-excel",
            file: "input/Q2-sales.xlsx",
          },
        },
      });
      expect(getInteractionTitle(item)).toBe("导出表格（input/Q2-sales.xlsx）");
    });
  });

  describe("riskClassForTool", () => {
    it("classifies work tools mirroring the backend policy", () => {
      expect(riskClassForTool("work_read_file")).toBe("read");
      expect(riskClassForTool("work_write_file")).toBe("write_local");
      expect(riskClassForTool("work_edit_file")).toBe("write_local");
      expect(riskClassForTool("work_execute")).toBe("exec");
      expect(riskClassForTool("work_run_command")).toBe("exec");
      expect(riskClassForTool("feishu_send_message")).toBe("external");
    });
  });

  describe("buildStandingRuleFromProposal", () => {
    it("builds an exec-class rule from a capability proposal", () => {
      const item = makeInboxItem("item-exec", {
        itemType: "permission_request",
        payload: {
          toolName: "work_execute",
          parameters: { resource_id: "work-excel", action: "export_table" },
          standingRuleProposal: {
            toolName: "work_execute",
            targetPattern: "work-excel.*",
            scope: "task",
          },
        },
      });

      const rule = buildStandingRuleFromProposal(item);
      expect(rule).not.toBeNull();
      expect(rule?.toolName).toBe("work_execute");
      expect(rule?.targetPattern).toBe("work-excel.*");
      expect(rule?.riskClass).toBe("exec");
      expect(rule?.id).toMatch(/^rule-/);
      expect(rule?.grantedAt).toBeTruthy();
    });

    it("returns null when the item carries no proposal", () => {
      const item = makeInboxItem("item-none", {
        itemType: "permission_request",
        payload: { toolName: "work_run_command", parameters: { target: "ls -la" } },
      });
      expect(buildStandingRuleFromProposal(item)).toBeNull();
    });
  });

  describe("getInteractionTitle & getInteractionDescription", () => {
    it("generates user-friendly Chinese titles and descriptions for directory requests", () => {
      const item = makeInboxItem("item-1", {
        payload: {
          path: "/Users/cengwenqi/crm",
          purpose: "读取客户清单以更新报告",
        },
      });

      expect(getInteractionTitle(item)).toBe("允许访问客户数据目录？");
      expect(getInteractionDescription(item)).toBe("读取客户清单以更新报告");
    });

    it("provides fallback description when purpose is missing", () => {
      const item = makeInboxItem("item-1", {
        payload: {
          path: "/Users/cengwenqi/crm",
          purpose: "",
        },
      });

      expect(getInteractionTitle(item)).toBe("允许访问客户数据目录？");
      expect(getInteractionDescription(item)).toBe(
        "Agent 需要读取该目录中的数据，以继续当前任务。",
      );
    });

    it("handles question elicitation items", () => {
      const item = makeInboxItem("item-q", {
        itemType: "question_elicitation",
        payload: {
          question: "请确认是否覆盖生产数据库？",
        },
      });

      expect(getInteractionTitle(item)).toBe("请确认是否覆盖生产数据库？");
    });
  });
});
