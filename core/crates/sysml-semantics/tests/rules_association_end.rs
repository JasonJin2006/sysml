//! `validateAssociationEndTypes` — an end names exactly one type.
//!
//! The "zero types" direction is **not** a violation: the specification gives every end of an
//! association or a connector an implied subsetting of `Links::Link::participant`, and the
//! reference implementation materialises it (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/
//! implied.rs:144-186`). `AssociationTest_EndRedefinitionGoodCase.kerml.xt` writes `end x;` under
//! `// XPECT noErrors`, so reporting it would reject a conforming model.
//!
//! What is judged is the other direction: `end z : A, B` names two. `type_refs` keeps them all
//! since the projection stopped reading only the first target.

use sysml_semantics::{check, Severity};

fn sem041(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM041")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn an_end_with_no_type_is_accepted() {
    // The type is implied: every end subsets `Links::Link::participant`, so a silent end is not
    // a model that failed to say something — it is the ordinary spelling.
    // `AssociationTest_EndRedefinitionGoodCase.kerml.xt` is `assoc A { end x; end [1..*]
    // feature y; }` under `// XPECT noErrors`.
    let src = r#"
package P {
	part def A;
	connection def C {
		part labelled : A;
		end x : A;
		end y;
	}
}
"#;
    assert!(
        sem041(src).is_empty(),
        "an untyped end is legal, got {:?}",
        sem041(src)
    );
}

#[test]
fn an_end_naming_two_types_is_rejected() {
    // `AssociationTest_EndTypes_invalid.kerml.xt:33`: `end y : C1, C2;`
    let src = r#"
package P {
	part def A;
	part def B;
	connection def C {
		end y : A, B;
	}
}
"#;
    let found = sem041(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("names 2 types"), "got {found:?}");
}

#[test]
fn a_typed_end_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def B;
	connection def C {
		end x : A;
		end z : B;
	}
}
"#;
    assert!(sem041(src).is_empty(), "both ends name a type");
}

#[test]
fn an_end_outside_an_association_is_not_judged() {
    // The rule is stated of `ownedEndFeature`, which only an association owns.
    let src = r#"
package P {
	part def Holder {
		end y;
	}
}
"#;
    assert!(
        sem041(src).is_empty(),
        "an end in a part is not an association end, got {:?}",
        sem041(src)
    );
}

#[test]
fn a_redeclared_end_that_widens_the_inherited_type_is_rejected() {
    // `AssociationTest_EndTypes_invalid.kerml.xt:35-38`: `assoc A2 specializes A1 { end x : C2; }`
    // with `A1`'s `end x : C1`. The redeclaration redefines the inherited end of the same name, so
    // the end's `type` — `deriveFeatureType`, the typings of the feature and of what it redefines,
    // redundant supertypes removed — is `{C2, C1}`: two types, and the pilot reports it. Counting
    // the written clause alone saw one.
    let src = r#"
package P {
	class C1;
	class C2;
	assoc A1 { end x : C1; }
	assoc A2 specializes A1 { end x : C2; }
}
"#;
    let found = sem041(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("2 types"), "got {found:?}");
}

#[test]
fn a_multi_typed_end_of_an_association_structure_is_rejected() {
    // `assoc struct` is one identity — `AssociationStructure`, whose supers are `Association` and
    // `Structure` — so it owns `ownedEndFeature` and the rule applies to its ends exactly as to an
    // `assoc`'s. The kind used to fall through the owner test, so this model reported nothing.
    let src = r#"
package P {
	part def A;
	part def B;
	assoc struct R {
		end y : A, B;
	}
}
"#;
    let found = sem041(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("names 2 types"), "got {found:?}");
}

#[test]
fn a_typed_end_of_an_association_structure_is_accepted() {
    // The nearest conforming model: the same owner, one type per end.
    let src = r#"
package P {
	part def A;
	part def B;
	assoc struct R {
		end x : A;
		end y : B;
	}
}
"#;
    assert!(sem041(src).is_empty(), "got {:?}", sem041(src));
}

#[test]
fn a_redeclared_end_inside_an_association_structure_is_judged() {
    // The inherited-type half of `AssociationTest_EndTypes_invalid.kerml.xt` under the combined
    // kind: `R2`'s `end x : C2` redefines `R1`'s `end x : C1`, so the type set is `{C2, C1}`.
    // `R1` is abstract so that its single end is exempt from the arity rule and this test speaks
    // only about the end-type rule.
    let src = r#"
package P {
	class C1;
	class C2;
	abstract assoc struct R1 { end x : C1; }
	assoc struct R2 specializes R1 { end x : C2; }
}
"#;
    let found = sem041(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("2 types"), "got {found:?}");
}
