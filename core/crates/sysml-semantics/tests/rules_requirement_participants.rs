//! `validateActorMembershipOwningType` and `validateStakeholderMembershipOwningType`.

use sysml_semantics::{check, Severity};

fn sem055(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM055")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn an_actor_inside_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		actor driver;
	}
}
"#;
    assert!(
        sem055(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateActorMembershipOwningType")),
        "got {:?}",
        sem055(src)
    );
}

#[test]
fn an_actor_inside_a_requirement_is_accepted() {
    let src = r#"
package P {
	requirement def R {
		actor driver;
	}
}
"#;
    assert!(sem055(src).is_empty(), "got {:?}", sem055(src));
}

#[test]
fn a_stakeholder_inside_a_requirement_is_accepted() {
    let src = r#"
package P {
	requirement def R {
		stakeholder regulator;
	}
}
"#;
    assert!(sem055(src).is_empty(), "got {:?}", sem055(src));
}

#[test]
fn a_stakeholder_inside_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		stakeholder regulator;
	}
}
"#;
    assert!(
        sem055(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateStakeholderMembershipOwningType")),
        "got {:?}",
        sem055(src)
    );
}

#[test]
fn a_stakeholder_inside_a_concern_is_accepted() {
    // A `concern def` is a requirement a view addresses, so it takes stakeholders.
    let src = r#"
package P {
	concern def Safety {
		stakeholder regulator;
	}
}
"#;
    assert!(sem055(src).is_empty(), "got {:?}", sem055(src));
}

#[test]
fn stakeholder_is_one_member_not_two() {
    // Before the keyword arm, `stakeholder owner;` lowered to a `stakeholder` attribute and an
    // `owner` attribute — two members where the model wrote one.
    let src = r#"
package P {
	requirement def R {
		stakeholder regulator;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let r = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .find(|e| e.name == "R")
        .expect("R");
    let names: Vec<_> = r.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["regulator"], "one member, got {names:?}");
}

// -- validateSatisfyRequirementUsageReference -------------------------------

#[test]
fn a_satisfy_naming_a_requirement_is_accepted() {
    let src = r#"
package P {
	requirement def R1;
	requirement r2 : R1;
	part p {
		satisfy r2 by p;
	}
}
"#;
    assert!(sem055(src).is_empty(), "got {:?}", sem055(src));
}

#[test]
fn a_satisfy_naming_a_part_is_rejected() {
    // The referenced feature has to be a `RequirementUsage`; a part is not one.
    let src = r#"
package P {
	part def Wheel;
	part wheel : Wheel;
	part p {
		satisfy wheel by p;
	}
}
"#;
    let found = sem055(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0]
            .2
            .contains("validateSatisfyRequirementUsageReference"),
        "got {found:?}"
    );
}

#[test]
fn a_satisfy_naming_a_viewpoint_is_accepted() {
    // A `ViewpointUsage` is a `RequirementUsage`, and the projection gives the `viewpoint def` and
    // `viewpoint v : V` spellings one kind, so the metaclass question has to answer for both.
    let src = r#"
package P {
	viewpoint def Perspective {
		subject v : P;
	}
	part def P;
	part p {
		satisfy Perspective by p;
	}
}
"#;
    assert!(sem055(src).is_empty(), "got {:?}", sem055(src));
}
