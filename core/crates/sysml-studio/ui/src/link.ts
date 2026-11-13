/**
 * The element a source line belongs to.
 *
 * A declaration owns the lines below it until the next declaration at the same or a shallower
 * depth, so a cursor on a member belongs to that member and a cursor on a blank line inside a
 * definition belongs to the definition. The rule is "the deepest declaration at or above the
 * line", which is what a reader means by "the thing I am looking at".
 */

export interface SourceLine {
  line: number;
  key: string;
}

/** Flatten an outline tree into `(line, key)` pairs, keeping the tree's order. */
export function flattenOutline(entries: unknown): SourceLine[] {
  const out: SourceLine[] = [];
  const walk = (list: unknown) => {
    if (!Array.isArray(list)) return;
    for (const entry of list) {
      if (!entry || typeof entry !== 'object') continue;
      const record = entry as { line?: unknown; key?: unknown; children?: unknown };
      if (typeof record.line === 'number' && typeof record.key === 'string') {
        out.push({ line: record.line, key: record.key });
      }
      walk(record.children);
    }
  };
  walk(entries);
  return out;
}

/**
 * The key of the declaration a cursor line falls in.
 *
 * Ties go to the later declaration: two elements on one line are two statements, and the second
 * is the one the cursor is nearer. A line above every declaration belongs to nothing.
 */
export function elementOnLine(lines: readonly SourceLine[], cursorLine: number): string | null {
  let best: SourceLine | null = null;
  for (const entry of lines) {
    if (entry.line > cursorLine) continue;
    if (!best || entry.line >= best.line) best = entry;
  }
  return best?.key ?? null;
}
