import type { LibraryCategory, LibraryItem, LibraryItemSummary } from "$lib/types/work";

export interface LibraryCategoryMeta {
  label: string;
  icon: string;
  badgeClass: string;
}

export function formatLibraryCategory(category: LibraryCategory): LibraryCategoryMeta {
  switch (category) {
    case "doc":
      return {
        label: "参考文档",
        icon: "📄",
        badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
      };
    case "template":
      return {
        label: "常用模板",
        icon: "📑",
        badgeClass: "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20",
      };
    case "rule":
      return {
        label: "业务规则",
        icon: "⚖️",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
      };
    case "dataset":
      return {
        label: "数据集/样本",
        icon: "📊",
        badgeClass:
          "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
      };
    case "link":
      return {
        label: "外部链接",
        icon: "🔗",
        badgeClass: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20",
      };
    default:
      return {
        label: "通用资料",
        icon: "📁",
        badgeClass: "bg-muted text-muted-foreground border-border",
      };
  }
}

export function filterLibrarySummaries(
  items: LibraryItemSummary[],
  categoryFilter: LibraryCategory | "all",
  scopeFilter: "all" | "global" | "workspace",
  workspaceId: string,
  query: string,
): LibraryItemSummary[] {
  const q = query.trim().toLowerCase();
  return items.filter((item) => {
    // Category filter
    if (categoryFilter !== "all" && item.category !== categoryFilter) {
      return false;
    }

    // Scope filter
    if (scopeFilter === "global" && item.workspaceId) {
      return false;
    }
    if (scopeFilter === "workspace") {
      if (!item.workspaceId) return false;
      if (workspaceId && item.workspaceId !== workspaceId) return false;
    }

    // Query search
    if (q) {
      const matched =
        item.title.toLowerCase().includes(q) ||
        item.description.toLowerCase().includes(q) ||
        item.contentPreview.toLowerCase().includes(q) ||
        item.tags.some((t) => t.toLowerCase().includes(q)) ||
        item.sourcePath?.toLowerCase().includes(q) ||
        item.collection?.toLowerCase().includes(q) ||
        Object.entries(item.metadata).some(
          ([key, value]) => key.toLowerCase().includes(q) || value.toLowerCase().includes(q),
        );
      if (!matched) return false;
    }

    return true;
  });
}

export function formatLibraryItemReference(item: LibraryItem | LibraryItemSummary): string {
  return `<!-- 引用资料: ${item.title} (ID: ${item.id}) -->`;
}
