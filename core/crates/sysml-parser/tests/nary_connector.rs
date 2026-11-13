//! The n-ary connector and the named include, as the grammar writes them.
//!
//! `NaryConnectorPart` (`SysML.xtext:1079-1082`) is `'(' end ',' end (',' end)* ')'`, and
//! `IncludeUseCaseUsage` (`SysML.xtext:2300-2306`) takes `use case` with an optional declaration.

use sysml_parser::ast::{BodyMember, Statement};
use sysml_parser::parse;

fn connect_statement_ends(src: &str) -> Vec<String> {
    let doc = parse(src).unwrap_or_else(|e| panic!("must parse: {e}"));
    fn find(members: &[BodyMember]) -> Option<Vec<String>> {
        for m in members {
            match m {
                BodyMember::Statement(Statement::Connect {
                    from, to, extra, ..
                }) => {
                    let mut ends = vec![from.clone(), to.clone()];
                    ends.extend(extra.iter().cloned());
                    return Some(ends);
                }
                BodyMember::Declaration(d) => {
                    if let Some(found) = find(&d.body) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    find(&doc.document.members).expect("a connect statement")
}

#[test]
fn an_n_ary_connect_statement_keeps_every_end_in_order() {
    let ends = connect_statement_ends(
        "package P { part car { part a; part b; part e; connect (a, b, e); } }",
    );
    assert_eq!(ends, ["a", "b", "e"]);
}

#[test]
fn an_n_ary_connection_declaration_parses_with_or_without_a_name() {
    for src in [
        "package P { part car { part a; part b; part e; connection c connect (a, b, e); } }",
        "package P { part car { part a; part b; part e; connection connect (a, b, e); } }",
    ] {
        parse(src).unwrap_or_else(|e| panic!("must parse: {e}\n{src}"));
    }
}

#[test]
fn one_end_in_parentheses_is_a_syntax_error() {
    // A connector relates at least two ends, and the grammar requires the comma that says so. A
    // parse that recovers still keeps the rejected member as an error, which is what gets reported.
    let src = "package P { part car { part a; connect (a); } }";
    let doc = parse(src).expect("the file parses with an error member");
    assert!(
        !doc.document.errors().is_empty(),
        "one end in parentheses must be reported as an error"
    );
}

#[test]
fn a_binary_connect_is_unchanged() {
    let ends = connect_statement_ends("package P { part car { part a; part b; connect a to b; } }");
    assert_eq!(ends, ["a", "b"]);
}

#[test]
fn an_include_with_a_name_and_a_type_parses() {
    let src = "package P { use case def T; use case def U { include use case i : T; } }";
    parse(src).unwrap_or_else(|e| panic!("must parse: {e}"));
}
