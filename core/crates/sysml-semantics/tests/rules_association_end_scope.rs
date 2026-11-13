//! `validateAssociationEndTypes` judges the ends of an *Association*, not those of a connector.

use sysml_semantics::check;

fn sem041(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM041")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_connection_typed_by_two_definitions_is_not_an_association_with_two_end_types() {
    // `ConnectionUsage_Invalid.sysml.xt:55`: the pilot leaves this line clean, and the kernel
    // reported each of `de`'s two ends as naming two types.
    let found = sem041(
        r#"package P {
    part def B; part def C; part def D; part def E;
    connection def DE { end : D[1]; end : E[1]; }
    connection def BC { end : B[1]; end : C[1]; }
    part def A;
    part apart : A {
        part b : B[1];
        part c : C[1];
        connection de : DE, BC connect b to c;
    }
}"#,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_association_and_a_connection_definition_are_still_judged() {
    // Positive control: `AssociationTest_EndTypes_invalid.kerml.xt:33` and a `connection def`,
    // which is an `AssociationStructure`.
    let found = sem041(
        r#"package P {
    class C1; class C2;
    assoc A1 { end x : C1; end y : C1, C2; }
    part def B; part def C;
    connection def BC { end b : B, C; end c : C; }
}"#,
    );
    assert_eq!(found.len(), 2, "{found:?}");
}
