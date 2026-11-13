import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rows, tokenize, type Token } from './highlight.ts';

const text = (tokens: Token[]) => tokens.map((token) => token.text).join('');
const kinds = (source: string) => tokenize(source).map((token) => token.kind);

/**
 * The invariant every other test leans on: colouring returns pieces, not a rewritten string.
 *
 * A highlighter that loses a character changes what the model says while looking like it is only
 * drawing. That failure is invisible in a screenshot and expensive in a review.
 */
const SAMPLES = [
  '',
  'package P {\n    part def Brake;\n}\n',
  '// a line comment\npart def X;\n',
  '/* a block\n   comment */\npart def Y;\n',
  'attribute s : String = "a \\" quoted \\" value";',
  'attribute n : Real = 1_000.25;',
  '@Safety { level = 3 }\npart def Brake;',
  '#fmeaspec part p;',
  'attribute m [0..*] : Real;',
  '/* never closed\nstill a comment',
  '"never closed',
  'attribute e : Real = 1 + ;',
];

for (const sample of SAMPLES) {
  test(`colouring keeps every character: ${JSON.stringify(sample.slice(0, 24))}`, () => {
    assert.equal(text(tokenize(sample)), sample);
    assert.equal(rows(sample).map(text).join('\n'), sample);
  });
}

test('a block comment is one piece and may span lines', () => {
  const tokens = tokenize('/* one\ntwo */\npart');
  assert.deepEqual(tokens[0], { kind: 'comment', text: '/* one\ntwo */' });
  assert.equal(tokens.at(-1)?.kind, 'keyword');
});

test('an escaped quote does not end the string', () => {
  const tokens = tokenize('"a \\" b" x');
  assert.deepEqual(tokens[0], { kind: 'string', text: '"a \\" b"' });
  assert.equal(tokens[1].kind, 'plain');
});

test('an unterminated comment or string runs to the end rather than disappearing', () => {
  assert.deepEqual(tokenize('/* open'), [{ kind: 'comment', text: '/* open' }]);
  assert.deepEqual(tokenize('"open'), [{ kind: 'string', text: '"open' }]);
});

test('multiplicity keeps its dots out of the number', () => {
  // `1..*` is a range, not the real number `1.`. Colouring the dots as a number would suggest a
  // value where the model wrote a bound.
  const tokens = tokenize('[0..*]');
  assert.deepEqual(tokens.filter((token) => token.kind === 'number').map((token) => token.text), ['0']);
});

test('annotations are recognised with their qualifier', () => {
  assert.deepEqual(tokenize('@FMEA::Safety')[0], { kind: 'annotation', text: '@FMEA::Safety' });
  assert.deepEqual(tokenize('#fmeaspec')[0], { kind: 'annotation', text: '#fmeaspec' });
});

test('reserved words are keywords and names are not', () => {
  const tokens = tokenize('part def Brake');
  assert.deepEqual(tokens.map((token) => token.text), ['part', ' ', 'def', ' ', 'Brake']);
  assert.deepEqual(kinds('part def Brake'), ['keyword', 'plain', 'keyword', 'plain', 'plain']);
});

test('a word that merely starts like a keyword is a name', () => {
  // `partition` is not `part`, and colouring it as one would suggest a construct the model did not
  // write.
  assert.deepEqual(kinds('partition'), ['plain']);
});

test('rows never carry a line break of their own', () => {
  for (const sample of SAMPLES) {
    for (const row of rows(sample)) {
      assert.ok(!text(row).includes('\n'), JSON.stringify(text(row)));
    }
  }
});
