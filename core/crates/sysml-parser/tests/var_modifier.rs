//! `var` after a kind keyword is a modifier, not a name.
//!
//! `attribute var y : Integer` is the spelling the notation allows alongside `var attribute y`.
//! The prefix loop only knew `ref` and `end`, so `var` was taken for the member's name and the
//! feature it introduced was lost — two elements where the model wrote one.

use sysml_parser::ast::BodyMember;
use sysml_parser::parse;

fn names(src: &str) -> Vec<String> {
    let parsed = parse(src).expect("must parse");
    fn walk(members: &[BodyMember], out: &mut Vec<String>) {
        for m in members {
            if let BodyMember::Declaration(d) = m {
                if !d.head.name.text.is_empty() {
                    out.push(d.head.name.text.clone());
                }
                walk(&d.body, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&parsed.document.members, &mut out);
    out
}

#[test]
fn var_after_attribute_names_the_feature() {
    let found = names("package P {\n\tattribute var y : Integer;\n}\n");
    assert!(
        !found.contains(&"var".to_string()),
        "`var` must not become a member, got {found:?}"
    );
    assert!(
        found.contains(&"y".to_string()),
        "the feature is `y`, got {found:?}"
    );
}

#[test]
fn var_before_attribute_still_works() {
    let found = names("package P {\n\tvar attribute y : Integer;\n}\n");
    assert!(!found.contains(&"var".to_string()), "got {found:?}");
    assert!(found.contains(&"y".to_string()), "got {found:?}");
}
