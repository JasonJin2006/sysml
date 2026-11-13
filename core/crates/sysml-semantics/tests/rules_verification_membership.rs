//! `validateRequirementVerificationMembershipOwningType`: a verification belongs to the objective
//! of a case.

use sysml_semantics::{check, Severity};

fn sem068(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM068")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_verification_outside_an_objective_is_rejected() {
    let src = r#"
package P {
	requirement def R;
	part def Holder {
		verify bad : R;
	}
}
"#;
    assert_eq!(sem068(src).len(), 1, "got {:?}", sem068(src));
}

#[test]
fn a_verification_inside_an_objective_is_accepted() {
    let src = r#"
package P {
	requirement def R;
	analysis case C {
		objective obj {
			verify good : R;
		}
	}
}
"#;
    assert!(sem068(src).is_empty(), "got {:?}", sem068(src));
}

#[test]
fn a_verification_case_usage_is_not_judged() {
    // `verification v : T;` declares a verification case usage and may stand anywhere. Collapsing
    // it onto the same kind as `verify r : R` made this rule reject it.
    let src = r#"
package P {
	part def MassTest;
	part def Holder {
		verification vehicleMassTest : MassTest;
	}
}
"#;
    assert!(
        sem068(src).is_empty(),
        "a verification case usage is a different construct, got {:?}",
        sem068(src)
    );
}

#[test]
fn the_report_names_the_owner() {
    let src = r#"
package P {
	requirement def R;
	part def Holder {
		verify bad : R;
	}
}
"#;
    let (_, _, msg) = sem068(src).into_iter().next().expect("one violation");
    assert!(msg.contains("Holder"), "must name the owner: {msg}");
}

#[test]
fn a_verification_in_a_generic_cases_objective_is_rejected() {
    // `case def VerificationPlan { objective { verify r; } }` — both OCL conjuncts hold, and the
    // pilot still states an error there: the objective has to belong to a verification case
    // (`Verification_invalid.sysml.xt:81`).
    let src = r#"
package P {
	requirement def R;
	requirement r : R;
	case def Plan {
		objective obj {
			verify r;
		}
	}
}
"#;
    let found = sem068(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("objective"), "got {found:?}");
}

#[test]
fn a_verification_in_a_verification_defs_objective_is_accepted() {
    // The shape the same model leaves clean: `verification def VerificationCase { objective {
    // verify requirement : R; } }`.
    let src = r#"
package P {
	requirement def R;
	verification def Case {
		objective obj {
			verify requirement : R;
		}
	}
}
"#;
    assert!(sem068(src).is_empty(), "got {:?}", sem068(src));
}
