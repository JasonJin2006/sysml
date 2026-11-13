//! What happens to the text after the first thing the parser cannot read.
//!
//! The crate's own documentation makes two promises: parsing never panics, and it never silently
//! drops input. The second is the one that needed tests. A parser that reports an error and then
//! loses the rest of the body looks identical to one that recovered, from every check downstream:
//! `check` is already failing, so nobody looks at the outline.

use sysml_parser::ast::{BodyMember, Document};
use sysml_parser::parse;

/// Every member name the document states, in order.
fn member_names(members: &[BodyMember]) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(m: &BodyMember, out: &mut Vec<String>) {
        if let BodyMember::Declaration(d) = m {
            if !d.head.name.text.is_empty() {
                out.push(d.head.name.text.clone());
            }
            for child in &d.body {
                walk(child, out);
            }
        }
    }
    for member in members {
        walk(member, &mut out);
    }
    out
}

fn errors(text: &str) -> Vec<(sysml_parser::ast::Span, String)> {
    parse(text).expect("the document parses").document.errors()
}

#[test]
fn a_token_that_starts_no_member_is_reported() {
    // `???` is not valid SysML and starts no production. It used to return `Ok(None)` from the
    // declaration parser, so the token vanished and `check` printed "no errors".
    let found = errors("package P {\n\tpart def A;\n\t???\n\tpart def B;\n}\n");
    assert_eq!(found.len(), 1, "one unreadable token, one error: {found:?}");
    assert!(
        found[0].1.contains("found"),
        "the error names what it found: {found:?}"
    );
}

#[test]
fn the_declaration_after_a_bad_token_survives() {
    // Recovery is only worth having if it stops before the next declaration. Skipping to the
    // enclosing `}` swallowed `part def B;`, so one bad token cost two declarations.
    let doc = parse("package P {\n\tpart def A;\n\t???\n\tpart def B;\n}\n").expect("parses");
    let names = member_names(&doc.document.members);
    assert!(
        names.iter().any(|n| n == "A"),
        "before the error: {names:?}"
    );
    assert!(names.iter().any(|n| n == "B"), "after the error: {names:?}");
}

#[test]
fn an_unterminated_string_is_an_error_not_a_panic() {
    let err = parse("package P {\n\tattribute s = \"abc\n}\n").unwrap_err();
    assert!(err.message.contains("unterminated"), "{err:?}");
}

#[test]
fn an_unterminated_comment_is_an_error_not_a_panic() {
    let err = parse("package P {\n\t/* never closed\n}\n").unwrap_err();
    assert!(err.message.contains("unterminated"), "{err:?}");
}

#[test]
fn a_missing_semicolon_is_an_error_and_keeps_both_declarations() {
    // Valid-looking text with one missing `;`. The parser reports it, and the declaration after
    // it is still read rather than folded into the error.
    let text = "package P {\n\tpart def A\n\tpart def B;\n}\n";
    let doc = parse(text).expect("parses");
    assert!(doc.document.has_errors(), "the missing `;` is reported");
    let names = member_names(&doc.document.members);
    assert!(
        names.iter().any(|n| n == "B"),
        "the declaration after the missing semicolon: {names:?}"
    );
}
