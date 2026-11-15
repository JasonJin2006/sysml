import { useId } from 'react';
import type { CanvasJudgement, CanvasNode, Display, Rect } from './types';
import { actorFigure, forStereotype, headerInk } from './style';
/**
 * One box: body, stereotype, label, compartment rows, ports, resize handle.
 *
 * `rect` is passed in rather than read off the node so a drag can render from the in-flight
 * geometry without copying the node or waiting for the server.
 */
export function NodeShape({
  node,
  rect,
  selected,
  display,
  judgement,
  onPointerDown,
  view = 'gv',
}: {
  node: CanvasNode;
  rect: Rect;
  selected: boolean;
  display: Display;
  judgement: CanvasJudgement | null;
  onPointerDown: (event: React.PointerEvent<SVGGElement>) => void;
  view?: string;
}) {
  const { x, y, w, h } = rect;
  const clipId = useId();
  const cx = x + w / 2;
  const classes = [
    'node',
    node.is_def ? 'is-def' : '',
    node.unmet ? 'unsure' : '',
    selected ? 'selected' : '',
  ].filter(Boolean).join(' ');
  const style = forStereotype(node.stereotype);
  const ink = style.header.startsWith('var(') ? 'var(--ui-text)' : headerInk(style);

  if (view === 'sv') {
    const headerHeight = Math.min(46, h);
    return (
      <g className={`${classes} sequence-participant`} data-key={node.key}>
        <rect x={x} y={y} width={w} height={h} fill="transparent" pointerEvents="none" />
        <line
          x1={cx}
          x2={cx}
          y1={y + headerHeight}
          y2={y + h}
          stroke="var(--ui-edge)"
          strokeWidth={0.8}
          strokeDasharray="5 4"
          pointerEvents="none"
        />
        <g onPointerDown={onPointerDown}>
          <rect
            x={x}
            y={y}
            width={w}
            height={headerHeight}
            className="node-body"
            style={{ fill: style.header, stroke: selected ? 'var(--ui-accent)' : style.border }}
          />
          {display.stereotype && (
            <text x={cx} y={y + 17} textAnchor="middle" className="stereo" style={{ fill: ink }}>
              {`«${node.stereotype}»`}
            </text>
          )}
          <text x={cx} y={y + (display.stereotype ? 32 : 26)} textAnchor="middle" className="label" style={{ fill: ink }}>
            {node.label}
          </text>
        </g>
      </g>
    );
  }

  if (['initial', 'done', 'decision', 'merge', 'fork', 'join'].includes(node.symbol)) {
    const symbolInk = selected ? 'var(--ui-accent)' : 'var(--ui-text)';
    const strokeWidth = selected ? 2 : 1;
    return (
      <g className={`${classes} symbol-${node.symbol}`} data-key={node.key} onPointerDown={onPointerDown}>
        <rect x={x} y={y} width={w} height={h} fill="transparent" className="node-hit" />
        {node.symbol === 'initial' ? (
          <circle cx={cx} cy={y + h / 2} r={Math.min(w, h) / 2} fill={symbolInk} stroke={symbolInk} strokeWidth={strokeWidth} />
        ) : node.symbol === 'done' ? (
          <g>
            <circle cx={cx} cy={y + h / 2} r={Math.min(w, h) / 2} fill="none" stroke={symbolInk} strokeWidth={strokeWidth} />
            <circle cx={cx} cy={y + h / 2} r={Math.min(w, h) * 0.3} fill={symbolInk} />
          </g>
        ) : node.symbol === 'decision' || node.symbol === 'merge' ? (
          <path d={`M${cx} ${y} L${x + w} ${y + h / 2} L${cx} ${y + h} L${x} ${y + h / 2} Z`} fill="var(--ui-canvas)" stroke={symbolInk} strokeWidth={strokeWidth} />
        ) : (
          <rect x={x} y={y} width={w} height={h} fill={symbolInk} stroke={symbolInk} strokeWidth={strokeWidth} />
        )}
        {node.symbol !== 'initial' && node.symbol !== 'done' && node.label && (
          <text x={cx} y={y + h + 12} textAnchor="middle" className="row" style={{ fill: 'var(--ui-text)' }} pointerEvents="none">{node.label}</text>
        )}
      </g>
    );
  }

  if (node.symbol === 'actor') {
    return (
      <g className={classes} data-key={node.key} onPointerDown={onPointerDown}>
        <path
          className="node-hit"
          d={`M${x} ${y}h${w}v${h}h${-w}Z`}
          fill="transparent"
        />
        <g dangerouslySetInnerHTML={{ __html: actorFigure(cx, y + 4, selected ? 'var(--ui-accent)' : 'var(--ui-text)') }} />
        {display.stereotype && (
          <text x={cx} y={y + 110} textAnchor="middle" className="stereo" style={{ fill: 'var(--ui-text3)' }}>
            {`«${node.stereotype}»`}
          </text>
        )}
        <text x={cx} y={y + 123} textAnchor="middle" className="label" style={{ fill: 'var(--ui-text)' }}>{node.label}</text>
      </g>
    );
  }

  let rowY = y + 34;
  const rows: React.ReactNode[] = [];
  if (node.rows.length > 0) {
    rows.push(
      <line key="rule" x1={x} x2={x + w} y1={rowY} y2={rowY} stroke={style.border} strokeWidth={0.5} />,
    );
    if (node.stereotype.split(/\s+/).includes('state')) {
      rows.push(
        <text key="state-actions" x={cx} y={rowY + 11} textAnchor="middle" style={{ fill: 'var(--ui-text3)', fontSize: 9 }}>state actions</text>,
      );
      rowY += 14;
    }
    if (display.members) {
      for (const row of node.rows) {
        rows.push(
          <text key={`row-${rowY}`} x={x + 8} y={rowY + 12} className="row" style={{ fill: 'var(--ui-text2)' }}>{row}</text>,
        );
        rowY += 16;
      }
    } else {
      // The compartment is still there and still has content; an ellipsis says "hidden", where an
      // empty box would say "this element has no members".
      rows.push(
        <text key="row-hidden" x={cx} y={rowY + 12} textAnchor="middle" className="row" style={{ fill: 'var(--ui-text2)' }}>…</text>,
      );
      rowY += 16;
    }
  }

  // Ports: a small square straddling the border, its name outside the frame. They start below the
  // compartment and stop at the bottom edge rather than drawing over it.
  const ports: React.ReactNode[] = [];
  if (display.ports) {
    let leftSlot = 0;
    let rightSlot = 0;
    for (const port of node.ports) {
      const side = port.side === 'left' ? -1 : 1;
      const slot = port.side === 'left' ? (leftSlot += 1) : (rightSlot += 1);
      const py = rowY + 9 + (slot - 1) * 18;
      if (py > y + h - 8) break;
      const px = side < 0 ? x : x + w;
      ports.push(
        <rect key={`port-${port.name}`} x={px - 5} y={py - 5} width={10} height={10} fill="var(--ui-node-fill)" stroke="var(--ui-text2)" />,
        <text
          key={`port-name-${port.name}`}
          x={px + side * 9}
          y={py + 3.5}
          textAnchor={side < 0 ? 'end' : 'start'}
          className="port-name"
          style={{ fill: 'var(--ui-text2)' }}
        >
          {port.name}
        </text>,
      );
    }
  }

  const radius = node.is_def ? 0 : 6;

  return (
    <g className={classes} data-key={node.key} onPointerDown={onPointerDown}>
      <defs>
        <clipPath id={clipId}>
          <rect x={x} y={y} width={w} height={h} rx={radius} />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clipId})`}>
        <rect x={x} y={y} width={w} height={h} fill={style.body} />
        <rect x={x} y={y} width={w} height={Math.min(34, h)} fill={style.header} className="node-band" />
        {display.stereotype && (
          <text x={cx} y={y + 13} textAnchor="middle" className="stereo" style={{ fill: ink }}>
            {`«${node.stereotype}»`}
          </text>
        )}
        <text
          x={cx}
          y={display.stereotype ? y + 25 : y + 20}
          textAnchor="middle"
          className="label"
          style={{ fill: ink }}
        >
          {node.label}
        </text>
        {rows}
      </g>
      {/* The outline paints last so a full-height header cannot cover its stroke. */}
      <rect
        x={x}
        y={y}
        width={w}
        height={h}
        rx={radius}
        className="node-body"
        style={{ fill: 'none', stroke: selected ? 'var(--ui-accent)' : node.unmet ? 'var(--ui-red-edge)' : style.border }}
        pointerEvents="none"
      />
      {ports}
      {judgement && <VerdictBadge judgement={judgement} x={x + w - 9} y={y + 9} />}
      <rect
        x={x + w - 7}
        y={y + h - 7}
        width={7}
        height={7}
        className="node-resize"
        data-handle="resize"
      />
    </g>
  );
}

/**
 * The verdict, on the box, where a reviewer cannot miss it.
 *
 * A hollow badge means the record is stale: it was judged against a model state that no longer
 * exists, so the tick is history rather than an answer. Drawing it filled would be the one lie
 * this product cannot afford.
 */
export function VerdictBadge({
  judgement,
  x,
  y,
}: {
  judgement: CanvasJudgement;
  x: number;
  y: number;
}) {
  const glyph = {
    pass: '✓',
    fail: '✗',
    inconclusive: '?',
    error: '!',
  }[judgement.verdict] ?? '?';
  const title = [
    `${judgement.verdict}${judgement.stale ? ' (stale)' : ''}`,
    judgement.detail,
    judgement.judged_by ? `judged by ${judgement.judged_by}` : '',
    judgement.at,
    judgement.stale ? `judged against ${judgement.judged_against}` : '',
  ].filter(Boolean).join(' ');
  return (
    <g className={`judge ${judgement.verdict}${judgement.stale ? ' stale' : ''}`}>
      <title>{title}</title>
      <circle cx={x} cy={y} r={7} />
      <text x={x} y={y + 3.2} textAnchor="middle">{glyph}</text>
    </g>
  );
}
