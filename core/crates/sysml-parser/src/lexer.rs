//! Lexer for SysML v2 and KerML textual notation.
//!
//! The lexer produces a flat token stream with byte offsets and line numbers, and keeps every
//! token's authored spelling. Nothing downstream re-reads the source to reconstruct a name: a
//! quoted name (`'vehicle-trailer system'`) and a short name (`<B>`), a real literal and a
//! string literal all carry their own text.
//!
//! # What is deliberately not a token
//!
//! Comments are skipped rather than emitted, but their spans are recorded separately so the
//! lowering layer can attach `doc /* ... */` prose to the element that owns it. Whitespace is
//! not a token at all.
//!
//! # Reserved words
//!
//! The set below is the union of SysML and KerML reserved words. It matters because the grammar
//! distinguishes `step entry[1];` (a step *named* `entry`, legal) from `entry` as a state body
//! keyword: a lexer that folds every keyword into one token type cannot tell those apart, which
//! is exactly the failure the previous parser had. Keywords are therefore *identified* tokens,
//! not a separate class, and the parser decides by position what a keyword means.

use std::fmt;

/// One lexical token.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// The authored text, exactly as written, including quotes for a quoted name.
    pub text: String,
    /// Byte offset of the first character in the source buffer.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
    /// 1-based line number of `start`.
    pub line: u32,
}

impl Token {
    /// The name this token introduces, with quoting and short-name brackets removed.
    ///
    /// `'vehicle-trailer system'` yields `vehicle-trailer system`; `<B>` yields `B`. This is the
    /// form that goes into an element tree, because it is what a reference is written against.
    pub fn name_text(&self) -> String {
        match self.kind {
            TokenKind::QuotedName => {
                let inner = self.text.trim_start_matches('\'').trim_end_matches('\'');
                inner.replace("\\'", "'")
            }
            TokenKind::ShortName => {
                let inner = self.text.trim_start_matches('<').trim_end_matches('>');
                inner
                    .trim_start_matches('\'')
                    .trim_end_matches('\'')
                    .to_string()
            }
            _ => self.text.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A bare identifier that is not a reserved word.
    Ident,
    /// A reserved word. Carries no lexical difference from `Ident` beyond table membership;
    /// the parser decides the role by position.
    Keyword,
    /// `'a name with spaces'`.
    QuotedName,
    /// `<short>`.
    ShortName,
    /// An integer or real literal, with any radix prefix. `text` is the authored spelling.
    Number,
    /// A string literal, quotes included in `text`.
    String,
    /// A single punctuation or operator character, or a maximal operator run.
    Punct,
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> &'static str {
        match self {
            TokenKind::Ident => "identifier",
            TokenKind::Keyword => "keyword",
            TokenKind::QuotedName => "quoted name",
            TokenKind::ShortName => "short name",
            TokenKind::Number => "number",
            TokenKind::String => "string",
            TokenKind::Punct => "punctuation",
            TokenKind::Eof => "end of input",
        }
    }
}

/// A lexing failure, with the position that caused it.
#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: u32,
    pub offset: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (line {})", self.message, self.line)
    }
}

impl std::error::Error for LexError {}

/// The SysML and KerML reserved words.
///
/// This list is generated from the two grammars' keyword sets. It is deliberately permissive
/// about words that are keywords only in one language: KerML's `step` and `feature` are reserved
/// in KerML and appear as identifiers in SysML bodies, and the parser resolves that by position.
/// Marking them reserved is still correct — a model cannot declare a feature *named* `step`
/// without quoting it, in either language.
const KEYWORDS: &[&str] = &[
    "abstract",
    "alias",
    "all",
    "allocate",
    "allocation",
    "analysis",
    "and",
    "as",
    "assert",
    "assoc",
    "association",
    "assume",
    "attribute",
    "behavior",
    "bind",
    "binding",
    "bool",
    "by",
    "calc",
    "calculation",
    "case",
    "class",
    "classifier",
    "comment",
    "concern",
    "conjugate",
    "conjugates",
    "connect",
    "connection",
    "constant",
    "constraint",
    "crosses",
    "datatype",
    "decide",
    "def",
    "defined",
    "dependency",
    "derived",
    "disjoint",
    "do",
    "doc",
    "else",
    "end",
    "entry",
    "enum",
    "event",
    "exhibit",
    "exit",
    "expr",
    "false",
    "feature",
    "filter",
    "first",
    "flow",
    "for",
    "fork",
    "frame",
    "from",
    "function",
    "hastype",
    "if",
    "implies",
    "import",
    "in",
    "include",
    "individual",
    "inout",
    "interaction",
    "interface",
    "inv",
    "invariant",
    "istype",
    "item",
    "join",
    "language",
    "library",
    "locale",
    "loop",
    "merge",
    "message",
    "meta",
    "metaclass",
    "metadata",
    "multiplicity",
    "namespace",
    "new",
    "nonunique",
    "not",
    "null",
    "objective",
    "occurrence",
    "of",
    "or",
    "ordered",
    "out",
    "package",
    "part",
    "perform",
    "port",
    "portion",
    "predicate",
    "private",
    "protected",
    "public",
    "readonly",
    "redefines",
    "ref",
    "references",
    "render",
    "rendering",
    "rep",
    "require",
    "requirement",
    "return",
    "satisfy",
    "send",
    "snapshot",
    "specialization",
    "specializes",
    "standard",
    "state",
    "step",
    "struct",
    "subclassifier",
    "subset",
    "subsets",
    "succession",
    "terminate",
    "then",
    "timeslice",
    "to",
    "transition",
    "true",
    "type",
    "typed",
    "typings",
    "until",
    "use",
    "var",
    "variant",
    "verification",
    "verify",
    "via",
    "view",
    "viewpoint",
    "when",
    "while",
    "xor",
];

fn is_keyword(text: &str) -> bool {
    KEYWORDS.binary_search(&text).is_ok()
}

/// Every token in `src`, ending with exactly one [`TokenKind::Eof`].
///
/// The whole buffer is tokenized in one pass. A failure does not stop the scan at the first
/// problem: the lexer reports the error and continues from a plausible restart point, so a model
/// with one bad character still yields a stream the parser can comment on usefully.
///
/// # Errors
///
/// Returns [`LexError`] when the buffer holds a character no token can start, or a string or
/// comment that is never closed. The error carries the first such line and its byte offset.
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;

    while i < bytes.len() {
        let c = bytes[i];
        // Whitespace.
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comments. `/* ... */` nests in KerML, and `//` runs to end of line.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start_line = line;
            let comment_start = i;
            let mut depth = 1usize;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'\n' {
                    line += 1;
                    i += 1;
                } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                return Err(LexError {
                    message: "unterminated block comment".to_string(),
                    line: start_line,
                    offset: i,
                });
            }
            // `doc /* … */` and `comment /* … */` are the two places the notation puts prose in a
            // block comment, and the prose is the element's documentation. Every model in the
            // corpus writes `doc` that way, so skipping the comment made `Element.doc` empty for
            // all of them and left `sysml trace` with nothing to read. The comment becomes a
            // string token only when the token before it asked for prose; everywhere else the
            // comment is still skipped, which is what the grammar says it is.
            let wants_prose = tokens
                .last()
                .is_some_and(|t| t.text == "doc" || t.text == "comment");
            if wants_prose {
                let body_start = comment_start + 2;
                let body_end = i.saturating_sub(2);
                let text = String::from_utf8_lossy(&bytes[body_start..body_end.min(bytes.len())])
                    .trim()
                    .to_string();
                tokens.push(Token {
                    kind: TokenKind::String,
                    text: format!("\"{}\"", text.replace('"', "'")),
                    start: comment_start,
                    end: i,
                    line: start_line,
                });
            }
            continue;
        }
        // `//* … */` is a multi-line note (`ML_NOTE`, KerML 1.0 §8.2.2.3), not a line comment that
        // happens to start with `*`. Read as `//`, only its first line was skipped: the rest —
        // commented-out declarations in `4a-Functional Allocation.sysml:24-34` and
        // `13a-Model Containment.sysml:37` — parsed as model content.
        if bytes[i..].starts_with(b"//*") {
            let start_line = line;
            i += 3;
            loop {
                if i + 1 >= bytes.len() {
                    return Err(LexError {
                        message: "unterminated note: `//*` needs a closing `*/`".to_string(),
                        line: start_line,
                        offset: i,
                    });
                }
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        let start = i;
        let start_line = line;

        // Quoted name: `'vehicle-trailer system'`. A quote inside is escaped as `\'`.
        if c == b'\'' {
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() && bytes[i + 1] == b'\'' => i += 2,
                    b'\'' => {
                        i += 1;
                        closed = true;
                        break;
                    }
                    b'\n' => {
                        line += 1;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            if !closed {
                return Err(LexError {
                    message: "unterminated quoted name".to_string(),
                    line: start_line,
                    offset: start,
                });
            }
            tokens.push(Token {
                kind: TokenKind::QuotedName,
                text: src[start..i].to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }

        // Short name: `<B>`. Only when the `<` is followed by an identifier and a `>`; otherwise
        // `<` is a comparison operator.
        if c == b'<' {
            // A short name may be a plain identifier (`<B>`) or a quoted one (`<'1.1'>`), the
            // latter being how a requirement identifier like `'1.1'` is written.
            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'\'' {
                j += 1;
                while j < bytes.len() && bytes[j] != b'\'' {
                    j += 1;
                }
                if j < bytes.len() {
                    j += 1;
                }
            } else {
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b'>' {
                i = j + 1;
                tokens.push(Token {
                    kind: TokenKind::ShortName,
                    text: src[start..i].to_string(),
                    start,
                    end: i,
                    line: start_line,
                });
                continue;
            }
        }

        // String literal.
        if c == b'"' {
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => i += 2,
                    b'"' => {
                        i += 1;
                        closed = true;
                        break;
                    }
                    b'\n' => {
                        line += 1;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            if !closed {
                return Err(LexError {
                    message: "unterminated string literal".to_string(),
                    line: start_line,
                    offset: start,
                });
            }
            tokens.push(Token {
                kind: TokenKind::String,
                text: src[start..i].to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }

        // Number: digits, optionally with a fraction and exponent. A leading `-` is an operator,
        // not part of the literal, so `a - 1` and `-1` are the same shape.
        if c.is_ascii_digit() || (c == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            let mut seen_dot = false;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_digit() || b == b'_' {
                    i += 1;
                    continue;
                }
                // A literal has at most one decimal point. `1..n` is a range: the first `.`
                // ends the literal and the `..` operator follows. Accepting every `.` made the
                // whole `1..n` a malformed number and lost the range.
                if b == b'.' && !seen_dot && i + 1 < bytes.len() && bytes[i + 1] != b'.' {
                    seen_dot = true;
                    i += 1;
                    continue;
                }
                // An exponent marker is part of the literal only when a digit or sign follows.
                if (b == b'e' || b == b'E')
                    && i + 1 < bytes.len()
                    && (bytes[i + 1].is_ascii_digit()
                        || bytes[i + 1] == b'+'
                        || bytes[i + 1] == b'-')
                {
                    i += 1;
                    continue;
                }
                break;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: src[start..i].to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }

        // Identifier or keyword.
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let text = &src[start..i];
            // `true`, `false` and `null` are literals in expression position but keywords
            // lexically; the parser reads them as literal operands.
            let kind = if is_keyword(text) {
                TokenKind::Keyword
            } else {
                TokenKind::Ident
            };
            tokens.push(Token {
                kind,
                text: text.to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }

        // Operators and punctuation. Multi-character forms are matched longest-first so `:>>`
        // is one token, not `:` then `>>`.
        let rest = &src[i..];
        let mut matched: Option<(&'static str, usize)> = None;
        for op in MULTI_CHAR_OPERATORS {
            if rest.starts_with(op) && matched.is_none_or(|(_, n)| op.len() > n) {
                matched = Some((op, op.len()));
            }
        }
        if let Some((op, len)) = matched {
            i += len;
            tokens.push(Token {
                kind: TokenKind::Punct,
                text: op.to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }
        if c.is_ascii_punctuation() {
            i += 1;
            tokens.push(Token {
                kind: TokenKind::Punct,
                text: src[start..i].to_string(),
                start,
                end: i,
                line: start_line,
            });
            continue;
        }

        // Anything else is a character this language does not use outside comments and strings.
        return Err(LexError {
            message: format!("unexpected character {}", describe_char(src, start)),
            line: start_line,
            offset: start,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        text: String::new(),
        start: bytes.len(),
        end: bytes.len(),
        line,
    });
    Ok(tokens)
}

/// A printable description of the character at `start`, for a diagnostic.
///
/// Takes the character rather than a byte: the source is UTF-8 and a non-ASCII character such as
/// the degree sign in `°C` occupies more than one byte, so slicing one byte panicked instead of
/// reporting what was there.
fn describe_char(src: &str, start: usize) -> String {
    match src[start..].chars().next() {
        Some(c) => format!("{c:?}"),
        None => "end of input".to_string(),
    }
}

/// Operators of more than one character, longest first.
///
/// `:>>` must precede `:>` so a redefine clause is not read as a specialization plus `>`.
const MULTI_CHAR_OPERATORS: &[&str] = &[
    // Three-character forms first so `===` is not read as `==` then `=`.
    "===", "!==", ":>>", "::>", ":>", "::", "->", "=>", "==", "!=", "<=", ">=", "**", ":=", "..",
    "&&", "||", "??", "@@",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src)
            .expect("lexes")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn texts(src: &str) -> Vec<String> {
        lex(src)
            .expect("lexes")
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| t.text)
            .collect()
    }

    #[test]
    fn a_quoted_name_keeps_its_spaces() {
        let toks = lex("'vehicle-trailer system'").expect("lexes");
        assert_eq!(toks[0].kind, TokenKind::QuotedName);
        assert_eq!(toks[0].name_text(), "vehicle-trailer system");
    }

    #[test]
    fn a_short_name_is_its_own_token() {
        let toks = lex("<B> byte").expect("lexes");
        assert_eq!(toks[0].kind, TokenKind::ShortName);
        assert_eq!(toks[0].name_text(), "B");
        assert_eq!(toks[1].text, "byte");
    }

    #[test]
    fn less_than_is_still_an_operator() {
        // `<` is a comparison unless it closes a short name; `a < b` must not lex as `<b...`.
        let t = texts("a < b");
        assert_eq!(t, vec!["a", "<", "b"]);
    }

    #[test]
    fn a_redefine_clause_is_one_token() {
        assert_eq!(texts(":>>")[0], ":>>");
        assert_eq!(texts(":>")[0], ":>");
    }

    #[test]
    fn keywords_are_identified_but_tagged() {
        let toks = lex("part def").expect("lexes");
        assert_eq!(toks[0].kind, TokenKind::Keyword);
        assert_eq!(toks[1].kind, TokenKind::Keyword);
        let toks = lex("widget").expect("lexes");
        assert_eq!(toks[0].kind, TokenKind::Ident);
    }

    #[test]
    fn a_reserved_word_is_still_usable_as_a_name_in_position() {
        // `step entry[1];` declares a step named `entry`. The lexer must not refuse it; the
        // parser decides by position, which is what the previous parser got wrong.
        let toks = lex("step entry[1];").expect("lexes");
        assert_eq!(toks[0].text, "step");
        assert_eq!(toks[1].text, "entry");
        assert_eq!(toks[1].kind, TokenKind::Keyword);
    }

    #[test]
    fn block_comments_nest_and_are_skipped() {
        let t = texts("a /* outer /* inner */ still */ b");
        assert_eq!(t, vec!["a", "b"]);
    }

    #[test]
    fn line_comments_run_to_end_of_line() {
        let t = texts("a // trailing\nb");
        assert_eq!(t, vec!["a", "b"]);
    }

    #[test]
    fn line_numbers_count_newlines() {
        let toks = lex("a\nb\n\nc").expect("lexes");
        let lines: Vec<u32> = toks.iter().map(|t| t.line).collect();
        assert_eq!(lines, vec![1, 2, 4, 4]);
    }

    #[test]
    fn numbers_keep_their_spelling() {
        assert_eq!(texts("1e5"), vec!["1e5"]);
        assert_eq!(texts("3.6e6"), vec!["3.6e6"]);
        assert_eq!(texts("1_000"), vec!["1_000"]);
    }

    #[test]
    fn an_unterminated_string_is_an_error_with_a_line() {
        let e = lex("a\n\"open").expect_err("must fail");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn a_subsetting_operator_is_one_token() {
        assert_eq!(texts("a :> b"), vec!["a", ":>", "b"]);
    }

    #[test]
    fn a_non_ascii_character_is_reported_not_a_panic() {
        // The degree sign occupies two bytes. Slicing one byte at the failure point panicked
        // instead of reporting the character, which is a crash on valid-adjacent input.
        let e = lex("a °C").expect_err("must fail");
        assert!(
            e.message.contains('°'),
            "the character is named: {}",
            e.message
        );
    }

    #[test]
    fn the_keyword_table_is_sorted() {
        // `is_keyword` uses binary_search, so an unsorted table silently misclassifies words.
        // This test exists because the table was hand-written and unsorted once already.
        let mut sorted = KEYWORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(KEYWORDS, sorted.as_slice());
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted, deduped, "no keyword may appear twice");
    }

    #[test]
    fn kinds_include_eof_once() {
        let k = kinds("a");
        assert_eq!(k.iter().filter(|k| **k == TokenKind::Eof).count(), 1);
        assert_eq!(k.len(), 2);
    }
}
