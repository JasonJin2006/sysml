import { test } from 'node:test';
import assert from 'node:assert/strict';
import { forStereotype, headerInk, isActor, PLAIN, styleTheme } from './style.ts';

/**
 * The canvas draws one picture and `sysml render` draws another, from the same model. These
 * assertions are what keeps them the same picture: the light values here are `sysml_view::style`'s
 * own, so a table that drifts shows up as a failing test rather than as two colours in a report.
 */

test('the light table is the kernel table', () => {
  styleTheme('light');
  assert.deepEqual(forStereotype('«part def»'), {
    header: '#88B7E3',
    body: '#F7F9FF',
    border: '#5D9DDB',
  });
  assert.equal(forStereotype('«requirement def»').header, '#F5B272');
  assert.equal(forStereotype('«state»').header, '#80D6D6');
});

test('a definition and a usage of one construct differ', () => {
  styleTheme('light');
  const def = forStereotype('«part def»');
  const usage = forStereotype('«part»');
  assert.notEqual(def.header, usage.header);
  assert.equal(def.border, usage.border, 'one family, one border');
});

test('the dark table keeps the hue and drops the lightness', () => {
  styleTheme('dark');
  const dark = forStereotype('«part def»');
  // A body that is nearly white on a dark canvas is a hole, not a box.
  assert.ok(lightness(dark.body) < 0.3, `${dark.body} is a dark body`);
  assert.ok(lightness(dark.header) < 0.5, `${dark.header} is a dark band`);
  assert.equal(dark.border, '#5D9DDB', 'the border stays the family colour');
  styleTheme('light');
});

test('different constructs get different colours', () => {
  for (const palette of ['light', 'dark'] as const) {
    styleTheme(palette);
    const headers = [
      '«part def»',
      '«enum def»',
      '«requirement def»',
      '«state»',
      '«action»',
      '«metadata def»',
    ].map((s) => forStereotype(s).header);
    assert.equal(new Set(headers).size, headers.length, `${palette} table`);
  }
  styleTheme('light');
});

test('a kind outside the table keeps the plain box', () => {
  styleTheme('light');
  assert.deepEqual(forStereotype('«attribute»'), PLAIN);
  assert.deepEqual(forStereotype('«subject»'), PLAIN);
  assert.deepEqual(forStereotype(''), PLAIN);
});

test('the grouped spellings share one colour', () => {
  styleTheme('light');
  for (const [a, b] of [
    ['«port def»', '«part def»'],
    ['«interface»', '«part»'],
    ['«enumeration def»', '«enum def»'],
    ['«action def»', '«action»'],
  ]) {
    assert.deepEqual(forStereotype(a), forStereotype(b), `${a} and ${b}`);
  }
});

test('a name on a saturated band takes white ink', () => {
  assert.equal(headerInk({ header: '#1F4373', body: '#FFF', border: '#000' }), '#FFFFFF');
  assert.equal(headerInk({ header: '#BDDCFF', body: '#FFF', border: '#000' }), '#1A1A1A');
});

test('only an actor defaults to a person; stakeholder and part remain boxes', () => {
  assert.ok(isActor('«actor»'));
  assert.ok(!isActor('«stakeholder»'));
  assert.ok(!isActor('«part»'));
  assert.ok(!isActor(''));
});

test('a stereotype written without guillemets still reads', () => {
  styleTheme('light');
  assert.equal(forStereotype('part def').header, '#88B7E3');
  assert.ok(isActor('actor'));
});

function lightness(colour: string): number {
  const value = Number.parseInt(colour.slice(1), 16);
  return ((value >> 16 & 0xff) + (value >> 8 & 0xff) + (value & 0xff)) / 3 / 255;
}
