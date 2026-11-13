//! `validateStateSubactionMembershipOwningType`: entry / do / exit belong to a state.

use sysml_semantics::{check, Severity};

fn sem051(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM051")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_state_action_in_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		entry action a;
	}
}
"#;
    assert_eq!(sem051(src).len(), 1, "got {:?}", sem051(src));
}

#[test]
fn the_three_subactions_inside_a_state_definition_are_accepted() {
    let src = r#"
package P {
	state def S {
		entry action a1;
		do action a2;
		exit action a3;
	}
}
"#;
    assert!(
        sem051(src).is_empty(),
        "a state definition owns all three, got {:?}",
        sem051(src)
    );
}

#[test]
fn a_state_usage_also_owns_them() {
    let src = r#"
package P {
	state def S;
	state s : S {
		entry action a;
	}
}
"#;
    assert!(sem051(src).is_empty(), "got {:?}", sem051(src));
}

#[test]
fn the_report_names_the_owner_and_its_kind() {
    let src = r#"
package P {
	part def Holder {
		entry action a;
	}
}
"#;
    let (_, _, msg) = sem051(src).into_iter().next().expect("one violation");
    assert!(msg.contains("Holder"), "must name the owner: {msg}");
    assert!(
        msg.contains("part def"),
        "must say what the owner is instead: {msg}"
    );
}
