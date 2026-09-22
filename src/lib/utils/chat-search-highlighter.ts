/**
 * Utility for searching text in a chat container and highlighting matching words (WorkBuddy style).
 * It uses DOM Range/Text manipulation to wrap matched tokens with <mark> tags without breaking the tree.
 */

const HIGHLIGHT_CLASS = "agent-search-highlight";
const ACTIVE_CLASS = "agent-search-highlight-active";

export interface SearchHighlighterState {
  query: string;
  totalMatches: number;
  currentIndex: number;
}

export class ChatSearchHighlighter {
  private container: HTMLElement | null = null;
  private marks: HTMLElement[] = [];
  private currentIndex = -1;
  private currentQuery = "";

  constructor(container?: HTMLElement | null) {
    if (container) {
      this.container = container;
    }
  }

  setContainer(container: HTMLElement | null) {
    if (this.container !== container) {
      this.clear();
      this.container = container;
    }
  }

  /**
   * Search for query across container and highlight all occurrences.
   * Returns the total count of matches.
   */
  search(query: string): number {
    this.clear();
    const cleanQuery = query.trim();
    if (!cleanQuery || !this.container) {
      this.currentQuery = "";
      this.currentIndex = -1;
      return 0;
    }

    this.currentQuery = cleanQuery;
    const lowerQuery = cleanQuery.toLowerCase();

    // Collect all valid text nodes, skipping ignored elements (like collapsed thinking, buttons, etc.)
    const textNodes: Text[] = [];
    const walker = document.createTreeWalker(this.container, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;

        // Exclude hidden elements, scripts, styles, buttons, inputs, svgs, and explicit exclusions
        if (
          parent.closest("[data-exclude-search='true']") ||
          parent.closest(".chat-thought-process") ||
          parent.closest(".sr-only") ||
          parent.tagName === "SCRIPT" ||
          parent.tagName === "STYLE" ||
          parent.tagName === "INPUT" ||
          parent.tagName === "TEXTAREA" ||
          parent.tagName === "SELECT" ||
          parent.tagName === "BUTTON" ||
          parent.tagName === "SVG"
        ) {
          return NodeFilter.FILTER_REJECT;
        }

        if (!node.nodeValue || !node.nodeValue.toLowerCase().includes(lowerQuery)) {
          return NodeFilter.FILTER_SKIP;
        }

        return NodeFilter.FILTER_ACCEPT;
      },
    });

    let currentNode = walker.nextNode();
    while (currentNode) {
      textNodes.push(currentNode as Text);
      currentNode = walker.nextNode();
    }

    const marks: HTMLElement[] = [];

    // Highlight matches in each text node
    for (const textNode of textNodes) {
      const parent = textNode.parentNode;
      if (!parent) continue;

      const text = textNode.nodeValue || "";
      let lastIndex = 0;
      let matchIndex: number;
      const lowerText = text.toLowerCase();
      const fragment = document.createDocumentFragment();

      while ((matchIndex = lowerText.indexOf(lowerQuery, lastIndex)) !== -1) {
        // Append text before match
        if (matchIndex > lastIndex) {
          fragment.appendChild(document.createTextNode(text.substring(lastIndex, matchIndex)));
        }

        // Create <mark> for matched text
        const mark = document.createElement("mark");
        mark.className = HIGHLIGHT_CLASS;
        mark.textContent = text.substring(matchIndex, matchIndex + cleanQuery.length);
        fragment.appendChild(mark);
        marks.push(mark);

        lastIndex = matchIndex + cleanQuery.length;
      }

      // Append remainder of text
      if (lastIndex < text.length) {
        fragment.appendChild(document.createTextNode(text.substring(lastIndex)));
      }

      parent.replaceChild(fragment, textNode);
    }

    this.marks = marks;

    if (marks.length > 0) {
      this.goTo(0);
    } else {
      this.currentIndex = -1;
    }

    return marks.length;
  }

  /**
   * Jump to a specific match index (0-based) and scroll it into view.
   */
  goTo(index: number): boolean {
    if (this.marks.length === 0) {
      this.currentIndex = -1;
      return false;
    }

    // Unset previous active mark
    if (this.currentIndex >= 0 && this.currentIndex < this.marks.length) {
      this.marks[this.currentIndex]?.classList.remove(ACTIVE_CLASS);
    }

    // Wrap around index
    this.currentIndex = (index + this.marks.length) % this.marks.length;
    const targetMark = this.marks[this.currentIndex];

    if (targetMark) {
      targetMark.classList.add(ACTIVE_CLASS);
      targetMark.scrollIntoView({ behavior: "smooth", block: "center", inline: "nearest" });
      return true;
    }

    return false;
  }

  next(): boolean {
    if (this.marks.length === 0) return false;
    return this.goTo(this.currentIndex + 1);
  }

  prev(): boolean {
    if (this.marks.length === 0) return false;
    return this.goTo(this.currentIndex - 1);
  }

  /**
   * Clear all highlights and restore pristine DOM text nodes.
   */
  clear(): void {
    if (!this.container || this.marks.length === 0) {
      this.marks = [];
      this.currentIndex = -1;
      this.currentQuery = "";
      return;
    }

    const marks = this.container.querySelectorAll<HTMLElement>(`mark.${HIGHLIGHT_CLASS}`);
    const parentsToNormalize = new Set<Node>();

    marks.forEach((mark) => {
      const parent = mark.parentNode;
      if (parent) {
        parentsToNormalize.add(parent);
        const textNode = document.createTextNode(mark.textContent || "");
        parent.replaceChild(textNode, mark);
      }
    });

    parentsToNormalize.forEach((parent) => {
      parent.normalize();
    });

    this.marks = [];
    this.currentIndex = -1;
    this.currentQuery = "";
  }

  getCurrentIndex(): number {
    return this.currentIndex;
  }

  getTotalCount(): number {
    return this.marks.length;
  }

  getQuery(): string {
    return this.currentQuery;
  }
}
