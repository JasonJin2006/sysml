/**
 * Syntax colour for the source view.
 *
 * The rule this module exists to keep: colouring must never change what the source *says*. Every
 * function here returns pieces whose text concatenates back to the input exactly, and a test asserts
 * that on the awkward cases — a block comment spanning lines, a string with an escaped quote, an
 * unterminated comment at the end of a file. A highlighter that eats a character is a highlighter
 * that lies about the model.
 *
 * This is a lexer, not a parser. It knows comments, strings, numbers, `@Annotations` and the words
 * the language reserves; everything else is a name and gets no colour. Colouring by syntax tree
 * would mean a second parser in the front end, and that one would disagree with the Rust parser the
 * moment either changed.
 */

export type TokenKind = 'comment' | 'string' | 'number' | 'keyword' | 'annotation' | 'plain';

export interface Token {
  kind: TokenKind;
  text: string;
}

/// Words the language reserves. Curated rather than exhaustive: a word missing here is drawn like a
/// name, which is what it looks like in a model that never uses it.
const KEYWORDS = new Set([
  'package', 'import', 'private', 'public', 'protected', 'alias', 'def', 'abstract',
  'variation', 'variant', 'individual', 'part', 'attribute', 'item', 'port', 'connection',
  'interface', 'ref', 'in', 'out', 'inout', 'calc', 'return', 'constraint', 'assert', 'assume',
  'require', 'requirement', 'satisfy', 'subject', 'actor', 'stakeholder', 'concern', 'objective',
  'action', 'state', 'transition', 'accept', 'then', 'done', 'send', 'view', 'expose', 'render',
  'metadata', 'enum', 'occurrence', 'event', 'snapshot', 'timeslice', 'message', 'flow',
  'allocate', 'bind', 'first', 'merge', 'decide', 'while', 'if', 'else', 'for', 'loop', 'doc',
  'comment', 'rep', 'language', 'end', 'derived', 'readonly', 'ordered', 'nonunique', 'subsets',
  'redefines', 'specializes', 'typing', 'conjugation', 'analysis', 'case', 'verification', 'use',
  'perform', 'exhibit', 'include', 'not', 'and', 'or', 'xor', 'true', 'false', 'null',
]);

const isDigit = (ch: string | undefined) => ch !== undefined && ch >= '0' && ch <= '9';
const isWordStart = (ch: string | undefined) =>
  ch !== undefined && ((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch === '_');
const isWord = (ch: string | undefined) => isWordStart(ch) || isDigit(ch);

/** Split the text into coloured pieces that concatenate back to it. */
export function tokenize(text: string): Token[] {
  const out: Token[] = [];
  let at = 0;
  const take = (kind: TokenKind, end: number) => {
    if (end > at) out.push({ kind, text: text.slice(at, end) });
    at = end;
  };

  while (at < text.length) {
    const ch = text[at];

    // `//` runs to the end of the line; the newline is not part of it.
    if (ch === '/' && text[at + 1] === '/') {
      const end = text.indexOf('\n', at);
      take('comment', end < 0 ? text.length : end);
      continue;
    }
    // `/* … */` may span lines and may be unterminated, which is a file being typed.
    if (ch === '/' && text[at + 1] === '*') {
      const end = text.indexOf('*/', at + 2);
      take('comment', end < 0 ? text.length : end + 2);
      continue;
    }
    // A string, honouring `\"` so the quote inside does not end it early.
    if (ch === '"' || ch === "'") {
      let cur = at + 1;
      while (cur < text.length) {
        if (text[cur] === '\\') {
          cur += 2;
          continue;
        }
        if (text[cur] === ch) {
          cur += 1;
          break;
        }
        cur += 1;
      }
      take('string', Math.min(cur, text.length));
      continue;
    }
    // `@Safety` and `#Tag` are annotations; the qualified form is one piece.
    if ((ch === '@' || ch === '#') && (isWordStart(text[at + 1]) || text[at + 1] === '/')) {
      let cur = at + 1;
      while (cur < text.length && (isWord(text[cur]) || text[cur] === ':' || text[cur] === '/')) {
        cur += 1;
      }
      take('annotation', cur);
      continue;
    }
    // A number: digits with `_`, and a fraction only when a digit follows the dot, so `1..*` keeps
    // its dots.
    if (isDigit(ch)) {
      let cur = at;
      while (isDigit(text[cur]) || text[cur] === '_') cur += 1;
      if (text[cur] === '.' && isDigit(text[cur + 1])) {
        cur += 1;
        while (isDigit(text[cur]) || text[cur] === '_') cur += 1;
      }
      take('number', cur);
      continue;
    }
    if (isWordStart(ch)) {
      let cur = at;
      while (isWord(text[cur])) cur += 1;
      const word = text.slice(at, cur);
      take(KEYWORDS.has(word) ? 'keyword' : 'plain', cur);
      continue;
    }
    // Anything else — punctuation, operators, whitespace — is one plain run.
    let cur = at + 1;
    while (
      cur < text.length &&
      !(text[cur] === '/' || text[cur] === '"' || text[cur] === "'" || text[cur] === '@' ||
        text[cur] === '#' || isWord(text[cur]))
    ) {
      cur += 1;
    }
    take('plain', cur);
  }
  return out;
}

/**
 * The same pieces, broken into rows that fit one `<span>` per line.
 *
 * A token that spans lines — a block comment — is split at each newline, so a row never carries a
 * line break of its own. Joining a row's text gives that line, and joining the rows with `\n` gives
 * the text back.
 */
export function rows(text: string): Token[][] {
  const out: Token[][] = [[]];
  for (const token of tokenize(text)) {
    let rest = token.text;
    let newline = rest.indexOf('\n');
    while (newline >= 0) {
      if (newline > 0) out[out.length - 1].push({ kind: token.kind, text: rest.slice(0, newline) });
      out.push([]);
      rest = rest.slice(newline + 1);
      newline = rest.indexOf('\n');
    }
    if (rest !== '') out[out.length - 1].push({ kind: token.kind, text: rest });
  }
  return out;
}
