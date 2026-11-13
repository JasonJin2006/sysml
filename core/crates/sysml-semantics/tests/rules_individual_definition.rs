//! `validateOccurrenceUsageIndividualDefinition`: an occurrence usage names at most one
//! individual definition.

use sysml_semantics::{check, Severity};

fn sem072(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM072")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn two_individual_types_are_rejected() {
    let src = r#"
package P {
	individual part def V1;
	individual part def V2;
	part two : V1, V2;
}
"#;
    assert_eq!(sem072(src).len(), 1, "got {:?}", sem072(src));
}

#[test]
fn one_individual_type_is_accepted() {
    let src = r#"
package P {
	individual part def V1;
	part one : V1;
}
"#;
    assert!(sem072(src).is_empty());
}

#[test]
fn one_individual_beside_a_plain_type_is_accepted() {
    let src = r#"
package P {
	individual part def V1;
	part def Plain;
	part three : V1, Plain;
}
"#;
    assert!(
        sem072(src).is_empty(),
        "only one of the two is an individual, got {:?}",
        sem072(src)
    );
}

#[test]
fn a_usage_with_one_type_is_not_judged() {
    let src = r#"
package P {
	individual part def V1;
	individual part def V2;
	part one : V1;
}
"#;
    assert!(sem072(src).is_empty(), "got {:?}", sem072(src));
}

#[test]
fn the_keyword_spelling_counts_too() {
    // `individual` is the kind keyword in `individual def A1` and `individual two : A1, B1`, not a
    // prefix in front of one (`SysML.xtext:845-851`), so `isIndividual` has to come from the kind.
    // Read from the prefix alone, the count was zero and the rule stayed silent
    // (`IndividualUsage_Invalid.sysml.xt:47`).
    let src = r#"
package P {
	individual def A1;
	individual def B1;
	individual two : A1, B1;
}
"#;
    assert_eq!(sem072(src).len(), 1, "got {:?}", sem072(src));
}

#[test]
fn both_types_reach_the_tree() {
    // `part two : V1, V2` stated two types and only the first reached the tree before
    // Element::type_refs existed.
    let src = r#"
package P {
	individual part def V1;
	individual part def V2;
	part two : V1, V2;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let two = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .find(|e| e.name == "two")
        .expect("two");
    assert_eq!(two.type_refs, vec!["V1".to_string(), "V2".to_string()]);
    assert_eq!(
        two.type_ref.as_deref(),
        Some("V1"),
        "the first is still type_ref"
    );
}
