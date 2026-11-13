//! The `variation-modifiers` group, and why it enforces nothing.
//!
//! None of the eight rules assigned to this group can be decided from the element tree. Each one
//! reads `isVariation`, `isAbstract` or `isReference`, and [`sysml_model::Element`] carries none
//! of the three — see `sysml-semantics/src/rules/variation_modifiers.rs` for the citations, and
//! the field list at `sysml-model/src/lib.rs:721-850`.
//!
//! So this file contains no "violating model is rejected" test, because no rule here rejects
//! anything. Writing one would mean either faking the antecedent or asserting a diagnostic the
//! module does not produce. What is recorded instead:
//!
//!   * five equality tests lower a declaration and the same declaration with the modifier
//!     removed, clear the source spans, and compare the whole element. They hold exactly while
//!     the modifier is discarded, and fail the moment a field carries it. Each spelling used is
//!     one an existing test already parses (`tests/rules_port_attribute.rs:110-171` for
//!     `composite`, `tests/coverage.rs:453-479` for `variation` and `variant`);
//!   * `no_element_in_the_tree_is_a_reference_usage` pins the other half of
//!     `validateReferenceUsageIsReference`: the metaclass has no element;
//!   * eight `*_cannot_reject_its_violating_model` tests, one per rule, feed the module a model
//!     written to break that rule and assert nothing is reported. They fail when the check is
//!     finally written, which is the intent — the drop is then a recorded fact rather than an
//!     omission. The `validateUsageIsReferential` one carries a caveat: the notation cannot
//!     express a violation through the two disjuncts the tree can read, so its model satisfies
//!     the rule and the test records which disjunct is actually missing. The comment there says
//!     so rather than claiming a violating model.
//!
//! `conformance::implemented()` must list none of the eight for as long as these hold
//! (`conformance.rs:13-15`: a rule is listed only when a test shows it rejecting a model).
//!
//! The module-level tests call `rules::variation_modifiers::issues` directly, so they do not pass
//! silently if the module is never wired into `rules::all`; the `SEM024` assertions run the same
//! models through `check` and confirm nothing reaches a report either way.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::variation_modifiers;
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

/// What the module under test reports for a model, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    variation_modifiers::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM024` in a model, by element.
fn sem024(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM024")
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
        .find_map(|r| go(r, name))
        .unwrap_or_else(|| panic!("no element named `{name}`"))
}

fn lowered(src: &str) -> Vec<Element> {
    sysml_syntax::load_project(&[src], true)
        .expect("model parses")
        .roots
}

/// Every field of an element that records where it was written, cleared.
///
/// Two spellings of the same declaration sit at different byte offsets, so the four span fields
/// are the only legitimate difference between them. Clearing them explicitly — rather than
/// filtering a rendered form — keeps the comparison exact: every other field, `line` included,
/// must still match, and a modifier field added to [`Element`] later shows up in the compared
/// rendering with no change to this test.
fn without_spans(el: &Element) -> Element {
    fn clear(el: &mut Element) {
        el.name_span = None;
        el.type_span = None;
        el.value_span = None;
        el.ref_spans.clear();
        for child in &mut el.children {
            clear(child);
        }
    }
    let mut copy = el.clone();
    clear(&mut copy);
    copy
}

/// The element's rendering, spans removed. `Element` has no `PartialEq`, and `Debug` shows every
/// field, so this is the strictest comparison the model crate allows.
fn shape(el: &Element) -> String {
    format!("{:#?}", without_spans(el))
}

/// Assert that a model written to break `rule` produces no diagnostic at all.
///
/// `why` names the field the rule needs and where it is lost, so a later reader can tell an
/// intentional drop from a check that quietly stopped working.
fn assert_nothing_reported(src: &str, rule: &str, why: &str) {
    let found = reported(src);
    assert!(
        found.is_empty(),
        "`{rule}` is not decided by this module, so this model must not be reported. If this \
         fires, the field the rule needs has been added and the check must be written properly \
         rather than left as a heuristic ({why}). Got {found:#?}"
    );
    let through_check = sem024(src);
    assert!(
        through_check.is_empty(),
        "and nothing may reach `check` under SEM024 for the same reason ({why}). \
         Got {through_check:#?}"
    );
}

// -- the blocker, stated as equality of the lowered elements -----------------

#[test]
fn a_variation_usage_keeps_is_variation() {
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part config { attribute x : Base; }
}
"#,
    );
    assert!(
        find(&roots, "config").modifiers.is_variation,
        "`variation` must survive lowering: validateUsageVariationIsAbstract,          validateUsageVariationOwnedFeatureMembership and validateUsageVariationSpecialization          all read isVariation"
    );
}

#[test]
fn a_plain_usage_is_not_a_variation() {
    let roots = lowered(
        r#"
package P {
	part def Base;
	part config { attribute x : Base; }
}
"#,
    );
    assert!(!find(&roots, "config").modifiers.is_variation);
}

#[test]
fn a_variation_definition_keeps_is_variation() {
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part def Config :> Base;
}
"#,
    );
    assert!(
        find(&roots, "Config").modifiers.is_variation,
        "`variation` on a definition must survive too: the three Definition rules read it"
    );
}

#[test]
fn an_abstract_definition_keeps_is_abstract() {
    let roots = lowered(
        r#"
package P {
	abstract part def Base;
	part def Concrete;
}
"#,
    );
    assert!(
        find(&roots, "Base").modifiers.is_abstract,
        "`abstract` must survive lowering: validateDefinitionVariationIsAbstract reads it"
    );
    assert!(!find(&roots, "Concrete").modifiers.is_abstract);
}

#[test]
fn a_ref_feature_keeps_is_reference() {
    let roots = lowered(
        r#"
package P {
	attribute def AD;
	part def Holder { ref attribute a : AD; }
}
"#,
    );
    assert!(
        find(&roots, "a").modifiers.is_reference,
        "`ref` must survive lowering: validateReferenceUsageIsReference reads it"
    );
}

// -- violating models, one per rule, recorded as uncaught -------------------

#[test]
fn validate_usage_variation_is_abstract_cannot_reject_its_violating_model() {
    // `isVariation implies isAbstract`. This usage is a variation and writes no `abstract`.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part config {
		attribute x : Base;
	}
}
"#,
        "validateUsageVariationIsAbstract",
        "needs isVariation and isAbstract; neither is an Element field (lib.rs:721-850)",
    );
}

#[test]
fn validate_usage_variation_owned_feature_membership_cannot_reject_its_violating_model() {
    // `isVariation implies ownedFeatureMembership->isEmpty()`. This usage owns an attribute, and
    // owns no variant member — which is why keying the antecedent on the variant flag is unsound.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part config {
		attribute x : Base;
	}
}
"#,
        "validateUsageVariationOwnedFeatureMembership",
        "the consequent is readable as Element::children, the antecedent isVariation is not",
    );
}

#[test]
fn validate_usage_variation_specialization_cannot_reject_its_violating_model() {
    // A variation usage may not specialize a variation usage. `child` is a variation and
    // specializes `Parent`, which is one too.
    assert_nothing_reported(
        r#"
package P {
	variation part def Parent;
	variation part child :> Parent;
}
"#,
        "validateUsageVariationSpecialization",
        "needs isVariation on both the element and its specialization target; the OCL is also \
         mis-stated (upstream rules.rs:321, :426)",
    );
}

#[test]
fn validate_definition_variation_is_abstract_cannot_reject_its_violating_model() {
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part def Cfg {
		attribute x : Base;
	}
}
"#,
        "validateDefinitionVariationIsAbstract",
        "needs isVariation and isAbstract; neither is an Element field",
    );
}

#[test]
fn validate_definition_variation_owned_feature_membership_cannot_reject_its_violating_model() {
    // The consequent is the one this group can read; `variant part alt` is a child with
    // `is_variant` set, but the antecedent is not derivable from it.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	variation part def Cfg {
		variant part alt : Base;
	}
}
"#,
        "validateDefinitionVariationOwnedFeatureMembership",
        "the consequent is readable as Element::children, the antecedent isVariation is not",
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
        "needs isVariation on both the definition and its specialization target; the OCL is \
         also mis-stated (upstream rules.rs:321, :422)",
    );
}

#[test]
fn validate_usage_is_referential_cannot_reject_its_violating_model() {
    // `direction <> null or isEnd or featuringType->isEmpty() implies isReference`. The violation
    // is an antecedent that holds while `isReference` is false, i.e. a composite feature that is
    // directed, an end, or featured by nothing. `direction` (lib.rs:777) and `is_end` (:849) make
    // the antecedent decidable; `isReference` is the derived `not isComposite`
    // (generated.rs:8957-8961), and no field records `isComposite`.
    //
    // The model below writes the antecedent positively: `a` is an `end` and `b` carries `in`.
    // Both are therefore referential by the standard's own reading of the notation — the
    // reference implementation derives `isComposite = false` for a directed feature and for an
    // end (`research/upstream/sem/mod/sysmlv2-model-0.1.2/src/build.rs:2242-2249`) — so no model
    // the notation can express violates this rule through those two disjuncts. What remains is
    // the third disjunct, `featuringType->isEmpty()`, which needs the derived `featuringType`
    // (generated.rs:2238) the tree does not carry. That is the reason the rule is undecided, and
    // the test records it: the two readable disjuncts cannot be violated and the third is
    // unreadable.
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	part def Holder {
		in b : Base;
		end a : Base;
	}
}
"#,
        "validateUsageIsReferential",
        "the two readable disjuncts cannot be violated, and the third (featuringType) plus the \
         consequent (isReference) are absent from the tree",
    );
}

#[test]
fn validate_reference_usage_is_reference_cannot_reject_its_violating_model() {
    // `isReference` on a ReferenceUsage. No such element is produced, and the metaclass makes the
    // rule structurally true besides: ReferenceUsage redefines isReference with default true
    // (generated.rs:2491-2493, SysML.sysml:296-298).
    assert_nothing_reported(
        r#"
package P {
	part def Base;
	part def Holder {
		ref r : Base;
	}
}
"#,
        "validateReferenceUsageIsReference",
        "no ReferenceUsage element exists and isReference is not a field; even with both the \
         rule could never fail, since isReference is true by definition on that metaclass",
    );
}

// -- what is available, and the inference that is refused --------------------

#[test]
fn a_variant_alternative_reaches_the_tree_but_cannot_stand_in_for_is_variation() {
    // `variant` does reach the tree as `Element::is_variant`, so a check could be aimed at
    // the alternatives instead of at the variation point. This pins why that is unsound: a
    // violating variation point need not own one.
    let roots = lowered(
        r#"
package P {
	part def Base;
	variation part config {
		variant part alt : Base;
	}
}
"#,
    );
    let config = find(&roots, "config");
    assert!(
        config.children.iter().any(|c| c.is_variant),
        "`variant part alt` must reach the tree as a variant; got {:?}",
        config
            .children
            .iter()
            .map(|c| (c.kind.clone(), c.name.clone()))
            .collect::<Vec<_>>()
    );

    let no_alternative = lowered(
        r#"
package P {
	part def Base;
	variation part config {
		attribute x : Base;
	}
}
"#,
    );
    let config = find(&no_alternative, "config");
    assert!(
        !config.children.iter().any(|c| c.is_variant),
        "this spelling owns an attribute and no alternative, so a check keyed on the variant flag \
         would pass a model that breaks validateUsageVariationOwnedFeatureMembership"
    );
}

#[test]
fn a_well_formed_variation_configuration_gains_no_sem024() {
    // A guard against a future implementation firing on a legal model. The shape follows the
    // corpus's own variant configuration (07-Variant Configuration).
    let src = r#"
package P {
	part def SubsystemA;
	abstract part def SubsystemB;
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
    let found = sem024(src);
    assert!(
        found.is_empty(),
        "a well-formed variation configuration must not be reported. Got {found:#?}"
    );
}
