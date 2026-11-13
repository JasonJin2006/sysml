//! The effective `isComposite` the lowering fills, and the rules that read it.
//!
//! `deriveUsageIsReference` is `isReference = not isComposite` (`sysmlv2-model`
//! `generated.rs:8957-8961`), and no SysML usage can write the word `composite`
//! (`SysML.xtext:556-566`), so a tree that stored only the written word answered "not composite"
//! for every usage and left six rules unable to reject anything. These tests pin the value each
//! shape gets, both directions, and the two rules that depend on it.

use sysml_model::Element;
use sysml_resolve::Index;
use sysml_semantics::{check, Severity};

fn loaded(src: &str) -> sysml_syntax::Loaded {
    sysml_syntax::load_project(&[src], true).expect("model parses")
}

fn find<'a>(roots: &'a [Element], name: &str) -> &'a Element {
    fn go<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
        if el.name == name {
            return Some(el);
        }
        el.children.iter().find_map(|c| go(c, name))
    }
    roots
        .iter()
        .find_map(|r| go(r, name))
        .unwrap_or_else(|| panic!("no element named `{name}`"))
}

/// Whether the element named `name` came out composite.
fn composite(src: &str, name: &str) -> bool {
    find(&loaded(src).roots, name).is_composite
}

fn sem031(src: &str) -> Vec<String> {
    let l = loaded(src);
    check(&l.roots, &l.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM031")
        .map(|i| i.message.clone())
        .collect()
}

// -- the derivation ---------------------------------------------------------

#[test]
fn a_part_owned_by_a_type_is_composite() {
    let src = r#"
package P {
	part def Wheel;
	part def Vehicle {
		part wheel : Wheel;
	}
}
"#;
    assert!(
        composite(src, "wheel"),
        "a part of a vehicle is one of its parts"
    );
}

#[test]
fn a_ref_feature_is_not_composite() {
    let src = r#"
package P {
	part def Wheel;
	part def Vehicle {
		ref wheel : Wheel;
	}
}
"#;
    assert!(!composite(src, "wheel"), "`ref` states the opposite");
}

#[test]
fn a_usage_at_package_level_is_not_composite() {
    // A package is a `Namespace`, not a `Type`, so the usage has no owning type: there is nothing
    // it could be a part of. This is also the shape `validateUsageIsReferential` reports on.
    let src = r#"
package P {
	part def Wheel;
	part wheel : Wheel;
}
"#;
    assert!(!composite(src, "wheel"), "a package is not made of parts");
}

#[test]
fn a_directed_feature_is_not_composite() {
    let src = r#"
package P {
	action def Move {
		in distance : Real;
	}
}
"#;
    assert!(
        !composite(src, "distance"),
        "a parameter's value is supplied by its caller, not owned by the behaviour"
    );
}

#[test]
fn an_attribute_usage_is_not_composite() {
    let src = r#"
package P {
	part def Vehicle {
		attribute mass : Real;
	}
}
"#;
    assert!(
        !composite(src, "mass"),
        "an attribute usage is always referential"
    );
}

#[test]
fn a_port_is_composite_only_inside_a_port() {
    let inside = r#"
package P {
	port def Outer {
		port inner : Outer;
	}
}
"#;
    let under_part = r#"
package P {
	port def Outer;
	part def Holder {
		port p : Outer;
	}
}
"#;
    assert!(
        composite(inside, "inner"),
        "a port nested in a port is part of it"
    );
    assert!(
        !composite(under_part, "p"),
        "a port on a part is referential"
    );
}

#[test]
fn an_end_is_not_composite() {
    let src = r#"
package P {
	part def Wheel;
	assoc Link {
		end w : Wheel;
	}
}
"#;
    assert!(
        !composite(src, "w"),
        "validateFeatureEndNotDerivedAbstractCompositeOrPortion: an end is not composite"
    );
}

#[test]
fn the_written_word_decides_against_the_default() {
    let src = r#"
package P {
	part def Wheel;
	part def Vehicle {
		composite part first : Wheel;
		ref composite part second : Wheel;
	}
}
"#;
    assert!(
        composite(src, "first"),
        "`composite` states the value outright"
    );
    assert!(
        !composite(src, "second"),
        "`ref` decides first, so the co-written `composite` does not make it one"
    );
}

#[test]
fn a_condition_or_a_relationship_is_not_a_part() {
    // `assert constraint` is a condition on the value and `exhibit` names a state the owner takes
    // part in; neither adds a value the owner is made of. The official models state both:
    // `attribute e: Real { assert constraint { ... } }` (`15_01-Constants.sysml:24`) and
    // `exhibit vehicleStates.on` inside a snapshot (`6-Individual and Snapshots.sysml:113`).
    let src = r#"
package P {
	attribute def Real2 {
		attribute value : Real;
		assert constraint { value > 0 }
	}
	part def Vehicle {
		exhibit operations;
	}
}
"#;
    let l = loaded(src);
    let assert_el = find(&l.roots, "assert");
    assert!(
        !assert_el.is_composite,
        "an assertion about an attribute is not a part of it"
    );
    // `exhibit operations;` declares nothing — the notation gives it a reference and no name — so
    // the element is found by its kind.
    let exhibit = l
        .roots
        .iter()
        .flat_map(|r| std::iter::once(r).chain(r.children.iter().flat_map(|c| c.children.iter())))
        .find(|el| el.performs.as_deref() == Some("operations"))
        .expect("the exhibit");
    assert!(!exhibit.is_composite, "an exhibited state is not a part");
}

#[test]
fn a_part_inside_an_attribute_definition_is_composite() {
    // `validateAttributeDefinitionFeatures: feature->forAll(not isComposite)` — the rule can only
    // reject this if the default makes the part composite.
    let src = r#"
package P {
	part def Wheel;
	attribute def Mass {
		part bad : Wheel;
	}
}
"#;
    assert!(
        composite(src, "bad"),
        "an attribute has no parts, so a part is composite"
    );
    let found = sem031(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("every feature of an attribute"),
        "got {found:?}"
    );
}

// -- featuring types --------------------------------------------------------

#[test]
fn the_owning_type_features_the_feature() {
    // `Feature::owningType subsets featuringType` (`KerML.kerml:166`).
    let l = loaded(
        r#"
package P {
	part def Vehicle {
		part wheel : Wheel;
	}
	part def Wheel;
}
"#,
    );
    let index = Index::build(&l.roots);
    let vehicle = find(&l.roots, "Vehicle").id;
    let wheel = find(&l.roots, "wheel").id;
    assert_eq!(
        index.featuring_types_of(wheel),
        vec![vehicle],
        "the type that owns a feature is among its featuring types"
    );
    assert!(index.features(wheel, vehicle));
}

#[test]
fn a_package_level_usage_has_no_featuring_type() {
    let l = loaded(
        r#"
package P {
	part def Wheel;
	part wheel : Wheel;
}
"#,
    );
    let index = Index::build(&l.roots);
    let wheel = find(&l.roots, "wheel").id;
    assert!(
        index.featuring_types_of(wheel).is_empty(),
        "a package is not a type and features nothing"
    );
}

// -- validateUsageIsReferential --------------------------------------------

#[test]
fn a_composite_usage_nothing_features_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	composite part wheel : Wheel;
}
"#;
    let found = sem031(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("nothing features it"), "got {found:?}");
}

#[test]
fn a_composite_usage_with_a_featuring_type_is_accepted() {
    // The antecedent is false: the owning type features it, so it may be composite.
    let src = r#"
package P {
	part def Wheel;
	part def Vehicle {
		composite part wheel : Wheel;
	}
}
"#;
    assert!(sem031(src).is_empty(), "got {:?}", sem031(src));
}

#[test]
fn a_plain_usage_nothing_features_is_accepted() {
    // Not composite, so the consequent holds and the disjuncts do not matter.
    let src = r#"
package P {
	part def Wheel;
	part wheel : Wheel;
}
"#;
    assert!(sem031(src).is_empty(), "got {:?}", sem031(src));
}

#[test]
fn a_composite_directed_feature_is_rejected() {
    let src = r#"
package P {
	action def Move {
		composite in distance : Real;
	}
}
"#;
    let found = sem031(src);
    assert!(
        found.iter().any(|m| m.contains("it is directed")),
        "a directed feature that states `composite` contradicts its own direction: {found:?}"
    );
}
