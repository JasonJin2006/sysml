import type { Camera, CanvasDocument, CanvasEdge, Point, Rect } from './types';

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
  const atStart = edge.arrow === 'composite';
  const anchor = atStart ? points[0] : points[points.length - 1];
  const candidates = atStart ? points.slice(1) : points.slice(0, -1).reverse();
  const toward = candidates.find(([x, y]) => Math.hypot(x - anchor[0], y - anchor[1]) > 0.01);
  if (!toward) return null;
  const dx = anchor[0] - toward[0];
  const dy = anchor[1] - toward[1];
  const length = Math.hypot(dx, dy);
  return { anchor, ux: dx / length, uy: dy / length };
}

/** Filled diamond, touching the owner and extending out along the line. */
export function compositeHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const length = 8;
  const half = 4;
  const px = -uy;
  const py = ux;
  return `M${x},${y} L${x - ux * length + px * half},${y - uy * length + py * half} `
    + `L${x - ux * length * 2},${y - uy * length * 2} `
    + `L${x - ux * length - px * half},${y - uy * length - py * half} Z`;
}

/** Hollow triangle: its tip touches the target border, its base stays outside. */
export function typingHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const half = 4;
  const length = 8;
  const px = -uy;
  const py = ux;
  return `M${x},${y} `
    + `L${x - ux * length + px * half},${y - uy * length + py * half} `
    + `L${x - ux * length - px * half},${y - uy * length - py * half} Z`;
}

/** The typing dots sit behind the triangle, outside the target. */
export function typingDots({ anchor: [x, y], ux, uy }: ArrowGeometry): Point[] {
  const px = -uy;
  const py = ux;
  return [-1, 1].map((side) => [x - ux * 12 + px * 3 * side, y - uy * 12 + py * 3 * side]);
}

/** Open head for annotation and transition edges. */
export function satisfyHeadD({ anchor: [x, y], ux, uy }: ArrowGeometry): string {
  const px = -uy;
  const py = ux;
  const length = 8;
  const half = 4;
  return `M${x},${y} l${-ux * length + px * half},${-uy * length + py * half} `
    + `M${x},${y} l${-ux * length - px * half},${-uy * length - py * half}`;
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
  // A pane smaller than this has not been laid out yet. Fitting to it would zoom the picture down
  // to the 0.08 fallback and leave it a speck, and nothing re-runs the fit afterwards. Returning
  // null leaves the camera alone; the caller retries once the pane has a real size.
  if (box.width < 240 || box.height < 240) return null;
  const pad = 48;
  const points: Point[] = [
    [0, 0], [doc.width, doc.height],
    ...[...doc.nodes, ...doc.frames].flatMap((r): Point[] => [[r.x, r.y], [r.x + r.w, r.y + r.h]]),
    ...doc.edges.flatMap((edge) => edge.points),
  ];
  const minX = Math.min(...points.map(([x]) => x));
  const minY = Math.min(...points.map(([, y]) => y));
  const width = Math.max(...points.map(([x]) => x)) - minX;
  const height = Math.max(...points.map(([, y]) => y)) - minY;
  const zoom = Math.min(
    (box.width - pad * 2) / width,
    (box.height - pad * 2) / height,
    1.4,
  );
  const fitted = Math.max(zoom, 0.08);
  const floor_hides_most =
    width * MIN_READABLE_ZOOM > box.width * 2.5 ||
    height * MIN_READABLE_ZOOM > box.height * 2.5;
  const applied = floor_hides_most ? fitted : Math.max(fitted, MIN_READABLE_ZOOM);
  const fits = width * applied <= box.width && height * applied <= box.height;
  return {
    zoom: applied,
    x: (fits ? (box.width - width * applied) / 2 : pad) - minX * applied,
    y: (fits ? (box.height - height * applied) / 2 : pad) - minY * applied,
  };
}

/** Translate the moving endpoint, leaving an omitted endpoint fixed. */
export function shiftEdge(
  edge: CanvasEdge,
  from: { dx: number; dy: number } | undefined,
  to: { dx: number; dy: number } | undefined,
): CanvasEdge {
  const source = from ?? { dx: 0, dy: 0 };
  const target = to ?? { dx: 0, dy: 0 };
  const points = edge.points;
  if (points.length < 2) return edge;
  if (source.dx === target.dx && source.dy === target.dy) {
    if (source.dx === 0 && source.dy === 0) return edge;
    const translate = ([x, y]: Point): Point => [x + source.dx, y + source.dy];
    return { ...edge, points: points.map(translate), hops: edge.hops.map(translate) };
  }
  const first = points[0];
  const last = points[points.length - 1];
  const next = points.find(([x, y]) => Math.hypot(x - first[0], y - first[1]) > 0.01);
  const previous = [...points].reverse().find(([x, y]) => Math.hypot(x - last[0], y - last[1]) > 0.01);
  if (!next || !previous) return edge;
  const start: Point = [first[0] + source.dx, first[1] + source.dy];
  const end: Point = [last[0] + target.dx, last[1] + target.dy];
  const direction = (a: Point, b: Point): Point => {
    const horizontal = Math.abs(b[0] - a[0]) > Math.abs(b[1] - a[1]);
    return horizontal ? [Math.sign(b[0] - a[0]), 0] : [0, Math.sign(b[1] - a[1])];
  };
  const startDirection = direction(first, next);
  const endDirection = direction(last, previous);
  const lead = 20;
  const exit: Point = [start[0] + startDirection[0] * lead, start[1] + startDirection[1] * lead];
  const entry: Point = [end[0] + endDirection[0] * lead, end[1] + endDirection[1] * lead];
  let bends: Point[];
  if (startDirection[0] === 0 && endDirection[0] === 0) {
    const y = (exit[1] + entry[1]) / 2;
    bends = [[exit[0], y], [entry[0], y]];
  } else if (startDirection[1] === 0 && endDirection[1] === 0) {
    const x = (exit[0] + entry[0]) / 2;
    bends = [[x, exit[1]], [x, entry[1]]];
  } else {
    bends = [startDirection[0] !== 0 ? [entry[0], exit[1]] : [exit[0], entry[1]]];
  }
  const moved = [start, exit, ...bends, entry, end].filter(
    (point, index, all) => index === 0 || point[0] !== all[index - 1][0] || point[1] !== all[index - 1][1],
  );
  // A changed route has different crossings; stale jump arcs must not float beside it.
  return { ...edge, points: moved, hops: [] };
}

/** Normalize legacy layout direction, then map attachment points onto the current rectangles. */
export function placeEdge(
  edge: CanvasEdge,
  from: Rect | undefined,
  to: Rect | undefined,
  movedFrom: Rect | undefined,
  movedTo: Rect | undefined,
): CanvasEdge {
  if (edge.points.length < 2) return edge;
  let directed = edge;
  if (from && to) {
    const distance = ([x, y]: Point, rect: Rect): number => {
      const dx = Math.max(rect.x - x, 0, x - rect.x - rect.w);
      const dy = Math.max(rect.y - y, 0, y - rect.y - rect.h);
      if (dx || dy) return Math.hypot(dx, dy);
      return Math.min(x - rect.x, rect.x + rect.w - x, y - rect.y, rect.y + rect.h - y);
    };
    const first = edge.points[0];
    const last = edge.points[edge.points.length - 1];
    const forward = distance(first, from) + distance(last, to);
    const backward = distance(first, to) + distance(last, from);
    if (backward + 0.01 < forward) directed = { ...edge, points: [...edge.points].reverse() };
  }
  const displacement = (point: Point, before: Rect | undefined, after: Rect | undefined) => {
    if (!before || !after) return undefined;
    return {
      dx: after.x + (point[0] - before.x) * after.w / before.w - point[0],
      dy: after.y + (point[1] - before.y) * after.h / before.h - point[1],
    };
  };
  return shiftEdge(
    directed,
    displacement(directed.points[0], from, movedFrom),
    displacement(directed.points[directed.points.length - 1], to, movedTo),
  );
}
