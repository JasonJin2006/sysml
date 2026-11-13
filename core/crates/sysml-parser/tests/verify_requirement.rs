//! `verify r1 : R;` is one declaration, not two.
//!
//! `verify` introduces a RequirementVerificationUsage (`SysML.xtext:2262-2276`). The word was
//! unknown to the kind dispatch, so the member fell apart: `verify` and `r1` became two
//! attributes and the requirement being verified was lost.

use sysml_parser::ast::BodyMember;
use sysml_parser::parse;

fn members(src: &str) -> Vec<String> {
    let parsed = parse(src).expect("must parse");
    fn walk(members: &[BodyMember], out: &mut Vec<String>) {
        for m in members {
            if let BodyMember::Declaration(d) = m {
                out.push(d.head.name.text.clone());
                walk(&d.body, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&parsed.document.members, &mut out);
    out
}

#[test]
fn verify_yields_one_member_named_for_the_verification() {
    let found = members("package P {\n\treq def R;\n\tpart def Q {\n\t\tverify r1 : R;\n\t}\n}\n");
    assert!(
        !found.contains(&"verify".to_string()),
        "`verify` is a keyword, not a name, got {found:?}"
    );
    assert!(
        found.contains(&"r1".to_string()),
        "the verification is named `r1`, got {found:?}"
    );
}

#[test]
fn the_verified_requirement_is_the_type() {
    let parsed = parse("package P {\n\treq def R;\n\tpart def Q {\n\t\tverify r1 : R;\n\t}\n}\n")
        .expect("must parse");
    let mut found = None;
    fn walk(members: &[BodyMember], found: &mut Option<String>) {
        for m in members {
            if let BodyMember::Declaration(d) = m {
                if d.head.name.text == "r1" {
                    *found = d
                        .head
                        .relations
                        .first()
                        .and_then(|r| r.targets.first())
                        .map(|t| t.text.clone());
                }
                walk(&d.body, found);
            }
        }
    }
    walk(&parsed.document.members, &mut found);
    assert_eq!(found.as_deref(), Some("R"), "the verification names R");
}
