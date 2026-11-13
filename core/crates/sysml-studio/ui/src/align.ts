import type { Rect } from './types';

export type AlignMode =
  | 'left'
  | 'center-h'
  | 'right'
  | 'top'
  | 'center-v'
  | 'bottom'
  | 'distribute-h'
  | 'distribute-v';

export interface Box extends Rect {
  key: string;
}

/**
 * New rectangles for a set of boxes, moved into a line or spaced evenly.
 *
 * Alignment moves boxes, it does not resize them, so width and height are carried through
 * untouched: a person aligning three boxes is asking for the same outline, not for three boxes of
 * the same size. Distribution keeps the two extreme boxes where they are and puts the rest in
 * between, because moving the extremes would also move the line the person just chose.
 *
 * Pure and exported: the arithmetic is the part that can be wrong, and it can be checked without a
 * browser.
 */
export function alignRects(boxes: Box[], how: AlignMode): Box[] {
  if (boxes.length < 2) return boxes.map((b) => ({ ...b }));

  const left = Math.min(...boxes.map((b) => b.x));
  const right = Math.max(...boxes.map((b) => b.x + b.w));
  const top = Math.min(...boxes.map((b) => b.y));
  const bottom = Math.max(...boxes.map((b) => b.y + b.h));

  const moved = boxes.map((b) => ({ ...b }));

  switch (how) {
    case 'left':
      for (const b of moved) b.x = left;
      break;
    case 'right':
      for (const b of moved) b.x = right - b.w;
      break;
    case 'center-h': {
      const middle = (left + right) / 2;
      for (const b of moved) b.x = middle - b.w / 2;
      break;
    }
    case 'top':
      for (const b of moved) b.y = top;
      break;
    case 'bottom':
      for (const b of moved) b.y = bottom - b.h;
      break;
    case 'center-v': {
      const middle = (top + bottom) / 2;
      for (const b of moved) b.y = middle - b.h / 2;
      break;
    }
    case 'distribute-h': {
      const ordered = [...moved].sort((p, q) => p.x - q.x);
      const span = ordered[ordered.length - 1].x + ordered[ordered.length - 1].w - ordered[0].x;
      const used = ordered.reduce((sum, b) => sum + b.w, 0);
      const gap = (span - used) / (ordered.length - 1);
      let cursor = ordered[0].x;
      for (const b of ordered) {
        b.x = Math.round(cursor);
        cursor += b.w + gap;
      }
      break;
    }
    case 'distribute-v': {
      const ordered = [...moved].sort((p, q) => p.y - q.y);
      const span = ordered[ordered.length - 1].y + ordered[ordered.length - 1].h - ordered[0].y;
      const used = ordered.reduce((sum, b) => sum + b.h, 0);
      const gap = (span - used) / (ordered.length - 1);
      let cursor = ordered[0].y;
      for (const b of ordered) {
        b.y = Math.round(cursor);
        cursor += b.h + gap;
      }
      break;
    }
  }

  // Round only the coordinate that moved. Rounding both would also nudge a box along the axis the
  // person did not align, and a sidecar full of half pixels reads as a box that is "almost" in
  // line.
  return moved.map((b, i) => ({
    ...b,
    x: b.x === boxes[i].x ? b.x : Math.round(b.x),
    y: b.y === boxes[i].y ? b.y : Math.round(b.y),
  }));
}
