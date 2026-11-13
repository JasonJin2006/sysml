//! `ref` before a relationship operator is a modifier, not a name.
//!
//! `ref :>> x` declares an anonymous reference feature that redefines `x`: the grammar puts `ref`
//! in `FeaturePrefix` and lets the feature name be absent (`KerML.xtext`, `FeatureDeclaration`),
//! so the operator follows the prefix directly. The prefix loop took `ref` only when a name came
//! after it, so the word became the member's name and every such declaration projected as an
//! element called `ref` — twelve of them across the official validation models. Where one body
//! held two, `SEM091` reported a duplicate name the model does not have.

use sysml_parser::ast::{BodyMember, Declaration, RelationOp};
use sysml_parser::parse;

fn declarations(src: &str) -> Vec<Declaration> {
    let parsed = parse(src).unwrap_or_else(|e| panic!("must parse: {e}\nsource:\n{src}"));
    fn walk(members: &[BodyMember], out: &mut Vec<Declaration>) {
        for m in members {
            if let BodyMember::Declaration(d) = m {
                out.push((**d).clone());
                walk(&d.body, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&parsed.document.members, &mut out);
    out
}

#[test]
fn a_redefinition_supplies_the_name_of_an_anonymous_ref() {
    let decls = declarations("package P {\n\titem def C {\n\t\tref :>> x;\n\t}\n}\n");
    let feature = decls
        .iter()
        .find(|d| d.head.prefixes.is_ref)
        .expect("the declaration must keep its `ref` prefix");
    assert_eq!(feature.head.name.text, "x");
    assert_eq!(
        feature
            .head
            .target_of(RelationOp::Redefines)
            .map(|t| t.text.as_str()),
        Some("x")
    );
}

#[test]
fn the_word_never_becomes_a_member_of_its_own() {
    let decls =
        declarations("package P {\n\titem def C {\n\t\tref :>> a;\n\t\tref :>> b;\n\t}\n}\n");
    let names: Vec<&str> = decls.iter().map(|d| d.head.name.text.as_str()).collect();
    assert!(
        !names.contains(&"ref"),
        "`ref` must not become an element, got {names:?}"
    );
    assert_eq!(names, vec!["P", "C", "a", "b"]);
}

#[test]
fn the_other_prefixes_before_ref_survive() {
    // `abstract ref :>> trailerHitch[1]` is the spelling
    // `3c-Function-based Behavior-structure mod-2.sysml:38` uses.
    let decls = declarations("package P {\n\titem def C {\n\t\tabstract ref :>> y[1];\n\t}\n}\n");
    let feature = decls
        .iter()
        .find(|d| d.head.prefixes.is_ref)
        .expect("must keep `ref`");
    assert!(
        feature.head.prefixes.is_abstract,
        "`abstract` is a prefix too"
    );
    assert_eq!(feature.head.name.text, "y");
}

#[test]
fn a_feature_named_ref_keeps_its_name() {
    // The narrowing is deliberate: `:` is not a relationship operator for this purpose, so a model
    // that names a feature `ref` and types it still reads the word as the name. `ref : T` — an
    // anonymous reference feature typed directly — is a spelling no model in the corpus writes,
    // and accepting it would trade a silent wrong name for a silent missing one.
    let decls = declarations("package P {\n\tpart ref : A;\n}\n");
    let part = decls.iter().find(|d| d.head.name.text == "ref");
    let part = part.expect("`ref` here is the feature's name");
    assert!(!part.head.prefixes.is_ref);
}
