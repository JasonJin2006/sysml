//! `validateControlNodeOwningType`, proved both ways.

use sysml_semantics::{check, Severity};

fn sem040(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM040")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_control_node_inside_an_action_is_accepted() {
    let src = r#"
package P {
	action a {
		first start then mid;
		merge mid;
	}
}
"#;
    assert!(
        sem040(src).is_empty(),
        "a merge inside an action is legal, got {:?}",
        sem040(src)
    );
}

#[test]
fn a_control_node_in_a_package_is_rejected() {
    let src = r#"
package P {
	merge orphan;
}
"#;
    let found = sem040(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateControlNodeOwningType")),
        "a control node at package level has no action owner; got {found:?}"
    );
    // The same declaration breaks `validateControlNodeIsComposite`: a package is not a type, so the
    // effective `isComposite` is false. Two rules, one declaration, both genuinely violated — the
    // module note records the overlap.
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateControlNodeIsComposite")),
        "and it is not composite for the same reason; got {found:?}"
    );
    assert_eq!(found.len(), 2, "one diagnostic per rule: {found:?}");
}

#[test]
fn first_then_keeps_the_rest_of_the_body_in_place() {
    // `first a then b { ... }` is a succession. Reading only `first a` left `then b` as the next
    // member, and everything after it escaped the enclosing action — the official 3a model lost
    // its `merge` and the rest of `provide power` that way.
    let src = r#"
package P {
	action a {
		first start then mid {
			doc /* a succession with a body */
		}
		merge mid;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let action = loaded
        .roots
        .iter()
        .flat_map(|r| r.children.iter())
        .find(|e| e.name == "a")
        .expect("the action");
    let names: Vec<&str> = action.children.iter().map(|c| c.name.as_str()).collect();
    // The control node's name is the `merge` keyword and its target is `mid`; what the test pins
    // is that it did not escape the action body.
    assert!(
        names.iter().any(|n| *n == "merge" || *n == "mid"),
        "the merge must stay inside the action, got {names:?}"
    );
}
