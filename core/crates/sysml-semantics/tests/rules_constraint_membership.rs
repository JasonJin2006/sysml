//! `validateRequirementConstraintMembershipOwningType`: a require / assume clause belongs to a
//! requirement.

use sysml_semantics::{check, Severity};

fn sem062(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM062")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_require_clause_in_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		require constraint stray;
	}
}
"#;
    assert_eq!(sem062(src).len(), 1, "got {:?}", sem062(src));
}

#[test]
fn a_require_clause_in_a_requirement_is_accepted() {
    let src = r#"
package P {
	requirement def R {
		require constraint ok;
	}
}
"#;
    assert!(sem062(src).is_empty(), "got {:?}", sem062(src));
}

#[test]
fn an_assume_clause_is_judged_too() {
    let src = r#"
package P {
	part def Holder {
		assume constraint stray;
	}
}
"#;
    assert_eq!(sem062(src).len(), 1, "got {:?}", sem062(src));
}

#[test]
fn a_plain_constraint_anywhere_is_not_judged() {
    // `constraint c { ... }` lowers to the same kind but is not a RequirementConstraintMembership;
    // the official models put those inside part definitions.
    let src = r#"
package P {
	part def Tire {
		constraint hasLegalProfileDepth { 1 > 0 }
	}
}
"#;
    assert!(
        sem062(src).is_empty(),
        "a plain constraint is not a require clause, got {:?}",
        sem062(src)
    );
}

#[test]
fn a_named_require_clause_becomes_a_member() {
    // Before, `require constraint c1;` produced no element at all: the name was unreachable.
    let src = r#"
package P {
	requirement def R {
		require constraint c1;
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
    assert_eq!(names, vec!["c1"], "got {names:?}");
}

#[test]
fn a_named_require_clause_keeps_the_type_it_names() {
    // `require constraint c1 : C;` names a `ConstraintDefinition` the constraint is an instance
    // of. The parser read the `: C` and threw it away, so the element arrived untyped and any
    // rule that asks what a constraint is typed by had nothing to read.
    let src = r#"
package P {
	constraint def C;
	requirement def R {
		require constraint c1 : C;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let c1 = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .flat_map(|c| c.children.iter())
        .find(|e| e.name == "c1")
        .expect("c1");
    assert_eq!(c1.type_ref.as_deref(), Some("C"), "got {c1:?}");
}
