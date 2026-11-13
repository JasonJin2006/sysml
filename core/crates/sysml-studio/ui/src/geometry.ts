import type { Camera, CanvasDocument, CanvasEdge, Point } from './types';

// Geometry the browser has to reproduce from the server's numbers. The server decides *where*
// everything is (`sysml-view`); these functions only turn those coordinates into path data, so a
// difference between the SVG export and the canvas would have to come from here.

/** Radius of the arc drawn where one line crosses another. Mirrors `jumps::RADIUS`. */
export const HOP_RADIUS = 4;

/**
 * Smallest zoom at which a 12px label still renders at 9px.
 *
 * Fitting a 900-unit-tall picture into a 520px viewport asks for ~0.5, and at that zoom every
 * label is 6px tall: the whole diagram is "visible" and none of it is readable. The floor keeps
 * the text legible and lets the picture overflow instead, which is what a person wants from a
 * first look.
 */
export const MIN_READABLE_ZOOM = 0.75;

/**
 * Path data for one edge, arcing over each point where another line crosses it.
 *
 * The hop points come from the server (`sysml_view::jumps`), not from this file: the export and
 * the canvas have to agree on where a line jumps, and two implementations would drift.
 */
export function edgePath(edge: CanvasEdge): string {
  const points = edge.points;
  if (points.length < 2) return '';
  const jumps = edge.hops ?? [];
  let d = `M${points[0][0].toFixed(1)} ${points[0][1].toFixed(1)}`;
  for (let i = 1; i < points.length; i += 1) {
    const a = points[i - 1];
    const b = points[i];
    const horizontal = Math.abs(a[1] - b[1]) < 0.01;
    if (!horizontal) {
      d += ` L${b[0].toFixed(1)} ${b[1].toFixed(1)}`;
      continue;
    }
    const lo = Math.min(a[0], b[0]);
    const hi = Math.max(a[0], b[0]);
    const here = jumps
      .filter(([x, y]) => Math.abs(a[1] - y) < 0.01 && x > lo + HOP_RADIUS && x < hi - HOP_RADIUS)
      .map(([x]) => x)
      .sort((p, q) => p - q);
    const forward = b[0] > a[0];
    for (const at of here) {
      const start = forward ? at - HOP_RADIUS : at + HOP_RADIUS;
      const end = forward ? at + HOP_RADIUS : at - HOP_RADIUS;
      const sweep = forward ? 1 : 0;
      d += ` L${start.toFixed(1)} ${a[1].toFixed(1)}`;
      d += ` A${HOP_RADIUS} ${HOP_RADIUS} 0 0 ${sweep} ${end.toFixed(1)} ${a[1].toFixed(1)}`;
    }
    d += ` L${b[0].toFixed(1)} ${b[1].toFixed(1)}`;
  }
  return d;
}

export interface ArrowGeometry {
  anchor: Point;
  /** Unit vector pointing from the line into the box the symbol sits on. */
  ux: number;
  uy: number;
}

/**
 * Where the arrowhead goes and which way it points.
 *
 * A composite diamond marks the owner, which is the start of the line; typing and satisfy point
 * at the target, which is the end.
 */
export function arrowGeometry(edge: CanvasEdge): ArrowGeometry | null {
  const points = edge.points;
  if (points.length < 2) return null;
  const first = points[0];
  const last = points[points.length - 1];
  const second = points[1];
  const prev = points[points.length - 2];
  const atStart = edge.arrow === 'composite';
  const anchor = atStart ? first : last;
  const toward = atStart ? second : prev;
  const dx = anchor[0] - toward[0];
  const dy = anchor[1] - toward[1];
  const length = Math.hypot(dx, dy) || 1;
  return { anchor, ux: dx / length, uy: dy / length };
}

/** Filled diamond, pointing back along the line into the owner. */
export function compositeHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const size = 5;
  const px = -uy;
  const py = ux;
  const back = [x + ux * size * 2, y + uy * size * 2];
  return `M${x},${y} L${x + ux * size + px * size},${y + uy * size + py * size} `
    + `L${back[0]},${back[1]} `
    + `L${x + ux * size - px * size},${y + uy * size - py * size} Z`;
}

/** Hollow triangle: the `:|>` decoration, not a plain specialization triangle. */
export function typingHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const w = 4.5;
  const h = 7;
  const px = -uy;
  const py = ux;
  return `M${x + ux * h * 0.4},${y + uy * h * 0.4} `
    + `L${x - ux * h * 0.6 + px * w},${y - uy * h * 0.6 + py * w} `
    + `L${x - ux * h * 0.6 - px * w},${y - uy * h * 0.6 - py * w} Z`;
}

/** The two dots that make the typing symbol `:|>` rather than a bare triangle. */
export function typingDots({ anchor: [x, y], ux, uy }: ArrowGeometry): Point[] {
  const px = -uy;
  const py = ux;
  return [-1, 1].map((side) => [x + ux * 16 + px * 3 * side, y + uy * 16 + py * 3 * side]);
}

/** Open double tick, the `satisfy` marker. */
export function satisfyHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const px = -uy;
  const py = ux;
  const len = 4;
  return `M${x},${y} l${-(ux * len) + px * len},${-(uy * len) + py * len} `
    + `M${x},${y} l${-(ux * len) - px * len},${-(uy * len) - py * len}`;
}

export interface LabelGeometry {
  x: number;
  y: number;
  width: number;
}

/**
 * Where an edge label goes: the midpoint of the longest run, which is where a reader looks.
 *
 * The run lengths are measured in the same units as the picture, so this does not need the box
 * geometry — only the polyline the server already placed.
 */
export function edgeLabelAt(edge: CanvasEdge): LabelGeometry | null {
  const points = edge.points;
  if (!edge.label || points.length < 2) return null;
  let best = 1;
  let bestLength = -1;
  for (let i = 1; i < points.length; i += 1) {
    const length = Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
    if (length > bestLength) {
      bestLength = length;
      best = i;
    }
  }
  const a = points[best - 1];
  const b = points[best];
  return {
    x: (a[0] + b[0]) / 2,
    y: (a[1] + b[1]) / 2,
    width: edge.label.length * 5.2 + 6,
  };
}

/** The box the picture has to fit into: the canvas pane's own size. */
export interface Viewport {
  width: number;
  height: number;
}

/**
 * Camera that fits the picture, floored at the zoom where labels stay readable.
 *
 * The floor keeps labels legible, and a picture slightly larger than the viewport is easier to
 * read that way. A picture several times the viewport is not: at the floor only its top-left
 * corner is on screen, and a reader pressing `fit` sees boxes with no lines between them while
 * the lines sit outside the window. So the floor applies only while it still lets a useful part
 * of both axes through; past that, fitting wins and the whole picture is shown.
 */
export function fitCamera(doc: CanvasDocument | null, box: Viewport | null): Camera | null {
  if (!doc || !box || !doc.width || !doc.height) return null;
  const pad = 48;
  const zoom = Math.min(
    (box.width - pad * 2) / doc.width,
    (box.height - pad * 2) / doc.height,
    1.4,
  );
  const fitted = Math.max(zoom, 0.08);
  const floor_hides_most =
    doc.width * MIN_READABLE_ZOOM > box.width * 2.5 ||
    doc.height * MIN_READABLE_ZOOM > box.height * 2.5;
  const applied = floor_hides_most ? fitted : Math.max(fitted, MIN_READABLE_ZOOM);
  // Centre only when the whole picture fits. Otherwise anchor the top-left corner, so the first
  // thing on screen is the start of the diagram rather than the empty middle of it.
  const fits = doc.width * applied <= box.width && doc.height * applied <= box.height;
  return {
    zoom: applied,
    x: fits ? (box.width - doc.width * applied) / 2 : pad,
    y: fits ? (box.height - doc.height * applied) / 2 : pad,
  };
}
