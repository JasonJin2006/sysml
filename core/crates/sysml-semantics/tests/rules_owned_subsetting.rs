//! At most one `references` and one `crosses` per feature.

use sysml_semantics::{check, Severity};

fn sem056(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM056")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn two_references_clauses_are_rejected() {
    let src = r#"
package P {
	part p;
	part q;
	part bad references p, q;
}
"#;
    assert!(
        sem056(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateFeatureOwnedReferenceSubsetting")),
        "got {:?}",
        sem056(src)
    );
}

#[test]
fn one_references_clause_is_accepted() {
    let src = r#"
package P {
	part p;
	part q references p;
}
"#;
    assert!(sem056(src).is_empty(), "got {:?}", sem056(src));
}

#[test]
fn two_crosses_clauses_are_rejected() {
    let src = r#"
package P {
	part def A;
	part def B;
	part bad crosses A, B;
}
"#;
    assert!(
        sem056(src)
            .iter()
            .any(|(_, _, m)| m.contains("validateFeatureOwnedCrossSubsetting")),
        "got {:?}",
        sem056(src)
    );
}

#[test]
fn one_crosses_clause_is_accepted() {
    let src = r#"
package P {
	part def A;
	part r crosses A;
}
"#;
    assert!(sem056(src).is_empty(), "got {:?}", sem056(src));
}

#[test]
fn an_ordinary_subsets_clause_is_not_bounded() {
    // `subsets` has no such bound; only the two singled-out kinds do.
    let src = r#"
package P {
	part p;
	part q;
	part r subsets p, q;
}
"#;
    assert!(
        sem056(src).is_empty(),
        "a general subsets clause may name several, got {:?}",
        sem056(src)
    );
}
