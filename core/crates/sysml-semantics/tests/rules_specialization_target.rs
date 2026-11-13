//! `validateSpecificationSpecificNotConjugated`: a conjugated type may not specialize.
//!
//! The OCL is `not specific.isConjugated`, and `specific` is the type that specializes. The rule
//! therefore constrains the subtype, not the supertype — a distinction this file now pins in both
//! directions, because the module used to read it the other way round.

use sysml_semantics::{check, Severity};

fn sem063(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM063")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_conjugated_type_that_specializes_is_rejected() {
    // `Both` is the specific type of the specialization, and it is conjugated.
    let src = r#"
package P {
	port def FuelOut;
	port def FuelIn conjugates FuelOut;
	port def Both conjugates FuelOut :> FuelIn;
}
"#;
    let found = sem063(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.ends_with("Both"),
        "reported at the subtype: {found:?}"
    );
}

#[test]
fn a_conjugated_type_with_no_specialization_is_accepted() {
    let src = r#"
package P {
	port def A;
	port def B conjugates A;
}
"#;
    assert!(sem063(src).is_empty(), "got {:?}", sem063(src));
}

#[test]
fn specializing_a_conjugated_type_is_accepted() {
    // The supertype is conjugated and the subtype is not, so the OCL holds. The pilot's own
    // direction model writes this shape and states no error for it
    // (`Redefinition_DirectionConformance_invalid.kerml.xt:50`).
    let src = r#"
package P {
	port def A;
	port def B conjugates A;
	port def C :> B;
}
"#;
    assert!(sem063(src).is_empty(), "got {:?}", sem063(src));
}

#[test]
fn a_standalone_specialization_of_a_conjugated_type_is_rejected() {
    // `Specialization_invalid.kerml.xt:30` writes the relation as a statement rather than as a
    // declaration: `subtype C specializes B;`, with `C` conjugated on the line above.
    let src = r#"
package P {
	classifier A;
	classifier B;
	classifier C conjugates A;
	subtype C specializes B;
}
"#;
    let found = sem063(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.ends_with("C"),
        "reported at the specific type: {found:?}"
    );
}

#[test]
fn the_statement_reads_the_arrow_spelling_too() {
    let src = r#"
package P {
	classifier A;
	classifier B;
	classifier C conjugates A;
	subtype C :> B;
}
"#;
    assert_eq!(sem063(src).len(), 1, "got {:?}", sem063(src));
}

#[test]
fn a_standalone_specialization_of_a_plain_type_is_accepted() {
    let src = r#"
package P {
	classifier A;
	classifier B;
	subtype A specializes B;
}
"#;
    assert!(sem063(src).is_empty(), "got {:?}", sem063(src));
}

#[test]
fn specializing_the_unconjugated_original_is_accepted() {
    let src = r#"
package P {
	port def A;
	port def B conjugates A;
	port def D :> A;
}
"#;
    assert!(sem063(src).is_empty(), "got {:?}", sem063(src));
}

#[test]
fn the_tilde_spelling_marks_a_conjugated_type_too() {
    // `port def Both ~FuelOut :> FuelIn` writes the conjugation with a sigil instead of the word.
    let src = r#"
package P {
	port def FuelOut;
	port def FuelIn conjugates FuelOut;
	port def Both ~FuelOut :> FuelIn;
}
"#;
    assert_eq!(sem063(src).len(), 1, "got {:?}", sem063(src));
}
