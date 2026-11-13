import { test } from 'node:test';
import assert from 'node:assert/strict';
import { waitingOf } from './waiting.ts';

test('only the facts that are waiting are shown', () => {
  const out = waitingOf({
    values_without_record: 6,
    assumptions_unapproved: 0,
    open_questions: 2,
    judgements_stale: 0,
  });
  assert.deepEqual(
    out.map((entry) => entry.key),
    ['values with no record', 'open questions'],
  );
});

test('a zero is dropped, not shown as a passed check', () => {
  const out = waitingOf({ values_without_record: 0, open_questions: 1 });
  assert.equal(out.length, 1);
  assert.equal(out[0].key, 'open questions');
});

test('nothing waiting is an empty list, so the panel can say so plainly', () => {
  assert.deepEqual(waitingOf({}), []);
  assert.deepEqual(
    waitingOf({ values_without_record: 0, assumptions_unapproved: 0, open_questions: 0, judgements_stale: 0 }),
    [],
  );
});

test('the largest number comes first', () => {
  const out = waitingOf({
    values_without_record: 2,
    assumptions_unapproved: 9,
    open_questions: 5,
    judgements_stale: 1,
  });
  assert.deepEqual(
    out.map((entry) => entry.count),
    [9, 5, 2, 1],
  );
});

test('a missing field counts as zero rather than breaking the panel', () => {
  const out = waitingOf({ open_questions: 3 });
  assert.equal(out.length, 1);
  assert.equal(out[0].count, 3);
});
