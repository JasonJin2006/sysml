//! `flow of name : Type from a to b` is one flow, not three members.
//!
//! The connector-form test scans forward looking for `to` or `from`. Both are keywords and the
//! name walk accepts keywords, so the scan ran past the ends of the statement and never saw
//! either: the shape test said "declaration", and `of`, the payload name and `from` fell into the
//! body as separate members.

use sysml_model::Kind;

fn members(src: &str) -> Vec<(String, Kind)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    loaded
        .roots
        .iter()
        .find(|e| e.name == "P")
        .expect("P")
        .children
        .iter()
        .map(|c| (c.name.clone(), c.kind.clone()))
        .collect()
}

#[test]
fn a_flow_with_a_payload_is_one_member_whose_payload_is_its_member() {
    // `flow of fuel : Fuel from a.b to c.d` declares an **unnamed** flow whose payload is `fuel`.
    // The payload used to arrive as the flow's own name, so two unnamed flows in one body looked
    // like two members called `fuel` — which is what the official corpus's `3d-Function-based
    // Behavior-item` writes, and what `validateNamespaceDistinguishibility` reported before the
    // payload became a member.
    let found = members(
        r#"
package P {
	part a { part b; }
	part c { part d; }
	flow of fuel : Fuel from a.b to c.d;
}
"#,
    );
    let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["a", "c", "flow"],
        "the flow keeps no name of its own: {names:?}"
    );
    assert!(!names.contains(&"of"), "`of` is a keyword, got {names:?}");
    assert!(
        !names.contains(&"from"),
        "`from` is a keyword, got {names:?}"
    );
    assert!(
        found
            .iter()
            .any(|(n, k)| n == "flow" && *k == Kind::FlowUsage),
        "and it must be a flow *usage* — `FlowUsage returns SysML::FlowUsage` — got {found:?}"
    );
}

#[test]
fn the_payload_is_a_member_of_the_flow() {
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
	part a { part b; }
	part c { part d; }
	flow of fuel : Fuel from a.b to c.d;
}
"#],
        true,
    )
    .expect("model parses");
    let flow = loaded.roots[0]
        .children
        .iter()
        .find(|c| c.kind == Kind::FlowUsage)
        .expect("the flow");
    let payloads: Vec<(&str, &Kind)> = flow
        .children
        .iter()
        .map(|c| (c.name.as_str(), &c.kind))
        .collect();
    assert_eq!(
        payloads,
        [("fuel", &Kind::PayloadFeature)],
        "the payload is the flow's own feature"
    );
    assert_eq!(flow.children[0].type_ref.as_deref(), Some("Fuel"));
}

#[test]
fn a_flow_with_only_a_name_still_parses() {
    let found = members(
        r#"
package P {
	part a { part b; }
	part c { part d; }
	flow fuel from a.b to c.d;
}
"#,
    );
    let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"fuel"), "got {names:?}");
    assert!(!names.contains(&"from"), "got {names:?}");
}
