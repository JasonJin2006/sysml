//! A variation's members are all variants —
//! `validateDefinitionVariationOwnedFeatureMembership` and
//! `validateUsageVariationOwnedFeatureMembership`.

use sysml_semantics::{check, Severity};

fn sem071(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM071")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn an_ordinary_member_of_a_variation_is_rejected() {
    let src = r#"
package P {
	variation part def V {
		variant part a;
		part stray;
	}
}
"#;
    assert_eq!(sem071(src).len(), 1, "got {:?}", sem071(src));
}

#[test]
fn all_variants_is_accepted() {
    let src = r#"
package P {
	variation part def V {
		variant part a;
		variant part b;
	}
}
"#;
    assert!(sem071(src).is_empty(), "got {:?}", sem071(src));
}

#[test]
fn a_variant_with_no_kind_keyword_is_still_a_variant() {
    // §7.6.7 allows omitting the kind keyword. Before the parser handled it, the bare form
    // lowered to an attribute and every variation written that way looked like it held a
    // non-variant.
    let src = r#"
package P {
	variation requirement Choice : R {
		variant high;
		variant normal;
	}
	requirement def R;
}
"#;
    assert!(
        sem071(src).is_empty(),
        "a bare variant is a variant, got {:?}",
        sem071(src)
    );
}

#[test]
fn a_non_variation_is_not_judged() {
    let src = r#"
package P {
	part def Plain {
		part x;
	}
}
"#;
    assert!(sem071(src).is_empty(), "got {:?}", sem071(src));
}
