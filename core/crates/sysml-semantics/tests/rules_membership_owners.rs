//! Which element may own which membership, proved both ways.

use sysml_semantics::{check, Severity};

fn sem038(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM038")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn an_objective_owned_by_a_part_is_rejected() {
    let src = r#"
package P {
	requirement def R;
	part def NotACase { objective o : R; }
}
"#;
    let found = sem038(src);
    assert!(
        found.iter().any(
            |(_, _, m)| m.contains("validateObjectiveMembershipOwningType")
                || m.contains("an `objective` may only be owned by a case")
        ),
        "got {found:?}"
    );
}

#[test]
fn an_objective_owned_by_a_case_is_accepted() {
    let src = r#"
package P {
	requirement def R;
	case def C { objective o : R; }
}
"#;
    assert!(sem038(src).is_empty(), "a case may own an objective");
}

#[test]
fn an_objective_owned_by_a_verification_case_is_accepted() {
    // `VerificationCaseDefinition is a CaseDefinition for the purpose of verification`
    // (generated.rs:1010), so it is in the owner list. The official 9-Verification model writes
    // exactly this shape.
    let src = r#"
package P {
	requirement def R;
	verification def V { objective o : R; }
}
"#;
    assert!(
        sem038(src).is_empty(),
        "a verification case may own an objective"
    );
}

#[test]
fn a_subject_owned_by_a_part_is_rejected() {
    // validateSubjectMembershipOwningType: the owner must be a requirement, a case or a concern.
    let src = r#"
package P {
	part def A { subject s : A; }
}
"#;
    let found = sem038(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("a `subject` may only be owned")),
        "validateSubjectMembershipOwningType: got {found:?}"
    );
}

#[test]
fn a_subject_owned_by_a_requirement_is_accepted() {
    let src = r#"
package P {
	part def A;
	requirement R { subject s : A; }
}
"#;
    assert!(sem038(src).is_empty());
}

#[test]
fn a_subject_bound_inside_an_objective_is_accepted() {
    // `10a-Analysis.sysml:48` writes `objective : T { subject = Case::result; }`. An objective is
    // a RequirementUsage in the metamodel, so its subject is owned by a requirement.
    let src = r#"
package P {
	requirement def R;
	analysis def C {
		objective o : R {
			subject = C::result;
		}
	}
}
"#;
    assert!(
        sem038(src).is_empty(),
        "a subject inside an objective is owned by a requirement; got {:?}",
        sem038(src)
    );
}
