//! A variation may not specialize another variation, proved both ways.

use sysml_semantics::{check, Severity};

fn sem048(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM048")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_variation_definition_specializing_a_variation_is_rejected() {
    let src = r#"
package P {
	variation part def Base;
	variation part def Narrow :> Base;
}
"#;
    let found = sem048(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateDefinitionVariationSpecialization")),
        "got {found:?}"
    );
}

#[test]
fn a_variation_specializing_a_plain_definition_is_accepted() {
    let src = r#"
package P {
	part def Plain;
	variation part def V :> Plain;
}
"#;
    assert!(
        sem048(src).is_empty(),
        "only another variation is off limits, got {:?}",
        sem048(src)
    );
}

#[test]
fn an_abstract_definition_is_not_a_variation() {
    // `abstract` alone does not make a variation, so specializing it stays legal.
    let src = r#"
package P {
	abstract part def Base;
	variation part def V :> Base;
}
"#;
    assert!(sem048(src).is_empty());
}

#[test]
fn a_variation_usage_specializing_a_variation_usage_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		variation part base : Holder;
		variation part narrow :> base;
	}
}
"#;
    let found = sem048(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateUsageVariationSpecialization")),
        "got {found:?}"
    );
}

#[test]
fn the_report_names_the_supertype() {
    let src = r#"
package P {
	variation part def Base;
	variation part def Narrow :> Base;
}
"#;
    let (_, _, msg) = sem048(src).into_iter().next().expect("one violation");
    assert!(
        msg.contains("P::Base"),
        "the message must name the target: {msg}"
    );
}
