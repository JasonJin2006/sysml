import { test } from 'node:test';
import assert from 'node:assert/strict';
import { arrowGeometry, compositeHeadD, placeEdge, shiftEdge, typingDots, typingHeadD } from './geometry.ts';
import type { CanvasEdge, Point, Rect } from './types.ts';

function edge(points: Point[]): CanvasEdge {
  return {
    index: 0, from: 0, to: 1, arrow: 'composite', label: null, points, hops: [],
    from_key: 'P::a', to_key: 'P::b',
  };
}

function orthogonal(points: Point[]) {
  for (let i = 1; i < points.length; i += 1) {
    const a = points[i - 1];
    const b = points[i];
    assert.ok(a[0] === b[0] || a[1] === b[1], `${a} → ${b} is diagonal`);
  }
}

function vertices(path: string): Point[] {
  return [...path.matchAll(/[ML](-?[\d.]+),(-?[\d.]+)/g)].map((match) => [+match[1], +match[2]]);
}

test('a still box leaves its lines alone', () => {
  const line = edge([[0, 0], [10, 0], [10, 20]]);
  assert.equal(shiftEdge(line, undefined, undefined), line);
  assert.equal(shiftEdge(line, { dx: 0, dy: 0 }, { dx: 0, dy: 0 }), line);
});

test('dragging only the source never moves the target endpoint', () => {
  const moved = shiftEdge(edge([[0, 0], [10, 0], [10, 20]]), { dx: 5, dy: 5 }, undefined);
  assert.deepEqual(moved.points[0], [5, 5]);
  assert.deepEqual(moved.points.at(-1), [10, 20]);
  orthogonal(moved.points);
});

test('dragging only the target never moves the source endpoint', () => {
  const moved = shiftEdge(edge([[0, 0], [10, 0], [10, 20]]), undefined, { dx: 5, dy: 5 });
  assert.deepEqual(moved.points[0], [0, 0]);
  assert.deepEqual(moved.points.at(-1), [15, 25]);
  orthogonal(moved.points);
});

test('each endpoint follows its own displacement with an orthogonal bridge', () => {
  const line = edge([[0, 0], [10, 0], [10, 20], [20, 20]]);
  const moved = shiftEdge(line, { dx: 3, dy: 0 }, { dx: 0, dy: 4 });
  assert.deepEqual(moved.points[0], [3, 0]);
  assert.deepEqual(moved.points.at(-1), [20, 24]);
  orthogonal(moved.points);
});

test('a two-point vertical wire bends during a sideways drag instead of becoming diagonal', () => {
  const moved = shiftEdge(edge([[20, 30], [20, 90]]), { dx: 40, dy: 0 }, undefined);
  assert.deepEqual(moved.points[0], [60, 30]);
  assert.deepEqual(moved.points.at(-1), [20, 90]);
  assert.ok(moved.points.length > 2);
  assert.ok(moved.points[1][1] > 30, 'it still leaves the source downwards');
  assert.ok(moved.points.at(-2)![1] < 90, 'it still approaches the target from above');
  orthogonal(moved.points);
});

test('a group translation preserves its saved route and jump arcs exactly', () => {
  const line = { ...edge([[0, 0], [40, 0]]), hops: [[20, 0]] as Point[] };
  const moved = shiftEdge(line, { dx: 10, dy: 7 }, { dx: 10, dy: 7 });
  assert.deepEqual(moved.points, [[10, 7], [50, 7]]);
  assert.deepEqual(moved.hops, [[30, 7]]);
});

test('changed crossings do not leave stale jump arcs floating beside the preview', () => {
  const line = { ...edge([[0, 0], [40, 0]]), hops: [[20, 0]] as Point[] };
  assert.deepEqual(shiftEdge(line, { dx: 10, dy: 7 }, undefined).hops, []);
});

test('a line with fewer than two points is returned as it is', () => {
  const line = edge([[0, 0]]);
  assert.equal(shiftEdge(line, { dx: 5, dy: 5 }, undefined), line);
});

test('a moved edge keeps its relationship metadata', () => {
  const line = { ...edge([[0, 0], [10, 0]]), label: 'satisfy', from_key: 'P::x', to_key: 'P::y' };
  const moved = shiftEdge(line, { dx: 1, dy: 1 }, undefined);
  assert.equal(moved.label, 'satisfy');
  assert.equal(moved.from_key, 'P::x');
  assert.equal(moved.to_key, 'P::y');
  assert.equal(moved.arrow, 'composite');
});

test('a reversed rank route is normalized before its source box is moved', () => {
  const source: Rect = { x: 0, y: 80, w: 100, h: 40 };
  const target: Rect = { x: 0, y: 0, w: 100, h: 40 };
  const line = edge([[50, 40], [50, 80]]);
  const moved = placeEdge(line, source, target, { ...source, x: 20 }, undefined);
  assert.deepEqual(moved.points[0], [70, 80]);
  assert.deepEqual(moved.points.at(-1), [50, 40]);
  orthogonal(moved.points);
});

test('resizing preserves the fractional attachment slot rather than forcing the midpoint', () => {
  const source: Rect = { x: 0, y: 0, w: 100, h: 40 };
  const target: Rect = { x: 200, y: 0, w: 100, h: 40 };
  const line = edge([[100, 10], [200, 10]]);
  const moved = placeEdge(line, source, target, { x: 10, y: 20, w: 160, h: 80 }, undefined);
  assert.deepEqual(moved.points[0], [170, 40], 'right border, one quarter of the height');
  assert.deepEqual(moved.points.at(-1), [200, 10]);
  orthogonal(moved.points);
});

test('already-directed static points and slots stay exactly as the server supplied them', () => {
  const source = { x: 0, y: 0, w: 100, h: 40 };
  const target = { x: 0, y: 80, w: 100, h: 40 };
  const line = edge([[28, 40], [28, 80]]);
  assert.equal(placeEdge(line, source, target, undefined, undefined), line);
});

test('duplicate endpoint vertices do not produce a directionless arrow', () => {
  const line = edge([[10, 10], [10, 10], [10, 30]]);
  const geometry = arrowGeometry(line)!;
  assert.equal(geometry.ux, 0);
  assert.equal(geometry.uy, -1);
  assert.equal(arrowGeometry(edge([[10, 10], [10, 10]])), null);
});

test('the composite diamond stays outside the owner in every direction', () => {
  for (const [ux, uy] of [[0, -1], [0, 1], [-1, 0], [1, 0]]) {
    const anchor: Point = [100, 100];
    const points = vertices(compositeHeadD({ anchor, ux, uy }));
    assert.equal(points.length, 4);
    for (const [x, y] of points) assert.ok((x - 100) * ux + (y - 100) * uy <= 0);
    assert.ok(points.some(([x, y]) => (x - 100) * ux + (y - 100) * uy === -16));
  }
});

test('typing triangle and dots stay outside the target in every direction', () => {
  for (const [ux, uy] of [[0, -1], [0, 1], [-1, 0], [1, 0]]) {
    const geometry = { anchor: [100, 100] as Point, ux, uy };
    const points = [...vertices(typingHeadD(geometry)), ...typingDots(geometry)];
    assert.deepEqual(points[0], [100, 100]);
    for (const [x, y] of points) assert.ok((x - 100) * ux + (y - 100) * uy <= 0);
  }
});
