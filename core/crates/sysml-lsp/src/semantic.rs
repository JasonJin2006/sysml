//! Giving the editor enough information to colour a model by what its names *mean*.
//!
//! A grammar file can tell `part` from `def` — both are keywords, both get one colour. It cannot
//! tell a declaration from a use: in `part battery : Battery`, the first `battery` is a feature
//! being created and the second `Battery` is a definition being referred to, and they are the same
//! kind of token to a regex. That difference is the whole point of semantic highlighting: a reader
//! scanning a model finds the definitions by their colour.
//!
//! # What is coloured
//!
//! * **A declaration's own name** is drawn as its kind. A `part def` reads like a type, a `part`
//!   like a field — the same distinction the diagram makes by drawing definitions square and usages
//!   rounded.
//! * **A name written in a reference** is drawn as what it binds to. The `Battery` in
//!   `part battery : Battery` names a definition wherever that definition sits, so it gets the
//!   type colour even if the use is inside a usage.
//! * **A keyword** is drawn as a keyword, from the lexer's own table, so the set cannot disagree
//!   with what the parser accepts.
//!
//! # Why the client must not re-derive this
//!
//! An editor could colour by scope with its own parse, and then the colours would disagree with
//! `check` about which name is broken and with the diagram about which box is a definition. One
//! answer, computed here, is the reason the language server exists.

use sysml_model::{Element, Kind, TextRange};
use sysml_resolve::{Index, Resolution};

/// The token types this server emits, in the order the legend lists them.
///
/// The protocol sends the legend once and then only numbers, so the *order* here is the contract:
/// a type is added at the end, or every colour in every file shifts by one.
pub const TYPES: &[&str] = &[
    "keyword",   // 0
    "namespace", // 1
    "type",      // 2
    "class",     // 3
    "parameter", // 4
    "property",  // 5
    "variable",  // 6
];

/// The modifiers, in the order the legend lists them. Each is a bit in the token's modifier mask.
pub const MODIFIERS: &[&str] = &["declaration", "definition"];

const KEYWORD: u32 = 0;
const NAMESPACE: u32 = 1;
const TYPE: u32 = 2;
const CLASS: u32 = 3;
const PARAMETER: u32 = 4;
const PROPERTY: u32 = 5;
const VARIABLE: u32 = 6;

const DECLARATION: u32 = 1;
const DEFINITION: u32 = 2;

/// A token as the protocol wants it: line, column, length, type, modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub line: u32,
    pub column: u32,
    pub length: u32,
    pub kind: u32,
    pub modifiers: u32,
}

/// Every token of `text`, ready to encode.
///
/// `file` is the buffer's index: a span carries it, and a token from another file is not this
/// document's business. The result is sorted by position, which the protocol requires — a client
/// that receives them unordered draws colour in the wrong place.
pub fn tokens(roots: &[Element], library: &[Element], text: &str, file: usize) -> Vec<Token> {
    let mut out = Vec::new();
    // Keywords first. A name can never be a keyword — the parser would not accept it unquoted —
    // so nothing later can collide with these, but the sort below keeps the list ordered anyway.
    keywords(text, &mut out);

    let index = Index::build(roots).with_library(library);
    for element in walk(roots) {
        if element.kind == Kind::Unsupported {
            // A construct the grammar rejected is already an error; colouring it as a name would
            // say the model has an element there when it does not.
            continue;
        }
        if let Some(span) = element.name_span.filter(|span| span.file == file) {
            if let Some(length) = length_of(text, span) {
                push(
                    &mut out,
                    text,
                    span,
                    length,
                    kind_of(element),
                    modifiers(element),
                );
            }
        }
        // Each written reference, coloured by what it binds to. The element's own `ref_spans`
        // records where it was written; where it points is the resolver's answer, and a name that
        // does not resolve keeps the reference colour rather than being guessed at.
        for (written, span) in &element.ref_spans {
            if span.file != file {
                continue;
            }
            let Some(length) = length_of(text, *span) else {
                continue;
            };
            let target = match index.resolve(element.id, written) {
                Resolution::Found(id) => index.kind.get(id).map(kind_of_kind),
                _ => None,
            };
            let kind = target.unwrap_or(TYPE);
            push(&mut out, text, *span, length, kind, 0);
        }
    }
    out.sort_by_key(|token| (token.line, token.column));
    out.dedup_by(|b, a| {
        a.line == b.line && a.column == b.column && a.length == b.length && a.kind == b.kind
    });
    out
}

/// The protocol's delta encoding: line deltas, then column deltas.
///
/// A token is five numbers: how many lines since the last one, how many columns since the last one
/// on its line, its length, its type, its modifiers. Sending absolute positions instead is a decode
/// error the client reports as "no highlighting", with nothing to point at.
pub fn encode(tokens: &[Token]) -> Vec<u32> {
    let mut out = Vec::with_capacity(tokens.len() * 5);
    let mut last_line = 0u32;
    let mut last_column = 0u32;
    for token in tokens {
        let delta_line = token.line - last_line;
        let delta_column = if delta_line == 0 {
            token.column - last_column
        } else {
            token.column
        };
        out.extend_from_slice(&[
            delta_line,
            delta_column,
            token.length,
            token.kind,
            token.modifiers,
        ]);
        last_line = token.line;
        last_column = token.column;
    }
    out
}

/// The colour a declaration's own name gets, from the kind it declares.
fn kind_of(element: &Element) -> u32 {
    kind_of_kind(&element.kind)
}

fn kind_of_kind(kind: &Kind) -> u32 {
    match kind {
        Kind::Package => NAMESPACE,
        Kind::AttributeUsage => PROPERTY,
        Kind::Parameter => PARAMETER,
        Kind::Subject | Kind::Objective => VARIABLE,
        // A definition reads as a class, which is what an editor's theme draws as the name of a
        // type; every other usage is a member of something and reads as a property. This is the
        // same split the diagram draws — definitions square, usages rounded — so the two pictures
        // agree about what a name is.
        other if other.is_definition() => CLASS,
        _ => PROPERTY,
    }
}

/// The modifier mask for a declaration's name.
fn modifiers(element: &Element) -> u32 {
    let mut mask = DECLARATION;
    if element.is_definition {
        mask |= DEFINITION;
    }
    mask
}

/// A span's length in UTF-16 units, or `None` when it is empty or holds a line break.
///
/// The protocol counts columns in UTF-16 units, not bytes, and a token cannot span a line. A quoted
/// name (`'vehicle-trailer system'`) is one token, so its own quotes are part of it — the editor
/// draws the whole thing.
fn length_of(text: &str, span: TextRange) -> Option<u32> {
    let slice = text.get(span.start..span.end)?;
    if slice.is_empty() || slice.contains('\n') {
        return None;
    }
    Some(slice.encode_utf16().count() as u32)
}

/// Where the protocol counts this token.
fn position_of(text: &str, span: TextRange) -> (u32, u32) {
    let offset = span.start.min(text.len());
    let mut line = 0u32;
    let mut start = 0usize;
    for (at, byte) in text.bytes().enumerate() {
        if at >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            start = at + 1;
        }
    }
    let column = text[start..offset].encode_utf16().count() as u32;
    (line, column)
}

fn push(out: &mut Vec<Token>, text: &str, span: TextRange, length: u32, kind: u32, modifiers: u32) {
    let (line, column) = position_of(text, span);
    out.push(Token {
        line,
        column,
        length,
        kind,
        modifiers,
    });
}

/// Every keyword in `text`, from the lexer's own table.
///
/// A hand-written list here would be a second spelling of the grammar, and the two would drift the
/// first time a keyword was added. `sysml_parser::lex` is the authority.
fn keywords(text: &str, out: &mut Vec<Token>) {
    let Ok(tokens) = sysml_parser::lex(text) else {
        // A buffer the lexer cannot finish is a buffer the parser will report; colouring half of it
        // is a picture the diagnostics then contradict.
        return;
    };
    for token in tokens {
        if token.kind != sysml_parser::TokenKind::Keyword {
            continue;
        }
        let span = TextRange {
            file: usize::MAX,
            start: token.start,
            end: token.end,
        };
        let Some(length) = length_of(text, span) else {
            continue;
        };
        out.extend([{
            let (line, column) = position_of(text, span);
            Token {
                line,
                column,
                length,
                kind: KEYWORD,
                modifiers: 0,
            }
        }]);
    }
}

/// Every element of a forest, pre-order.
fn walk(roots: &[Element]) -> Vec<&Element> {
    fn one<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        out.push(el);
        for child in &el.children {
            one(child, out);
        }
    }
    let mut out = Vec::new();
    for root in roots {
        one(root, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(src: &str) -> Vec<Element> {
        sysml_syntax::load(src).expect("parses").roots
    }

    fn coloured(src: &str) -> Vec<(String, u32)> {
        let roots = load(src);
        tokens(&roots, &[], src, 0)
            .into_iter()
            .map(|t| {
                let line_start = src
                    .lines()
                    .take(t.line as usize)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                let text = &src[line_start..];
                let byte = text
                    .char_indices()
                    .scan(0usize, |units, (i, c)| {
                        let at = *units;
                        *units += c.len_utf16();
                        Some((at, i))
                    })
                    .find(|(units, _)| *units == t.column as usize)
                    .map(|(_, i)| i)
                    .unwrap_or(0);
                let word: String = text[byte..].chars().take(t.length as usize).collect();
                (word, t.kind)
            })
            .collect()
    }

    #[test]
    fn a_keyword_is_a_keyword() {
        let found = coloured("package P {\n}\n");
        assert!(
            found.contains(&("package".to_string(), KEYWORD)),
            "{found:?}"
        );
    }

    #[test]
    fn a_definition_is_a_class_and_a_usage_is_a_property() {
        let src = "package P {\n  part def Battery;\n  part def Car {\n    part battery : Battery;\n  }\n}\n";
        let found = coloured(src);
        let battery_def = found
            .iter()
            .find(|(w, k)| w == "Battery" && *k == CLASS)
            .expect("the definition is a class");
        assert_eq!(battery_def.1, CLASS);
        let usage = found
            .iter()
            .filter(|(w, _)| w == "battery")
            .map(|(_, k)| *k)
            .next()
            .expect("the usage is coloured");
        assert_eq!(usage, PROPERTY, "a part usage is a property");
    }

    #[test]
    fn a_reference_is_coloured_by_what_it_points_at() {
        // The `Battery` after the colon names a definition, so it is coloured as one even though
        // it sits inside a usage's declaration.
        let src = "package P {\n  part def Battery;\n  part def Car {\n    part battery : Battery;\n  }\n}\n";
        let found = coloured(src);
        let occurrences: Vec<u32> = found
            .iter()
            .filter(|(w, _)| w == "Battery")
            .map(|(_, k)| *k)
            .collect();
        assert!(occurrences.len() >= 2, "{found:?}");
        assert!(
            occurrences.iter().all(|k| *k == CLASS),
            "both the declaration and the reference read as definitions: {found:?}"
        );
    }

    #[test]
    fn a_package_is_a_namespace() {
        let found = coloured("package P {\n}\n");
        assert!(found.contains(&("P".to_string(), NAMESPACE)), "{found:?}");
    }

    #[test]
    fn an_unparseable_construct_gets_no_colour() {
        // The buffer lowers `syntax error` as an element; colouring it would claim the model has
        // something there.
        let found = coloured("package P {\n  this is not sysml\n}\n");
        assert!(
            !found.iter().any(|(w, _)| w.contains("syntax")),
            "{found:?}"
        );
    }

    /// Encoding: five numbers per token, deltas relative to the previous one.
    #[test]
    fn the_encoding_is_a_delta_per_token() {
        let tokens = vec![
            Token {
                line: 0,
                column: 2,
                length: 4,
                kind: KEYWORD,
                modifiers: 0,
            },
            Token {
                line: 0,
                column: 8,
                length: 1,
                kind: NAMESPACE,
                modifiers: 3,
            },
            Token {
                line: 2,
                column: 4,
                length: 3,
                kind: CLASS,
                modifiers: 3,
            },
        ];
        assert_eq!(
            encode(&tokens),
            vec![
                0, 2, 4, KEYWORD, 0, // first token on line 0
                0, 6, 1, NAMESPACE, 3, // same line: delta column 8 - 2 = 6
                2, 4, 3, CLASS, 3, // two lines down: column is absolute again
            ]
        );
    }

    #[test]
    fn tokens_are_sorted_and_unique() {
        let src = "package P {\n  part def A;\n}\n";
        let roots = load(src);
        let list = tokens(&roots, &[], src, 0);
        for pair in list.windows(2) {
            assert!(
                (pair[0].line, pair[0].column) <= (pair[1].line, pair[1].column),
                "out of order"
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for token in &list {
            assert!(
                seen.insert((token.line, token.column, token.length, token.kind)),
                "duplicate token at {}:{}",
                token.line,
                token.column
            );
        }
    }
}
