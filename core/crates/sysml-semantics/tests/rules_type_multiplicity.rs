//! The `type-multiplicity` group: `validateFeatureEndMultiplicity` under `SEM033`, and why the
//! other two rules of the group cannot be decided from the element tree.
//!
//! The rule that runs is proved by models that violate it — `[0..1]`, `[4]`, `[*]`, `[1..*]`,
//! `[0..*]`, `[2..4]` and `[*..1]` on an `end` feature — and by the legal bounds that must stay
//! silent (`[1]`, `[1..1]`, no bound at all, and a bound written as an expression). Asserting only
//! the legal side would not distinguish "the rule runs" from "the rule is absent".
//!
//! `reported` calls the module directly, so these tests pass on the module's own behaviour rather
//! than on the wiring. Registering the module in `rules::all` and in `conformance::implemented()`
//! is the integrator's step — this cluster may add only its `pub mod` line to `rules/mod.rs` — and
//! a test asserting full-`check` output would fail until then without saying anything about the
//! rule.
//!
//! The two dropped rules read facts [`sysml_model::Element`] does not carry, and the tests at the
//! end pin the lowering fact each drop rests on. The `validateTypeOwnedMultiplicity` pin is a
//! shape equality between an owned `multiplicity` and an owned `datatype`: it holds while the two
//! lower to the same [`Kind::KermlType`], and starts failing the moment the lowering keeps the
//! keyword — which is the change that would make the rule decidable.
//!
//! Three tests pin a lowering defect this module inherits rather than hides: a *named* owned cross
//! feature is lowered as the `end` element and carries the cross multiplicity. The module
//! documentation says why no sound guard exists in this tree.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::type_multiplicity;

/// What the module under test reports, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    type_multiplicity::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
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

/// Load a project, for the tests that read the tree rather than the report.
fn load(src: &str) -> Vec<Element> {
    sysml_syntax::load_project(&[src], true)
        .expect("model parses")
        .roots
}

/// An element's whole shape with spans and line cleared, so two declarations that differ only by
/// something the lowering discards compare equal.
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
    let found = reported(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`[0..1]`")),
        "an `end` bounded 0..1 must be reported under SEM033, got {found:?}"
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
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
    let found = reported(src);
    assert!(
        found.is_empty(),
        "an unwritten bound is supplied 1..1 by the reference implementation, got {found:?}"
    );
}

#[test]
fn the_standard_library_shape_of_an_end_is_accepted() {
    // `Systems Library/Connections.sysml:43-44` writes `end source: Anything` with no bound, and
    // the official validation corpus does the same. Rejecting it would reject the library every
    // project imports.
    let src = r#"
package P {
	part def Anything;
	abstract connection def BinaryConnection {
		end source : Anything;
		end target : Anything;
	}
}
"#;
    let found = reported(src);
    assert!(
        found.is_empty(),
        "an end with no bound must not be reported, got {found:?}"
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
    let found = reported(src);
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
    let found = reported(src);
    assert!(
        found.is_empty(),
        "the rule is bounded to `isEnd` features, got {found:?}"
    );
}

#[test]
fn an_end_that_writes_no_bound_but_redefines_a_bounded_one_is_not_judged() {
    // The OCL's `multiplicities()` falls back to the inherited multiplicity when the feature
    // writes none. That half is not implemented: `supertypes` folds `subsets` and `:>>` targets
    // together with `:>` (`sysml-syntax/src/lower.rs:367-402`), so walking it would also fire on
    // an end that merely subsets a bounded feature. This pins the missed violation as the chosen
    // reading: only the base end, which writes `[0..1]` itself, is reported.
    //
    // The assertion is on the report rather than on the tree because the redefining declaration's
    // own shape is not what this module decides; if the lowering starts copying the inherited
    // bound onto it, the second name appears here and the test fails.
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[0..1];
	}
	connection def TightFitting :> Fitting {
		end wheels :>> wheels;
	}
}
"#;
    let names: Vec<String> = reported(src).into_iter().map(|(_, at, _)| at).collect();
    assert_eq!(
        names,
        ["P::Fitting::wheels"],
        "only the base end writes a bound that is not 1..1: {names:?}"
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
    assert_eq!(found[0].0, "SEM033", "the group's code: {found:?}");
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
    let mut names: Vec<String> = reported(src).into_iter().map(|(_, at, _)| at).collect();
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
// instead of silently changing what SEM033 reports.

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
    let roots = load(src);
    let conn = find(&roots, "Causation");
    assert_eq!(conn.children.len(), 1, "one end: {:?}", conn.children);

    let end = &conn.children[0];
    assert_eq!(end.name, "theCause", "the end keeps its own name");
    assert!(end.is_end, "the marker belongs to the end");
    assert_eq!(end.multiplicity, None, "the end itself is 1..1");
    assert_eq!(end.cross_name.as_deref(), Some("theCauses"));
    assert_eq!(end.cross_multiplicity.as_deref(), Some("[*]"));

    assert!(
        reported(src).is_empty(),
        "the cross bound is not the end's, got {:?}",
        reported(src)
    );
}

#[test]
fn the_specifications_own_cross_feature_example_is_not_reported() {
    // The `HubDeviceConnection` figure on page 69 of `research/corpus/sysml2-spec.pdf` writes
    // `end connectingHub [0..1] ordered part hub : Hub` and `end connectedDevices [1..*] part
    // device : Device`, and displays the bounds as *cross* multiplicities while the ends
    // themselves are 1..1. Both used to be reported; the prefix run now keeps the name and the
    // bound off the end, so the ends carry no written multiplicity and the rule is silent.
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
        reported(src).is_empty(),
        "the bounds belong to the cross features, got {:?}",
        reported(src)
    );
}

#[test]
fn an_anonymous_cross_feature_keeps_its_end_and_its_bound_apart() {
    // `end [0..1] part hub : Hub` states the bound of the owned cross feature; the end itself is
    // 1..1 (2024-12 pilot release notes: "the multiplicity of end features is now required to be
    // 1..1… the multiplicity should be moved to the position of a cross multiplicity"). The
    // lowering used to produce two siblings, neither an end, and now keeps one end with the bound
    // in `cross_multiplicity`.
    let src = r#"
package P {
	part def Hub;
	connection def HubDeviceConnection {
		end [0..1] part hub : Hub;
	}
}
"#;
    let roots = load(src);
    let conn = find(&roots, "HubDeviceConnection");
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
        reported(src).is_empty(),
        "the end's own multiplicity is unstated, so nothing is reported"
    );
}

// -- validateTypeOwnedMultiplicity -------------------------------------------
//
// `ownedMember->selectByKind(Multiplicity)->size() <= 1`. The violation is two owned
// multiplicities on one type. Nothing is reported, because an owned multiplicity and an owned
// KerML classifier lower to the same kind with the keyword discarded.

#[test]
fn a_type_owning_two_multiplicities_is_reported() {
    let src = r#"
package P {
	part def M {
		multiplicity a [1..*];
		multiplicity b [1..*];
	}
}
"#;
    let found = reported(src);
    assert!(
        found.iter().any(|(_, _, m)| m.contains("at most one")),
        "validateTypeOwnedMultiplicity now counts owned multiplicities, got {found:?}"
    );
}

#[test]
fn an_owned_datatype_is_distinguishable_from_an_owned_multiplicity() {
    // `Kind::KermlType` is what every unmodelled KerML classifier keyword becomes
    // (`sysml-syntax/src/lower.rs:256-264`), so the two members are indistinguishable and
    // counting `KermlType` children would report the datatype model as well. This equality is the
    // drop: it fails the moment the lowering keeps which keyword was written.
    let src = r#"
package P {
	part def M {
		multiplicity a [1..*];
	}
	part def N {
		datatype a [1..*];
	}
}
"#;
    let roots = load(src);
    let owned_multiplicity = find(&roots, "M")
        .children
        .iter()
        .find(|c| c.name == "a")
        .expect("the owned multiplicity");
    let owned_datatype = find(&roots, "N")
        .children
        .iter()
        .find(|c| c.name == "a")
        .expect("the owned datatype");
    // `multiplicity` has no kind of its own and still falls into the catch-all; `datatype` now
    // keeps its keyword. They used to be indistinguishable, which is what left
    // validateTypeOwnedMultiplicity undecidable: a rule counting owned Multiplicity members could
    // not tell one from a legal nested datatype.
    assert_eq!(
        owned_multiplicity.kind,
        Kind::MultiplicityDef,
        "an owned `multiplicity` now keeps its keyword"
    );
    assert_eq!(
        owned_datatype.kind,
        Kind::DataTypeDef,
        "an owned `datatype` keeps its keyword, so validateTypeOwnedMultiplicity can tell them apart"
    );
    assert_ne!(
        shape(owned_multiplicity),
        shape(owned_datatype),
        "the two declarations must now differ in the element tree"
    );
}

// -- validateFeatureMultiplicityDomain ---------------------------------------
//
// `multiplicity <> null implies multiplicity.featuringType = featuringType`. Neither side exists:
// `Element` has no featuring-type field, and a multiplicity is the authored bracket text rather
// than an element with featuring types of its own.

#[test]
fn a_feature_carrying_a_multiplicity_is_not_reported() {
    // The violation would be a multiplicity whose featuring types differ from the feature's. No
    // model can state that here: the only way the parser attaches a multiplicity to a feature is
    // the inline bracket, and the notation that would write a domain (`featured by`) is not read.
    let src = r#"
package P {
	part def Wheel;
	part def Car {
		part front : Wheel[0..1];
		part rear : Wheel[1..*];
	}
}
"#;
    let found = reported(src);
    assert!(
        found.is_empty(),
        "`validateFeatureMultiplicityDomain` cannot fire while neither side is readable, got {found:?}"
    );
}

#[test]
fn a_features_multiplicity_is_authored_text_and_not_an_element() {
    // This is the blocker, stated as what the tree does carry. `Element::multiplicity` is
    // `Option<String>` (`sysml-model/src/lib.rs:848-854`); a `Multiplicity` element would be a
    // node of its own with a name. When one appears, this assertion starts failing and the rule
    // becomes decidable.
    let src = r#"
package P {
	part def Wheel;
	part def Car {
		part wheels : Wheel[0..*];
	}
}
"#;
    let roots = load(src);
    let wheels = find(&roots, "Car")
        .children
        .iter()
        .find(|c| c.name == "wheels")
        .expect("the bounded feature");
    assert_eq!(
        wheels.multiplicity.as_deref(),
        Some("[0..*]"),
        "the bound is kept as text"
    );
    assert_eq!(
        wheels.type_ref.as_deref(),
        Some("Wheel"),
        "the bound is stripped off the type reference so the type still resolves"
    );
    assert!(
        wheels.children.is_empty(),
        "the bound owns no element under the feature: {:?}",
        wheels.children
    );
    let mut names = Vec::new();
    for root in &roots {
        collect_names(root, &mut names);
    }
    assert!(
        !names.iter().any(|n| n.contains('[')),
        "no element in the tree is named after a bound, so no Multiplicity element exists: {names:?}"
    );
}

/// Every element name in the tree, for the check that no bound lowers to a named element.
fn collect_names(el: &Element, out: &mut Vec<String>) {
    out.push(el.name.clone());
    for child in &el.children {
        collect_names(child, out);
    }
}

// -- validateTypeOwnedMultiplicity ------------------------------------------

#[test]
fn a_type_owning_two_multiplicities_is_rejected() {
    let src = r#"
package P {
	part def A {
		multiplicity m1 : [1];
		multiplicity m2 : [2];
	}
}
"#;
    let found = reported(src);
    assert!(
        found.iter().any(|(_, _, m)| m.contains("at most one")),
        "validateTypeOwnedMultiplicity: got {found:?}"
    );
}

#[test]
fn a_type_owning_one_multiplicity_is_accepted() {
    let src = r#"
package P {
	part def A { multiplicity m : [1..*]; }
}
"#;
    assert!(reported(src).is_empty(), "one owned multiplicity is legal");
}

#[test]
fn an_owned_multiplicity_keeps_its_keyword() {
    let src = r#"
package P {
	part def A { multiplicity m : [1..*]; }
}
"#;
    let roots = load(src);
    let owned = find(&roots, "A")
        .children
        .iter()
        .find(|c| c.name == "m")
        .expect("the owned multiplicity");
    assert_eq!(
        owned.kind.keyword(),
        "multiplicity",
        "the keyword is what lets the rule count these"
    );
}

#[test]
fn a_second_multiplicity_is_reported_at_the_member_that_is_one_too_many() {
    // `Type_Multiplicity_invalid.kerml.xt:17-21`: two `multiplicity` members in one classifier.
    // The pilot writes the assertion on the second one, so a report on the type's own line is a
    // miss there and a false positive there — one defect on both sides of the score.
    let src = r#"
package P {
	classifier C {
		multiplicity subsets Base::zeroOrOne;
		multiplicity subsets Base::zeroToMany;
	}
}
"#;
    let found = reported(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("second multiplicity") && found[0].2.contains("line 5"),
        "the report names the member that is one too many, got {found:?}"
    );
}
