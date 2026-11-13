//! `validateFeatureCrossFeatureSpecialization`: a redefined end's cross feature specializes its own.
//!
//! `crossFeature <> null implies ownedRedefinition.redefinedFeature.crossFeature->forAll(f |
//! f <> null implies crossFeature.specializes(f))`. Both directions are proved, and the second test
//! is the one that caught a real gap: `feature c subsets b` is a `Specialization`
//! (KerML 8.3.3.3.2), and until the subtype walk included `subsets`, `c` looked unrelated to `b`
//! and a conforming model was reported.

use sysml_semantics::{check, Severity};

fn reported(src: &str) -> bool {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM073")
        .any(|i| {
            i.message
                .contains("validateFeatureCrossFeatureSpecialization")
        })
}

#[test]
fn a_cross_feature_that_does_not_specialize_the_one_it_replaces_is_rejected() {
    // `A2::y` crosses `C1::a`; `A1::y`, which it redefines, crosses `C2::b`. `C1::a` is not a
    // subtype of `C2::b`. This is the pilot's own assertion at `CrossFeatures_invalid.kerml.xt:47`.
    let src = r#"
package P {
	class C1 { feature a : C2; }
	class C2 { feature b : C1; }
	assoc A1 {
		end x : C1 crosses y.b;
		end y : C2 crosses y.b;
	}
	assoc A2 :> A1 {
		end x : C1 crosses y.b;
		end y : C2 crosses x.a;
	}
}
"#;
    assert!(reported(src));
}

#[test]
fn a_cross_feature_reached_through_a_subsetting_is_accepted() {
    // `c subsets b` makes `c` a subtype of `b`, so crossing `c` where the redefined end crosses
    // `b` conforms. Without `subsets` in the subtype graph this model was reported.
    let src = r#"
package P {
	class C1 { feature a : C2; }
	class C2 { feature b : C1; feature c subsets b; }
	assoc A1 {
		end x : C1 crosses y.b;
		end y : C2 crosses y.b;
	}
	assoc A2 :> A1 {
		end x : C1 crosses y.c;
		end y : C2 crosses y.b;
	}
}
"#;
    assert!(!reported(src));
}

#[test]
fn an_end_that_crosses_nothing_says_nothing_to_compare() {
    let src = r#"
package P {
	class C1;
	class C2 { feature b : C1; }
	assoc A1 {
		end x : C1;
		end y : C2 crosses y.b;
	}
	assoc A2 :> A1 {
		end x : C1;
		end y : C2 crosses y.b;
	}
}
"#;
    assert!(!reported(src));
}
