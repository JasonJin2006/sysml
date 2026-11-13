//! `validatePartUsagePartDefinition`: a part's type must be an occurrence definition.
//!
//! The cases mirror the OMG pilot implementation's `PartUsage_invalid.sysml.xt`, which marks
//! `part p1: Real;`, `part p2: att;`, `part p3: act;` and `part p4: AttDef;` as errors and leaves
//! `part aPart;` alone.

use sysml_semantics::{check, Severity};

fn sem079(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM079")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_part_typed_by_a_scalar_is_rejected() {
    let src = r#"
package P {
	public import ScalarValues::*;
	part def A { part p1 : Real; }
}
"#;
    assert_eq!(sem079(src).len(), 1, "got {:?}", sem079(src));
}

#[test]
fn a_part_typed_by_an_attribute_definition_is_rejected() {
    let src = r#"
package P {
	attribute def AttDef;
	part def A { part p4 : AttDef; }
}
"#;
    assert_eq!(sem079(src).len(), 1, "got {:?}", sem079(src));
}

#[test]
fn an_untyped_part_is_accepted() {
    // The pilot implementation leaves `part aPart;` alone in the same file where it marks the
    // typed ones as errors. This is the distinction the first attempt at this rule missed.
    let src = r#"
package P {
	part def A { part aPart; }
}
"#;
    assert!(sem079(src).is_empty(), "got {:?}", sem079(src));
}

#[test]
fn a_part_typed_by_a_part_definition_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part def Car { part wheels : Wheel; }
}
"#;
    assert!(sem079(src).is_empty(), "got {:?}", sem079(src));
}

#[test]
fn a_part_typed_by_an_item_definition_is_accepted() {
    let src = r#"
package P {
	item def Fuel;
	part def Car { part tank : Fuel; }
}
"#;
    assert!(sem079(src).is_empty(), "got {:?}", sem079(src));
}

#[test]
fn an_unresolved_type_is_left_alone() {
    let src = r#"
package P {
	part def A { part p : Nowhere; }
}
"#;
    assert!(
        sem079(src).is_empty(),
        "a dangling name is a reference problem, got {:?}",
        sem079(src)
    );
}
