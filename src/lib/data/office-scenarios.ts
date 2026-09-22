export interface ScenarioTag {
  icon: string;
  label: string;
  prompt: string;
}

export const SCENARIO_TAGS: ScenarioTag[] = [
  { icon: "📄", label: "文档处理", prompt: "帮我处理文档：" },
  { icon: "💼", label: "金融服务", prompt: "帮我完成金融分析：" },
  { icon: "📈", label: "数据分析及可视化", prompt: "帮我分析数据并生成可视化图表：" },
  { icon: "📱", label: "个人工作台", prompt: "帮我整理个人工作台任务与信息：" },
  { icon: "🖥️", label: "幻灯片 (PPT)", prompt: "帮我制作幻灯片 PPT：" },
  { icon: "🔍", label: "深度研究", prompt: "帮我进行深度研究并输出报告：" },
  { icon: "📊", label: "商业分析", prompt: "帮我进行商业与行业分析：" },
  { icon: "🎬", label: "视频脚本", prompt: "帮我编写视频脚本：" },
  { icon: "🎯", label: "产品管理", prompt: "帮我进行产品需求规划与功能设计：" },
  { icon: "✍️", label: "写作翻译", prompt: "帮我进行多语言翻译与文本润色：" },
];
