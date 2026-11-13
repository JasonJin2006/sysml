import { test } from 'node:test';
import assert from 'node:assert/strict';
import { judgePoll } from './poll.ts';

test('a digest that differs from the picture asks for a reload', () => {
  assert.equal(judgePoll({ shown: 'a', answered: 'b', superseded: false }), 'changed');
});

test('the same digest changes nothing', () => {
  assert.equal(judgePoll({ shown: 'a', answered: 'a', superseded: false }), 'unchanged');
});

test('before the first document there is nothing to reload', () => {
  // The page has drawn nothing, so there is no picture for the files to differ from.
  assert.equal(judgePoll({ shown: null, answered: 'b', superseded: false }), 'unchanged');
});

test('a poll answered after a later load started is ignored', () => {
  // The person edits a value: the edit is written, then the page loads the new document. A poll
  // sent before the edit and answered after it carries the new digest while the page still holds
  // the old one. Acting on it would reload again and report the edit as a change made on disk.
  assert.equal(judgePoll({ shown: 'a', answered: 'b', superseded: true }), 'ignore');
  assert.equal(judgePoll({ shown: 'a', answered: 'a', superseded: true }), 'ignore');
});
