import { describe, expect, it } from "vitest";
import {
  filterLibrarySummaries,
  formatLibraryCategory,
  formatLibraryItemReference,
} from "../work-library";
import type { LibraryItemSummary } from "$lib/types/work";

describe("work-library utility", () => {
  const sampleItems: LibraryItemSummary[] = [
    {
      id: "lib-1",
      workspaceId: null,
      title: "代码审查与架构准则",
      description: "所有提交需符合 Rust 与 TS 规范",
      category: "rule",
      tags: ["code", "standards"],
      contentPreview: "规范详情...",
      sourceArtifactId: null,
      sourcePath: "/workspace/rules.md",
      collection: "规范",
      metadata: { owner: "platform" },
      citationCount: 0,
      updatedAt: "2026-08-30T00:00:00Z",
    },
    {
      id: "lib-2",
      workspaceId: "ws-1",
      title: "季度财务简报模版",
      description: "财务指标汇总表格模版",
      category: "template",
      tags: ["finance", "template"],
      contentPreview: "# 季度财务复盘...",
      sourceArtifactId: null,
      sourcePath: "/workspace/finance-template.md",
      collection: "财务",
      metadata: {},
      citationCount: 0,
      updatedAt: "2026-08-29T00:00:00Z",
    },
    {
      id: "lib-3",
      workspaceId: "ws-2",
      title: "用户画像数据集",
      description: "2026 Q2 活跃用户分布",
      category: "dataset",
      tags: ["users", "data"],
      contentPreview: "age,region,active_days...",
      sourceArtifactId: "art-1",
      sourcePath: "/workspace/users.csv",
      collection: "数据",
      metadata: {},
      citationCount: 0,
      updatedAt: "2026-08-28T00:00:00Z",
    },
  ];

  it("formats categories properly", () => {
    expect(formatLibraryCategory("doc").label).toBe("参考文档");
    expect(formatLibraryCategory("template").label).toBe("常用模板");
    expect(formatLibraryCategory("rule").label).toBe("业务规则");
    expect(formatLibraryCategory("dataset").label).toBe("数据集/样本");
    expect(formatLibraryCategory("link").label).toBe("外部链接");
  });

  it("filters by category correctly", () => {
    const rules = filterLibrarySummaries(sampleItems, "rule", "all", "", "");
    expect(rules.length).toBe(1);
    expect(rules[0].id).toBe("lib-1");
  });

  it("filters by scope correctly", () => {
    const globalOnly = filterLibrarySummaries(sampleItems, "all", "global", "", "");
    expect(globalOnly.length).toBe(1);
    expect(globalOnly[0].id).toBe("lib-1");

    const ws1Only = filterLibrarySummaries(sampleItems, "all", "workspace", "ws-1", "");
    expect(ws1Only.length).toBe(1);
    expect(ws1Only[0].id).toBe("lib-2");
  });

  it("filters by query keyword correctly", () => {
    const queried = filterLibrarySummaries(sampleItems, "all", "all", "", "财务");
    expect(queried.length).toBe(1);
    expect(queried[0].id).toBe("lib-2");

    const tagQueried = filterLibrarySummaries(sampleItems, "all", "all", "", "standards");
    expect(tagQueried.length).toBe(1);
    expect(tagQueried[0].id).toBe("lib-1");
  });

  it("formats reference token string correctly", () => {
    const ref = formatLibraryItemReference(sampleItems[0]);
    expect(ref).toBe("<!-- 引用资料: 代码审查与架构准则 (ID: lib-1) -->");
  });

  it("searches provenance, collections, and metadata", () => {
    expect(filterLibrarySummaries(sampleItems, "all", "all", "", "规范")).toHaveLength(1);
    expect(filterLibrarySummaries(sampleItems, "all", "all", "", "finance-template")).toHaveLength(
      1,
    );
    expect(filterLibrarySummaries(sampleItems, "all", "all", "", "platform")).toHaveLength(1);
  });
});
