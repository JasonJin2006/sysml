import { test } from 'node:test';
import assert from 'node:assert/strict';
import { alignRects, type Box } from './align.ts';

const boxes = (): Box[] => [
  { key: 'a', x: 0, y: 0, w: 100, h: 40 },
  { key: 'b', x: 40, y: 100, w: 60, h: 30 },
  { key: 'c', x: 300, y: 50, w: 80, h: 50 },
];

const at = (result: Box[]) => Object.fromEntries(result.map((b) => [b.key, [b.x, b.y]]));

test('left, right, top and bottom line the edges up', () => {
  assert.deepEqual(at(alignRects(boxes(), 'left')), { a: [0, 0], b: [0, 100], c: [0, 50] });
  assert.deepEqual(at(alignRects(boxes(), 'right')), { a: [280, 0], b: [320, 100], c: [300, 50] });
  assert.deepEqual(at(alignRects(boxes(), 'top')), { a: [0, 0], b: [40, 0], c: [300, 0] });
  assert.deepEqual(at(alignRects(boxes(), 'bottom')), { a: [0, 90], b: [40, 100], c: [300, 80] });
});

test('centring uses the shared middle, not each box against itself', () => {
  // Span is 0..380 across, 0..130 down.
  assert.deepEqual(at(alignRects(boxes(), 'center-h')), { a: [140, 0], b: [160, 100], c: [150, 50] });
  assert.deepEqual(at(alignRects(boxes(), 'center-v')), { a: [0, 45], b: [40, 50], c: [300, 40] });
});

test('distribution keeps the extremes and evens the gaps', () => {
  // Across: a at 0 and c ending at 380 stay; the free space is 380 - 240 = 140, so each gap is 70.
  assert.deepEqual(at(alignRects(boxes(), 'distribute-h')), { a: [0, 0], b: [170, 100], c: [300, 50] });
  // Down: a at 0 and b ending at 130 stay; free space 130 - 120 = 10, gap 5.
  assert.deepEqual(at(alignRects(boxes(), 'distribute-v')), { a: [0, 0], b: [40, 100], c: [300, 45] });
});

test('sizes are never changed', () => {
  for (const how of ['left', 'right', 'top', 'bottom', 'center-h', 'center-v'] as const) {
    for (const box of alignRects(boxes(), how)) {
      const original = boxes().find((b) => b.key === box.key)!;
      assert.equal(box.w, original.w, `${how} must not resize`);
      assert.equal(box.h, original.h, `${how} must not resize`);
    }
  }
});

test('alignment moves only the axis it aligns', () => {
  // A box that is already in line keeps its other coordinate, so repeated alignment does not
  // creep across the picture.
  const moved = at(alignRects(boxes(), 'left'));
  assert.equal(moved.b[1], 100, 'left-align leaves y alone');
  assert.equal(moved.c[1], 50, 'left-align leaves y alone');
});

test('one box is returned unchanged', () => {
  const single: Box[] = [{ key: 'a', x: 10, y: 20, w: 30, h: 40 }];
  assert.deepEqual(alignRects(single, 'left'), single);
});

test('fractional coordinates snap to whole pixels on the axis that moved', () => {
  const odd: Box[] = [
    { key: 'a', x: 0, y: 0.5, w: 10, h: 10 },
    { key: 'b', x: 5.5, y: 3.25, w: 10, h: 10 },
  ];
  const result = alignRects(odd, 'left');
  assert.equal(result[0].x, 0);
  assert.equal(result[1].x, 0);
  assert.equal(result[0].y, 0.5, 'the untouched axis keeps its half pixel');
  assert.equal(result[1].y, 3.25, 'the untouched axis keeps its fractional coordinate');
});
