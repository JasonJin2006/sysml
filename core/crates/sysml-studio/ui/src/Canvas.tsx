import { useCallback, useRef, useState } from 'react';
import type { Camera, CanvasDocument, CanvasNode, Display, Rect } from './types';
// `fitCamera` lives in `geometry.ts` so the tests can import it without pulling in React; this
// re-export is what `App` and the toolbar reach it through.
export { fitCamera } from './geometry';
import { EdgeLayer, FrameLayer, MarksLayer, NodeLayer } from './Scene';
import type { Marks } from './marks';
import { t } from './i18n';

/** One box's rectangle, keyed by qualified name: what the placement routes take. */
export interface Placement extends Rect {
  key: string;
}

type Drag =
  | { kind: 'pan'; startX: number; startY: number; grabX: number; grabY: number; moved: boolean }
  | { kind: 'move'; starts: Placement[]; grab: { x: number; y: number }; moved: boolean }
  | { kind: 'resize'; start: Placement; grab: { x: number; y: number }; moved: boolean };

/**
 * The drawing surface and every gesture on it.
 *
 * The camera lives in the parent because the toolbar's `fit` button has to move it too. Drags stay
 * in a ref: a pointermove is not a state change anyone else needs to see, and routing each one
 * through React state would re-render the whole document at pointer speed.
 */
export function Canvas({
  doc,
  view,
  camera,
  setCamera,
  selection,
  select,
  display,
  marks,
  overrides,
  setOverrides,
  onPlaced,
  onDragChange,
  viewportRef,
}: {
  doc: CanvasDocument | null;
  view: string;
  camera: Camera;
  setCamera: (camera: Camera) => void;
  selection: string[];
  /** Replace the selection, or toggle one key into it. */
  select: (keys: string[], additive?: boolean) => void;
  display: Display;
  /** What changed since the baseline, or null when nothing is tracked or nothing changed. */
  marks: Marks | null;
  overrides: Record<string, Rect>;
  setOverrides: (rects: Record<string, Rect>) => void;
  onPlaced: (placements: Placement[]) => void;
  onDragChange: (dragging: boolean) => void;
  /** Owned by the app: `fit` has to measure the same box the gestures do. */
  viewportRef: React.RefObject<HTMLDivElement | null>;
}) {
  const viewport = viewportRef;
  const drag = useRef<Drag | null>(null);
  const [cursor, setCursor] = useState<'pan' | 'node' | null>(null);

  const toWorld = useCallback((clientX: number, clientY: number) => {
    const box = viewport.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    return {
      x: (clientX - box.left - camera.x) / camera.zoom,
      y: (clientY - box.top - camera.y) / camera.zoom,
    };
  }, [camera]);

  const rectOf = useCallback((key: string): Rect | null => {
    const override = overrides[key];
    if (override) return override;
    const node = doc?.nodes.find((n) => n.key === key);
    return node ? { x: node.x, y: node.y, w: node.w, h: node.h } : null;
  }, [doc, overrides]);

  const beginDrag = useCallback((event: React.PointerEvent<SVGGElement>, node: CanvasNode) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    const additive = event.ctrlKey || event.metaKey || event.shiftKey;
    // Ctrl-click adds or removes a box without starting a gesture; a plain click on a box that is
    // already part of the selection drags the whole group, which is what the selected outline
    // promises.
    if (additive) {
      select([node.key], true);
      return;
    }
    const resizing = (event.target as Element).classList.contains('node-resize');
    if (resizing) {
      const start = rectOf(node.key);
      if (!start) return;
      select([node.key]);
      drag.current = {
        kind: 'resize',
        start: { key: node.key, ...start },
        grab: toWorld(event.clientX, event.clientY),
        moved: false,
      };
    } else {
      const keys = selection.includes(node.key) ? selection : [node.key];
      select(keys);
      const starts = keys
        .map((key) => {
          const rect = rectOf(key);
          return rect ? { key, ...rect } : null;
        })
        .filter((p): p is Placement => p !== null);
      if (starts.length === 0) return;
      drag.current = {
        kind: 'move',
        starts,
        grab: toWorld(event.clientX, event.clientY),
        moved: false,
      };
    }
    onDragChange(true);
    setCursor('node');
    viewport.current?.setPointerCapture(event.pointerId);
  }, [onDragChange, rectOf, select, selection, toWorld]);

  const onPointerDown = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    drag.current = {
      kind: 'pan',
      startX: camera.x,
      startY: camera.y,
      grabX: event.clientX,
      grabY: event.clientY,
      moved: false,
    };
    setCursor('pan');
    viewport.current?.setPointerCapture(event.pointerId);
  }, [camera]);

  const onPointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (!active) return;
    if (active.kind === 'pan') {
      const dx = event.clientX - active.grabX;
      const dy = event.clientY - active.grabY;
      if (Math.abs(dx) > 2 || Math.abs(dy) > 2) active.moved = true;
      setCamera({ ...camera, x: active.startX + dx, y: active.startY + dy });
      return;
    }
    const at = toWorld(event.clientX, event.clientY);
    const dx = Math.round(at.x - active.grab.x);
    const dy = Math.round(at.y - active.grab.y);
    if (Math.abs(dx) > 0 || Math.abs(dy) > 0) active.moved = true;
    if (active.kind === 'move') {
      const moved: Record<string, Rect> = {};
      for (const start of active.starts) {
        moved[start.key] = { x: start.x + dx, y: start.y + dy, w: start.w, h: start.h };
      }
      setOverrides(moved);
    } else {
      setOverrides({
        [active.start.key]: {
          x: active.start.x,
          y: active.start.y,
          w: Math.max(70, active.start.w + dx),
          h: Math.max(44, active.start.h + dy),
        },
      });
    }
  }, [camera, setCamera, setOverrides, toWorld]);

  const onPointerUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    drag.current = null;
    onDragChange(false);
    setCursor(null);
    // Releasing a capture the element does not hold throws, and an exception here would skip the
    // save below: the drag would look like it worked and be gone on the next reload.
    try {
      viewport.current?.releasePointerCapture(event.pointerId);
    } catch {
      /* no capture to release */
    }
    if (!active) return;
    if (active.kind === 'pan') {
      if (!active.moved) select([]);
      return;
    }
    if (!active.moved) return;
    const starts = active.kind === 'move' ? active.starts : [active.start];
    const placements: Placement[] = [];
    for (const start of starts) {
      const rect = rectOf(start.key);
      if (rect) placements.push({ key: start.key, ...rect });
    }
    if (placements.length > 0) onPlaced(placements);
  }, [onDragChange, onPlaced, rectOf, select]);

  const onWheel = useCallback((event: React.WheelEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey) event.preventDefault();
    const box = viewport.current?.getBoundingClientRect();
    if (!box) return;
    const mx = event.clientX - box.left;
    const my = event.clientY - box.top;
    const before = {
      x: (mx - camera.x) / camera.zoom,
      y: (my - camera.y) / camera.zoom,
    };
    const factor = Math.exp(-event.deltaY * 0.0016);
    const zoom = Math.min(4, Math.max(0.08, camera.zoom * factor));
    setCamera({ zoom, x: mx - before.x * zoom, y: my - before.y * zoom });
  }, [camera, setCamera]);

  const empty = doc !== null && doc.nodes.length === 0;

  return (
    <div
      className={`viewport${cursor ? ` ${cursor === 'pan' ? 'dragging' : 'moving-node'}` : ''}`}
      ref={viewport}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onWheel={onWheel}
    >
      <svg id="scene" xmlns="http://www.w3.org/2000/svg">
        <g transform={`translate(${camera.x} ${camera.y}) scale(${camera.zoom})`}>
          {doc && (
            <>
              <EdgeLayer doc={doc} select={(key) => select([key])} />
              <FrameLayer frames={doc.frames ?? []} selection={selection} select={(key) => select([key])} />
              <NodeLayer
                doc={doc}
                selection={selection}
                display={display}
                overrides={overrides}
                beginDrag={beginDrag}
              />
              {marks && <MarksLayer doc={doc} marks={marks} overrides={overrides} />}
            </>
          )}
        </g>
      </svg>
      {empty && (
        <div className="empty">
          {view === 'iv'
            ? t('The interconnection view draws instances only. This model declares definitions and no part usages.')
            : t('Nothing in this model can be drawn.')}
        </div>
      )}
      <div className="hint">
        {t('Drag a box to move it, drag its corner to resize, scroll to zoom, drag the background to pan')}
      </div>
    </div>
  );
}

