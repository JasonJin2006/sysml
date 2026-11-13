import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filterCommands, matches, type Command } from './commands.ts';

// The component translates a command's id at render time; the tests use the id as the title, which
// is what the English table maps it to anyway.
const title = (command: Command) => command.id;

const ids = (commands: Command[]) => commands.map((command) => command.id);

test('an empty query keeps every command', () => {
  const commands: Command[] = [
    { id: 'Fit the picture', run: () => undefined },
    { id: 'Check the model', run: () => undefined },
  ];
  assert.equal(filterCommands(commands, '', title).length, 2);
  assert.equal(filterCommands(commands, '   ', title).length, 2);
});

test('letters in order match across words', () => {
  // The reason it is a subsequence: a person types `gv`, not "general view".
  assert.equal(matches('gv', 'general view'), true);
  assert.equal(matches('chk', 'Check the model'), true);
  assert.equal(matches('fit', 'Fit the picture'), true);
});

test('letters out of order do not match', () => {
  assert.equal(matches('vg', 'general view'), false);
  assert.equal(matches('zzz', 'Fit the picture'), false);
});

test('the match ignores case', () => {
  assert.equal(matches('CHECK', 'check the model'), true);
  assert.equal(matches('check', 'CHECK THE MODEL'), true);
});

test('spaces in the query are ignored', () => {
  assert.equal(matches('c h e c k', 'check the model'), true);
});

test('a command is matched by its id as well as its title', () => {
  const commands: Command[] = [
    { id: 'fit', run: () => undefined },
    { id: 'Export the table', run: () => undefined },
  ];
  assert.deepEqual(ids(filterCommands(commands, 'fit', title)), ['fit']);
});

test('the filter keeps the given order', () => {
  const commands: Command[] = [
    { id: 'alpha one', run: () => undefined },
    { id: 'beta', run: () => undefined },
    { id: 'alpha two', run: () => undefined },
  ];
  assert.deepEqual(ids(filterCommands(commands, 'alpha', title)), ['alpha one', 'alpha two']);
});

test('nothing matching gives an empty list, not the whole list', () => {
  const commands: Command[] = [{ id: 'fit', run: () => undefined }];
  assert.deepEqual(filterCommands(commands, 'qqq', title), []);
});
