//! `validateRedefinitionFeaturingTypes`: a redefinition must add a featuring type.
//!
//! The OCL compares the two featuring-type sets and states `<>`. Two owners differ, so the case
//! the rule decides is a redefinition in the *same* type as its target — and the package-level
//! case, where neither feature has a featuring type at all.

use sysml_semantics::{check, Severity};

fn sem075(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM075")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_redefinition_in_the_same_type_is_rejected() {
    // Both features belong to `Base`, so the redefinition adds no featuring type the target does
    // not already have.
    let src = r#"
package P {
	part def Base {
		part x;
		part y redefines x;
	}
}
"#;
    let found = sem075(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.ends_with("y"),
        "reported at the redefinition: {found:?}"
    );
}

#[test]
fn a_redefinition_in_a_subtype_is_accepted() {
    let src = r#"
package P {
	part def Base { part x; }
	part def Sub :> Base { part y redefines x; }
}
"#;
    assert!(sem075(src).is_empty(), "got {:?}", sem075(src));
}

#[test]
fn a_redefinition_of_an_unrelated_feature_is_not_this_rule() {
    // `{Bad, Anything}` against `{Other, Anything}` differs, so the OCL holds. A redefinition that
    // names a feature it does not inherit is a subsetting question, judged elsewhere.
    let src = r#"
package P {
	part def Base { part x; }
	part def Other { part z; }
	part def Bad :> Base { part w redefines Other::z; }
}
"#;
    assert!(sem075(src).is_empty(), "got {:?}", sem075(src));
}

#[test]
fn a_package_level_redefinition_is_rejected() {
    // `Redefinition_OwningType_Invalid.sysml.xt:33`: neither feature has a featuring type, so the
    // two sets are `{Anything}` on both sides.
    let src = r#"
package P {
	part wheel : Wheel;
	part wheel1 redefines wheel;
	part def Wheel;
}
"#;
    let found = sem075(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.ends_with("wheel1"),
        "reported at the redefinition: {found:?}"
    );
}

#[test]
fn an_alias_target_is_compared_where_the_aliased_feature_lives() {
    // `Redefinition_OwningType_Invalid.sysml.xt:42`: `eng1` is `alias eng1 for Vehicle::eng` inside
    // the supertype, so the feature it names belongs to `Vehicle` — the same type the redefinition
    // lives in.
    let src = r#"
package P {
	part def Engine;
	part def B {
		part eng : Engine;
		alias eng1 for Vehicle::eng;
	}
	part def Vehicle :> B {
		part eng : Engine;
		part smallEng : Engine redefines eng1;
	}
}
"#;
    let found = sem075(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.ends_with("smallEng"),
        "reported at the redefinition: {found:?}"
    );
}

#[test]
fn a_redefinition_repeating_its_own_name_is_left_alone() {
    // `part cyl :>> cyl` resolves to the new `cyl` itself. That is a dangling reference, and
    // comparing a feature with itself would report every such line.
    let src = r#"
package P {
	part def Engine { part cyl; }
	part def Small :> Engine { part cyl redefines cyl; }
}
"#;
    assert!(sem075(src).is_empty(), "got {:?}", sem075(src));
}

#[test]
fn an_unrelated_owner_is_left_alone() {
    // The official 3c model redefines inside an anonymous `connect ... to ...`; that owner is not a
    // definition, so it is not a featuring-type question.
    let src = r#"
package P {
	part def Base { part x; }
	part def Holder {
		ref r :> Base::x;
	}
}
"#;
    assert!(sem075(src).is_empty(), "got {:?}", sem075(src));
}

#[test]
fn an_unresolved_target_is_left_alone() {
    let src = r#"
package P {
	part def Bad { part w redefines Nowhere; }
}
"#;
    assert!(
        sem075(src).is_empty(),
        "a dangling target is a reference problem, got {:?}",
        sem075(src)
    );
}
