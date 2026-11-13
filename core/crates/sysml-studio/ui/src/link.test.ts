import { test } from 'node:test';
import assert from 'node:assert/strict';
import { elementOnLine, flattenOutline } from './link.ts';

const OUTLINE = [
  {
    line: 1,
    key: 'P',
    children: [
      {
        line: 6,
        key: 'P::Battery',
        children: [
          { line: 7, key: 'P::Battery::energy' },
          { line: 8, key: 'P::Battery::mass' },
        ],
      },
      { line: 11, key: 'P::Airframe', children: [{ line: 12, key: 'P::Airframe::mass' }] },
    ],
  },
];

test('a cursor on a declaration names that declaration', () => {
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 6), 'P::Battery');
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 11), 'P::Airframe');
});

test('a cursor below a member belongs to that member', () => {
  // Line 8 declares `mass`; nothing else is declared until line 11.
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 8), 'P::Battery::mass');
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 9), 'P::Battery::mass');
});

test('a blank line inside a definition belongs to the definition', () => {
  // Line 5 is the blank line inside `Battery`'s body, above `energy`.
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 5), 'P');
});

test('a line above every declaration belongs to nothing', () => {
  assert.equal(elementOnLine(flattenOutline(OUTLINE), 0), null);
});

test('two declarations on one line: the later one wins', () => {
  // `part a; part b;` is two statements on one line, and the cursor is nearer the second.
  const lines = [
    { line: 1, key: 'P' },
    { line: 3, key: 'P::a' },
    { line: 3, key: 'P::b' },
  ];
  assert.equal(elementOnLine(lines, 3), 'P::b');
});

test('an empty outline names nothing', () => {
  assert.equal(elementOnLine([], 4), null);
  assert.deepEqual(flattenOutline(null), []);
  assert.deepEqual(flattenOutline({}), []);
});

test('the flatten keeps the tree order', () => {
  const keys = flattenOutline(OUTLINE).map((entry) => entry.key);
  assert.deepEqual(keys, [
    'P',
    'P::Battery',
    'P::Battery::energy',
    'P::Battery::mass',
    'P::Airframe',
    'P::Airframe::mass',
  ]);
});
