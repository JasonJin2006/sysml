//! `validateFeatureReferenceExpressionReferentIsFeature`: a name in value position names a value.
//!
//! The notation writes a bare name in value position as a `FeatureReferenceExpression`, and the
//! specification defines that as an expression "whose `result` is bound to a `referent` `Feature`".
//! So a name that resolves to a *definition* — a type, which nothing instantiates here — is not
//! something the value can denote.
//!
//! Both directions are proved: the rejected shapes are the ones the sentence rules out, and the
//! accepted ones are the spellings the corpus and the pilot use, including the callee of a call,
//! which names something callable and is judged elsewhere.

use sysml_semantics::{check, Severity};

fn sem002(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM002")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

fn reported(src: &str) -> bool {
    sem002(src)
        .iter()
        .any(|(_, _, m)| m.contains("validateFeatureReferenceExpressionReferentIsFeature"))
}

#[test]
fn a_dotted_reference_that_lands_on_a_type_is_reported() {
    // `v1.S` reaches a struct through `v1`'s type. It went unreported because the resolver behind
    // `result_of` walked `::` only: asked for `v1.S` it looked for a member literally called
    // `v1.S` and answered Unresolved, so every dotted reference in a value was invisible to this
    // rule (`FeatureChain_invalid.kerml.xt:61`).
    let src = r#"
package P {
    struct V {
        struct S;
        feature n : Integer;
    }
    feature v1 : V;
    feature x : Integer = v1.S;
}
"#;
    assert!(reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_dotted_reference_that_lands_on_a_feature_is_accepted() {
    let src = r#"
package P {
    struct V {
        feature n : Integer;
    }
    feature v1 : V;
    feature x : Integer = v1.n;
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_definition_used_as_a_value_is_rejected() {
    // `Wheel` is what a part is typed by; the model has no instance of it to read.
    let src = r#"
package P {
	part def Wheel;
	attribute x : Real = Wheel;
}
"#;
    assert!(reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_feature_used_as_a_value_is_accepted() {
    let src = r#"
package P {
	part def Wheel {
		attribute mass : Real;
	}
	part w : Wheel;
	attribute m : Real = w.mass;
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_callee_is_not_judged_as_a_value() {
    // `F(1)` names something callable. Whether it may be called is the invocation rules'
    // question, and reporting the callee here rejected `EngineEvaluation(...)` in the corpus.
    let src = r#"
package P {
	calc def F {
		in x : Real;
		return : Real = x;
	}
	attribute y : Real = F(1);
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_variant_used_as_a_value_is_accepted() {
    // A variant member is an alternative a configuration selects, so it denotes a value even
    // though the projection declares it like a definition.
    let src = r#"
package P {
	variation part def Config {
		variant part a;
	}
	part selected : Config = Config::a;
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}

#[test]
fn an_association_structure_used_as_a_value_is_rejected() {
    // `assoc struct` is `AssociationStructure` — one identity that is both an `Association` and a
    // `Structure`. Both supers are definitions and neither denotes a value, so the combined kind is
    // a type: `PairLink` is what a part is typed by, not something an attribute can read. The kind
    // was absent from the rule's definition list, so this expression was accepted as a feature
    // reference.
    let src = r#"
package P {
	part def A;
	assoc struct PairLink {
		end x : A;
		end y : A;
	}
	attribute z = PairLink;
}
"#;
    assert!(reported(src), "got {:?}", sem002(src));
}

#[test]
fn an_association_structure_feature_used_as_a_value_is_accepted() {
    // The nearest conforming model: the *usage* the structure types denotes a value, so reading its
    // end is legal even though the structure itself is not a value.
    let src = r#"
package P {
	part def A;
	assoc struct PairLink {
		end x : A;
		end y : A;
	}
	part linkage : PairLink;
	attribute z = linkage.x;
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}

#[test]
fn a_multiplicity_used_as_a_value_is_accepted() {
    // `multiplicity em [1..4]; ... = em;` is the pilot's `MultiplicityAlias_valid`, which states no
    // error: a multiplicity denotes a value, not a type.
    let src = r#"
package P {
	struct E {
		multiplicity em [1..4];
		feature v = em;
	}
}
"#;
    assert!(!reported(src), "got {:?}", sem002(src));
}
