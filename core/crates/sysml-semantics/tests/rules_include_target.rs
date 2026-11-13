//! `validateIncludeUseCaseUsageReference`: `include` names a use case.

use sysml_semantics::{check, Severity};

fn sem076(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM076")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn including_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Chassis;
	use case Host {
		include Chassis;
	}
}
"#;
    assert_eq!(sem076(src).len(), 1, "got {:?}", sem076(src));
}

#[test]
fn including_a_use_case_is_accepted() {
    let src = r#"
package P {
	use case Park;
	use case Host {
		include Park;
	}
}
"#;
    assert!(sem076(src).is_empty(), "got {:?}", sem076(src));
}

#[test]
fn including_a_use_case_definition_is_rejected() {
    // A definition describes the class; what is included is one use case.
    let src = r#"
package P {
	use case def Park;
	use case Host {
		include Park;
	}
}
"#;
    assert_eq!(
        sem076(src).len(),
        1,
        "a use case definition is not a usage, got {:?}",
        sem076(src)
    );
}

#[test]
fn a_use_case_without_def_lowers_as_a_usage() {
    // Before the split, `use case X` lowered to Kind::UseCaseDef, so the tree held no use-case
    // usage at all and this rule had nothing to accept.
    let src = r#"
package P {
	use case Park;
	use case Host {
		include Park;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let park = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .find(|e| e.name == "Park")
        .expect("Park");
    assert_eq!(park.kind, sysml_model::Kind::UseCaseUsage);
}

#[test]
fn an_unresolved_target_is_left_alone() {
    let src = r#"
package P {
	use case Host {
		include Nowhere;
	}
}
"#;
    assert!(
        sem076(src).is_empty(),
        "a dangling target is a reference problem, got {:?}",
        sem076(src)
    );
}
