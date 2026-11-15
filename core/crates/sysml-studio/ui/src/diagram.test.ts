import { test } from 'node:test';
import assert from 'node:assert/strict';
import { projectSelection, wholeModel, type DiagramSelection } from './diagram.ts';

const scoped: DiagramSelection = { view: 'gv', only: null, scope: 'P::Vehicle', depth: 3, layout: 'tree' };

test('compatible projection changes retain the element, depth and layout', () => {
  for (const view of ['gv', 'stv', 'afv'] as const) assert.deepEqual(projectSelection(scoped, view), { ...scoped, view });
});

test('incompatible projections require explicitly choosing the whole model', () => {
  for (const view of ['iv', 'pv', 'tv', 'sv'] as const) {
    assert.equal(projectSelection(scoped, view), null);
    assert.deepEqual(projectSelection(wholeModel(scoped), view), { ...scoped, view, scope: null, depth: null });
  }
});

test('a declared projection stays fixed until the whole model is chosen', () => {
  const declared = { ...scoped, scope: null, only: 'P::architecture' };
  assert.equal(projectSelection(declared, 'gv'), null);
  assert.deepEqual(projectSelection(wholeModel(declared), 'gv'), { ...scoped, scope: null, depth: null });
  assert.deepEqual(wholeModel(declared), { ...scoped, scope: null, depth: null });
});
