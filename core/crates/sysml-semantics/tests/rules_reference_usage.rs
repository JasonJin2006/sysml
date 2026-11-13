//! `perform` and `exhibit` name the right kind, proved both ways.

use sysml_semantics::{check, Severity};

fn sem044(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM044")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_perform_naming_a_part_is_rejected() {
    // `perform bad;` references `bad`. The rule asks about the referenced feature's own metaclass,
    // so a `ref` typed by an action definition is still a `ReferenceUsage` — which is exactly the
    // shape `ActionUsage_invalid.sysml.xt:58` marks "Must reference an action".
    let src = r#"
package P {
	part def NotAnAction;
	ref bad : NotAnAction;
	action a { perform bad; }
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validatePerformActionUsageReference")),
        "got {found:?}"
    );
}

#[test]
fn a_perform_naming_an_action_is_accepted() {
    let src = r#"
package P {
	action def Act;
	action ok : Act;
	action a { perform ok; }
}
"#;
    assert!(sem044(src).is_empty(), "an action names an action");
}

#[test]
fn an_exhibit_naming_a_part_is_rejected() {
    let src = r#"
package P {
	part def NotAState;
	part bad : NotAState;
	action a { exhibit bad; }
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateExhibitStateUsageReference")),
        "got {found:?}"
    );
}

#[test]
fn an_exhibit_naming_a_state_is_accepted() {
    let src = r#"
package P {
	state def St;
	state ok : St;
	action a { exhibit ok; }
}
"#;
    assert!(sem044(src).is_empty(), "a state names a state");
}

#[test]
fn a_specialization_of_the_right_kind_is_accepted() {
    // The rule is `oclIsKindOf`, so a subtype counts.
    let src = r#"
package P {
	action def Base;
	action def Special :> Base;
	action ok : Special;
	action a { perform ok; }
}
"#;
    assert!(
        sem044(src).is_empty(),
        "a specialization of an action is an action, got {:?}",
        sem044(src)
    );
}

#[test]
fn a_perform_of_something_absent_is_not_judged() {
    // `perform x;` with no `x` — a dangling reference, which the reference pass reports. This rule
    // stays silent rather than reporting the same name twice.
    let src = r#"
package P {
	action a { perform x; }
}
"#;
    assert!(sem044(src).is_empty());
}

#[test]
fn a_dotted_perform_is_judged_by_the_feature_it_denotes() {
    // `perform ok.detail` names a member of what `ok` is typed by, so the single-name lookup is
    // the wrong question — the same reason `validateAssignmentActionUsageReferent` uses
    // `resolve_path`. `detail` is an action here, so nothing is reported; `ActionUsage_invalid`
    // states the same shape with an action typed by a part definition.
    let src = r#"
package P {
	action def Act;
	action a {
		action ok : Act {
			action detail;
		}
		perform ok.detail;
	}
}
"#;
    assert!(
        sem044(src).is_empty(),
        "a member of an action is an action, got {:?}",
        sem044(src)
    );
}

// -- the `do` clause of a transition is a perform ---------------------------

#[test]
fn a_transition_effect_naming_an_action_is_accepted() {
    let src = r#"
package P {
	action def Drive;
	state def S {
		state a;
		state b;
		a then b do Drive;
	}
}
"#;
    assert!(sem044(src).is_empty(), "got {:?}", sem044(src));
}

#[test]
fn a_transition_effect_naming_a_part_is_rejected() {
    // `EffectBehaviorUsage` is a `PerformedActionUsage`: the clause names what the transition
    // performs, and a part is not an action.
    let src = r#"
package P {
	part def Wheel;
	part w : Wheel;
	state def S {
		state a;
		state b;
		a then b do w;
	}
}
"#;
    let found = sem044(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("validatePerformActionUsageReference"),
        "got {found:?}"
    );
}

// -- validateAssignmentActionUsageReferent ---------------------------------

#[test]
fn an_assignment_to_a_feature_is_accepted() {
    let src = r#"
package P {
	attribute def Real;
	action def A {
		attribute t : Real;
		assign t := 1;
	}
}
"#;
    assert!(
        !sem044(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateAssignmentActionUsageReferent")),
        "got {:?}",
        sem044(src)
    );
}

#[test]
fn an_assignment_to_a_definition_is_rejected() {
    // `assign Wheel := 1` names a definition, not a feature: nothing is being written.
    let src = r#"
package P {
	part def Wheel;
	action def A {
		assign Wheel := 1;
	}
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateAssignmentActionUsageReferent")),
        "got {found:?}"
    );
}

#[test]
fn an_assignment_through_a_dotted_path_is_accepted() {
    // `assign i.a := 1` writes a member of `i`, not a name in scope: the single-name lookup
    // asks whether `i.a` is declared where the assignment sits, which it never is.
    let src = r#"
package P {
	item def I {
		attribute a;
	}
	item i : I;
	action def A {
		assign i.a := 1;
	}
}
"#;
    assert!(
        !sem044(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateAssignmentActionUsageReferent")),
        "got {:?}",
        sem044(src)
    );
}

#[test]
fn an_assignment_lowers_to_its_own_kind() {
    // The word used to be a `ControlFlow` name, so no rule about an assignment could find one.
    let src = r#"
package P {
	attribute def Real;
	action def A {
		attribute t : Real;
		assign t := 1;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    fn find<'a>(el: &'a sysml_model::Element) -> Option<&'a sysml_model::Element> {
        if el.kind == sysml_model::Kind::AssignmentActionUsage {
            return Some(el);
        }
        el.children.iter().find_map(find)
    }
    assert!(
        loaded.roots.iter().find_map(find).is_some(),
        "an assign element"
    );
}

// -- validateEventOccurrenceUsageReference ---------------------------------

#[test]
fn an_event_typed_by_an_occurrence_is_accepted() {
    let src = r#"
package P {
	part def Arrival;
	event e : Arrival;
}
"#;
    assert!(
        !sem044(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateEventOccurrenceUsageReference")),
        "got {:?}",
        sem044(src)
    );
}

#[test]
fn an_event_denoting_a_data_value_is_rejected() {
    // `event e;` denotes `attribute e : Real`. The reference is what the rule reads, so the same
    // model with a `:` clause (`event e : Real;`) is the *typing* question and reports under
    // `typed_by_definition` instead — the grammar gives that form no declared name either
    // (`SysML.xtext:865-870`).
    let src = r#"
package P {
	attribute def Real;
	attribute e : Real;
	event e;
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateEventOccurrenceUsageReference")),
        "got {found:?}"
    );
}

// -- validateAssertConstraintUsageReference --------------------------------

#[test]
fn an_assertion_naming_a_constraint_is_accepted() {
    let src = r#"
package P {
	constraint def C;
	part def Holder {
		assert constraint ok : C;
	}
}
"#;
    assert!(
        !sem044(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateAssertConstraintUsageReference")),
        "got {:?}",
        sem044(src)
    );
}

#[test]
fn an_assertion_naming_a_data_type_is_rejected() {
    // The typed form `assert constraint bad : NotC;` is judged by `typed_by_definition`'s
    // AssertConstraint row, which asks the same question and also counts the types —
    // `validateAssertConstraintUsageReference` (SEM044) reads only the bare `assert path;` form,
    // because reporting both gave one line two errors
    // (`ConstraintUsage_Invalid.sysml.xt:42`).
    let src = r#"
package P {
	attribute def NotC;
	part def Holder {
		assert constraint bad : NotC;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "SEM090" && i.message.contains("is a constraint")),
        "an assertion names a constraint: {:?}",
        report
            .issues
            .iter()
            .map(|i| (i.code, i.message.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        sem044(src).is_empty(),
        "and SEM044 leaves the typed form alone, got {:?}",
        sem044(src)
    );
}

#[test]
fn a_bare_assertion_naming_a_constraint_is_accepted() {
    // `assert path;` — the reference form, kept in the target end.
    let src = r#"
package P {
	constraint def C;
	requirement def R {
		assert C;
	}
}
"#;
    assert!(
        !sem044(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateAssertConstraintUsageReference")),
        "got {:?}",
        sem044(src)
    );
}

#[test]
fn an_assignment_to_a_feature_that_cannot_vary_is_rejected() {
    // `mayTimeVary` is whether the usage is featured by the snapshots of its owner. A `datatype`
    // has no lifetime, so its members have one value and nothing to assign. A `class` is not this
    // case: `Object` is a class in the kernel library, so a class may be an occurrence, and the
    // reference states no error for `var feature x;` inside one
    // (`Feature_variable_invalid.kerml.xt:29`).
    let src = r#"
package P {
	datatype C {
		attribute x : Real;
		action a {
			assign x := 1;
		}
	}
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateAssignmentActionUsage")),
        "got {found:?}"
    );
}

#[test]
fn an_assignment_to_a_variable_is_accepted() {
    let src = r#"
package P {
	action def A {
		attribute var x : Real;
		assign x := 1;
	}
}
"#;
    let found = sem044(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateAssignmentActionUsage")),
        "got {found:?}"
    );
}

#[test]
fn an_event_denoting_a_reference_usage_is_rejected() {
    // `event a;` declares nothing: it denotes `ref a : A`, and a reference usage is not an
    // occurrence, so `referencedFeatureTarget()` is not one either
    // (`OccurrenceUsage_invalid.sysml.xt:56`).
    let src = r#"
package P {
	occurrence def A;
	ref a : A;
	event a;
}
"#;
    let found = sem044(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateEventOccurrenceUsageReference")),
        "got {found:?}"
    );
}

#[test]
fn an_event_denoting_an_occurrence_is_accepted() {
    let src = r#"
package P {
	occurrence def A;
	occurrence e : A;
	event e;
}
"#;
    assert!(
        sem044(src).is_empty(),
        "an occurrence is what an event denotes"
    );
}

#[test]
fn an_event_denoting_a_member_of_an_occurrence_is_accepted() {
    // `event a.areal;` — the target is a member of what `a` is typed by, so the single-name
    // lookup asks the wrong question. `OccurrenceUsage_invalid.sysml.xt:63` names an occurrence
    // there, and this rule has nothing to say about it.
    let src = r#"
package P {
	occurrence def B;
	occurrence def A {
		occurrence inner : B;
	}
	ref a : A;
	event a.inner;
}
"#;
    assert!(
        sem044(src).is_empty(),
        "a member of an occurrence is an occurrence"
    );
}
