//! `validateFeatureCrossFeatureType`: a feature and the feature it crosses share a type.

use sysml_semantics::{check, Severity};

fn sem073(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM073")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_crossing_with_a_different_type_is_rejected() {
    let src = r#"
package P {
	part def A;
	part def B;
	part def Holder {
		end part diff : A crosses B;
	}
}
"#;
    assert_eq!(sem073(src).len(), 1, "got {:?}", sem073(src));
}

#[test]
fn a_crossing_with_the_same_type_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		end part same : A crosses A;
	}
}
"#;
    assert!(sem073(src).is_empty());
}

#[test]
fn a_qualified_name_matches_its_leaf() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		end part same : A crosses P::A;
	}
}
"#;
    assert!(
        sem073(src).is_empty(),
        "`P::A` and `A` reach the same definition, got {:?}",
        sem073(src)
    );
}

#[test]
fn a_feature_with_no_type_is_not_judged() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		end part untyped crosses A;
	}
}
"#;
    assert!(
        sem073(src).is_empty(),
        "nothing to compare, got {:?}",
        sem073(src)
    );
}

#[test]
fn a_redundant_supertype_does_not_split_one_type() {
    // Both sides are `{B}`: `x : A, B` loses `A` because `B :> A` (`deriveFeatureType`), and `y` is
    // typed `B`. Comparing the written clauses instead took `x`'s first type `A` and reported this
    // legal model.
    let src = r#"
package P {
	part def A;
	part def B :> A;
	part def Holder {
		end part x : A, B;
		end part y : B crosses x;
	}
}
"#;
    assert!(
        sem073(src).is_empty(),
        "both sides are `B`, got {:?}",
        sem073(src)
    );
}

#[test]
fn a_redundant_supertype_still_hides_a_real_difference() {
    // The mirror: `y : A, B` is `{B}` and `x` is `{A}`, so the crossing is between two kinds of
    // thing even though `y`'s written clause contains `A`.
    let src = r#"
package P {
	part def A;
	part def B :> A;
	part def Holder {
		end part x : A;
		end part y : A, B crosses x;
	}
}
"#;
    assert_eq!(
        sem073(src).len(),
        1,
        "the sets are B and A, got {:?}",
        sem073(src)
    );
}

#[test]
fn a_named_cross_feature_is_compared_by_its_type_not_its_name() {
    // `crosses x` names the cross feature, not a type. The earlier fallback compared the written
    // type text `B` with the string `x` and reported every model written this way.
    let src = r#"
package P {
	part def B;
	part def Holder {
		end part x : B;
		end part y : B crosses x;
	}
}
"#;
    assert!(
        sem073(src).is_empty(),
        "`x` is a feature typed `B`, got {:?}",
        sem073(src)
    );
}
