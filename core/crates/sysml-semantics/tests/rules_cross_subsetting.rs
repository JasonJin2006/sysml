//! `validateCrossSubsettingCrossingFeature`: a feature that crosses another is an end.

use sysml_semantics::{check, Severity};

fn sem067(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM067")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_crossing_that_is_not_an_end_is_rejected() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		part bad crosses A;
	}
}
"#;
    assert_eq!(sem067(src).len(), 1, "got {:?}", sem067(src));
}

#[test]
fn an_end_that_crosses_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		end part ok crosses A;
	}
}
"#;
    assert!(sem067(src).is_empty(), "got {:?}", sem067(src));
}

#[test]
fn a_feature_with_no_crossing_is_not_judged() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		part plain : A;
	}
}
"#;
    assert!(sem067(src).is_empty(), "got {:?}", sem067(src));
}

fn identity_errors(src: &str) -> Vec<(String, String, String)> {
    sem067(src)
        .into_iter()
        .filter(|(_, _, message)| message.contains("validateFeatureCrossingSpecialization"))
        .collect()
}

#[test]
fn an_owned_cross_feature_is_not_a_different_feature_of_the_same_type() {
    let src = r#"
package P {
    class C;
    class D { feature cross : C; }
    assoc A {
        end cross [0..1] feature x : C crosses y.cross;
        end y : D;
    }
}
"#;
    let found = identity_errors(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].1, "P::A::x");
    assert!(found[0].2.contains("P::A::x::cross"));
    assert!(found[0].2.contains("P::D::cross"));
}

#[test]
fn an_import_can_resolve_back_to_the_same_owned_cross_feature() {
    let src = r#"
package P {
    class C;
    class D;
    assoc A {
        end x : C { public import y::y1; }
        end y : D crosses x.y1 {
            member feature y1 [0..1] featured by C;
        }
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let y = index.qualified.iter().position(|q| q == "P::A::y").unwrap();
    let owned = sysml_semantics::derived::owned_cross_feature(&index, y).unwrap();
    assert_eq!(index.qualified[owned], "P::A::y::y1");
    assert_eq!(
        sysml_semantics::derived::cross_feature(&index, y),
        Some(owned)
    );
    assert!(identity_errors(src).is_empty());
}

#[test]
fn an_owned_only_end_has_no_explicit_identity_conflict() {
    let src = r#"
package P {
    class C;
    assoc A {
        end local [0..1] feature x : C;
        end y : C { member feature back [0..1] featured by C; }
    }
}
"#;
    assert!(identity_errors(src).is_empty());
}

#[test]
fn an_anonymous_owned_cross_feature_has_an_identity() {
    let src = r#"
package P {
    class C;
    class D { feature cross : C; }
    assoc A {
        end [0..1] feature x : C crosses y.cross;
        end y : D;
    }
}
"#;
    let found = identity_errors(src);
    assert_eq!(found.len(), 1, "got {found:?}");
}

#[test]
fn an_unresolved_cross_feature_does_not_imply_a_different_identity() {
    let src = r#"
package P {
    class C;
    assoc A {
        end own [0..1] feature x : C crosses y.missing;
        end y : C;
    }
}
"#;
    assert!(identity_errors(src).is_empty());
}

#[test]
fn an_unowned_cross_feature_has_no_owned_identity_constraint() {
    let src = r#"
package P {
    class C;
    class D { feature cross : C; }
    assoc A {
        end x : C crosses y.cross;
        end y : D;
    }
}
"#;
    assert!(identity_errors(src).is_empty());
}

#[test]
fn a_non_end_on_the_same_source_line_is_not_an_opposite_end() {
    let src = r#"
package P {
    class C { feature cross : C; }
    assoc A {
        end x : C; feature unrelated : C;
        end y : C crosses unrelated.cross;
    }
}
"#;
    let found = sem067(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("validateCrossSubsettingCrossedFeature"));
}

// -- validateCrossSubsettingCrossedFeature: the chain starts at the other end -------

#[test]
fn a_crossing_that_starts_at_itself_is_rejected() {
    // `crosses y.b` chains through `y`; on a binary association `y` is the opposite end for `x`
    // and the end itself for `y`. The pilot states this at `CrossFeatures_invalid.kerml.xt:41`.
    let src = r#"
package P {
	class C1 { feature a : C2; }
	class C2 { feature b : C1; }
	assoc A1 {
		end x : C1 crosses y.b;
		end y : C2 crosses y.b;
	}
}
"#;
    let found = sem067(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateCrossSubsettingCrossedFeature")),
        "got {found:?}"
    );
}

#[test]
fn a_crossing_through_the_opposite_end_is_accepted() {
    let src = r#"
package P {
	class C1 { feature a : C2; }
	class C2 { feature b : C1; }
	assoc A1 {
		end x : C1 crosses y.b;
		end y : C2 crosses x.a;
	}
}
"#;
    let found = sem067(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateCrossSubsettingCrossedFeature")),
        "got {found:?}"
    );
}

#[test]
fn a_single_name_crossing_states_no_chain() {
    // `crosses T` names one thing, so `chainingFeatures->size() = 2` fails and the OCL says nothing.
    let src = r#"
package P {
	class C1;
	class C2;
	assoc A1 {
		end x : C1 crosses C2;
		end y : C2;
	}
}
"#;
    let found = sem067(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateCrossSubsettingCrossedFeature")),
        "got {found:?}"
    );
}
