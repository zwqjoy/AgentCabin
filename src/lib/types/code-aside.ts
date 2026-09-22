export type CodeAsideTabType = "review" | "terminal" | "browser" | "file";

export interface CodeAsideTab {
  id: string;
  type: CodeAsideTabType;
  title: string;
  filePath?: string;
  url?: string;
  turnDiff?: string;
  closable?: boolean;
}

export const CODE_ASIDE_TAB_TITLES: Record<CodeAsideTabType, string> = {
  review: "审查",
  terminal: "终端",
  browser: "新标签页",
  file: "文件",
};

export const CODE_ASIDE_SHORTCUTS: Record<CodeAsideTabType, string> = {
  review: "^⇧G",
  terminal: "^`",
  browser: "⌘T",
  file: "⌘P",
};
