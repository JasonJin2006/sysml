import type { CanvasNode, CanvasValue, Rect } from './types';

/**
 * The part of a document this comparison reads.
 *
 * Explicit rather than the whole document: it is the contract, and it lets a test build two
 * pictures without inventing every unrelated field.
 */
export type Picture = {
  nodes: Array<Pick<CanvasNode, 'key' | 'label' | 'x' | 'y' | 'w' | 'h'>>;
  values: Array<Pick<CanvasValue, 'key' | 'text'>>;
};

/** What changed between two renders of the same view. */
export interface Marks {
  /** Keys that exist now and did not before. */
  added: Set<string>;
  /** Boxes that are gone: key, label and the rectangle they last occupied. */
  removed: Array<{ key: string; label: string; rect: Rect }>;
  /** Values whose written text changed, by qualified name. */
  values: Map<string, { from: string; to: string }>;
}

export function isClean(marks: Marks): boolean {
  return marks.added.size === 0 && marks.removed.length === 0 && marks.values.size === 0;
}

/**
 * Compare two documents of the same view.
 *
 * The point is "what did the agent change while I was not looking", so it compares the picture
 * the person is looking at against the one that was there when the page opened. Only facts the
 * canvas draws are compared: a key appearing or disappearing, and the text of a value. Comparing
 * the whole element tree would mark every box for an edit that changed nothing visible.
 *
 * Pure: no DOM, no fetch. The arithmetic is the part that can be subtly wrong, so it is kept
 * where it can be checked on its own.
 */
export function diffDocuments(before: Picture, after: Picture): Marks {
  const was = new Map(before.nodes.map((n) => [n.key, n]));
  const now = new Map(after.nodes.map((n) => [n.key, n]));

  const added = new Set<string>();
  for (const key of now.keys()) {
    if (!was.has(key)) added.add(key);
  }

  const removed: Marks['removed'] = [];
  for (const [key, node] of was) {
    if (now.has(key)) continue;
    removed.push({
      key,
      label: node.label,
      rect: { x: node.x, y: node.y, w: node.w, h: node.h },
    });
  }
  // Stable order: the ghost boxes must not shuffle between renders.
  removed.sort((p, q) => (p.key < q.key ? -1 : p.key > q.key ? 1 : 0));

  const values = new Map<string, { from: string; to: string }>();
  const beforeByKey = new Map(before.values.map((v) => [v.key, v.text]));
  for (const value of after.values) {
    const old = beforeByKey.get(value.key);
    if (old !== undefined && old !== value.text) {
      values.set(value.key, { from: old, to: value.text });
    }
  }

  return { added, removed, values };
}
