/** Sanitize permission rules: trim, filter empty, deduplicate (preserve order). */
export function sanitizeRules(rules: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const r of rules) {
    const trimmed = r.trim();
    if (trimmed && !seen.has(trimmed)) {
      seen.add(trimmed);
      result.push(trimmed);
    }
  }
  return result;
}

/** Filter rules by case-insensitive substring match. Empty search returns all. */
export function filterRules(rules: string[], search: string): string[] {
  const q = search.trim().toLowerCase();
  if (!q) return rules;
  return rules.filter((r) => r.toLowerCase().includes(q));
}
