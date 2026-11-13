//! `validateFlowDefinitionFlowEnds`: a flow definition has at most two ends.

use sysml_semantics::{check, Severity};

fn sem050(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM050")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_flow_definition_with_three_ends_is_rejected() {
    let src = r#"
package P {
	flow def F {
		end a;
		end b;
		end c;
	}
}
"#;
    assert_eq!(sem050(src).len(), 1, "got {:?}", sem050(src));
}

#[test]
fn a_flow_definition_with_two_ends_is_accepted() {
    let src = r#"
package P {
	flow def F {
		end a;
		end b;
	}
}
"#;
    assert!(sem050(src).is_empty());
}

#[test]
fn a_flow_definition_with_one_end_is_accepted() {
    // The rule is an upper bound; a one-ended flow is odd but not a violation.
    let src = r#"
package P {
	flow def F {
		end a;
	}
}
"#;
    assert!(sem050(src).is_empty());
}

#[test]
fn a_flow_usage_end_is_not_a_flow_definition_end() {
    // `flow of X` inside an action is a usage; only `flow def` carries this bound.
    let src = r#"
package P {
	part def A;
	action a {
		flow of A;
	}
}
"#;
    assert!(
        sem050(src).is_empty(),
        "a flow usage is not a flow definition, got {:?}",
        sem050(src)
    );
}

#[test]
fn the_surplus_end_is_the_one_reported_not_the_flow() {
    // `FlowUsage_Invalid.sysml.xt:53`: the assertion sits on `end c;`, and the flow that owns it
    // already has `a` inherited from `H`, so its own `a` and `b` fill the two slots.
    let src = r#"
package P {
	flow def H {
		end a;
	}
	flow def I :> H {
		end a;
		end b;
		end c;
	}
}
"#;
    let found = sem050(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(
        found[0].1, "P::I::c",
        "the report belongs on the end, got {found:?}"
    );
}

#[test]
fn a_flow_inheriting_too_many_ends_is_reported_on_itself() {
    // `FlowUsage_Invalid.sysml.xt:57`: `flow def J :> I;` writes no end of its own, so the flow
    // itself is the only place to put the report. The pilot asserts both ends of the pair — the
    // surplus end of `I` (`:53`) and `J` (`:57`) — because each flow is a violation of its own.
    let src = r#"
package P {
	flow def H {
		end a;
	}
	flow def I :> H {
		end a;
		end b;
		end c;
	}
	flow def J :> I;
}
"#;
    let found = sem050(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found.iter().any(|(_, at, _)| at == "P::J"),
        "the inheriting flow is named: {found:?}"
    );
    assert!(
        found.iter().any(|(_, at, _)| at.ends_with("I::c")),
        "and so is the surplus end of the flow it inherits from: {found:?}"
    );
}

#[test]
fn a_flow_with_two_ends_is_accepted() {
    let src = r#"
package P {
	flow def F {
		end a;
		end b;
	}
}
"#;
    assert!(
        sem050(src).is_empty(),
        "two ends is legal, got {:?}",
        sem050(src)
    );
}
