//! `validateFeatureChainingFeatureNotOne`: a chain passes through at least two features.

use sysml_semantics::{check, Severity};

fn sem082(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM082")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_chain_of_one_feature_is_rejected() {
    // The pilot implementation's `feature v1_ chains v1;`.
    let src = r#"
package P {
	feature v1;
	feature v1_ chains v1;
}
"#;
    assert_eq!(sem082(src).len(), 1, "got {:?}", sem082(src));
}

#[test]
fn a_chain_through_a_typed_feature_is_accepted() {
    // `c.b` reads `b` out of what `c` is: `A` declares it, so the step is featured within the one
    // before it and `validateFeatureChainingFeatureConformance` has nothing to say.
    let src = r#"
package P {
	part def A {
		part b;
	}
	part c : A;
	part two chains c.b;
}
"#;
    assert!(sem082(src).is_empty(), "got {:?}", sem082(src));
}

#[test]
fn a_chain_the_previous_step_does_not_feature_is_rejected() {
    // Both features are declared beside the chain, so `b` resolves — but `a` is untyped and does
    // not feature it. This test used to assert the opposite, on the reading that the pilot's
    // silence about the two-segment case (`FeatureChain_invalid.kerml.xt` asserts only the
    // one-segment one) meant it was legal. Silence is not a statement, and the OCL is explicit:
    // `chainingFeature->at(2).isFeaturedWithin(chainingFeature->at(1))`.
    let src = r#"
package P {
	feature a;
	feature b;
	feature two chains a.b;
}
"#;
    assert_eq!(sem082(src).len(), 1, "got {:?}", sem082(src));
}

#[test]
fn a_chain_into_a_type_that_lacks_the_name_is_rejected() {
    // The step resolves against the enclosing scope when the previous one provides nothing, so
    // `b.m` finds the package's `m` — a feature `W` does not declare, and therefore one `b` does
    // not feature. The chain used to resolve to it in silence.
    let src = r#"
package P {
	attribute m;
	part def W;
	part b : W;
	part def Q {
		part x chains b.m;
	}
}
"#;
    assert_eq!(sem082(src).len(), 1, "got {:?}", sem082(src));
}

#[test]
fn a_feature_with_no_chain_is_not_judged() {
    let src = r#"
package P {
	feature a;
	feature plain;
}
"#;
    assert!(sem082(src).is_empty(), "got {:?}", sem082(src));
}

#[test]
fn chains_reaches_the_tree() {
    // The word had no arm in the relation parser, so `chains v1` left the path in the body.
    let src = r#"
package P {
	feature v1;
	feature v1_ chains v1;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let v = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .find(|e| e.name == "v1_")
        .expect("v1_");
    assert_eq!(v.chains, vec!["v1".to_string()]);
}

#[test]
fn a_chain_through_itself_is_rejected() {
    // The pilot implementation's `feature h chains f.h;` — the last segment is the feature itself.
    let src = r#"
package P {
	feature f;
	feature h chains f.h;
}
"#;
    assert!(
        sem082(src)
            .iter()
            .any(|(_, _, msg)| msg.contains("validateFeatureChainingFeaturesNotSelf")),
        "got {:?}",
        sem082(src)
    );
}

#[test]
fn a_chain_through_a_differently_named_feature_is_accepted() {
    // `f` is typed so the step resolves as a member of its type: the question here is whether `i`
    // names itself, and an untyped `f` would answer a second question (is `h` featured within `f`)
    // at the same time.
    let src = r#"
package P {
	part def F {
		feature h;
	}
	part f : F;
	feature i chains f.h;
}
"#;
    assert!(
        sem082(src).is_empty(),
        "`i` chains through `h`, not through itself, got {:?}",
        sem082(src)
    );
}

#[test]
fn both_official_chain_assertions_are_reported() {
    // Type_Relationships_invalid.kerml carries exactly these two.
    let src = r#"
package P {
	feature f;
	feature h chains f.h;
	feature g chains f;
}
"#;
    let found = sem082(src);
    assert_eq!(found.len(), 2, "got {found:?}");
}
