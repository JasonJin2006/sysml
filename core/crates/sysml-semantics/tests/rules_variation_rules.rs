//! The `variation-rules` group (`SEM032`), and why none of its seven rules is decided.
//!
//! The module note in `sysml-semantics/src/rules/variation_rules.rs` gives the reason per rule.
//! This file records the facts those reasons rest on, and — for each rule — asserts that a model
//! written to break it produces no diagnostic. A drop that is only described in prose is
//! indistinguishable from a check that quietly never fires; a test that feeds the module a
//! violating model and sees nothing come back is not.
//!
//! Those "nothing is reported" tests fail the moment a check is written, which is the intent: the
//! drop is then a recorded fact with an expiry date rather than an omission. The tests above each
//! one pin the field or spelling that makes the current answer correct, so a change to the front
//! end that removes the blocker shows up here too.
//!
//! The module-level tests call `rules::variation_rules::issues` directly, so they cannot pass by
//! accident if the module is never wired into `rules::all`; the `SEM032` assertions run the same
//! models through `check` and confirm nothing reaches a report either way.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::variation_rules;
use sysml_semantics::{check, Severity};

/// The helper from `tests/rules.rs`: every error `check` reports, as `(code, at, message)`.
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
    variation_rules::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM032` a full `check` produces.
fn sem032(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM032")
        .collect()
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

/// Assert that a model written to break `rule` produces no diagnostic at all.
///
/// `why` names the field, spelling or metamodel default the rule needs and where it goes missing,
/// so a later reader can tell an intentional drop from a check that quietly stopped working.
fn assert_nothing_reported(src: &str, rule: &str, why: &str) {
    let found = reported(src);
    assert!(
        found.is_empty(),
        "`{rule}` is not decided by this module, so this model must not be reported. If this \
         fires, the blocker is gone and the check has to be written properly rather than left as \
         a heuristic ({why}). Got {found:#?}"
    );
    let through_check = sem032(src);
    assert!(
        through_check.is_empty(),
        "and nothing may reach `check` under SEM032 for the same reason ({why}). \
         Got {through_check:#?}"
    );
}

#[test]
fn the_module_reports_under_its_own_code() {
    assert_eq!(
        variation_rules::CODE,
        "SEM032",
        "this group's reports belong under SEM032"
    );
}

// -- validateUsageVariationIsAbstract / validateDefinitionVariationIsAbstract -----
//
// `isVariation implies isAbstract` (`data/spec_rules.rs:917` and `:959`). Both flags exist, and the
// lowering fills `is_abstract` from `is_variation` (`derive_modifiers`): the specification says a
// variation "is always abstract, so the `abstract` keyword is not used on a variation"
// (research/corpus/sysml2-spec.pdf, PDF page 74 / printed page 43, §7.6.7), and the grammar gives
// the two words one slot (`SysML.xtext:491`, `:559`). With the value filled the rule has no model
// to reject, which is why the ledger records it as not applicable rather than enforced.

#[test]
fn a_variation_usage_is_abstract_as_well_as_a_variation() {
    // `variation part config` writes one word. The specification says a variation "is always
    // abstract, so the `abstract` keyword is not used on a variation"
    // (research/corpus/sysml2-spec.pdf, PDF page 74 / printed page 43, §7.6.7), so the effective
    // `isAbstract` is true even though the model wrote no `abstract`.
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part config { attribute x : Base; }
}
"#,
    );
    let config = find(&roots, "config");
    assert!(
        config.modifiers.is_variation,
        "`variation` must survive lowering: the three Usage rules read isVariation"
    );
    assert!(
        config.modifiers.is_abstract,
        "a variation is abstract whether or not the model writes the word"
    );
}

#[test]
fn a_variation_definition_is_abstract_as_well_as_a_variation() {
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part def Config :> Base;
}
"#,
    );
    let config = find(&roots, "Config");
    assert!(config.modifiers.is_variation);
    assert!(
        config.modifiers.is_abstract,
        "the same holds on a definition: `variation part def Config` writes no `abstract`"
    );
}

#[test]
fn the_only_spelling_that_satisfies_the_abstract_rule_is_one_the_grammar_forbids() {
    // Both prefix productions make the two words alternatives, never a pair:
    // `BasicDefinitionPrefix` is `isAbstract ?= 'abstract' | isVariation ?= 'variation'`
    // (research/upstream/SysML.xtext:491) and `RefPrefix` is
    // `( isAbstract ?= 'abstract' | isVariation ?= 'variation')?` (`:559`). Our parser is laxer —
    // `prefixes()` reads a run of words in one loop (`sysml-parser/src/parser.rs:251-321`,
    // `abstract` at `:267-270`, `variation` at `:271-274`) — so this spelling parses here even
    // though it is not a legal SysML model.
    let roots = lowered(
        r#"
package P {
	abstract variation part p;
}
"#,
    );
    let p = find(&roots, "p");
    assert!(
        p.modifiers.is_abstract && p.modifiers.is_variation,
        "the lax spelling sets both flags; this is the only shape that satisfies \
         `isVariation implies isAbstract` in the tree, and the grammar forbids writing it"
    );
}

#[test]
fn validate_usage_variation_is_abstract_cannot_reject_its_violating_model() {
    // `variation part config` is abstract in the abstract syntax and carries no `abstract` word
    // in the text. A check over the authored flag would reject it — and with it the
    // specification's own variability example (`variation part def TransmissionChoices :>
    // Transmission { … }`, spec PDF page 687 / printed 656) and every `variation` declaration in
    // the official validation suite. Deriving `isAbstract` first, as the reference builder does
    // (research/upstream/mod/sysmlv2-model-0.1.2/src/build.rs:233-237), makes the rule hold of
    // everything and it can never fail.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part config { attribute x : Base; }
}
"#,
        "validateUsageVariationIsAbstract",
        "the tree holds the authored isAbstract; the rule reads the derived one",
    );
}

#[test]
fn validate_definition_variation_is_abstract_cannot_reject_its_violating_model() {
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part def Cfg { attribute x : Base; }
}
"#,
        "validateDefinitionVariationIsAbstract",
        "the tree holds the authored isAbstract; the rule reads the derived one",
    );
}

// -- validate*VariationOwnedFeatureMembership ------------------------------------
//
// `isVariation implies ownedFeatureMembership->isEmpty()` (`data/spec_rules.rs:923`, `:965`).
// `ownedFeatureMembership` is `ownedRelationship->selectByKind(FeatureMembership)`
// (`generated.rs:8093-8096`), and a `VariantMembership` is a sibling of `FeatureMembership`, not
// one of its kinds (`generated.rs:1772` vs `:1668`). The tree holds elements and no membership
// metaclass, and the `variant` marker survives only one of the three spellings the notation has.

#[test]
fn a_variation_that_owns_an_authored_attribute_keeps_it_as_a_child() {
    // The plainest violation of the consequent: an `OwnedFeatureMembership` that is not a
    // `VariantMembership`. It is readable as a child, which is why a check looks possible.
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part config { attribute x : Base; }
}
"#,
    );
    let config = find(&roots, "config");
    assert_eq!(
        config.children.len(),
        1,
        "the owned attribute is a child: {:?}",
        config.children
    );
    assert_eq!(config.children[0].kind, Kind::AttributeUsage);
}

#[test]
fn a_variant_that_names_its_kind_reaches_the_tree_as_a_variant() {
    // `variant part alt : Base` is the spelling that carries the marker: the parser reads the
    // word, sets `prefixes.is_variant`, and the lowering carries it onto the element, so the
    // alternative keeps `Kind::PartUsage` and answers `is_variant`.
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part config { variant part alt : Base; }
}
"#,
    );
    let config = find(&roots, "config");
    assert!(
        config.children.iter().any(|c| c.is_variant),
        "the alternative must be recognisable here: {:?}",
        config
            .children
            .iter()
            .map(|c| (c.kind.clone(), c.name.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_variant_that_omits_its_kind_is_still_a_variant() {
    // §7.6.7: "A non-variant usage can also be declared to act as a variant of a variation by not
    // including a kind keyword in the variant declaration." The word names the membership, so the
    // member keeps whatever kind the enclosing declaration implies — but it is a variant, and
    // without that the variation's body looked like it held an ordinary member.
    //
    // The parser used to take its no-kind path here, which returns `kind: Kind::Attribute` and
    // drops the `is_variant` flag, so a bare variant and an authored attribute were the same
    // element.
    let bare_variant = lowered(
        r#"
package P {
	variation requirement engineRqtChoice {
		variant highPerformanceRequirement;
	}
}
"#,
    );
    let authored = lowered(
        r#"
package P {
	variation requirement engineRqtChoice {
		attribute highPerformanceRequirement;
	}
}
"#,
    );
    let from_variant = find(&bare_variant, "highPerformanceRequirement");
    let from_attribute = find(&authored, "highPerformanceRequirement");
    assert_eq!(
        from_variant.kind,
        Kind::Variant,
        "a bare variant is a Kind::Variant"
    );
    assert_ne!(
        from_variant.kind, from_attribute.kind,
        "and it is not the same kind an authored attribute is"
    );
}

#[test]
fn a_variant_written_as_a_performance_loses_the_marker_and_the_variation_with_it() {
    // `variation perform action doXorY { variant perform doX; }` is the shape the official suite
    // writes (`07-Variant Configuration/7a1-Variant Configuration - General Concept-a.sysml:14-17`).
    // Both words are prefix runs, and the declaration is routed to `statement()` before any
    // prefix is read (`sysml-parser/src/parser.rs:853`), which hands it to `behavior()`
    // (`:3021-3051`) — a function that takes no prefixes at all. So the variation point itself is
    // not a variation in the tree, and neither alternative is a variant.
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation perform action doXorY {
		variant perform doX;
		variant perform doY;
	}
}
"#,
    );
    let owner = find(&roots, "doXorY");
    assert_eq!(owner.kind, Kind::Perform);
    assert!(
        !owner.modifiers.is_variation,
        "the enclosing `variation` is dropped on the statement path"
    );
    // `variant perform doX;` has no kind keyword, so it references `doX` and declares no name:
    // the alternative is found by what it performs.
    let alternative = owner
        .children
        .iter()
        .find(|el| el.performs.as_deref() == Some("doX"))
        .expect("the alternative");
    assert_eq!(
        alternative.kind,
        Kind::Perform,
        "the alternative is a performance, not a Kind::Variant"
    );
    assert!(
        !alternative.modifiers.is_variation,
        "and the alternative's `variant` is dropped with it"
    );
}

#[test]
fn a_definition_s_ports_are_copied_onto_a_variation_that_uses_it() {
    // `inherit_ports` (`sysml-syntax/src/lower.rs:1344-1389`) copies a definition's ports onto
    // every part usage typed by it and marks the copy `inherited`
    // (`sysml-model/src/lib.rs:797`, pushed at `lower.rs:1377`). Those children are not owned
    // memberships of anything, so counting them as `ownedFeatureMembership`s would report the
    // projection rather than the model.
    let roots = lowered(
        r#"
package P {
	part def Spec {
		port p1;
	}
	variation part config : Spec;
}
"#,
    );
    let config = find(&roots, "config");
    assert!(
        config.children.iter().any(|c| c.inherited),
        "the borrowed port is a child and is marked inherited: {:?}",
        config
            .children
            .iter()
            .map(|c| (c.name.clone(), c.inherited))
            .collect::<Vec<_>>()
    );
}

#[test]
fn validate_usage_variation_owned_feature_membership_cannot_reject_its_violating_model() {
    // The model owns a plain attribute, which is exactly what the consequent forbids. A check
    // over children would have to report it — but the same child shape is produced by the legal
    // bare variant spelling above, which the official suite uses
    // (`07-Variant Configuration/7b-Variant Configurations.sysml:82-85`). No guard separates
    // them, so the rule is dropped whole rather than narrowed to the cases that happen to be
    // unambiguous.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part config { attribute x : Base; }
}
"#,
        "validateUsageVariationOwnedFeatureMembership",
        "the membership metaclass is not on the tree, and the bare `variant` spelling lowers to \
         the same AttributeUsage an authored attribute does",
    );
}

#[test]
fn validate_definition_variation_owned_feature_membership_cannot_reject_its_violating_model() {
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part def Cfg { attribute x : Base; }
}
"#,
        "validateDefinitionVariationOwnedFeatureMembership",
        "the membership metaclass is not on the tree, and the bare `variant` spelling lowers to \
         the same AttributeUsage an authored attribute does",
    );
}

// -- validate*VariationSpecialization --------------------------------------------
//
// The two bodies (`data/spec_rules.rs:929`, `:953`) quantify over
// `ownedSpecialization.specific->exists(…)`. `specific` on a `Specialization` is the type doing
// the specializing (`generated.rs:2556-2560`, redefining `source`), and `ownedSpecialization` is
// already filtered to the specializations whose specific type is the element
// (`generated.rs:8117-8120`). So `specific` is always the element itself and both bodies reduce
// to `isVariation implies not P(self)` = `not isVariation`.

#[test]
fn a_variation_usage_specializing_a_variation_definition_carries_both_readable_ends() {
    // The *prose* reading is decidable from this tree: the specialized target is `supertypes`
    // (`sysml-model/src/lib.rs:757`) and the target's flag is `Modifiers`. What is not readable
    // from the OCL is the `general` end — the text names `specific` instead.
    let roots = lowered(
        r#"
package P {
	variation part def Parent;
	variation part child :> Parent;
}
"#,
    );
    let child = find(&roots, "child");
    assert_eq!(
        child.supertype_list(),
        vec!["Parent"],
        "the specialized target is readable"
    );
    assert!(
        child.modifiers.is_variation && find(&roots, "Parent").modifiers.is_variation,
        "and so is each end's flag — but the OCL reads `specific`, which is `child` itself"
    );
}

#[test]
fn validate_usage_variation_specialization_cannot_reject_its_violating_model() {
    // Read literally, `not ownedSpecialization.specific->exists(is a variation Definition or
    // Usage)` is false of `child` because `specific` is `child` and `child` is a variation, so the
    // rule rejects every variation. Read as the prose, it needs the `general` end the OCL never
    // names. The upstream semantics layer records the same conflict and runs neither
    // (research/upstream/sem/sysmlv2-semantics-0.1.2/src/rules.rs:321-323 and `:426-427`).
    assert_nothing_reported(
        r#"
package P {
	variation part def Parent;
	variation part child :> Parent;
}
"#,
        "validateUsageVariationSpecialization",
        "the OCL reads `specific` (the element itself) where the prose means `general`; either \
         reading rejects legal models or states a rule the specification does not",
    );
}

#[test]
fn validate_definition_variation_specialization_cannot_reject_its_violating_model() {
    assert_nothing_reported(
        r#"
package P {
	variation part def Parent;
	variation part def Child :> Parent;
}
"#,
        "validateDefinitionVariationSpecialization",
        "the OCL reads `specific` (the element itself) where the prose means `general`; either \
         reading rejects legal models or states a rule the specification does not",
    );
}

// -- validateEnumerationDefinitionIsVariation ------------------------------------
//
// `isVariation` (`data/spec_rules.rs:539`), asked of `EnumerationDefinition`, whose
// `isVariation` redefines the inherited property with the declared default `true`
// (`generated.rs:2205`).

#[test]
fn an_enumeration_definition_records_the_authored_flag_which_is_false() {
    // The metamodel default is `true` (`generated.rs:2205`) and the specification says the same
    // twice — "An EnumerationDefinition is considered semantically to be a variation"
    // (spec PDF page 311 / printed 280, §8.3.8.2) and "An enumeration definition is always
    // considered to be a variation … The keywords `abstract` and `variation` may not be used with
    // an enumeration definition" (PDF page 79 / printed 48, §7.8.2). The tree stores only what was
    // authored (`sysml-syntax/src/lower.rs:345`) and applies no default, so the flag is false on
    // the ordinary spelling.
    let roots = lowered(
        r#"
package P {
	enum def M { a; b; }
}
"#,
    );
    let m = find(&roots, "M");
    assert_eq!(m.kind, Kind::EnumDef);
    assert!(
        !m.modifiers.is_variation,
        "the tree holds the authored flag, which is false; the metaclass default is true"
    );
}

#[test]
fn the_variation_keyword_on_an_enumeration_definition_is_a_syntax_error() {
    // `EnumerationDefinition` takes no `BasicDefinitionPrefix`
    // (`research/upstream/SysML.xtext:767-770`), so `variation enum def M` is not a spelling the
    // grammar admits. The parser refuses it, and no tree can carry the flag on an enumeration
    // definition.
    assert!(
        issues("package P {\n\tvariation enum def M { a; b; }\n}\n")
            .iter()
            .any(|(code, _, _)| code == "SYN001"),
        "`variation enum def` must be a syntax error"
    );
}

#[test]
fn validate_enumeration_definition_is_variation_cannot_reject_its_violating_model() {
    // Read against the tree's flag, the rule rejects every enumeration definition, including the
    // notation the specification's own table shows (`enum def EnumerationDef1 { enum1; enum2; }`,
    // spec PDF page 79 / printed 48). Read with the metaclass default applied, it holds of
    // everything and can never fail.
    assert_nothing_reported(
        r#"
package P {
	enum def M { enum a; enum b; }
}
"#,
        "validateEnumerationDefinitionIsVariation",
        "the tree holds the authored flag, the metaclass default is true, and the notation has no \
         slot for the keyword",
    );
}

// -- the group as a whole -------------------------------------------------------

#[test]
fn one_model_with_every_violation_reports_nothing() {
    // One model per rule, each breaking it as the prose describes. Nothing is reported, which
    // fails when any of the seven checks is written.
    let src = r#"
package P {
	part def Base;
	enum def M { enum a; enum b; }
	variation part config { attribute x : Base; }
	variation part def Cfg { attribute y : Base; }
	variation part def Parent;
	variation part child :> Parent;
	variation part def Child :> Parent;
}
"#;
    assert!(
        reported(src).is_empty(),
        "none of these rules is decided, so none may fire"
    );
    assert!(
        sem032(src).is_empty(),
        "and nothing may reach `check` under SEM032 either"
    );
}

#[test]
fn a_well_formed_variation_configuration_gains_no_sem032() {
    // The shape the corpus calls a variant configuration (07-Variant Configuration). A legal
    // model has to stay silent for whatever check is written later.
    let src = r#"
package P {
	part def SubsystemA;
	part def SubsystemB;
	part anyVehicleConfig {
		variation part subsystemA : SubsystemA {
			variant part subsystem1 : SubsystemA;
			variant part subsystem2 : SubsystemA;
		}
		variation part subsystemB : SubsystemB {
			variant part subsystem3 : SubsystemB;
		}
	}
}
"#;
    let found = sem032(src);
    assert!(
        found.is_empty(),
        "a well-formed variation configuration must not be reported. Got {found:#?}"
    );
}
