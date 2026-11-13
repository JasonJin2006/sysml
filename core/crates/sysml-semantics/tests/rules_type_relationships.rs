//! The six rules on a type's `unions` / `intersects` / `differences` clauses.
//!
//! Each operator answers the same pair: not itself, and not exactly one operand. The tests run
//! the pair against every operator so a typo in the operator table shows up as a wrong rule name.

use sysml_semantics::{check, Severity};

fn sem046(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM046")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

fn mentions(src: &str, rule: &str) -> bool {
    sem046(src).iter().any(|(_, _, m)| m.contains(rule))
}

#[test]
fn a_type_unioning_itself_is_rejected() {
    let src = r#"
package P {
	datatype SelfUnion unions SelfUnion;
	datatype A;
	datatype Two unions A, A;
}
"#;
    assert!(
        mentions(src, "validateTypeUnioningTypesNotSelf"),
        "got {:?}",
        sem046(src)
    );
}

#[test]
fn a_type_intersecting_itself_is_rejected() {
    let src = r#"
package P {
	datatype SelfIntersect intersects SelfIntersect;
	datatype A;
	datatype Two intersects A, A;
}
"#;
    assert!(mentions(src, "validateTypeIntersectingTypesNotSelf"));
}

#[test]
fn a_type_differencing_itself_is_rejected() {
    let src = r#"
package P {
	datatype SelfDiff differences SelfDiff;
	datatype A;
	datatype Two differences A, A;
}
"#;
    assert!(mentions(src, "validateTypeDifferencingTypesNotSelf"));
}

// validateTypeOwnedUnioningNotOne, validateTypeOwnedIntersectingNotOne and
// validateTypeOwnedDifferencingNotOne are the same bound over three operators.
#[test]
fn a_single_operand_is_rejected_for_each_operator() {
    for clause in ["unions", "intersects", "differences"] {
        let src = format!("package P {{\n\tdatatype A;\n\tdatatype One {clause} A;\n}}\n");
        let found = sem046(&src);
        assert_eq!(
            found.len(),
            1,
            "`{clause} A` states one operand and must be reported once, got {found:?}"
        );
    }
}

#[test]
fn a_well_formed_clause_is_accepted() {
    let src = r#"
package P {
	datatype A;
	datatype B;
	datatype Combined unions A, B;
	datatype Overlap intersects A, B;
	datatype Rest differences A, B;
}
"#;
    assert!(
        sem046(src).is_empty(),
        "two operands naming other types is legal, got {:?}",
        sem046(src)
    );
}

#[test]
fn the_clause_stays_out_of_the_body() {
    // Before the parser read TypeRelationshipPart, `datatype B unions A` lowered `unions` and `A`
    // into the body as two stray attributes — the model gained two members it never wrote.
    let src = r#"
package P {
	datatype A;
	datatype B unions A;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let b = loaded
        .roots
        .iter()
        .flat_map(|r| r.children.iter())
        .find(|e| e.name == "B")
        .expect("B");
    assert!(
        b.children.is_empty(),
        "a unions clause is not a body member, got {:?}",
        b.children
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(b.unions, vec!["A".to_string()]);
}
