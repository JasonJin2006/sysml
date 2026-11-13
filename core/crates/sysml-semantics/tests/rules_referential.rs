//! The `referential` group: four rules under `SEM031`, and why the other two cannot be decided.
//!
//! Each rule that runs is proved by a model that breaks it, and by the legal shapes that must stay
//! silent. Asserting only the legal side would not distinguish "the rule runs" from "the rule is
//! absent".
//!
//!   * `validatePortUsageIsReference` — `composite port p : PD` under a part def and at the package
//!     root are rejected; a port kept by a port definition or a port usage, a port that writes no
//!     modifier, and an `end` are not.
//!   * `validateAttributeUsageIsReference` — `composite attribute a : AD` is rejected; an attribute
//!     that writes no modifier, a `ref` attribute, and `ref composite` are not.
//!   * `validateAttributeUsageFeatures` — a `composite part` inside an attribute usage is rejected;
//!     a plain one, a variant alternative and an enumeration value are not.
//!   * `validateAttributeDefinitionFeatures` — the same over an attribute definition's features.
//!
//! `reported` calls the module directly rather than going through `check`, because the wiring in
//! `rules::all` and the entry in `conformance::implemented()` are the integrator's step and this
//! cluster may add only its `pub mod` line to `rules/mod.rs`. Importing `sysml_semantics::rules::
//! referential` here is what proves the registration exists; a test asserting full-`check` output
//! would fail for a reason that says nothing about the rule.
//!
//! The two dropped rules are recorded as facts rather than omissions:
//!
//!   * `validateReferenceUsageIsReference` reads a metaclass no element projects, and the check
//!     that would enforce it is *structurally* unable to fail once it existed — `ReferenceUsage`
//!     redefines `isReference` with default `true`
//!     (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:2491-2493`). The test pins that
//!     no element in a `ref`-bearing model is a [`Kind::Ref`](sysml_model::Kind::Ref).
//!   * `validateUsageIsReferential` has two readable disjuncts that the metamodel makes satisfy its
//!     consequent, and a third that needs `featuringType`. The test feeds it a model with both
//!     readable disjuncts and asserts it says nothing, which is the drop stated as a fact.
//!
//! One test pins a projection collision this module inherits: the KerML keyword `feature` lowers to
//! [`Kind::AttributeUsage`](sysml_model::Kind::AttributeUsage), so a legal `composite feature f` is
//! reported as a composite attribute. The module documentation says why no field in the tree can
//! tell them apart; the test makes the day the field arrives visible.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::referential;

/// What the module under test reports, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    referential::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM031` in a model, by element.
fn sem031(src: &str) -> Vec<(String, String, String)> {
    reported(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM031")
        .collect()
}

/// The qualified names the module reports, sorted.
fn reported_at(src: &str) -> Vec<String> {
    let mut names: Vec<String> = sem031(src).into_iter().map(|(_, at, _)| at).collect();
    names.sort();
    names
}

fn lowered(src: &str) -> Vec<Element> {
    sysml_syntax::load_project(&[src], true)
        .expect("model parses")
        .roots
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

// -- validatePortUsageIsReference ------------------------------------------
//
// `owningType = null or not owningType.oclIsKindOf(PortDefinition) and not
// owningType.oclIsKindOf(PortUsage) implies isReference`. Under
// `deriveUsageIsReference` (`generated.rs:8957-8961`) `isReference` is `not isComposite`, so the
// violation is a composite port whose owning type is not a port definition or a port usage.

#[test]
fn a_composite_port_owned_by_a_part_definition_is_rejected() {
    // The owner is a `PartDefinition`, so the antecedent holds and the port must be referential.
    let src = r#"
package P {
	port def PD;
	part def Holder {
		composite port p : PD;
	}
}
"#;
    let found = sem031(src);
    assert_eq!(found.len(), 1, "one composite port, one report: {found:?}");
    assert_eq!(found[0].1, "P::Holder::p", "{found:?}");
    assert!(
        found[0].2.contains("`composite`"),
        "the message must name the written word: {found:?}"
    );
}

#[test]
fn a_composite_port_at_the_package_root_is_rejected() {
    // No owner at all means `owningType = null`, which is the antecedent's first disjunct — so the
    // consequent is required. A `Package` is a `Namespace` and not a `Type` (`KerML.kerml:449`,
    // `:256`), so a port owned by one is in the same position.
    //
    // Two rules reject this declaration, and both are right: a port outside a port must be
    // referential (`validatePortUsageIsReference`), and a usage nothing features must be
    // referential (`validateUsageIsReferential` — a package is not a type, so it features
    // nothing). Two diagnostics for two rules, which is why the element is named twice.
    let src = r#"
package P {
	port def PD;
	composite port p : PD;
}
"#;
    let found = sem031(src);
    assert_eq!(
        found
            .iter()
            .map(|(_, at, _)| at.as_str())
            .collect::<Vec<_>>(),
        ["P::p", "P::p"],
        "a port with no owning type must be referential"
    );
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("nothing features it")),
        "the usage rule reports the missing featuring type: {found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("owning type is not a port definition")),
        "the port rule reports the owning type: {found:?}"
    );
}

#[test]
fn a_composite_port_inside_a_port_definition_is_accepted() {
    // `owningType.oclIsKindOf(PortDefinition)` falsifies the antecedent, so the rule says nothing.
    let src = r#"
package P {
	port def PD {
		composite port inner : PD;
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a port definition may keep a composite port: {:?}",
        sem031(src)
    );
}

#[test]
fn a_composite_port_inside_a_port_usage_is_accepted() {
    // The other excluded owner: `owningType.oclIsKindOf(PortUsage)`.
    let src = r#"
package P {
	port def PD;
	part def H {
		port p : PD {
			composite port inner : PD;
		}
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a port usage may keep a composite port: {:?}",
        sem031(src)
    );
}

#[test]
fn a_port_that_writes_no_modifier_is_not_judged() {
    // At the package root, so the owner is not a port and the antecedent holds — the only thing
    // keeping this silent is the absent word. The metamodel's declared default is
    // `isComposite = false` (`generated.rs:2234`), so the port is referential and the rule holds.
    let src = r#"
package P {
	port def PD;
	port p : PD;
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a port with no modifier states nothing to reject: {:?}",
        sem031(src)
    );
}

#[test]
fn an_end_is_not_judged_as_a_port() {
    // The lowering relabels every `end` to `Kind::PortUsage` (`lower.rs:281-283`), and the metamodel
    // makes an end non-composite before its port arms run (`build.rs:2243-2249` vs `:2278-2283`), so
    // the port rule's consequent holds. A declaration that writes `composite` on an end still
    // states the contradiction, and `validateUsageIsReferential` reports it — an end must be
    // referential — as does `validateFeatureEndNotDerivedAbstractCompositeOrPortion` (SEM030). The
    // port rule stays silent, which is what this test pins.
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		composite end e : Wheel;
	}
}
"#;
    let roots = lowered(src);
    assert_eq!(
        find(&roots, "e").kind,
        Kind::PortUsage,
        "the end really does arrive as a PortUsage, which is why the guard is needed"
    );
    let found = sem031(src);
    assert!(
        found
            .iter()
            .all(|(_, _, m)| !m.contains("owning type is not a port definition")),
        "the port rule leaves an end to SEM030: {found:?}"
    );
    assert!(
        found.iter().any(|(_, _, m)| m.contains("it is an end")),
        "the usage rule reports the contradicted `composite`: {found:?}"
    );
}

#[test]
fn an_inherited_composite_port_is_not_reported_a_second_time() {
    // `inherit_ports` copies a definition's ports onto every part usage of it
    // (`lower.rs:1344-1395`), keeping the modifiers, so the copy carries `composite` too. It is not
    // an authored declaration, and reporting it would double every defect once per usage.
    let src = r#"
package P {
	port def PD;
	part def Holder {
		composite port p : PD;
	}
	part h : Holder;
}
"#;
    let names = reported_at(src);
    assert_eq!(
        names,
        ["P::Holder::p"],
        "the definition's port is the authored declaration; the copy on `h` is not: {names:?}"
    );
}

// -- validateAttributeUsageIsReference --------------------------------------
//
// The OCL is the single word `isReference`, i.e. `not isComposite`, with no antecedent.

#[test]
fn a_composite_attribute_is_rejected() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		composite attribute a : AD;
	}
}
"#;
    let found = sem031(src);
    assert_eq!(
        found.len(),
        1,
        "one composite attribute, one report: {found:?}"
    );
    assert_eq!(found[0].1, "P::Holder::a", "{found:?}");
}

#[test]
fn an_attribute_that_writes_no_modifier_is_not_judged() {
    // `isComposite = false` by the declared default, hence `isReference = true`: the rule holds.
    let src = r#"
package P {
	attribute def AD;
	part def Holder { attribute a : AD; }
}
"#;
    assert!(
        sem031(src).is_empty(),
        "an unmodified attribute is referential: {:?}",
        sem031(src)
    );
}

#[test]
fn a_ref_attribute_is_accepted() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder { ref attribute a : AD; }
}
"#;
    assert!(
        sem031(src).is_empty(),
        "`ref` is the other spelling of the same property: {:?}",
        sem031(src)
    );
}

#[test]
fn an_attribute_writing_both_words_is_accepted() {
    // `ref composite` states a contradiction the OCL does not resolve. The reference implementation
    // tests `ref` first (`build.rs:2243-2248`), so this check follows that order rather than
    // inventing a resolution: the declaration is referential under that reading.
    let src = r#"
package P {
	attribute def AD;
	part def Holder { ref composite attribute a : AD; }
}
"#;
    assert!(
        sem031(src).is_empty(),
        "`ref` wins under the reference reading: {:?}",
        sem031(src)
    );
}

#[test]
fn a_composite_attribute_definition_is_not_an_attribute_usage() {
    // `AttributeUsage` is the metaclass this rule is asked of; a definition is not one. The three
    // `AttributeDefinition` rules are the `*Features` pair and nothing else.
    let src = r#"
package P {
	composite attribute def AD;
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a definition is not an attribute usage: {:?}",
        sem031(src)
    );
}

// -- validateAttributeUsageFeatures ----------------------------------------
//
// `feature->forAll(not isComposite)` on an `AttributeUsage`.

#[test]
fn a_composite_part_inside_an_attribute_usage_is_rejected() {
    let src = r#"
package P {
	part def A;
	attribute def AD;
	part def Holder {
		attribute a : AD {
			composite part x : A;
		}
	}
}
"#;
    let found = sem031(src);
    assert_eq!(
        found.len(),
        1,
        "one composite feature, one report: {found:?}"
    );
    assert_eq!(
        found[0].1, "P::Holder::a::x",
        "the report names the owned feature, not its owner: {found:?}"
    );
    assert!(
        found[0].2.contains("`P::Holder::a`"),
        "the message names the attribute that owns it: {found:?}"
    );
}

#[test]
fn a_plain_part_inside_an_attribute_usage_is_rejected() {
    // The effective `isComposite` is what the rule reads, and a part owned by an attribute is
    // composite whether or not the source writes the word: an attribute has no parts, so the
    // declaration is the defect `validateAttributeUsageFeatures` names. Under the written word
    // alone this model passed, which was the projection's gap rather than the model's.
    let src = r#"
package P {
	part def A;
	attribute def AD;
	part def Holder {
		attribute a : AD {
			part x : A;
		}
	}
}
"#;
    let found = sem031(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].1, "P::Holder::a::x", "got {found:?}");
    assert!(
        found[0].2.contains("every feature of an attribute"),
        "got {found:?}"
    );
}

#[test]
fn a_composite_port_inside_an_attribute_usage_breaks_two_rules_at_once() {
    // Both rules reach this element, and both are genuinely violated: a port not owned by a port
    // must be referential (`validatePortUsageIsReference`), and no feature of an attribute usage
    // may be composite (`validateAttributeUsageFeatures`). Two diagnostics for two rules, not a
    // duplicate — the wording differs because the defects do.
    let src = r#"
package P {
	port def PD;
	attribute def AD;
	part def Holder {
		attribute a : AD {
			composite port x : PD;
		}
	}
}
"#;
    let found = sem031(src);
    assert_eq!(found.len(), 2, "two rules, two reports: {found:?}");
    assert!(
        found.iter().all(|(_, at, _)| at == "P::Holder::a::x"),
        "both are about the composite port: {found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("must be referential")),
        "one is the port rule: {found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("every feature of an attribute")),
        "the other is the attribute-features rule: {found:?}"
    );
}

#[test]
fn a_variant_alternative_inside_an_attribute_usage_is_not_a_feature() {
    // `VariantMembership specializes OwningMembership` (`generated.rs:1772`), not
    // `FeatureMembership`, so the alternative is outside `feature` and the OCL never reaches it.
    let src = r#"
package P {
	part def A;
	attribute def AD;
	part def Holder {
		attribute a : AD {
			variant part alt : A;
		}
	}
}
"#;
    let roots = lowered(src);
    assert!(
        find(&roots, "alt").is_variant,
        "`variant part alt` must reach the tree as a variant"
    );
    assert!(
        sem031(src).is_empty(),
        "a variant member is not in `feature`: {:?}",
        sem031(src)
    );
}

#[test]
fn an_enumeration_definition_is_not_judged_as_an_attribute_definition() {
    // `EnumerationDefinition specializes AttributeDefinition` (`generated.rs:1658`), so in the
    // metamodel this rule is asked of it too. The projection gives it its own kind (`enum def`
    // lowers to `Kind::EnumDef`, not `Kind::AttributeDef`), so this module does not reach it — a
    // recorded miss, not a pass. The test pins the shape so the miss is a fact rather than a
    // silence.
    let src = r#"
package P {
	enum def Colour {
		enum red;
		enum green;
	}
}
"#;
    let roots = lowered(src);
    let colour = find(&roots, "Colour");
    assert_eq!(
        colour.kind,
        Kind::EnumDef,
        "`enum def` must lower to Kind::EnumDef, which is why the attribute rules skip it"
    );
    assert_ne!(
        colour.kind,
        Kind::AttributeDef,
        "it is a specialization of AttributeDefinition in the metamodel but not in the tree"
    );
    assert!(
        sem031(src).is_empty(),
        "the two values are variant members and the owner is not an AttributeDefinition here: {:?}",
        sem031(src)
    );
}

#[test]
fn a_composite_definition_member_is_not_judged() {
    // `isComposite` is a `Feature` property (`generated.rs:2234`) and a definition is a `Type`, so
    // the word on a definition member sets nothing the OCL reads. This is a recorded under-report:
    // the tree accepts `composite part def` and the metamodel gives it no flag to violate.
    let src = r#"
package P {
	attribute def AD {
		composite part def X;
	}
}
"#;
    let roots = lowered(src);
    assert_eq!(
        find(&roots, "X").kind,
        Kind::PartDef,
        "the member really does lower to a definition"
    );
    assert!(
        sem031(src).is_empty(),
        "a definition member is not in `feature`: {:?}",
        sem031(src)
    );
}

// -- validateAttributeDefinitionFeatures -----------------------------------
//
// `feature->forAll(not isComposite)` on an `AttributeDefinition`.

#[test]
fn a_composite_part_inside_an_attribute_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	attribute def AD {
		composite part x : A;
	}
}
"#;
    let found = sem031(src);
    assert_eq!(
        found.len(),
        1,
        "one composite feature, one report: {found:?}"
    );
    assert_eq!(found[0].1, "P::AD::x", "{found:?}");
}

#[test]
fn an_attribute_definitions_plain_features_are_accepted() {
    // The shipped library's own shape: attributes and constraints inside an attribute definition.
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute def Tensor {
		attribute isBound : Boolean;
		attribute order : Integer;
		ref attribute mRef : Boolean;
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a plain attribute definition must not be reported: {:?}",
        sem031(src)
    );
}

// -- the rule's own reporting --------------------------------------------------

#[test]
fn the_report_names_the_rule_by_its_own_code() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		composite attribute a : AD;
	}
}
"#;
    let found = reported(src);
    assert_eq!(found.len(), 1, "one violation, one report: {found:?}");
    assert_eq!(found[0].0, "SEM031", "the group's code: {found:?}");
    assert_eq!(found[0].1, "P::Holder::a", "{found:?}");
    assert_eq!(
        found[0].2, "an attribute usage is always referential, but it states `composite`",
        "{found:?}"
    );
}

#[test]
fn every_violating_element_in_one_model_is_reported() {
    let src = r#"
package P {
	port def PD;
	attribute def AD;
	part def A;
	part def Holder {
		composite port p : PD;
		composite attribute a : AD {
			composite part nested : A;
		}
		attribute plain : AD;
		port compliant : PD;
	}
	attribute def Struct {
		composite part member : A;
	}
}
"#;
    assert_eq!(
        reported_at(src),
        [
            "P::Holder::a",
            "P::Holder::a::nested",
            "P::Holder::p",
            "P::Struct::member",
        ],
        "exactly the four elements that state `composite`"
    );
}

#[test]
fn a_composite_part_that_is_not_a_port_or_attribute_is_not_judged() {
    // The four rules are asked of a port usage, an attribute usage and an attribute definition.
    // A composite part under a part definition is none of those, and no rule in this group forbids
    // it — that is `validatePortUsageNestedUsagesNotComposite`'s neighbourhood, not this group's.
    let src = r#"
package P {
	part def A;
	part def Holder {
		composite part whole : A;
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a composite part is the ordinary shape of a part definition: {:?}",
        sem031(src)
    );
}

#[test]
fn a_well_formed_referential_model_gains_no_sem031() {
    // Ports nested in ports, an attribute inside a port definition, a reference port and a
    // reference attribute: the shapes the specification calls referential throughout.
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute def TorqueValue;
	port def ShaftPort {
		attribute maxTorque : TorqueValue;
		port inner : ShaftPort;
		ref port aux : ShaftPort;
	}
	part def Shaft {
		port outPort : ShaftPort;
		ref port auxPort : ShaftPort;
		attribute material : TorqueValue;
		ref attribute label : TorqueValue;
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "a well-formed referential model must not be reported: {:?}",
        sem031(src)
    );
}

// -- the two rules this module cannot decide --------------------------------

#[test]
fn no_element_in_the_tree_is_a_reference_usage() {
    // `validateReferenceUsageIsReference` is asked of the `ReferenceUsage` metaclass, and the tree
    // has no element for it: `Kind::Ref` is declared (`sysml-model/src/lib.rs:47-48`) but no
    // lowering path constructs it (`grep -rn "Kind::Ref" core/crates/` finds only its `keyword()`
    // and `group()` arms). A `ref` declaration lowers to whatever kind its keyword named.
    let src = r#"
package P {
	part def Base;
	part def Holder {
		ref r : Base;
		ref part p : Base;
		ref attribute a : Base;
	}
}
"#;
    let roots = lowered(src);
    let mut kinds: Vec<Kind> = Vec::new();
    fn go(el: &Element, kinds: &mut Vec<Kind>) {
        kinds.push(el.kind.clone());
        for child in &el.children {
            go(child, kinds);
        }
    }
    for root in &roots {
        go(root, &mut kinds);
    }
    assert!(
        !kinds.iter().any(|k| *k == Kind::Ref),
        "no element may be a Kind::Ref while the tree has no path that builds one: {kinds:?}"
    );
    assert!(
        find(&roots, "p").kind == Kind::PartUsage,
        "`ref part p` lowers to a part usage whose `ref` is a modifier, not a ReferenceUsage"
    );
}

#[test]
fn validate_usage_is_referential_says_nothing_about_its_two_readable_disjuncts() {
    // `direction <> null or isEnd or featuringType->isEmpty() implies isReference`. A directed
    // feature and an end both make the antecedent true, and the metamodel makes both non-composite
    // (`build.rs:2243-2249`), so the consequent holds and there is no violation to report. The third
    // disjunct needs `featuringType`, which the tree does not carry and the parser cannot write.
    let src = r#"
package P {
	part def Base;
	part def Holder {
		in b : Base;
		end a : Base;
	}
}
"#;
    assert!(
        sem031(src).is_empty(),
        "the two readable disjuncts cannot be violated: {:?}",
        sem031(src)
    );
}
// -- the projection collision that used to be here --------------------------

#[test]
fn a_kerml_feature_is_its_own_kind() {
    // `feature` / `expr` / `bool` / `step` all lowered to `Kind::AttributeUsage`, so a KerML
    // feature was judged by the SysML rules about an attribute: `feature f : T` with `T` a `class`
    // was rejected as "an attribute must be typed by attribute definitions", and a feature owning
    // a composite member broke "every feature of an attribute must be non-composite". Neither rule
    // is about KerML, and a model written in KerML has no attribute definitions.
    //
    // `feature` now has its own kind. `step` / `expr` / `bool` still share the attribute kind;
    // separating them is the same change and nothing has needed it yet.
    let kerml = r#"
package P {
	attribute def AD;
	part def Holder { composite feature f : AD; }
}
"#;
    let sysml = r#"
package P {
	attribute def AD;
	part def Holder { composite attribute f : AD; }
}
"#;
    let a = lowered(kerml);
    assert_eq!(
        find(&a, "f").kind,
        Kind::KermlFeature,
        "a KerML `feature` keeps its own kind"
    );
    assert_ne!(
        find(&a, "f").kind,
        find(&lowered(sysml), "f").kind,
        "the two spellings are different declarations and the tree says so"
    );
    assert_eq!(
        reported_at(kerml),
        Vec::<String>::new(),
        "a KerML feature is not an attribute, so the attribute rule leaves it alone"
    );
}
