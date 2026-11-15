import { test } from 'node:test';
import assert from 'node:assert/strict';
import { groupFiles, nameOf } from './files.ts';

test('the directory every file shares is dropped', () => {
  const groups = groupFiles([
    'core/examples/drone.sysml',
    'core/examples/fuel.sysml',
    'core/examples/rover.sysml',
  ]);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].dir, '');
  assert.deepEqual(
    groups[0].files.map((f) => f.name),
    ['drone.sysml', 'fuel.sysml', 'rover.sysml'],
  );
});

test('a shared prefix is removed by whole segments, not characters', () => {
  // `/a/bc` and `/a/bd` share `/a` as a directory, not `/a/b`.
  const groups = groupFiles(['/a/bc/x.sysml', '/a/bd/y.sysml']);
  assert.deepEqual(
    groups.map((g) => g.dir),
    ['bc', 'bd'],
  );
});

test('a file at the shared root sits in the empty group', () => {
  const groups = groupFiles(['core/a.sysml', 'core/sub/b.sysml']);
  assert.equal(groups.length, 2);
  assert.equal(groups[0].dir, '');
  assert.equal(groups[0].files[0].name, 'a.sysml');
  assert.equal(groups[1].dir, 'sub');
});

test('one file is its own group with no heading', () => {
  const groups = groupFiles(['some/deep/place/model.sysml']);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].dir, '');
  assert.equal(groups[0].files[0].name, 'model.sysml');
});

test('windows separators are understood', () => {
  const groups = groupFiles(['C:\\proj\\models\\a.sysml', 'C:\\proj\\models\\b.sysml']);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].dir, '');
  assert.deepEqual(
    groups[0].files.map((f) => f.name),
    ['a.sysml', 'b.sysml'],
  );
});

test('the full path is kept, because that is what the server addresses', () => {
  const groups = groupFiles(['core/examples/drone.sysml']);
  assert.equal(groups[0].files[0].path, 'core/examples/drone.sysml');
});

test('nothing to show is an empty list', () => {
  assert.deepEqual(groupFiles([]), []);
});

test('nameOf takes the last segment either way', () => {
  assert.equal(nameOf('a/b/c.sysml'), 'c.sysml');
  assert.equal(nameOf('a\\b\\c.sysml'), 'c.sysml');
  assert.equal(nameOf('c.sysml'), 'c.sysml');
});
