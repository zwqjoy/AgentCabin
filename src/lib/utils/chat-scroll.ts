export interface ChatScrollMetrics {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
}

/** Keep live chat output pinned only while the user is already near the bottom. */
export function isNearChatBottom(
  { scrollHeight, scrollTop, clientHeight }: ChatScrollMetrics,
  threshold = 40,
): boolean {
  return scrollHeight - scrollTop - clientHeight < threshold;
}
