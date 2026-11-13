//! Reading the derivations a model states.
//!
//! `connection d : Derivation connect C to R;` is the standard library's way of saying that one
//! requirement comes from another. Two callers need that pairing — the traceability picture and the
//! ladder in the CLI — so it is read once, here, and both ask this function.
//!
//! The parse is the awkward part: the statement becomes *two* elements on one line, the typed
//! connection and a bare `connect` carrying the ends. A rule that reads only the typed element finds
//! no ends; one that reads only `connect` finds no type. The join is by line number.

use sysml_view::derivations;

const HEAD: &str = r#"
package M {
    private import ScalarValues::*;
    private import RequirementDerivation::*;
    part def Thing { attribute x : Real = 3; }
    concern def C { stakeholder s : Thing; subject t : Thing; require constraint { t.x >= 1 } }
    requirement def R1 { subject t : Thing; require constraint { t.x >= 2 } }
    requirement def R2 { subject t : Thing; require constraint { t.x >= 3 } }
"#;

fn read(body: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load(&format!("{HEAD}{body}\n}}\n")).expect("parses");
    derivations(&loaded.roots)
}

#[test]
fn a_derivation_connection_yields_the_pair() {
    assert_eq!(
        read("    connection d : Derivation connect C to R1;"),
        vec![("C".to_string(), "R1".to_string())]
    );
}

#[test]
fn each_connection_is_its_own_pair() {
    let pairs = read(
        "    connection d1 : Derivation connect C to R1;\n    connection d2 : Derivation connect C to R2;",
    );
    assert_eq!(pairs.len(), 2, "{pairs:?}");
    assert!(pairs.contains(&("C".to_string(), "R2".to_string())));
}

#[test]
fn a_plain_connection_is_not_a_derivation() {
    // `connect` is also how parts are wired together. Only a connection typed `Derivation` counts,
    // which is what the line join is for: without it every connection in the model would be one.
    assert!(read("    connection w : Link connect R1 to R2;").is_empty());
}

#[test]
fn a_model_without_connections_has_no_derivations() {
    assert!(read("").is_empty());
}
