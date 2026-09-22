export interface WorkStarter {
  id: string;
  icon: string;
  label: string;
  description: string;
  prompt: string;
}

export interface WorkAutomationTemplate {
  id: string;
  icon: string;
  title: string;
  description: string;
  instructions: string;
  scheduleKind: "daily" | "weekdays" | "custom_weekly";
  time: string;
  days?: number[];
}

/**
 * Outcome-first starters keep the first Work decision about the user's goal,
 * not about AgentCabin's internal Workspace/Run model.
 */
export const WORK_STARTERS: WorkStarter[] = [
  {
    id: "document",
    icon: "📄",
    label: "整理一份文档",
    description: "提炼重点、待办和结论，输出可交付版本",
    prompt:
      "请帮我整理这份文档：提炼关键结论和待办，指出需要补充的信息，并输出一份结构清晰的可交付版本。",
  },
  {
    id: "data-report",
    icon: "📈",
    label: "生成数据报告",
    description: "清洗数据、找出趋势并生成图表",
    prompt:
      "请帮我分析这份数据：先检查并说明明显问题，再提炼关键指标、生成合适的可视化图表，并输出一份分析报告。",
  },
  {
    id: "presentation",
    icon: "🖥️",
    label: "制作一份 PPT",
    description: "把材料整理成结构清晰的演示文稿",
    prompt:
      "请根据我提供的材料制作一份结构清晰的 PPT，包含页面大纲、关键要点、图表建议和演讲备注。",
  },
  {
    id: "research",
    icon: "🔍",
    label: "做一次深度研究",
    description: "查找来源、比较观点并给出结论",
    prompt:
      "请研究这个主题：列出可靠来源，比较关键观点，说明不确定性，并输出一份有结论和引用的研究报告。",
  },
  {
    id: "file-organize",
    icon: "🗂️",
    label: "整理工作目录",
    description: "按规则归类、重命名并先预览变更",
    prompt:
      "请整理这个工作目录中的文件：先分析现状并提出归类和命名规则，等我确认后再执行变更，并汇总变更结果。",
  },
  {
    id: "product-plan",
    icon: "🎯",
    label: "规划一个产品",
    description: "从用户问题到功能和验收标准",
    prompt:
      "请把这个想法整理成产品需求方案，包含用户问题、目标、核心流程、功能范围、风险和验收标准。",
  },
];

/**
 * Templates make scheduled Work legible before the user has learned the
 * underlying task model. They are deliberately small and editable after open.
 */
export const WORK_AUTOMATION_TEMPLATES: WorkAutomationTemplate[] = [
  {
    id: "weekly-report",
    icon: "📝",
    title: "每周工作周报",
    description: "整理本周记录，输出完成、问题和下周计划",
    instructions:
      "整理工作目录中的本周记录，按完成事项、存在问题、下周计划三部分生成一份务实的周报，并保存到 output/weekly-report.md。",
    scheduleKind: "weekdays",
    time: "17:30",
  },
  {
    id: "daily-brief",
    icon: "🗞️",
    title: "每日信息简报",
    description: "汇总指定资料，生成当天重点和待办",
    instructions:
      "读取工作目录中当天新增的资料，提炼重要信息、变化和待办，生成一份简洁的每日简报，并保存到 output/daily-brief.md。",
    scheduleKind: "daily",
    time: "09:00",
  },
  {
    id: "data-refresh",
    icon: "📊",
    title: "定期刷新数据报告",
    description: "更新数据、检查异常并重新生成图表",
    instructions:
      "读取工作目录中的最新数据，检查异常和缺失值，更新关键指标与图表，并保存一份可交付的数据报告到 output/data-report.xlsx。",
    scheduleKind: "daily",
    time: "08:30",
  },
  {
    id: "meeting-prep",
    icon: "📅",
    title: "会议前准备",
    description: "从资料中整理背景、问题和会议清单",
    instructions:
      "整理工作目录中与即将召开的会议相关的资料，输出背景摘要、关键问题、待确认事项和会议清单到 output/meeting-prep.md。",
    scheduleKind: "weekdays",
    time: "08:45",
  },
];
