//! `validateMetadataFeatureMetaclassNotAbstract`: a metadata usage may not name an abstract
//! definition.

use sysml_semantics::{check, Severity};

fn sem065(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM065")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn annotating_with_an_abstract_metadata_definition_is_rejected() {
    let src = r#"
package P {
	abstract metadata def Base;
	part def Holder { @Base; part x; }
}
"#;
    assert_eq!(sem065(src).len(), 1, "got {:?}", sem065(src));
}

#[test]
fn annotating_with_a_concrete_metadata_definition_is_accepted() {
    let src = r#"
package P {
	metadata def Safety;
	part def Holder { @Safety; part x; }
}
"#;
    assert!(sem065(src).is_empty(), "got {:?}", sem065(src));
}

#[test]
fn a_specialization_of_an_abstract_base_is_concrete() {
    let src = r#"
package P {
	abstract metadata def Base;
	metadata def Safety :> Base;
	part def Holder { @Safety; part x; }
}
"#;
    assert!(
        sem065(src).is_empty(),
        "Safety is concrete even though Base is not, got {:?}",
        sem065(src)
    );
}

#[test]
fn the_report_names_the_abstract_definition() {
    let src = r#"
package P {
	abstract metadata def Base;
	part def Holder { @Base; part x; }
}
"#;
    let (_, _, msg) = sem065(src).into_iter().next().expect("one violation");
    assert!(msg.contains("P::Base"), "must name the target: {msg}");
    assert!(msg.contains("abstract"), "must say why: {msg}");
}
