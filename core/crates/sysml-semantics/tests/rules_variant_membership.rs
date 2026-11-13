//! `validateVariantMembershipOwningNamespace`: a variant belongs to a variation.

use sysml_semantics::{check, Severity};

fn sem054(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM054")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_variant_of_a_plain_definition_is_rejected() {
    let src = r#"
package P {
	part def Plain {
		variant part stray;
	}
}
"#;
    assert_eq!(sem054(src).len(), 1, "got {:?}", sem054(src));
}

#[test]
fn a_variant_of_a_variation_definition_is_accepted() {
    let src = r#"
package P {
	variation part def V {
		variant part a;
		variant part b;
	}
}
"#;
    assert!(
        sem054(src).is_empty(),
        "a variation owns its variants, got {:?}",
        sem054(src)
    );
}

#[test]
fn a_variation_usage_also_owns_variants() {
    let src = r#"
package P {
	part def Holder {
		variation part v : Holder;
		variant part a;
	}
}
"#;
    // The `variant part a` here belongs to `Holder`, not to `v`, because the notation puts it in
    // the enclosing body. `Holder` is not a variation, so this is one violation.
    assert_eq!(sem054(src).len(), 1, "got {:?}", sem054(src));
}

#[test]
fn the_report_names_the_owner_and_its_kind() {
    let src = r#"
package P {
	part def Plain {
		variant part stray;
	}
}
"#;
    let (_, _, msg) = sem054(src).into_iter().next().expect("one violation");
    assert!(msg.contains("Plain"), "must name the owner: {msg}");
    assert!(msg.contains("part def"), "must name its kind: {msg}");
}
