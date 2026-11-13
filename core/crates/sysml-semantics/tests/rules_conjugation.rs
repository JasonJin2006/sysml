//! `validateTypeAtMostOneConjugator`: a type conjugates at most one other type.

use sysml_semantics::{check, Severity};

fn sem057(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM057")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn two_conjugators_are_rejected() {
    let src = r#"
package P {
	port def A;
	port def B;
	port def C conjugates A, B;
}
"#;
    assert_eq!(sem057(src).len(), 1, "got {:?}", sem057(src));
}

#[test]
fn one_conjugator_is_accepted() {
    let src = r#"
package P {
	port def A;
	port def C conjugates A;
}
"#;
    assert!(sem057(src).is_empty());
}

#[test]
fn the_tilde_spelling_is_counted_too() {
    let src = r#"
package P {
	port def A;
	port def B;
	port def C ~A, ~B;
}
"#;
    assert_eq!(
        sem057(src).len(),
        1,
        "`~A, ~B` is two conjugators, got {:?}",
        sem057(src)
    );
}
