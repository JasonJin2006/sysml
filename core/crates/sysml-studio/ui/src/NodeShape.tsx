import type { CanvasJudgement, CanvasNode, Display, Rect } from './types';
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
}: {
  node: CanvasNode;
  rect: Rect;
  selected: boolean;
  display: Display;
  judgement: CanvasJudgement | null;
  onPointerDown: (event: React.PointerEvent<SVGGElement>) => void;
}) {
  const { x, y, w, h } = rect;
  const cx = x + w / 2;
  const classes = [
    'node',
    node.is_def ? 'is-def' : '',
    node.unmet ? 'unsure' : '',
    selected ? 'selected' : '',
  ].filter(Boolean).join(' ');

  let rowY = y + 34;
  const rows: React.ReactNode[] = [];
  if (node.rows.length > 0) {
    rows.push(
      <line key="rule" x1={x} x2={x + w} y1={rowY} y2={rowY} stroke="#000" strokeWidth={0.8} />,
    );
    if (display.members) {
      for (const row of node.rows) {
        rows.push(
          <text key={`row-${rowY}`} x={x + 8} y={rowY + 12} className="row">{row}</text>,
        );
        rowY += 16;
      }
    } else {
      // The compartment is still there and still has content; an ellipsis says "hidden", where an
      // empty box would say "this element has no members".
      rows.push(
        <text key="row-hidden" x={cx} y={rowY + 12} textAnchor="middle" className="row">…</text>,
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
        <rect key={`port-${port.name}`} x={px - 5} y={py - 5} width={10} height={10} fill="#fff" stroke="#000" />,
        <text
          key={`port-name-${port.name}`}
          x={px + side * 9}
          y={py + 3.5}
          textAnchor={side < 0 ? 'end' : 'start'}
          className="port-name"
        >
          {port.name}
        </text>,
      );
    }
  }

  return (
    <g className={classes} data-key={node.key} onPointerDown={onPointerDown}>
      <rect x={x} y={y} width={w} height={h} rx={node.is_def ? 0 : 8} className="node-body" />
      {display.stereotype && (
        <text x={cx} y={y + 13} textAnchor="middle" className="stereo">{`«${node.stereotype}»`}</text>
      )}
      {/* With the stereotype hidden the name takes the top of the box, so a box drawn without
          compartments does not start with an empty band. */}
      <text x={cx} y={display.stereotype ? y + 25 : y + 20} textAnchor="middle" className="label">
        {node.label}
      </text>
      {rows}
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
  ].filter(Boolean).join(' · ');
  return (
    <g className={`judge ${judgement.verdict}${judgement.stale ? ' stale' : ''}`}>
      <title>{title}</title>
      <circle cx={x} cy={y} r={7} />
      <text x={x} y={y + 3.2} textAnchor="middle">{glyph}</text>
    </g>
  );
}
