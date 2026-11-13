//! `validateSubsettingUniquenessConformance`: a subset may not drop uniqueness its target has.

use sysml_semantics::{check, Severity};

fn sem064(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM064")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_nonunique_subset_of_a_unique_feature_is_rejected() {
    let src = r#"
package P {
	part def A;
	part bb : A;
	part sb : A :> bb nonunique;
}
"#;
    assert_eq!(sem064(src).len(), 1, "got {:?}", sem064(src));
}

#[test]
fn a_unique_subset_of_a_nonunique_feature_is_accepted() {
    let src = r#"
package P {
	part def A;
	part bb : A nonunique;
	part sb : A :> bb;
}
"#;
    assert!(
        sem064(src).is_empty(),
        "narrowing is what a subset does, got {:?}",
        sem064(src)
    );
}

#[test]
fn both_nonunique_is_accepted() {
    let src = r#"
package P {
	part def A;
	part bb : A nonunique;
	part sb : A :> bb nonunique;
}
"#;
    assert!(sem064(src).is_empty(), "got {:?}", sem064(src));
}

#[test]
fn an_unresolved_target_is_left_alone() {
    let src = r#"
package P {
	part def A;
	part sb : A :> Nowhere nonunique;
}
"#;
    assert!(
        sem064(src).is_empty(),
        "a dangling target is a reference problem, got {:?}",
        sem064(src)
    );
}
