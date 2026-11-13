//! `subsetting multiplicity`: a subsetting may not widen the bound it narrows.
//!
//! Two warnings, one per end. The upper bound may not widen for any subsetting; the lower bound
//! may not shrink only for a redefinition, which is what the pilot's two messages say — "Subsetting
//! /redefining feature ... upper bound" and "**Redefining** feature ... lower bound". Both
//! directions are pinned here, plus the two exemptions that the pilot's own model states: `action`
//! and `state` have no default bound.

use sysml_semantics::{check, Severity};

fn sem092(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM092")
        .map(|i| format!("{}: {}", i.at, i.message))
        .collect()
}

#[test]
fn a_wider_bound_on_a_subsetting_is_reported() {
    // `[1..*]` on a feature whose supertype is bounded `[1..1]` denotes more than the supertype.
    let src = r#"
package P {
	attribute def Real;
	part def Holder {
		attribute a1 : Real;
		attribute a2[1..*] :> a1;
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("larger upper bound"), "got {found:?}");
}

#[test]
fn the_default_bound_of_one_applies_to_both_sides() {
    // Neither side writes a bound: both are `[1..1]`, so `[*]` is wider. The pilot model asserts
    // exactly this with a comment — "explicit subsettings do not exclude the default [1..1]".
    let src = r#"
package P {
	part def Holder {
		part p1;
		part p2[*] :> p1;
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 1, "got {found:?}");
}

#[test]
fn a_narrower_bound_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		part p1 : Wheel[0..*];
		part p2 : Wheel[0..1] :> p1;
	}
}
"#;
    assert!(sem092(src).is_empty(), "got {:?}", sem092(src));
}

#[test]
fn an_equal_bound_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		part p1 : Wheel[2];
		part p2 : Wheel[2] :> p1;
	}
}
"#;
    assert!(sem092(src).is_empty(), "got {:?}", sem092(src));
}

#[test]
fn an_action_usage_has_no_default_bound() {
    // The pilot's own comment: "These kinds of usages do not have default multiplicity", marking
    // `action n2[*] :> n1` clean. A behaviour is not a value with a count.
    let src = r#"
package P {
	part def Holder {
		action n1;
		action n2[*] :> n1;
	}
}
"#;
    assert!(sem092(src).is_empty(), "got {:?}", sem092(src));
}

#[test]
fn a_state_usage_has_no_default_bound_either() {
    let src = r#"
package P {
	part def Holder {
		state s1;
		state s2[*] :> s1;
	}
}
"#;
    assert!(sem092(src).is_empty(), "got {:?}", sem092(src));
}

#[test]
fn a_bound_is_warned_not_errored() {
    // The model is legal; the bound is almost always a mistake.
    let src = r#"
package P {
	part def Holder {
		part p1;
		part p2[*] :> p1;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let found: Vec<Severity> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM092")
        .map(|i| i.severity)
        .collect();
    assert_eq!(found, [Severity::Warning], "got {found:?}");
}

#[test]
fn a_bound_written_on_the_redefinition_target_is_read() {
    // `part redefines cyl[7]` writes the bound after the target. Every clause reader strips it,
    // so before the lowering kept it the element arrived with no bound at all.
    let src = r#"
package P {
	part def Cylinder;
	part def Engine {
		part cyl : Cylinder[4..6];
	}
	part def Small :> Engine {
		part redefines cyl[7];
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("[7]"),
        "the bound is the one written: {found:?}"
    );
}

#[test]
fn a_redefinition_that_shrinks_the_lower_bound_is_reported() {
    // `Redefinition_MultiplicityConformance_Invalid.sysml.xt:42`: `[1..6]` against `[4..6]` keeps
    // the upper bound and drops the lower one.
    let src = r#"
package P {
	part def Cylinder;
	part def Engine {
		part cyl : Cylinder[4..6];
	}
	part def Big :> Engine {
		part redefines cyl[1..6];
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("smaller lower bound"), "got {found:?}");
}

#[test]
fn both_ends_violated_are_two_warnings() {
    // `[1..8]` against `[4..6]` breaks both. The pilot states two warnings on one line
    // (`Redefinition_MultiplicityConformance_Invalid.sysml.xt:38-39`), so one report per element
    // would leave the second assertion unmatched.
    let src = r#"
package P {
	part def Cylinder;
	part def Engine {
		part cyl : Cylinder[4..6];
	}
	part def Mid :> Engine {
		part mycyl : Cylinder[1..8] redefines cyl;
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found.iter().any(|m| m.contains("smaller lower bound")),
        "the lower end: {found:?}"
    );
    assert!(
        found.iter().any(|m| m.contains("larger upper bound")),
        "the upper end: {found:?}"
    );
}

#[test]
fn a_subsetting_that_only_widens_the_lower_bound_is_silent() {
    // `attribute a3[*] :> a2`: `[*]` is `[0..*]`, so the lower bound is smaller, but the pilot
    // asserts only the upper-bound warning there (`DefaultMultiplicity_invalid.sysml.xt:57`) — the
    // lower-bound rule is worded for redefinitions.
    let src = r#"
package P {
	part def Holder {
		attribute a2;
		attribute a3[*] :> a2;
	}
}
"#;
    let found = sem092(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("larger upper bound"), "got {found:?}");
}
