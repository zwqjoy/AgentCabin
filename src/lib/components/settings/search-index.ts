export interface SearchIndexItem {
  id: string;
  tab: string;
  section?: string;
  title: string;
  keywords: string[];
  group: string;
}

export const SETTINGS_SEARCH_INDEX: SearchIndexItem[] = [
  // 个人
  {
    id: "gen-about",
    tab: "general",
    section: "about",
    title: "应用信息与版本",
    keywords: ["version", "about", "agentcabin", "关于", "版本", "logo"],
    group: "常规",
  },
  {
    id: "gen-paths",
    tab: "general",
    section: "config-paths",
    title: "应用配置存储路径",
    keywords: ["settings.json", "path", "config", "路径", "配置"],
    group: "常规",
  },
  {
    id: "gen-lang",
    tab: "general",
    section: "language",
    title: "界面语言 (Language)",
    keywords: ["language", "locale", "chinese", "语言", "中文"],
    group: "常规",
  },
  {
    id: "gen-sleep",
    tab: "general",
    section: "system",
    title: "阻止系统睡眠 (Prevent Sleep)",
    keywords: ["sleep", "prevent", "power", "睡眠", "休眠", "屏幕"],
    group: "常规",
  },
  {
    id: "gen-tray",
    tab: "general",
    section: "system",
    title: "系统托盘与窗口行为",
    keywords: ["tray", "close", "minimize", "托盘", "关闭", "后台"],
    group: "常规",
  },
  {
    id: "gen-workspace",
    tab: "general",
    section: "workspace",
    title: "默认工作空间 (Workspace CWD)",
    keywords: ["workspace", "cwd", "directory", "工作区", "目录", "路径"],
    group: "常规",
  },
  {
    id: "gen-session",
    tab: "general",
    section: "session-import",
    title: "CLI 会话导入 (Claude / Codex)",
    keywords: ["session", "import", "claude", "codex", "会话", "导入", "历史"],
    group: "常规",
  },
  {
    id: "gen-remote",
    tab: "general",
    section: "remote",
    title: "远程主机与 SSH",
    keywords: ["remote", "ssh", "host", "connection", "远程", "主机", "服务器"],
    group: "常规",
  },
  {
    id: "gen-debug",
    tab: "general",
    section: "debug",
    title: "调试与日志 (Debug Logs)",
    keywords: ["debug", "log", "logs", "verbose", "日志", "调试", "控制台"],
    group: "常规",
  },

  // 外观
  {
    id: "app-theme",
    tab: "appearance",
    section: "theme",
    title: "主题模式 (Light / Dark / System)",
    keywords: ["theme", "mode", "dark", "light", "system", "主题", "暗黑", "深色", "浅色"],
    group: "外观",
  },
  {
    id: "app-builtin",
    tab: "appearance",
    section: "builtin-themes",
    title: "预设主题 (Builtin Themes)",
    keywords: ["builtin", "midnight", "nord", "solarized", "gruvbox", "预设", "配色"],
    group: "外观",
  },
  {
    id: "app-color",
    tab: "appearance",
    section: "color-scheme",
    title: "颜色与强调色 (Accent Color)",
    keywords: ["accent", "color", "palette", "强调色", "色彩", "颜色"],
    group: "外观",
  },
  {
    id: "app-font",
    tab: "appearance",
    section: "fonts",
    title: "UI 与代码字体大小 (Typography)",
    keywords: ["font", "size", "code", "ui", "typography", "字体", "字号", "代码字体"],
    group: "外观",
  },
  {
    id: "app-motion",
    tab: "appearance",
    section: "motion",
    title: "减弱动画与特效 (Reduce Motion)",
    keywords: ["motion", "animation", "translucency", "glass", "毛玻璃", "半透明", "动画"],
    group: "外观",
  },
  {
    id: "app-advanced",
    tab: "appearance",
    section: "advanced",
    title: "主题导入与导出 (JSON)",
    keywords: ["json", "export", "import", "custom", "导出", "导入", "配置"],
    group: "外观",
  },

  // 键盘快捷键
  {
    id: "kb-list",
    tab: "keybindings",
    section: "shortcuts",
    title: "键盘快捷键设置",
    keywords: ["shortcut", "keybinding", "hotkey", "keyboard", "快捷键", "按键", "绑定"],
    group: "键盘快捷键",
  },

  // 使用情况
  {
    id: "usage-stats",
    tab: "usage",
    section: "overview",
    title: "使用情况和计费 (Token Usage)",
    keywords: ["usage", "token", "billing", "cost", "tokens", "计费", "统计", "用量", "消耗"],
    group: "使用情况和计费",
  },

  // 桌面宠物
  {
    id: "pet-main",
    tab: "pet",
    section: "main",
    title: "桌面宠物开关与选择 (Pet)",
    keywords: ["pet", "desktop pet", "grokbot", "clawd", "capy", "桌宠", "宠物", "挂件"],
    group: "桌面宠物",
  },
  {
    id: "pet-custom",
    tab: "pet",
    section: "custom",
    title: "自定义桌宠导入 (Custom Pet)",
    keywords: ["custom pet", "sprite", "upload", "zip", "png", "gif", "自定义宠物", "上传"],
    group: "桌面宠物",
  },

  // 能力中心
  {
    id: "cap-center",
    tab: "capability-center",
    section: "all",
    title: "能力中心 (Capability Center)",
    keywords: ["capability", "plugins", "skills", "connectors", "插件", "技能", "能力", "中心"],
    group: "能力中心",
  },

  // Runtime 运行时
  {
    id: "runtimes-main",
    tab: "runtimes",
    section: "overview",
    title: "Runtime 运行时概览与引擎配置",
    keywords: ["runtime", "runtimes", "agent", "engine", "运行时", "原生", "引擎"],
    group: "Runtime 运行时",
  },
  {
    id: "runtimes-codex",
    tab: "runtimes",
    section: "codex",
    title: "Codex 运行环境与沙箱",
    keywords: ["codex", "openai", "sandbox", "reasoning", "沙箱", "推理", "runtime"],
    group: "Runtime 运行时",
  },
  {
    id: "runtimes-claude",
    tab: "runtimes",
    section: "claude",
    title: "Claude Code CLI 设置",
    keywords: ["claude", "anthropic", "cli", "config", "克劳德", "runtime"],
    group: "Runtime 运行时",
  },
  {
    id: "runtimes-grok",
    tab: "runtimes",
    section: "grok",
    title: "Grok 思考级别与配置",
    keywords: ["grok", "xai", "thinking", "思考", "runtime"],
    group: "Runtime 运行时",
  },
  {
    id: "runtimes-dsh",
    tab: "runtimes",
    section: "dsh",
    title: "DeepSeek (DSH) 原生运行时设置",
    keywords: ["dsh", "deepseek", "harness", "深度求索", "runtime"],
    group: "Runtime 运行时",
  },
  {
    id: "runtimes-pi",
    tab: "runtimes",
    section: "pi",
    title: "Pi Agent 原生运行时设置",
    keywords: ["pi", "agent", "runtime", "pi agent"],
    group: "Runtime 运行时",
  },

  // Code
  {
    id: "code-main",
    tab: "code",
    section: "overview",
    title: "Code 极客编程模式设置",
    keywords: ["code", "coding", "vibe", "dev", "编程", "极客", "模式"],
    group: "Code 模式",
  },
  {
    id: "code-provider",
    tab: "code",
    section: "provider",
    title: "Code 默认 Runtime Provider 与载体配置",
    keywords: ["code", "provider", "runtime", "profile", "载体", "默认引擎"],
    group: "Code 模式",
  },
  {
    id: "code-rules",
    tab: "code",
    section: "rules",
    title: "Code 全局规则 (AGENTS.md)",
    keywords: ["code", "rules", "instruction", "prompt", "规则", "agents.md"],
    group: "Code 模式",
  },
  {
    id: "code-worktrees",
    tab: "code",
    section: "worktrees",
    title: "Code 隔离工作区 (Isolated Worktrees)",
    keywords: ["code", "worktree", "git", "isolation", "分支", "工作区"],
    group: "Code 模式",
  },

  // Work
  {
    id: "work-main",
    tab: "work",
    section: "overview",
    title: "Work 自主工作流模式设置",
    keywords: ["work", "workflow", "autonomous", "pi work", "dsh work", "工作流", "自主"],
    group: "Work 模式",
  },
  {
    id: "work-rules",
    tab: "work",
    section: "rules",
    title: "Work 全局规则 (Global Rules)",
    keywords: ["rules", "instruction", "prompt", "规则", "规范"],
    group: "Work 模式",
  },
  {
    id: "work-harness",
    tab: "work",
    section: "harness",
    title: "Work 运行时 Provider 投射",
    keywords: ["harness", "runtime", "provider", "projection", "投射"],
    group: "Work 模式",
  },

  // 模型与提供商
  {
    id: "models-providers",
    tab: "models",
    section: "providers",
    title: "全局模型与提供商 (Global Providers)",
    keywords: [
      "model",
      "models",
      "provider",
      "openai",
      "deepseek",
      "anthropic",
      "api key",
      "base url",
      "模型",
      "提供商",
      "密钥",
    ],
    group: "模型与提供商",
  },
  {
    id: "models-chatgpt",
    tab: "models",
    section: "subscription",
    title: "ChatGPT / OpenAI 订阅登录",
    keywords: ["chatgpt", "subscription", "login", "rate limit", "订阅", "登录", "额度"],
    group: "模型与提供商",
  },

  // Doctor
  {
    id: "doc-main",
    tab: "doctor",
    section: "all",
    title: "CLI 引擎检测与健康诊断 (Doctor)",
    keywords: [
      "doctor",
      "cli",
      "health",
      "diagnostics",
      "check",
      "detect",
      "检测",
      "引擎",
      "诊断",
      "健康",
    ],
    group: "CLI 引擎检测",
  },

  // 工具
  {
    id: "tool-web",
    tab: "web-access",
    section: "main",
    title: "网络访问配置 (Web Access)",
    keywords: ["web", "access", "search", "engine", "scrape", "网络", "搜索", "抓取"],
    group: "网络访问",
  },
  {
    id: "tool-browser",
    tab: "browser-use",
    section: "main",
    title: "浏览器自动化 (Browser Use)",
    keywords: ["browser", "chrome", "headless", "automation", "浏览器", "自动化"],
    group: "浏览器自动化",
  },
  {
    id: "tool-desktop",
    tab: "desktop-use",
    section: "main",
    title: "电脑控制 (Desktop / Computer Use)",
    keywords: ["desktop", "computer use", "screen", "display", "control", "电脑", "操控", "屏幕"],
    group: "电脑控制",
  },
  {
    id: "tool-mcp",
    tab: "capability-center",
    section: "mcp",
    title: "MCP 插件与服务器扩展",
    keywords: ["mcp", "protocol", "model context", "server", "插件", "扩展", "服务器"],
    group: "能力中心",
  },
];

export function searchSettings(query: string): SearchIndexItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return SETTINGS_SEARCH_INDEX.filter((item) => {
    if (item.title.toLowerCase().includes(q)) return true;
    if (item.group.toLowerCase().includes(q)) return true;
    return item.keywords.some((k) => k.toLowerCase().includes(q));
  });
}
