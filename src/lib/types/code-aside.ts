export type CodeAsideTabType = "review" | "terminal" | "file";

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
  file: "文件",
};

export const CODE_ASIDE_SHORTCUTS: Record<CodeAsideTabType, string> = {
  review: "^⇧G",
  terminal: "^`",
  file: "⌘P",
};
