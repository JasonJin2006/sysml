//! `validateMetadataFeatureMetaclass`, proved both ways.

use sysml_semantics::{check, Severity};

fn sem045(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM045")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_metadata_naming_a_part_is_rejected() {
    let src = r#"
package P {
	part def NotMeta;
	part def Holder { @NotMeta; part x; }
}
"#;
    let found = sem045(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateMetadataFeatureMetaclass")),
        "got {found:?}"
    );
}

#[test]
fn a_metadata_naming_a_metadata_definition_is_accepted() {
    let src = r#"
package P {
	metadata def Safety;
	part def Holder { @Safety; part x; }
}
"#;
    assert!(
        sem045(src).is_empty(),
        "a metadata definition is the right target"
    );
}

#[test]
fn a_specialization_of_a_metadata_definition_is_accepted() {
    let src = r#"
package P {
	metadata def Base;
	metadata def Special :> Base;
	part def Holder { @Special; part x; }
}
"#;
    assert!(
        sem045(src).is_empty(),
        "oclIsKindOf accepts a specialization, got {:?}",
        sem045(src)
    );
}

#[test]
fn the_report_names_the_definition_not_the_usage_itself() {
    // `@NotMeta` lowers to an element named `NotMeta` inside the holder, so a naive walk finds
    // the metadata itself and reports it as "a metadata naming a metadata".
    let src = r#"
package P {
	part def NotMeta;
	part def Holder { @NotMeta; part x; }
}
"#;
    let found = sem045(src);
    let (_, _, msg) = found.first().expect("one violation");
    assert!(
        msg.contains("a part def"),
        "the report must name the target's kind: {msg}"
    );
}

#[test]
fn a_metadata_body_member_that_redefines_nothing_is_rejected() {
    // `bad;` names a feature the metadata definition does not declare, so the body has no schema
    // for it. The pilot states this as "Must redefine an owning-type feature".
    let src = r#"
package P {
	metadata def A {
		attribute x : Integer;
	}
	attribute b {
		@A {
			bad;
		}
	}
}
"#;
    let found = sem045(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateMetadataFeatureBody")),
        "got {found:?}"
    );
}

#[test]
fn a_metadata_body_value_that_needs_an_evaluator_is_rejected() {
    // `x = ~3` states a conjugation where a value belongs, and an annotation is read as written.
    // A number on its own is a value, which is why the model beside it leaves `y = 3` alone.
    let src = r#"
package P {
	metadata def A {
		attribute x : Integer;
	}
	attribute b {
		@A {
			x = ~3;
		}
	}
}
"#;
    let found = sem045(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("cannot be evaluated at model level")),
        "got {found:?}"
    );
}

#[test]
fn a_metadata_body_member_naming_a_definition_feature_is_accepted() {
    // The bare `x = 3` spelling is the one the corpus and the pilot both write: the name is the
    // redefinition target, and `A` declares `x`. Reporting it would reject every conforming
    // annotation in the validation set.
    let src = r#"
package P {
	metadata def A {
		attribute x : Integer;
	}
	attribute b {
		@A {
			x = 3;
		}
	}
}
"#;
    let found = sem045(src);
    assert!(
        !found
            .iter()
            .any(|(_, _, m)| m.contains("validateMetadataFeatureBody")),
        "got {found:?}"
    );
}

#[test]
fn a_nested_body_is_judged_against_the_definitions_feature() {
    // `MetadataFeature_invalid.kerml.xt:71-75` and `MetadataUsage_Invalid.sysml.xt:87-91`: `u { v
    // = 1; bad; }` inside `@A`. The nested `u` lowers to an ordinary feature, not to a metadata
    // usage, so a walk over metadata usages never visited it — and judging its members against the
    // annotation's own `u` would find `bad` among them and call it declared. The schema is the
    // `u` that `A` declares.
    let src = r#"
package P {
	metadata def A {
		attribute x : Integer;
		attribute u {
			attribute v : Integer;
		}
	}
	attribute b {
		@A {
			x = 1;
			u {
				v = 1;
				bad;
			}
		}
	}
}
"#;
    let found = sem045(src);
    let bad: Vec<&(String, String, String)> = found
        .iter()
        .filter(|(_, at, _)| at.ends_with("::bad"))
        .collect();
    assert_eq!(
        bad.len(),
        1,
        "the nested `bad` must be reported, got {found:?}"
    );
    assert!(
        bad[0].2.contains("validateMetadataFeatureBody"),
        "got {bad:?}"
    );
    // The legal members of both bodies stay quiet.
    assert!(
        !found
            .iter()
            .any(|(_, at, _)| at.ends_with("::v") || at.ends_with("::x")),
        "`x` and `v` are features of the definition, got {found:?}"
    );
}
