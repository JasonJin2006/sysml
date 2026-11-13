//! `Bound features should have conforming types` — the two ends of a `bind` hold one value.
//!
//! The pilot states it as a warning: `bind a = b;` with `a : A` and `b : B` unrelated is almost
//! always a mistake, but the model is legal. `bind b = c;` with `C specializes B` is the shape
//! that conforms and carries no assertion (`BindingConnector_Invalid3.sysml.xt:61`; the warning
//! above it is at `:57`).

use sysml_semantics::{check, Severity};

fn conformance_warnings(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM060" && i.severity == Severity::Warning)
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn binding_two_unrelated_types_warns() {
    let src = r#"
package P {
	part def A;
	part def B;
	part p {
		part a : A;
		part b : B;
		bind a = b;
	}
}
"#;
    let found = conformance_warnings(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("unrelated types"), "got {found:?}");
}

#[test]
fn binding_a_subtype_to_its_supertype_is_accepted() {
    // `bind b = c;` with `C specializes B`.
    let src = r#"
package P {
	part def B;
	part def C :> B;
	part p {
		part b : B;
		part c : C;
		bind b = c;
	}
}
"#;
    assert!(
        conformance_warnings(src).is_empty(),
        "a subtype conforms, got {:?}",
        conformance_warnings(src)
    );
}

#[test]
fn binding_two_features_of_one_type_is_accepted() {
    let src = r#"
package P {
	part def A;
	part p {
		part a : A;
		part a2 : A;
		bind a = a2;
	}
}
"#;
    assert!(
        conformance_warnings(src).is_empty(),
        "one type on both ends, got {:?}",
        conformance_warnings(src)
    );
}

#[test]
fn an_operator_argument_cannot_bind_a_part_to_a_data_value_parameter() {
    let src = r#"
package P {
    part def Wheel;
    part assembly {
        part wheel : Wheel[2];
        ref x = wheel + 1;
    }
}
"#;
    let found = conformance_warnings(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("Wheel"), "got {found:?}");
    assert!(found[0].contains("DataValue"), "got {found:?}");
}

#[test]
fn numeric_and_string_operator_arguments_remain_accepted() {
    let src = r#"
package P {
    private import ScalarValues::*;
    attribute count : Integer;
    attribute length : Real;
    attribute text : String;
    ref sum = count + length;
    ref concatenated = text + "suffix";
}
"#;
    assert!(
        conformance_warnings(src).is_empty(),
        "got {:?}",
        conformance_warnings(src)
    );
}

#[test]
fn an_unresolved_operator_argument_is_left_to_reference_checking() {
    let src = r#"
package P {
    ref x = missing + 1;
}
"#;
    assert!(
        conformance_warnings(src).is_empty(),
        "got {:?}",
        conformance_warnings(src)
    );
}

#[test]
fn a_binding_end_with_no_resolvable_type_is_not_judged() {
    // A reference that does not resolve is the reference pass's report; guessing a type here would
    // add a second diagnostic for one defect.
    let src = r#"
package P {
	part def A;
	part p {
		part a : A;
		bind a = nowhere;
	}
}
"#;
    assert!(
        conformance_warnings(src).is_empty(),
        "an unresolved end is not judged, got {:?}",
        conformance_warnings(src)
    );
}
