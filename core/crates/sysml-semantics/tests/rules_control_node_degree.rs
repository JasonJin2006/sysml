//! How many successions may meet at a control node, proved both ways.

use sysml_semantics::{check, Severity};

fn sem047(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM047")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_merge_with_two_outgoing_successions_is_rejected() {
    let src = r#"
package P {
	action a {
		first s then m;
		merge m;
		first m then x;
		first m then y;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateMergeNodeOutgoingSuccessions")),
        "got {found:?}"
    );
}

#[test]
fn a_merge_with_one_outgoing_succession_is_accepted() {
    let src = r#"
package P {
	action a {
		first s then m;
		merge m;
		first m then x;
	}
}
"#;
    assert!(sem047(src).is_empty());
}

#[test]
fn a_decision_with_two_incoming_successions_is_rejected() {
    let src = r#"
package P {
	action a {
		first s then d;
		first t then d;
		decision d;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateDecisionNodeIncomingSuccessions")),
        "got {found:?}"
    );
}

#[test]
fn a_decision_with_one_incoming_succession_is_accepted() {
    let src = r#"
package P {
	action a {
		first s then d;
		decision d;
		first d then x;
		first d then y;
	}
}
"#;
    assert!(
        sem047(src).is_empty(),
        "a decision fans out; only its incoming side is bounded"
    );
}

#[test]
fn a_fork_takes_one_incoming_and_a_join_one_outgoing() {
    let fork = r#"
package P {
	action a {
		first s then f;
		first t then f;
		fork f;
	}
}
"#;
    assert!(
        sem047(fork)
            .iter()
            .any(|(_, _, m)| m.contains("validateForkNodeIncomingSuccessions")),
        "a fork with two arrivals must be reported"
    );
    let join = r#"
package P {
	action a {
		first s then j;
		join j;
		first j then x;
		first j then y;
	}
}
"#;
    assert!(
        sem047(join)
            .iter()
            .any(|(_, _, m)| m.contains("validateJoinNodeOutgoingSuccessions")),
        "a join with two departures must be reported"
    );
}

#[test]
fn a_transition_in_another_action_is_not_counted() {
    // Two actions may both name a node `m`; the count is per owner.
    let src = r#"
package P {
	action a {
		first s then m;
		merge m;
		first m then x;
	}
	action b {
		first s then m;
		merge m;
		first m then y;
	}
}
"#;
    assert!(
        sem047(src).is_empty(),
        "each action has its own `m` with one outgoing, got {:?}",
        sem047(src)
    );
}

// -- the multiplicity bounds on the ends ------------------------------------

#[test]
fn a_merge_arrival_with_a_wider_multiplicity_is_rejected() {
    let src = r#"
package P {
	action a {
		first x then [2] m;
		merge m;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateMergeNodeIncomingSuccessions")),
        "got {found:?}"
    );
}

#[test]
fn a_decision_departure_with_a_wider_multiplicity_is_rejected() {
    let src = r#"
package P {
	action a {
		first [2] d then y;
		decision d;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateDecisionNodeOutgoingSuccessions")),
        "got {found:?}"
    );
}

#[test]
fn a_zero_to_one_far_end_is_accepted() {
    // `[0..1]` is what `validateMergeNodeIncomingSuccessions` asks for, on the **source** end of
    // an arrival. This was once read as "the bound both rules ask for", on the belief that
    // `validateControlNodeIncomingSuccessions` asked for 0..1 as well. It asks for 1..1, on the
    // target end — the near end — so a `[0..1]` on the far end is legal and a `[0..1]` on the near
    // end is not.
    let src = r#"
package P {
	action a {
		first [0..1] x then m;
		action x;
		merge m;
	}
}
"#;
    let found = sem047(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateMergeNodeIncomingSuccessions")),
        "`[0..1]` is the bound the merge rule asks for, got {found:?}"
    );
}

#[test]
fn an_end_with_no_written_bound_is_accepted() {
    let src = r#"
package P {
	action a {
		first x then m;
		merge m;
	}
}
"#;
    assert!(
        sem047(src).is_empty(),
        "no bound written is not a wider bound"
    );
}

/// All four control-node degree rules name the keyword, not the node.
///
/// `decision d;` used to be reported as "a d may have at most one", because the message read the
/// element's name where it meant the kind of node. That was invisible while the element was named
/// after its keyword; once a control node took its declared name, every message read like a
/// statement about a one-letter type.
#[test]
fn a_degree_message_names_the_control_node_kind() {
    let cases = [
        (
            "decision",
            "first s then d;\n        first t then d;",
            "validateDecisionNodeIncomingSuccessions",
        ),
        (
            "fork",
            "first s then d;\n        first t then d;",
            "validateForkNodeIncomingSuccessions",
        ),
        (
            "merge",
            "first d then s;\n        first d then t;",
            "validateMergeNodeOutgoingSuccessions",
        ),
        (
            "join",
            "first d then s;\n        first d then t;",
            "validateJoinNodeOutgoingSuccessions",
        ),
    ];
    for (keyword, edges, rule) in cases {
        let src = format!(
            "package P {{\n    action def A {{\n        action s;\n        action t;\n        \
             {keyword} d;\n        {edges}\n    }}\n}}\n"
        );
        let found = sem047(&src);
        let (_, _, message) = found
            .iter()
            .find(|(_, _, m)| m.contains(rule))
            .unwrap_or_else(|| panic!("{keyword}: no {rule} in {found:?}"));
        assert!(
            message.contains(&format!("a {keyword} may have")),
            "{keyword}: the message names the kind, got {message}"
        );
        assert!(
            !message.contains("a d may"),
            "{keyword}: the message must not name the node, got {message}"
        );
    }
}

// -- the two `ControlNode` bounds on the near end ---------------------------
//
// These were listed as unevaluable for two rounds: the ledger said they contradicted
// `validateMergeNodeIncomingSuccessions` and `validateDecisionNodeOutgoingSuccessions`. They do
// not. `validateControlNodeIncomingSuccessions` reads `connectorEnd->at(2)` — the target — and
// demands 1..1, while the merge rule reads `connectorEnd->at(1)` — the source — and demands 0..1.
// Different ends, so both can hold of one model.

#[test]
fn a_control_node_arrival_with_a_wider_near_end_is_rejected() {
    let src = r#"
package P {
	action a {
		action s;
		decision d;
		first s then [2] d;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateControlNodeIncomingSuccessions")),
        "got {found:?}"
    );
}

#[test]
fn a_control_node_departure_with_a_wider_near_end_is_rejected() {
    let src = r#"
package P {
	action a {
		join j;
		first [2] j then s;
		action s;
	}
}
"#;
    let found = sem047(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateControlNodeOutgoingSuccessions")),
        "got {found:?}"
    );
}

/// The default is 1..1, so an end that writes no multiplicity satisfies both rules.
#[test]
fn a_control_node_end_with_no_written_bound_is_accepted() {
    let src = r#"
package P {
	action a {
		action s;
		decision d;
		first s then d;
	}
}
"#;
    let found = sem047(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateControlNode")),
        "an end with no bound states 1..1, got {found:?}"
    );
}
