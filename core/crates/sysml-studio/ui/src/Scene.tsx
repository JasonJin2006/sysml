import { useMemo } from 'react';
import type { CanvasDocument, CanvasEdge, CanvasFrame, CanvasNode, Display, Rect } from './types';
import { arrowGeometry, compositeHeadD, edgeLabelAt, edgePath, satisfyHeadD, typingDots, typingHeadD } from './geometry';
import { NodeShape } from './NodeShape';
import type { Marks } from './marks';

/**
 * Packages: a rectangle with its name in a title band.
 *
 * A package is a container, not a box with attributes, so it gets its own component rather than a
 * branch inside `NodeShape`. It is not draggable: moving a frame without moving its members would
 * put a package somewhere its contents are not.
 *
 * Only the title band selects the package. The body stays transparent to the canvas, so dragging
 * anywhere inside a large package still pans, which is what the inside of a frame looks like it
 * should do.
 */
export function FrameLayer({
  frames,
  selection,
  select,
}: {
  frames: CanvasFrame[];
  selection: string[];
  select: (key: string) => void;
}) {
  const chosen = new Set(selection);
  return (
    <>
      {frames.map((frame) => (
        <g key={frame.key} className={`frame${chosen.has(frame.key) ? ' selected' : ''}`} data-key={frame.key}>
          <rect x={frame.x} y={frame.y} width={frame.w} height={frame.h} className="frame-body" />
          <line
            x1={frame.x}
            x2={frame.x + frame.w}
            y1={frame.y + frame.title}
            y2={frame.y + frame.title}
            className="frame-rule"
          />
          <text x={frame.x + 8} y={frame.y + frame.title - 6} className="frame-title">{frame.label}</text>
          <rect
            x={frame.x}
            y={frame.y}
            width={frame.w}
            height={frame.title}
            className="frame-hit"
            onPointerDown={(event) => {
              if (event.button !== 0) return;
              event.stopPropagation();
              select(frame.key);
            }}
          />
        </g>
      ))}
    </>
  );
}

/**
 * Edges, drawn in two passes.
 *
 * Ink is one path per arrow kind, not one per relationship: edges that share a run (an owner's
 * fan-out, a typing trunk) used to be drawn as separate elements on top of each other, and each
 * element is composited again, so the shared stretch came out visibly darker than its neighbours.
 * Sub-paths inside one element stroke once.
 *
 * Hit-testing does not go through those: every edge has its own transparent polyline, drawn last
 * so it is on top, which is what a click lands on. Merging only the ink keeps selection exact.
 */
export function EdgeLayer({
  doc,
  select,
}: {
  doc: CanvasDocument;
  select: (key: string) => void;
}) {
  const layers = useMemo(() => {
    const ink = new Map<string, string[]>();
    for (const edge of doc.edges) {
      const parts = ink.get(edge.arrow);
      if (parts) parts.push(edgePath(edge));
      else ink.set(edge.arrow, [edgePath(edge)]);
    }
    return [...ink.entries()].map(([kind, parts]) => ({ kind, d: parts.join(' ') }));
  }, [doc.edges]);

  return (
    <>
      {layers.map(({ kind, d }) => (
        <path key={`ink-${kind}`} d={d} fill="none" className={`edge ${kind}`} vectorEffect="non-scaling-stroke" />
      ))}
      {doc.edges.map((edge) => <EdgeLabel key={`label-${edge.index}`} edge={edge} />)}
      {doc.edges.map((edge) => <EdgeHead key={`head-${edge.index}`} edge={edge} />)}
      {doc.edges.map((edge) => (
        <polyline
          key={`hit-${edge.index}`}
          points={edge.points.map((p) => `${p[0]},${p[1]}`).join(' ')}
          className="edge hit"
          onClick={(event) => {
            event.stopPropagation();
            // A relationship belongs to the element that states it. Falling back to the target
            // covers the directions where the source is not drawn.
            select(edge.from_key || edge.to_key);
          }}
        />
      ))}
    </>
  );
}

function EdgeHead({ edge }: { edge: CanvasEdge }) {
  const geo = arrowGeometry(edge);
  if (!geo) return null;
  if (edge.arrow === 'composite') {
    return <path d={compositeHeadD(geo)} fill="#000" stroke="#000" strokeWidth={0.8} />;
  }
  if (edge.arrow === 'typing') {
    return (
      <g>
        <path d={typingHeadD(geo)} fill="#fff" stroke="#000" strokeWidth={1} />
        {typingDots(geo).map(([cx, cy], i) => (
          <circle key={i} cx={cx} cy={cy} r={1.4} fill="#000" />
        ))}
      </g>
    );
  }
  // An annotation ends in the same open head a satisfaction does; the dashed stroke that tells
  // them apart is on the ink layer, where the class carries the arrow kind.
  if (edge.arrow === 'satisfy' || edge.arrow === 'annotate') {
    return <path d={satisfyHeadD(geo)} stroke="#000" strokeWidth={1.1} fill="none" />;
  }
  return null;
}

function EdgeLabel({ edge }: { edge: CanvasEdge }) {
  const at = edgeLabelAt(edge);
  if (!at || !edge.label) return null;
  return (
    <g>
      <rect
        x={at.x - at.width / 2}
        y={at.y - 10}
        width={at.width}
        height={12}
        className="edge-label-plate"
      />
      <text x={at.x} y={at.y - 1} textAnchor="middle" className="edge-label">{edge.label}</text>
    </g>
  );
}

/** The boxes: everything except packages, whose rectangles are their frames. */
export function NodeLayer({
  doc,
  selection,
  display,
  overrides,
  beginDrag,
}: {
  doc: CanvasDocument;
  selection: string[];
  display: Display;
  overrides: Record<string, Rect>;
  beginDrag: (event: React.PointerEvent<SVGGElement>, node: CanvasNode) => void;
}) {
  const chosen = new Set(selection);
  const verdicts = new Map((doc.judgements ?? []).map((j) => [j.key, j]));
  return (
    <>
      {doc.nodes.map((node) => {
        if (node.is_frame) return null;
        const rect = overrides[node.key] ?? node;
        return (
          <NodeShape
            key={node.key}
            node={node}
            rect={rect}
            selected={chosen.has(node.key)}
            display={display}
            judgement={verdicts.get(node.key) ?? null}
            onPointerDown={(event) => beginDrag(event, node)}
          />
        );
      })}
    </>
  );
}

/**
 * What changed since the baseline, drawn around the boxes.
 *
 * Three marks, each answering a different question: a halo says "this is new", a ghost says "this
 * is gone" with the name it had, and a plate under a box says "this number moved, from here to
 * there". The ghost exists because a deleted box and a box that was never there look identical
 * once it is gone.
 */
export function MarksLayer({
  doc,
  marks,
  overrides,
}: {
  doc: CanvasDocument;
  marks: Marks;
  overrides: Record<string, Rect>;
}) {
  const byKey = new Map(doc.nodes.map((n) => [n.key, n]));
  const halos: React.ReactNode[] = [];
  for (const key of marks.added) {
    const node = byKey.get(key);
    if (!node) continue;
    const rect = overrides[key] ?? node;
    halos.push(
      <rect
        key={`new-${key}`}
        x={rect.x - 3}
        y={rect.y - 3}
        width={rect.w + 6}
        height={rect.h + 6}
        rx={10}
        className="mark-new"
      />,
    );
  }

  const changed: React.ReactNode[] = [];
  for (const [key, change] of marks.values) {
    const node = byKey.get(key);
    if (!node) continue;
    const rect = overrides[key] ?? node;
    const text = `${change.from} → ${change.to}`;
    const width = text.length * 5.4 + 10;
    changed.push(
      <g key={`changed-${key}`} className="mark-change">
        <title>{key}</title>
        <rect x={rect.x} y={rect.y + rect.h + 4} width={width} height={14} rx={3} />
        <text x={rect.x + 5} y={rect.y + rect.h + 14}>{text}</text>
      </g>,
    );
  }

  return (
    <>
      {marks.removed.map((ghost) => (
        <g key={`ghost-${ghost.key}`} className="mark-ghost">
          <rect
            x={ghost.rect.x}
            y={ghost.rect.y}
            width={ghost.rect.w}
            height={ghost.rect.h}
            rx={8}
          />
          <text
            x={ghost.rect.x + ghost.rect.w / 2}
            y={ghost.rect.y + ghost.rect.h / 2 + 4}
            textAnchor="middle"
            className="ghost-label"
          >
            {ghost.label}
          </text>
        </g>
      ))}
      {halos}
      {changed}
    </>
  );
}
