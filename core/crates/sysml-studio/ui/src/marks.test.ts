import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diffDocuments, isClean, type Picture } from './marks.ts';

function picture(
  nodes: Array<{ key: string; label?: string; x?: number; y?: number; w?: number; h?: number }>,
  values: Array<{ key: string; text: string }> = [],
): Picture {
  return {
    nodes: nodes.map((n) => ({
      key: n.key,
      label: n.label ?? n.key,
      x: n.x ?? 0,
      y: n.y ?? 0,
      w: n.w ?? 100,
      h: n.h ?? 40,
    })),
    values,
  };
}

test('nothing changed means nothing is marked', () => {
  const before = picture([{ key: 'A' }, { key: 'B' }], [{ key: 'A', text: '1' }]);
  const after = picture([{ key: 'A' }, { key: 'B' }], [{ key: 'A', text: '1' }]);
  const marks = diffDocuments(before, after);
  assert.ok(isClean(marks), 'a reload with no edit must not paint anything');
});

test('moving a box is a layout change, not a model change', () => {
  const before = picture([{ key: 'A', x: 0, y: 0 }]);
  const after = picture([{ key: 'A', x: 300, y: 120 }]);
  assert.ok(
    isClean(diffDocuments(before, after)),
    'dragging a box is not an edit to the model and must not be marked as one',
  );
});

test('a new element is marked as added', () => {
  const before = picture([{ key: 'A' }]);
  const after = picture([{ key: 'A' }, { key: 'B' }]);
  const marks = diffDocuments(before, after);
  assert.deepEqual([...marks.added], ['B']);
  assert.equal(marks.removed.length, 0);
});

test('a deleted element leaves a ghost at the rectangle it last occupied', () => {
  const before = picture([{ key: 'A', label: 'Battery', x: 40, y: 60, w: 96, h: 34 }]);
  const after = picture([]);
  const marks = diffDocuments(before, after);
  assert.equal(marks.removed.length, 1);
  assert.equal(marks.removed[0].label, 'Battery');
  assert.deepEqual(marks.removed[0].rect, { x: 40, y: 60, w: 96, h: 34 });
  assert.equal(marks.added.size, 0);
});

test('a changed value carries both numbers', () => {
  const before = picture([{ key: 'A' }], [{ key: 'P::Battery::capacity', text: '900 [Wh]' }]);
  const after = picture([{ key: 'A' }], [{ key: 'P::Battery::capacity', text: '850 [Wh]' }]);
  const change = diffDocuments(before, after).values.get('P::Battery::capacity');
  assert.deepEqual(change, { from: '900 [Wh]', to: '850 [Wh]' });
});

test('a value that appears for the first time is not a change', () => {
  // The element is new, so the whole box is new; reporting the value as changed as well would
  // draw two marks over one fact.
  const before = picture([], []);
  const after = picture([{ key: 'A' }], [{ key: 'A', text: '1' }]);
  const marks = diffDocuments(before, after);
  assert.equal(marks.values.size, 0, 'no "from" exists, so there is nothing to compare');
  assert.deepEqual([...marks.added], ['A']);
});

test('ghosts keep a stable order', () => {
  const before = picture([{ key: 'C' }, { key: 'A' }, { key: 'B' }]);
  const marks = diffDocuments(before, picture([]));
  assert.deepEqual(marks.removed.map((r) => r.key), ['A', 'B', 'C']);
});
