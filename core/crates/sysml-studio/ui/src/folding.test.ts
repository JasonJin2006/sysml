import { test } from 'node:test';
import assert from 'node:assert/strict';
import { foldingKey, readFolded } from './folding.ts';

test('saved folding choices distinguish models, projections, scopes and filters', () => {
  const base = foldingKey(['a.sysml'], 'gv', null, null, '');
  for (const key of [
    foldingKey(['b.sysml'], 'gv', null, null, ''),
    foldingKey(['a.sysml'], 'stv', null, null, ''),
    foldingKey(['a.sysml'], 'gv', 'P::A', null, ''),
    foldingKey(['a.sysml'], 'gv', null, 'P::View', ''),
    foldingKey(['a.sysml'], 'gv', null, null, 'Approved'),
  ]) assert.notEqual(key, base);
});

test('saved choices keep explicit expansion and discard malformed entries', () => {
  assert.deepEqual(readFolded('{"P::A":true,"P::B":false,"P::C":"false","":true}'), { 'P::A': true, 'P::B': false });
  for (const text of [null, 'not json', 'null', '[]', 'true']) assert.deepEqual(readFolded(text), {});
});
