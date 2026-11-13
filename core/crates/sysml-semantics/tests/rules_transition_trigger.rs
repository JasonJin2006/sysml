//! `validateTransitionUsageTriggerActions`: a trigger belongs to a transition out of a state.

use sysml_semantics::{check, Severity};

fn sem061(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM061")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_trigger_on_a_transition_out_of_an_action_is_rejected() {
    let src = r#"
package P {
	state def S {
		action act;
		state b;
		transition bad first act accept e then b;
	}
}
"#;
    assert_eq!(sem061(src).len(), 1, "got {:?}", sem061(src));
}

#[test]
fn a_trigger_on_a_transition_out_of_a_state_is_accepted() {
    let src = r#"
package P {
	state def S {
		state a;
		state b;
		transition ok first a accept e then b;
	}
}
"#;
    assert!(sem061(src).is_empty(), "got {:?}", sem061(src));
}

#[test]
fn a_transition_with_no_trigger_is_accepted() {
    let src = r#"
package P {
	state def S {
		action act;
		state b;
		first act then b;
	}
}
"#;
    assert!(sem061(src).is_empty(), "no trigger, nothing to bound");
}

#[test]
fn an_unresolved_source_is_left_alone() {
    // A dangling source is a reference problem; reporting it here too would double the
    // diagnostic without saying anything new.
    let src = r#"
package P {
	state def S {
		state b;
		transition t first nowhere accept e then b;
	}
}
"#;
    assert!(
        sem061(src).is_empty(),
        "an unresolvable source is not judged, got {:?}",
        sem061(src)
    );
}

// -- validateTransitionFeatureMembershipGuardExpression --------------------

fn has(src: &str, needle: &str) -> bool {
    sem061(src).iter().any(|(_, _, m)| m.contains(needle))
}

const GUARD_PRELUDE: &str = r#"
package P {
	attribute def Real;
	attribute def Boolean;
	attribute mass : Real = 2;
	attribute ready : Boolean = true;
	state def S {
		state a;
		state b;
"#;

#[test]
fn a_guard_that_is_not_a_boolean_is_rejected() {
    let src = format!("{GUARD_PRELUDE}\t\ttransition t first a if mass then b;\n\t}}\n}}\n");
    assert!(
        has(&src, "validateTransitionFeatureMembershipGuardExpression"),
        "a Real guard states no condition: {:?}",
        sem061(&src)
    );
}

#[test]
fn a_boolean_guard_is_accepted() {
    let src = format!("{GUARD_PRELUDE}\t\ttransition t first a if ready then b;\n\t}}\n}}\n");
    assert!(
        !has(&src, "validateTransitionFeatureMembershipGuardExpression"),
        "got {:?}",
        sem061(&src)
    );
}

#[test]
fn a_comparison_guard_is_accepted() {
    let src = format!("{GUARD_PRELUDE}\t\ttransition t first a if mass > 0 then b;\n\t}}\n}}\n");
    assert!(
        !has(&src, "validateTransitionFeatureMembershipGuardExpression"),
        "a comparison is a Boolean: {:?}",
        sem061(&src)
    );
}

// -- validateTriggerInvocationExpressionWhenArgument -----------------------

#[test]
fn a_when_trigger_that_is_not_a_boolean_is_rejected() {
    let src =
        format!("{GUARD_PRELUDE}\t\ttransition t first a accept when mass then b;\n\t}}\n}}\n");
    assert!(
        has(&src, "validateTriggerInvocationExpressionWhenArgument"),
        "got {:?}",
        sem061(&src)
    );
}

#[test]
fn a_boolean_when_trigger_is_accepted() {
    let src =
        format!("{GUARD_PRELUDE}\t\ttransition t first a accept when ready then b;\n\t}}\n}}\n");
    assert!(
        !has(&src, "validateTriggerInvocationExpressionWhenArgument"),
        "got {:?}",
        sem061(&src)
    );
}

#[test]
fn a_comparison_when_trigger_is_accepted() {
    // `accept when 'sense temperature'.temp > vehicle.Tmax` is the corpus's shape.
    let src =
        format!("{GUARD_PRELUDE}\t\ttransition t first a accept when mass > 0 then b;\n\t}}\n}}\n");
    assert!(
        !has(&src, "validateTriggerInvocationExpressionWhenArgument"),
        "got {:?}",
        sem061(&src)
    );
}

// -- validateTriggerInvocationExpressionAtArgument -------------------------

#[test]
fn an_at_trigger_that_is_not_a_time_instant_is_rejected() {
    let src = format!("{GUARD_PRELUDE}\t\ttransition t first a accept at mass then b;\n\t}}\n}}\n");
    assert!(
        has(&src, "validateTriggerInvocationExpressionAtArgument"),
        "`mass` is a Real, not a time instant: {:?}",
        sem061(&src)
    );
}

#[test]
fn an_at_trigger_on_a_time_instant_is_accepted() {
    let src = r#"
package P {
	private import Time::*;
	attribute due : TimeInstantValue;
	state def S {
		state a;
		state b;
		transition t first a accept at due then b;
	}
}
"#;
    assert!(
        !has(src, "validateTriggerInvocationExpressionAtArgument"),
        "got {:?}",
        sem061(src)
    );
}

// -- validateTriggerInvocationExpressionAfterArgument: an `after` waits a duration -----

#[test]
fn an_after_with_a_duration_unit_is_accepted() {
    let src = r#"
package P {
	private import ScalarValues::*;
	private import SI::*;
	state def S {
		state a;
		state b;
		transition t first a accept after 5 [s] then b;
	}
}
"#;
    let found = sem061(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateTriggerInvocationExpressionAfterArgument")),
        "got {found:?}"
    );
}

#[test]
fn an_after_with_a_unit_that_is_not_a_duration_is_rejected() {
    // `[kg]` is a mass, so waiting on it says nothing about time. Only the unit half of the OCL is
    // decided; the header says which half and why.
    let src = r#"
package P {
	private import ScalarValues::*;
	private import SI::*;
	state def S {
		state a;
		state b;
		transition t first a accept after 5 [kg] then b;
	}
}
"#;
    let found = sem061(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateTriggerInvocationExpressionAfterArgument")),
        "got {found:?}"
    );
}

#[test]
fn an_after_with_no_unit_says_nothing() {
    // `after 5` states no unit, so there is nothing to compare and the reference pass has nothing
    // to report either.
    let src = r#"
package P {
	private import ScalarValues::*;
	state def S {
		state a;
		state b;
		transition t first a accept after 5 then b;
	}
}
"#;
    let found = sem061(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateTriggerInvocationExpressionAfterArgument")),
        "got {found:?}"
    );
}

// -- a trigger after the target, and a literal as a condition -----------------

/// `accept` may follow the target as well as precede it.
///
/// `transition off then on accept when x` is the same statement as
/// `transition off accept when x then on`. Reading only the clauses before `then` left
/// `accept when x` in the stream, where it became a member of the enclosing state and the
/// transition lost its trigger entirely: no `trigger_kind`, no argument, and nothing for the
/// trigger rules to judge.
#[test]
fn a_trigger_after_the_target_is_kept() {
    let src = r#"
package P {
    private import ScalarValues::*;
    state def Power {
        state off;
        state on;
        attribute mass : Real = 5 [kg];
        transition off then on accept when mass;
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    fn find(el: &sysml_model::Element) -> Option<&sysml_model::Element> {
        if el.kind == sysml_model::Kind::Transition {
            return Some(el);
        }
        el.children.iter().find_map(find)
    }
    let transition = loaded
        .roots
        .iter()
        .find_map(find)
        .expect("the transition lowered");
    assert_eq!(
        transition.trigger_kind.as_deref(),
        Some("when"),
        "the trigger keyword survives"
    );
    assert!(
        transition.trigger_expr.is_some(),
        "and so does its argument"
    );
    assert!(
        sem061(src)
            .iter()
            .any(|(_, _, m)| m.contains("not a Boolean")),
        "a `when` over a Real is reported, got {:?}",
        sem061(src)
    );
}

/// A numeric literal is definitively not a truth value.
///
/// `boolean_result` answered `None` for anything that was not `true`/`false`, which the callers
/// read as "cannot tell". For a literal that is the wrong answer: the type is the token in front
/// of the reader.
#[test]
fn a_numeric_literal_is_not_a_condition() {
    let src = r#"
package P {
    private import ScalarValues::*;
    state def Power {
        state off;
        state on;
        transition off then on if 5;
        transition on then off accept when 5;
    }
}
"#;
    let found: Vec<String> = sem061(src).into_iter().map(|(_, _, m)| m).collect();
    assert_eq!(
        found.len(),
        2,
        "both the guard and the trigger are reported, got {found:?}"
    );
    assert!(
        found.iter().all(|m| m.contains("not a Boolean")),
        "and both say why: {found:?}"
    );
}
