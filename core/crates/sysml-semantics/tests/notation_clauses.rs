//! `::` through a feature: an error in a value expression, accepted in a typing or a redefinition.
//!
//! The notation reaches a feature's members with `.`, so `f::a` in a value is "Must be an accessible
//! feature (use dot notation for nesting)" (`FeaturePath_Invalid.sysml.xt:11-12`). A typing or a
//! redefinition names its target as a qualified path, and the reference accepts a feature's member
//! there (`SimpleImportTestsFromOtherFile_Import3_FT.kerml.xt`, which states the model clean).

use sysml_resolve::{Index, Resolution};
use sysml_semantics::check;

/// The `(text, resolved)` of every reference a model makes.
fn reference_verdicts(src: &str) -> Vec<(String, bool)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    index
        .references(&loaded.roots)
        .into_iter()
        .map(|reference| {
            (
                reference.text,
                matches!(reference.target, Resolution::Found(_)),
            )
        })
        .collect()
}

/// The `SEM003` messages a model's values produce.
fn value_messages(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM003")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_typing_and_a_redefinition_may_reach_a_feature_member_with_colons() {
    let found = reference_verdicts(
        r#"package P {
	feature A { feature a1 {} }
	feature B { feature b : A; }
	feature Try : B {
		feature try : A::a1 redefines B::b;
	}
}"#,
    );
    for wanted in ["A::a1", "B::b"] {
        assert!(
            found.iter().any(|(text, ok)| text == wanted && *ok),
            "`{wanted}` is a qualified path a clause may name: {found:?}"
        );
    }
}

#[test]
fn a_value_that_reaches_a_feature_member_with_colons_is_reported() {
    // The same shape as the value form in `FeaturePath_Invalid`. The message names the path, so a
    // value that uses `.` instead is not reported.
    let found = value_messages(
        r#"package Q {
	part def F {
		part a : A;
	}
	part f : F;
	part def A {
		part g = f::a;
	}
}"#,
    );
    assert!(found.iter().any(|m| m.contains("f::a")), "{found:?}");
    let dotted = value_messages(
        r#"package Q {
	part def F {
		part a : A;
	}
	part f : F;
	part def A {
		part g = f.a;
	}
}"#,
    );
    assert!(dotted.iter().all(|m| !m.contains("f.a")), "{dotted:?}");
}
