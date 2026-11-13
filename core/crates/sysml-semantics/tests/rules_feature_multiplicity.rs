//! The `feature-multiplicity` group: `validateFeatureEndMultiplicity`, and why the other seven
//! rules of the group cannot be decided from the element tree.
//!
//! The rule that runs is proved by models that violate it — `[0..1]`, `[4]`, `[*]`, `[1..*]`,
//! `[0..*]`, `[2..4]` and `[*..1]` on an `end` feature — and by the legal bounds that must stay
//! silent (`[1]`, `[1..1]`, no bound at all, and a bound written as an expression). Asserting only
//! the legal side would not distinguish "the rule runs" from "the rule is absent".
//!
//! The seven dropped rules read fields [`sysml_model::Element`] does not carry; the tests at the
//! end record each blocker as an equality between two declarations that differ only in the
//! modifier, the shape `tests/rules_variation_modifiers.rs` uses. Those equalities hold while the
//! modifier is discarded and start failing the moment a field carries it, which is the point.
//!
//! The `issues` helper is the one from `tests/rules.rs`. `reported` calls the module directly so a
//! test does not pass because the module was never wired into `rules::all`.
//!
//! Four tests pin a lowering defect this module inherits rather than hides: a *named* owned cross
//! feature is lowered as the `end` element and carries the cross multiplicity, so
//! `end theCauses [*] occurrence theCause :> causes` reads as an end bounded `[*]`. The module
//! documentation says why no sound guard exists in this tree.

use sysml_model::Element;
use sysml_resolve::Index;
use sysml_semantics::rules::feature_multiplicity;
use sysml_semantics::{check, Severity};

fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

/// What the module under test reports, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    feature_multiplicity::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM021` a full `check` produces.
fn sem021(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM021")
        .collect()
}

/// The named element `name`, wherever it sits in the tree.
fn find<'a>(roots: &'a [Element], name: &str) -> &'a Element {
    fn go<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
        if el.name == name {
            return Some(el);
        }
        el.children.iter().find_map(|c| go(c, name))
    }
    roots
        .iter()
        .find_map(|root| go(root, name))
        .unwrap_or_else(|| panic!("no element named `{name}` in the model"))
}

/// An element's whole shape with spans and line cleared, so two declarations that differ only by
/// a modifier the lowering discards compare equal.
fn shape(el: &Element) -> String {
    fn clear(el: &mut Element) {
        el.id = 0;
        el.name_span = None;
        el.value_span = None;
        el.type_span = None;
        el.ref_spans.clear();
        el.line = 0;
        for child in &mut el.children {
            clear(child);
        }
    }
    let mut copy = el.clone();
    clear(&mut copy);
    format!("{copy:?}")
}

// -- validateFeatureEndMultiplicity ------------------------------------------
//
// `isEnd implies multiplicities()...->exists(hasBounds(1,1))` — an end must have multiplicity
// 1..1. A written bound that is not 1..1 is what the rule can be wrong about; the three shapes
// the text settles (an integer, `*`, and a pair of those) are the ones judged.

#[test]
fn an_end_bounded_zero_or_one_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[0..1];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`[0..1]`")),
        "an `end` bounded 0..1 must be reported under SEM021, got {found:?}"
    );
}

#[test]
fn an_end_with_a_fixed_non_one_bound_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels [4] : Wheel;
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`[4]`")),
        "an `end` bounded [4] must be reported, got {found:?}"
    );
}

#[test]
fn an_end_with_an_unbounded_limit_is_rejected() {
    let src = r#"
package P {
	part def Situation;
	connection def Violation {
		end situations : Situation[*];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Violation::situations" && msg.contains("`[*]`")),
        "an `end` bounded [*] must be reported, got {found:?}"
    );
}

#[test]
fn an_end_bounded_one_to_many_is_rejected() {
    let src = r#"
package P {
	part def Occurrence;
	connection def HappensDuring {
		end longerOccurrence : Occurrence[1..*];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::HappensDuring::longerOccurrence"
                && msg.contains("`[1..*]`")),
        "an `end` bounded 1..* must be reported, got {found:?}"
    );
}

#[test]
fn an_end_bounded_zero_to_many_is_rejected() {
    // `[*]` and `[0..*]` are different ranges that both fail: the upper bound is `*`. This test
    // exists because a check that only compared the *lower* bound would accept both.
    let src = r#"
package P {
	part def Item;
	connection def Touches {
		end touched : Item[0..*];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Touches::touched" && msg.contains("`[0..*]`")),
        "an `end` bounded 0..* must be reported, got {found:?}"
    );
}

#[test]
fn an_end_with_a_narrowed_range_not_containing_one_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[2..4];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`[2..4]`")),
        "an `end` bounded 2..4 must be reported, got {found:?}"
    );
}

#[test]
fn an_end_whose_lower_bound_is_unbounded_is_rejected() {
    // `hasBounds(1,1)` accepts a null lower only through `lower = upper`. A `*` lower is neither
    // 1 nor null, so `[*..1]` is not 1..1 even though its upper bound is 1.
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[*..1];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`[*..1]`")),
        "an `end` bounded *..1 must be reported, got {found:?}"
    );
}

#[test]
fn an_end_bounded_exactly_one_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[1];
		end hubs : Wheel[1..1];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found.is_empty(),
        "`hasBounds(1,1)` accepts both `[1]` and `[1..1]`, got {found:?}"
    );
}

#[test]
fn an_end_with_no_written_bound_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel;
	}
}
"#;
    let found = sem021(src);
    assert!(
        found.is_empty(),
        "an unwritten bound is the shape the shipped library uses, got {found:?}"
    );
}

#[test]
fn the_standard_library_ends_with_no_written_bound_are_accepted() {
    // `Systems Library/Connections.sysml:43-44` writes `end source: Anything` with no bound.
    // Rejecting it would reject the library every project imports, so this shape is pinned here
    // rather than left to the corpus run.
    let src = r#"
package P {
	part def Anything;
	abstract connection def BinaryConnection {
		end source : Anything;
		end target : Anything;
	}
}
"#;
    assert!(
        sem021(src).is_empty(),
        "an end with no bound must not be reported"
    );
}

#[test]
fn a_bound_written_as_an_expression_is_left_undecided() {
    let src = r#"
package P {
	part def Wheel;
	attribute n = 1;
	connection def Fitting {
		end wheels : Wheel[n];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found.is_empty(),
        "`[n]` needs the evaluator, and guessing would reject a model that may be right, got {found:?}"
    );
}

#[test]
fn a_bounded_feature_that_is_not_an_end_is_not_judged() {
    let src = r#"
package P {
	part def Wheel;
	part def Car {
		part wheels : Wheel[0..*];
		attribute tag : Wheel[2];
	}
}
"#;
    let found = sem021(src);
    assert!(
        found.is_empty(),
        "the rule is bounded to `isEnd` features, got {found:?}"
    );
}

#[test]
fn the_report_names_the_rule_by_its_own_code() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[0..1];
	}
}
"#;
    let found = reported(src);
    assert_eq!(found.len(), 1, "one violating end, one report: {found:?}");
    assert_eq!(found[0].0, "SEM021", "the group's code: {found:?}");
    assert_eq!(
        found[0].1, "P::Fitting::wheels",
        "qualified name: {found:?}"
    );
    assert_eq!(
        found[0].2, "an `end` feature must have multiplicity 1..1, but `[0..1]` is declared",
        "{found:?}"
    );
}

#[test]
fn every_violating_end_in_one_model_is_reported() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end left : Wheel[0..1];
		end right : Wheel[1..*];
		end back : Wheel[2];
		end spare : Wheel[1..1];
		end plain : Wheel;
	}
}
"#;
    let mut names: Vec<String> = sem021(src).into_iter().map(|(_, at, _)| at).collect();
    names.sort();
    assert_eq!(
        names,
        ["P::Fitting::back", "P::Fitting::left", "P::Fitting::right"],
        "exactly the three ends that are not 1..1"
    );
}

// -- the lowering defect this module inherits -------------------------------
//
// These do not assert that the behaviour is right. They pin what the element tree currently says,
// so a change in `lower.rs` that starts modelling an end's owned cross feature is noticed here
// instead of silently changing what SEM021 reports.

#[test]
fn a_named_owned_cross_feature_is_kept_off_the_end() {
    // §7.13.2: `end theCauses [*] occurrence theCause :> causes` declares a cross feature
    // `theCauses` and an end `theCause`. The lowering used to mark the cross feature `is_end` and
    // keep no end element, so this module read `[*]` as the end's own bound. The prefix run now
    // reads the name and the bound and the end keeps its own (unstated) multiplicity.
    let src = r#"
package P {
	part def Situation;
	connection def Causation {
		end theCauses [*] occurrence theCause :> causes
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let conn = find(&loaded.roots, "Causation");
    assert_eq!(conn.children.len(), 1, "one end: {:?}", conn.children);

    let end = &conn.children[0];
    assert_eq!(end.name, "theCause", "the end keeps its own name");
    assert!(end.is_end, "the marker belongs to the end");
    assert_eq!(end.multiplicity, None, "the end itself is 1..1");
    assert_eq!(
        end.cross_name.as_deref(),
        Some("theCauses"),
        "the cross feature's name is not the end's"
    );
    assert_eq!(
        end.cross_multiplicity.as_deref(),
        Some("[*]"),
        "the cross bound is not the end's"
    );
}

#[test]
fn the_specifications_own_cross_feature_example_is_not_reported() {
    // The `HubDeviceConnection` figure on page 69 of `research/corpus/sysml2-spec.pdf` writes
    // `end connectingHub [0..1] ordered part hub : Hub` and `end connectedDevices [1..*] part
    // device : Device`, and displays the bounds as *cross* multiplicities while the ends
    // themselves are 1..1.
    //
    // Both used to be reported, because the bound landed on the end. The prefix run now reads the
    // name and the bound, the head files them as `cross_name` and `cross_multiplicity`, and the
    // end keeps its own (unstated, therefore 1..1) multiplicity.
    let src = r#"
package P {
	part def Hub;
	part def Device;
	connection def HubDeviceConnection {
		end connectingHub [0..1] ordered part hub : Hub;
		end connectedDevices [1..*] part device : Device;
		attribute bandwidth : Real;
	}
}
"#;
    assert!(
        sem021(src).is_empty(),
        "the bounds belong to the cross features, got {:?}",
        sem021(src)
    );
}

#[test]
fn a_typed_cross_feature_and_an_end_with_the_same_bound_are_different_models() {
    // `end cross : X[0..1] part hub : Hub` is one end, `hub`, whose owned crossing feature is
    // `cross : X[0..1]` (`OwnedCrossingFeature : BasicFeaturePrefix FeatureDeclaration`,
    // `KerML.xtext:533-535`). `end cross : X[0..1]; part hub : Hub` is an end `cross` with its own
    // bound beside an ordinary part. This test used to assert the two lowered identically: the
    // parser stopped after `X[0..1]` and silently turned `part hub : Hub` into a sibling.
    let with_cross = r#"
package P {
	part def X;
	part def Hub;
	connection def C {
		end cross : X[0..1] part hub : Hub;
	}
}
"#;
    let with_own_bound = r#"
package P {
	part def X;
	part def Hub;
	connection def C {
		end cross : X[0..1];
		part hub : Hub;
	}
}
"#;
    let a = sysml_syntax::load_project(&[with_cross], true).expect("model parses");
    let b = sysml_syntax::load_project(&[with_own_bound], true).expect("model parses");

    let hub = find(&a.roots, "hub");
    assert!(hub.is_end, "`hub` is the end the cross feature belongs to");
    let cross = find(&a.roots, "cross");
    assert_eq!(cross.kind, sysml_model::Kind::KermlFeature);
    assert!(
        !cross.is_end,
        "the cross feature is owned by the end, not an end itself"
    );
    assert_eq!(cross.type_ref.as_deref(), Some("X"));
    assert_eq!(cross.multiplicity.as_deref(), Some("[0..1]"));
    assert!(
        hub.children.iter().any(|child| child.name == "cross"),
        "the cross feature is a member of `hub`"
    );

    let own = find(&b.roots, "cross");
    assert!(own.is_end, "written with `;`, `cross` is the end");
    assert_eq!(own.multiplicity.as_deref(), Some("[0..1]"));
    assert!(!find(&b.roots, "hub").is_end, "and `hub` an ordinary part");
    assert_ne!(shape(cross), shape(own));
}

#[test]
fn a_cross_feature_bound_is_not_reported_as_the_ends() {
    // The same model as above, asked the question the rule asks. `theCauses [*]` bounds the cross
    // feature; the end it belongs to is 1..1 and says nothing against the rule. This used to be
    // the module's stated false positive, and it closes by modelling the cross feature, not by
    // weakening the check — a genuine `end a : T[*]` still has the bound on the end and is still
    // reported (`an_end_bounded_zero_to_many_is_rejected`).
    let src = r#"
package P {
	part def Situation;
	connection def Causation {
		end theCauses [*] occurrence theCause :> causes
	}
}
"#;
    let found = sem021(src);
    assert!(
        found.is_empty(),
        "the cross bound is not the end's, got {found:?}"
    );
}

#[test]
fn an_anonymous_cross_feature_keeps_its_end_and_its_bound_apart() {
    // `end [0..1] part hub : Hub` is the specification's own spelling. The bound belongs to the
    // owned cross feature: the 2024-12 pilot release notes say "with the introduction of cross
    // features, the multiplicity of end features is now required to be 1..1… in either case, the
    // multiplicity should be moved to the position of a cross multiplicity".
    //
    // The lowering used to read `end` as a name and produce two siblings, neither an end. It now
    // keeps one end, with the bound on `cross_multiplicity` and the end's own multiplicity unstated.
    let src = r#"
package P {
	part def Hub;
	connection def HubDeviceConnection {
		end [0..1] part hub : Hub;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let conn = find(&loaded.roots, "HubDeviceConnection");
    assert_eq!(
        conn.children.len(),
        1,
        "one end, not a member called `end` beside it: {:?}",
        conn.children
    );
    let end = &conn.children[0];
    assert_eq!(end.name, "hub");
    assert!(end.is_end, "the marker survives the bound");
    assert_eq!(end.multiplicity, None, "the end itself is 1..1");
    assert_eq!(
        end.cross_multiplicity.as_deref(),
        Some("[0..1]"),
        "the bound belongs to the owned cross feature"
    );
    assert!(
        sem021(src).is_empty(),
        "an end whose own multiplicity is unstated is 1..1, so nothing is reported"
    );
}

// -- the seven rules this module cannot decide -------------------------------

#[test]
fn an_abstract_part_keeps_its_modifier() {
    let src = r#"
package P {
	part def X;
	part def H { part f : X; }
	part def H2 { abstract part f : X; }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let plain = find(&loaded.roots, "H").children.first().expect("f");
    let abstract_part = find(&loaded.roots, "H2").children.first().expect("f");
    assert!(
        !plain.modifiers.is_abstract,
        "an unmodified part is not abstract"
    );
    assert!(
        abstract_part.modifiers.is_abstract,
        "`abstract part f : X` must keep isAbstract on the element:          validateFeatureEndNotDerivedAbstractCompositeOrPortion reads it"
    );
}

#[test]
fn derived_portion_and_snapshot_parts_keep_their_modifiers() {
    let src = r#"
package P {
	part def X;
	part def H2 { derived part f : X; }
	part def H3 { portion part f : X; }
	part def H4 { snapshot part f : X; }
	part def H5 { timeslice part f : X; }
	part def H6 { composite part f : X; }
	part def H7 { ref part f : X; }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let get = |owner: &str| {
        find(&loaded.roots, owner)
            .children
            .first()
            .expect("f")
            .modifiers
            .clone()
    };
    assert!(get("H2").is_derived, "`derived` must survive lowering");
    assert_eq!(get("H3").portion.as_deref(), Some("portion"));
    assert_eq!(get("H4").portion.as_deref(), Some("snapshot"));
    assert_eq!(get("H5").portion.as_deref(), Some("timeslice"));
    assert!(get("H6").is_composite, "`composite` must survive lowering");
    assert!(get("H7").is_reference, "`ref` must survive lowering");
}

#[test]
fn an_individual_part_keeps_its_modifier() {
    let src = r#"
package P {
	part def X;
	part def H { part f : X; }
	part def H2 { individual part f : X; }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    assert!(
        !find(&loaded.roots, "H")
            .children
            .first()
            .expect("f")
            .modifiers
            .is_individual
    );
    assert!(
        find(&loaded.roots, "H2")
            .children
            .first()
            .expect("f")
            .modifiers
            .is_individual,
        "`individual` must survive lowering: validateOccurrenceUsageIndividualUsage reads it"
    );
}

// -- the dropped rules cannot reject their violating models ------------------

#[test]
fn the_portion_and_individual_violations_are_not_reported() {
    // One model per dropped rule, each breaking it as the prose describes. Nothing is reported,
    // which fails when the lowering starts carrying the field and the check is written.
    let src = r#"
package P {
	occurrence def Occ;
	occurrence def Ind;
	occurrence def Ind2;
	part def H {
		portion part a : Occ;
		individual part b : Occ;
		individual part c : Ind, Ind2;
		abstract part d : Occ;
		derived part e : Occ;
	}
}
"#;
    assert!(
        sem021(src).is_empty(),
        "none of these fields reaches the tree, so none of these rules can fire"
    );
}

#[test]
fn an_owned_multiplicity_is_not_counted() {
    let src = r#"
package P {
	part def M {
		multiplicity a : [1..*];
		multiplicity b : [1..*];
	}
}
"#;
    assert!(
        sem021(src).is_empty(),
        "`validateTypeOwnedMultiplicity` has no way to tell the two `KermlType` members from \
         anything else"
    );
}
