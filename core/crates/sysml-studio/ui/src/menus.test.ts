import { test } from 'node:test';
import assert from 'node:assert/strict';
import { displayMenu, marksMenu } from './menus.ts';

const PARTS = [
  { part: 'stereotype', title: 'Show or hide the stereotype line' },
  { part: 'members', title: 'Show or hide the member compartment' },
];

test('a display item is ticked exactly when the part is shown', () => {
  const items = displayMenu(PARTS, { stereotype: true, members: false }, [], 'en', () => undefined, () => undefined);
  assert.equal(items[0].checked, true);
  assert.equal(items[1].checked, false);
});

test('a missing flag counts as hidden, not as shown', () => {
  // A stored preference from an older version may not have the key at all.
  const items = displayMenu(PARTS, {}, [], 'en', () => undefined, () => undefined);
  assert.deepEqual(items.map((item) => item.checked), [false, false]);
});

test('the language items start a new group', () => {
  const items = displayMenu(PARTS, {}, ['en', 'zh'], 'en', () => undefined, () => undefined);
  assert.equal(items.length, 4);
  assert.equal(items[2].startsGroup, true);
  assert.equal(items[3].startsGroup, false);
});

test('the current language is the one ticked', () => {
  const items = displayMenu([], {}, ['en', 'zh'], 'zh', () => undefined, () => undefined);
  assert.deepEqual(items.map((item) => item.checked), [false, true]);
});

test('running a display item toggles the part it names', () => {
  const toggled: string[] = [];
  const items = displayMenu(PARTS, {}, [], 'en', (part) => toggled.push(part), () => undefined);
  items[1].run();
  assert.deepEqual(toggled, ['members']);
});

test('running a language item chooses that code', () => {
  const chosen: string[] = [];
  const items = displayMenu([], {}, ['en', 'zh'], 'en', () => undefined, (code) => chosen.push(code));
  items[1].run();
  assert.deepEqual(chosen, ['zh']);
});

test('the marks menu has two items and only the first is a toggle', () => {
  const items = marksMenu(true, () => undefined, () => undefined);
  assert.equal(items.length, 2);
  assert.equal(items[0].checked, true);
  assert.equal(items[1].checked, undefined);
});

test('running the marks items calls the two different actions', () => {
  const calls: string[] = [];
  const items = marksMenu(false, () => calls.push('toggle'), () => calls.push('clear'));
  items[0].run();
  items[1].run();
  assert.deepEqual(calls, ['toggle', 'clear']);
});
