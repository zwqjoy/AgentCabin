import { describe, it, expect, vi } from "vitest";
import { render } from "svelte/server";
import WorkBuddyCascadingMenu from "../WorkBuddyCascadingMenu.svelte";

vi.mock("$app/navigation", () => ({
  goto: vi.fn(),
  replaceState: vi.fn(),
  pushState: vi.fn(),
}));

vi.mock("$lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/api")>();
  return {
    ...actual,
    listAgentPlugins: vi.fn().mockResolvedValue([]),
    listSkills: vi.fn().mockResolvedValue([]),
    listPromptTemplates: vi.fn().mockResolvedValue([
      {
        id: "template-1",
        name: "review",
        description: "审阅当前改动",
        content: "请审阅当前改动",
        builtin: true,
      },
    ]),
    setAgentPluginBinding: vi.fn().mockResolvedValue(undefined),
  };
});

vi.mock("$lib/api/work", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/api/work")>();
  return {
    ...actual,
    listWorkConnectorPackages: vi.fn().mockResolvedValue([]),
    enableWorkConnectorPackage: vi.fn().mockResolvedValue(undefined),
  };
});

describe("WorkBuddyCascadingMenu primary categories", () => {
  it("exposes 提示词 as the first entry of the plus menu", () => {
    const { body } = render(WorkBuddyCascadingMenu, { props: { open: true } });

    for (const label of ["提示词", "添加文件", "模式", "专家", "技能", "连接器"]) {
      expect(body).toContain(`>${label}</span>`);
    }
    expect(body.indexOf(">提示词</span>")).toBeLessThan(body.indexOf(">添加文件</span>"));
    expect(body).toContain('aria-label="打开能力与模式菜单"');
  });
});
