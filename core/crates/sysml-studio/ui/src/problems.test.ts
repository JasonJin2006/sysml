import { test } from 'node:test';
import assert from 'node:assert/strict';
import { codeCounts, filterProblems, NO_FILTER, severityCounts, type Problem } from './problems.ts';

const P = (severity: Problem['severity'], code: string): Problem => ({
  severity,
  code,
  message: '',
  at: '',
  line: 1,
});

const LIST: Problem[] = [
  P('error', 'SYN001'),
  P('error', 'SEM001'),
  P('warning', 'SEM001'),
  P('warning', 'SEM001'),
];

test('no filter keeps everything', () => {
  assert.equal(filterProblems(LIST, NO_FILTER).length, 4);
});

test('a severity filter keeps only that severity', () => {
  const errors = filterProblems(LIST, { severity: 'error', code: null });
  assert.equal(errors.length, 2);
  assert.ok(errors.every((p) => p.severity === 'error'));
});

test('a code filter keeps only that code', () => {
  const sem = filterProblems(LIST, { severity: 'all', code: 'SEM001' });
  assert.equal(sem.length, 3);
  assert.ok(sem.every((p) => p.code === 'SEM001'));
});

test('the two filters combine', () => {
  const both = filterProblems(LIST, { severity: 'warning', code: 'SEM001' });
  assert.equal(both.length, 2);
});

test('a filter that matches nothing gives an empty list, not the whole list', () => {
  assert.deepEqual(filterProblems(LIST, { severity: 'all', code: 'NOPE' }), []);
});

test('the original order is kept', () => {
  const out = filterProblems(LIST, { severity: 'all', code: 'SEM001' });
  assert.deepEqual(
    out.map((p) => p.severity),
    ['error', 'warning', 'warning'],
  );
});

test('code counts are largest first, then by code', () => {
  const counts = codeCounts(LIST);
  assert.deepEqual(counts, [
    { code: 'SEM001', count: 3 },
    { code: 'SYN001', count: 1 },
  ]);
});

test('code counts come from the unfiltered set', () => {
  // Choosing "errors only" must not remove SEM001 from the menu that could bring it back.
  const errorsOnly = filterProblems(LIST, { severity: 'error', code: null });
  assert.deepEqual(
    codeCounts(errorsOnly).map((c) => c.code),
    ['SEM001', 'SYN001'],
  );
  assert.deepEqual(
    codeCounts(LIST).map((c) => c.code),
    ['SEM001', 'SYN001'],
  );
});

test('severity counts add up', () => {
  assert.deepEqual(severityCounts(LIST), { all: 4, error: 2, warning: 2, info: 0 });
  assert.deepEqual(severityCounts([]), { all: 0, error: 0, warning: 0, info: 0 });
});

test('ledger notices do not inflate warning or error counts', () => {
  const items = [...LIST, P('info', 'PROV000')];
  assert.deepEqual(severityCounts(items), { all: 5, error: 2, warning: 2, info: 1 });
  assert.deepEqual(filterProblems(items, { severity: 'info', code: null }), [items[4]]);
});
