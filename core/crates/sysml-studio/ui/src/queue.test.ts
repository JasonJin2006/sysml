import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildQueue, queueCounts } from './queue.ts';
import type { CanvasDocument } from './types.ts';

function doc(parts: {
  values?: Array<{ key: string; text: string; origin: string | null; approved_by?: string | null; decision_id?: string | null; why?: string | null }>;
  judgements?: Array<{ key: string; verdict: string; stale: boolean; judged_against?: string }>;
  issues?: Array<{ code: string; at: string; message: string }>;
}): CanvasDocument {
  return {
    title: 'm.sysml',
    files: ['m.sysml'],
    fingerprint: 'doc',
    view: 'gv',
    mode: 'extended',
    width: 0,
    height: 0,
    nodes: [],
    frames: [],
    edges: [],
    ledger_fingerprint: 'now',
    values: (parts.values ?? []).map((v) => ({
      key: v.key,
      text: v.text,
      line: 1,
      origin: v.origin,
      why: v.why ?? null,
      falsifier: null,
      approved_by: v.approved_by ?? null,
      cites: [],
      decision_id: v.decision_id ?? null,
      unit: '',
      dimension: '',
      unit_known: false,
    })),
    judgements: (parts.judgements ?? []).map((j) => ({
      key: j.key,
      verdict: j.verdict as 'pass' | 'fail' | 'inconclusive' | 'error',
      detail: '',
      judged_by: 'studio',
      at: '',
      judged_against: j.judged_against ?? 'old',
      stale: j.stale,
    })),
    issues: (parts.issues ?? []).map((i) => ({
      severity: 'error' as const,
      code: i.code,
      message: i.message,
      at: i.at,
      line: 1,
    })),
    questions: [],
    summary: {
      nodes: 0,
      edges: 0,
      errors: 0,
      warnings: 0,
      values_without_record: 0,
      assumptions_unapproved: 0,
      open_questions: 0,
      judgements: 0,
      judgements_stale: 0,
    },
  };
}

test('a value with no record becomes one row, and its diagnostic is not repeated', () => {
  const queue = buildQueue(doc({
    values: [{ key: 'P::B::capacity', text: '900 [Wh]', origin: null }],
    issues: [
      { code: 'PROV001', at: 'P::B::capacity', message: 'value `900 [Wh]` has no ledger entry' },
    ],
  }));
  assert.equal(queue.length, 1, 'one fact, one row');
  assert.equal(queue[0].kind, 'unrecorded');
  assert.equal(queue[0].detail, '900 [Wh]');
});

test('an unapproved assumption is its own row with the reasoning', () => {
  const queue = buildQueue(doc({
    values: [{
      key: 'P::m',
      text: '10 [kg]',
      origin: 'assumed',
      approved_by: null,
      decision_id: 'D003',
      why: 'vendor datasheet rounds up',
    }],
  }));
  assert.equal(queue.length, 1);
  assert.equal(queue[0].kind, 'unapproved');
  assert.equal(queue[0].decisionId, 'D003');
  assert.match(queue[0].detail, /vendor datasheet/);
});

test('an approved assumption is not in the queue', () => {
  const queue = buildQueue(doc({
    values: [{ key: 'P::m', text: '10 [kg]', origin: 'assumed', approved_by: 'human:ada' }],
  }));
  assert.equal(queue.length, 0);
});

test('a stale verdict is a row, a fresh one is not', () => {
  const queue = buildQueue(doc({
    judgements: [
      { key: 'P::Light', verdict: 'fail', stale: true },
      { key: 'P::Heavy', verdict: 'pass', stale: false },
    ],
  }));
  assert.equal(queue.length, 1);
  assert.equal(queue[0].kind, 'stale-verdict');
  assert.equal(queue[0].requirement, 'P::Light');
  assert.match(queue[0].detail, /judged against old/, 'the row says which model the record read');
});

test('diagnostics with nothing else to say are kept, in document order', () => {
  const queue = buildQueue(doc({
    issues: [
      { code: 'SEM004', at: 'P::Light', message: 'predicate reads `v.range`, which names nothing' },
      { code: 'SEM004', at: 'P::Heavy', message: 'predicate reads `v.mass`, which names nothing' },
    ],
  }));
  assert.deepEqual(queue.map((i) => i.key), ['P::Light', 'P::Heavy']);
});

test('the order is unrecorded, unapproved, stale, then diagnostics', () => {
  const queue = buildQueue(doc({
    values: [
      { key: 'P::a', text: '1', origin: null },
      { key: 'P::b', text: '2', origin: 'assumed', approved_by: null, decision_id: 'D1' },
    ],
    judgements: [{ key: 'P::Light', verdict: 'fail', stale: true }],
    issues: [{ code: 'SEM004', at: 'P::Light', message: 'broken' }],
  }));
  assert.deepEqual(queue.map((i) => i.kind), ['unrecorded', 'unapproved', 'stale-verdict', 'problem']);
});

test('counts add up', () => {
  const queue = buildQueue(doc({
    values: [{ key: 'P::a', text: '1', origin: null }],
    issues: [{ code: 'SEM004', at: 'P::Light', message: 'broken' }],
  }));
  assert.deepEqual(queueCounts(queue), { unrecorded: 1, unapproved: 0, 'stale-verdict': 0, problem: 1 });
});
