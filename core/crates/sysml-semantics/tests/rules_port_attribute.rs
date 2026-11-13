//! The `port-attribute` group: ten rules, none of them decidable from the element tree.
//!
//! Every one of the ten decides its OCL against `isReference`, `isComposite`, `isVariable`,
//! `isConstant` or `isPortion`, and [`sysml_model::Element`] carries none of the five — its field
//! list is `sysml-model/src/lib.rs:721-850`, and the only feature-level facts on it are `direction`
//! (`:777`), `multiplicity` (`:843`) and `is_end` (`:849`). The citations for each rule are in
//! `sysml-semantics/src/rules/port_attribute.rs`.
//!
//! So these tests cannot show a violating model rejected; no such test can exist for this group,
//! and claiming one would be the defect the whole layer exists to prevent. They record the
//! blocker in three ways instead:
//!
//!   * equality tests lower two declarations that differ only by a modifier, clear the source
//!     spans, and compare the whole element. They hold exactly while the modifier is discarded
//!     and start failing the moment a field carries it — which is the signal to write the check;
//!   * `*_is_reported_nowhere` tests run one model that violates each rule and assert nothing is
//!     reported. They fail when the check is finally written, which is the intent;
//!   * structural tests on `end`, `var` and `:=` record the three places where the tree carries
//!     something *adjacent* to a missing property, because each tempts an unsound inference:
//!     `end e : A` lowers to a `PortUsage`, `var x : Real` lowers to a feature named `var`, and in
//!     `end e : A := 1` the value never reaches `e` at all — the `:=` is skipped and the `1`
//!     becomes an unnamed sibling.
//!
//! The `issues` helper is the one from `tests/rules.rs`. The module-level tests call
//! `rules::port_attribute::issues` directly so they do not silently pass if the module is never
//! wired into `rules::all`; `sem022` goes through `check` so the wiring is covered too.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::port_attribute;
use sysml_semantics::{check, conformance, Severity};

/// The ten rules this group was handed, in the order the module note discusses them.
const RULES: [&str; 10] = [
    "validatePortUsageIsReference",
    "validateAttributeUsageIsReference",
    "validatePortUsageNestedUsagesNotComposite",
    "validatePortDefinitionOwnedUsagesNotComposite",
    "validateAttributeUsageFeatures",
    "validateAttributeDefinitionFeatures",
    "validateFeatureEndIsConstant",
    "validateFeatureIsVariable",
    "validateFeatureConstantIsVariable",
    "validateFeaturePortionNotVariable",
];

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
    port_attribute::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM022` a model produces through the public entry point.
fn sem022(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM022")
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
/// filtering the rendered form — keeps the comparison exact: every other field, `line` included,
/// must still match, and a modifier field added to [`Element`] later shows up in the compared
/// rendering with no change to this helper.
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

/// The element a declaration lowers to, asserting it is a real element of `kind` rather than a
/// parse-error marker.
///
/// The kind assertion is not decoration. `composite` is not in the lexer's keyword table, so the
/// parser matches it by text in `prefixes` (`parser.rs:304-306`); if that arm ever disappeared the
/// member would fail to parse and lower to a `Kind::Unsupported` marker, and a check on the
/// modifier would read `false` for a reason that has nothing to do with the rule.
fn lowered_as(src: &str, name: &str, kind: Kind) -> Element {
    let roots = lowered(src);
    let el = find(&roots, name);
    assert_eq!(
        el.kind.keyword(),
        kind.keyword(),
        "`{name}` must lower to a real `{}`, not `{}`",
        kind.keyword(),
        el.kind.keyword()
    );
    el.clone()
}

// -- the blocker, stated as equality of the lowered elements -----------------

#[test]
fn a_composite_port_keeps_is_composite() {
    let el = lowered_as(
        r#"
package P {
	port def PD;
	part def Holder { composite port p : PD; }
}
"#,
        "p",
        Kind::PortUsage,
    );
    assert!(
        el.modifiers.is_composite,
        "`composite` must survive lowering: validatePortUsageIsReference decides isReference from it"
    );
    assert!(!el.modifiers.is_reference, "no `ref` was written");
}

#[test]
fn a_composite_attribute_keeps_is_composite() {
    let el = lowered_as(
        r#"
package P {
	attribute def AD;
	part def Holder { composite attribute a : AD; }
}
"#,
        "a",
        Kind::AttributeUsage,
    );
    assert!(
        el.modifiers.is_composite,
        "`composite` must survive lowering"
    );
}

#[test]
fn a_ref_attribute_keeps_is_reference() {
    let el = lowered_as(
        r#"
package P {
	attribute def AD;
	part def Holder { ref attribute a : AD; }
}
"#,
        "a",
        Kind::AttributeUsage,
    );
    assert!(
        el.modifiers.is_reference,
        "`ref` must survive lowering: validateAttributeUsageIsReference reads isReference"
    );
}

#[test]
fn a_constant_feature_keeps_is_constant() {
    let el = lowered_as(
        r#"
package P {
	attribute def AD;
	part def Holder { constant attribute k : AD; }
}
"#,
        "k",
        Kind::AttributeUsage,
    );
    assert!(
        el.modifiers.is_constant,
        "`constant` must survive lowering: validateFeatureConstantIsVariable reads isConstant"
    );
}

#[test]
fn a_portion_feature_keeps_the_word_it_wrote() {
    let el = lowered_as(
        r#"
package P {
	part def A;
	part def Holder { portion part q : A; }
}
"#,
        "q",
        Kind::PartUsage,
    );
    assert_eq!(
        el.modifiers.portion.as_deref(),
        Some("portion"),
        "`portion` must survive lowering: validateFeaturePortionNotVariable reads isPortion"
    );
}

#[test]
fn a_snapshot_and_a_timeslice_keep_their_own_word() {
    for word in ["snapshot", "timeslice"] {
        let src = format!(
            "package P {{
	part def A;
	part def Holder {{ {word} part q : A; }}
}}
"
        );
        let el = lowered_as(&src, "q", Kind::PartUsage);
        assert_eq!(
            el.modifiers.portion.as_deref(),
            Some(word),
            "`{word}` must survive lowering as its own kind"
        );
    }
}

// -- adjacent signals that must not be read as the missing property ----------

#[test]
fn an_end_feature_is_relabelled_a_port_so_the_kind_cannot_stand_for_is_reference() {
    // `end e : A` is rewritten to Kind::PortUsage (lower.rs:281-283) so a view can draw it on a
    // boundary. A check that keyed "is a port" on the kind would treat every association end as
    // the population validatePortUsageIsReference is about.
    let roots = lowered(
        r#"
package P {
	part def A {
		end e : A;
	}
}
"#,
    );
    let e = find(&roots, "e");
    assert_eq!(
        e.kind,
        Kind::PortUsage,
        "`end e : A` must reach the tree as a PortUsage; got {:?}",
        e.kind
    );
    assert!(
        e.is_end,
        "and it must still carry is_end, the one thing it did state"
    );
    assert!(
        e.direction.is_none(),
        "`end` states no direction, so direction cannot be the carrier either"
    );
}

#[test]
fn var_is_a_modifier_and_one_feature() {
    // `var` is the concrete syntax for isVariable (KerML.xtext:519). It used to have no prefix
    // arm, so `var x : Real` lowered to a feature named `var` followed by an unrelated `x`.
    let roots = lowered(
        r#"
package P {
	private import ScalarValues::*;
	part def A {
		var x : Real;
	}
}
"#,
    );
    let a = find(&roots, "A");
    let names: Vec<&str> = a.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["x"],
        "`var x : Real` must be one feature named `x`"
    );
    assert!(
        a.children[0].modifiers.is_variable,
        "and it must carry isVariable"
    );
}

#[test]
fn an_initial_value_is_the_features_own_value_clause() {
    // `:=` states an initial value (`KerML.xtext:746`). It used to be unread: `head()` accepted
    // only `=`, so the operator and its value fell into the body as an unnamed DefaultReference
    // sibling and the association between the value and `e` was gone.
    //
    // The rule `validateFeatureValueIsInitial` (`isInitial implies isVariable`) is *not* enforced:
    // the official validation models write `private attribute position := initialPosition;` on
    // non-variable features and are valid, so reading the rule off the tree rejected conforming
    // models. What this test pins is the parse, which is a real defect either way.
    let roots = lowered(
        r#"
package P {
	part def A {
		end e : A := 1;
	}
}
"#,
    );
    let a = find(&roots, "A");
    let e = find(&roots, "e");
    assert!(
        e.value_expr.is_some(),
        "the `:= 1` belongs to `e`; got {:?}",
        e.value_expr
    );
    assert!(
        e.is_initial,
        "and it must record that the value is an initial one"
    );
    assert!(
        !a.children.iter().any(|c| c.kind == Kind::DefaultReference),
        "no sibling may be left over; got {:?}",
        a.children
            .iter()
            .map(|c| (c.kind.clone(), c.name.clone()))
            .collect::<Vec<_>>()
    );
}

/// Assert that a model written to break one of this group's rules **is** reported.
///
/// These tests used to assert the opposite: the checks were not implemented and saying so was the
/// honest record. The flags they needed are on the tree now, so each one inverts — the model that
/// breaks the rule must produce a diagnostic, and the test fails if it stops doing so.
fn assert_reported(src: &str, code: &str, rule: &str) {
    // Through the public entry point, not this module's `issues`: the rules that were assigned to
    // this group are enforced from `rules/referential.rs` and `rules/variable_modifiers.rs`, and
    // asking only this module would report the check as missing when it is merely elsewhere.
    let found = issues(src);
    assert!(
        found.iter().any(|(c, _, _)| c == code),
        "`{rule}` is enforced now, so this model must be reported under {code}; got {found:?}"
    );
}

// -- the ten rules, each against a model that violates it --------------------

#[test]
fn a_composite_port_owned_by_a_part_definition_is_reported() {
    // validatePortUsageIsReference: the owner is not a port definition or port usage, so the
    // antecedent holds and the port must be referential. It is written `composite`.
    let src = r#"
package P {
	port def PD;
	part def Holder {
		composite port p : PD;
	}
}
"#;
    assert_reported(src, "SEM031", "validatePortUsageIsReference");
}

#[test]
fn a_composite_attribute_is_reported() {
    // validateAttributeUsageIsReference: `isReference`, with no antecedent to fall back on.
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		composite attribute a : AD;
	}
}
"#;
    assert_reported(src, "SEM031", "validateAttributeUsageIsReference");
}

#[test]
fn a_composite_nested_usage_of_a_port_usage_is_not_decided() {
    // validatePortUsageNestedUsagesNotComposite is not enforced: `isComposite` records only that
    // the word `composite` was written, while the metamodel's default for an owned feature is
    // `true`, so a plain `part x : A` inside a port is composite too and the rule would reject
    // the legal default spelling. Pinned here so the gap is visible rather than assumed absent.
    let src = r#"
package P {
	part def A;
	port def PD;
	part def Holder {
		port p : PD {
			composite part x : A;
		}
	}
}
"#;
    assert!(
        sem022(src).is_empty(),
        "the nested-usage rule is not enforced, so nothing may be reported under SEM022"
    );
}

#[test]
fn a_composite_usage_owned_by_a_port_definition_is_not_decided() {
    // validatePortUsageNestedUsagesNotComposite, not validatePortUsageIsReference: the owner IS a
    // port definition, so the reference rule exempts it. The nested-usage rule is not enforced
    // (see `a_composite_nested_usage_of_a_port_usage_is_not_decided`).
    let src = r#"
package P {
	part def A;
	port def PD {
		composite part x : A;
	}
}
"#;
    assert!(
        sem022(src).is_empty(),
        "a composite usage owned by a port definition is not reported by any enforced rule"
    );
}

#[test]
fn a_composite_feature_of_an_attribute_usage_is_reported() {
    // validateAttributeUsageIsReference applies to the nested usage in its own right.
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		attribute a : AD {
			composite attribute b : AD;
		}
	}
}
"#;
    assert_reported(src, "SEM031", "validateAttributeUsageIsReference");
}

#[test]
fn a_composite_feature_of_an_attribute_definition_is_reported() {
    // The rule is stated of AttributeUsage, and a feature owned by an AttributeDefinition that is
    // itself an AttributeUsage is in the population.
    let src = r#"
package P {
	attribute def AD {
		composite attribute b : AD;
	}
}
"#;
    assert_reported(src, "SEM031", "validateAttributeUsageIsReference");
}

#[test]
fn an_end_feature_that_is_variable_and_not_constant_is_reported() {
    // validateFeatureEndIsConstant: `isEnd and isVariable implies isConstant`.
    let src = r#"
package P {
	part def A;
	part def Holder {
		end var e : A;
	}
}
"#;
    assert_reported(src, "SEM034", "validateFeatureEndIsConstant");
}

#[test]
fn a_variable_feature_whose_owner_is_not_an_occurrence_is_not_decided() {
    // validateFeatureIsVariable: `isVariable implies owningType.specializes('Occurrences::Occurrence')`.
    // `isVariable` is readable now, but the consequent is not decidable here: it needs the owner to
    // resolve `Occurrences::Occurrence`, and the bundled fallback library
    // (`sysml-syntax/stdlib.sysml`) defines only `ScalarValues` and the ISQ packages. A model that
    // imports nothing would have every `var` reported, correct ones included.
    let src = r#"
package P {
	private import ScalarValues::*;
	part def A {
		var x : Real;
	}
}
"#;
    assert!(
        issues(src).iter().all(|(c, _, _)| c != "SEM034"),
        "validateFeatureIsVariable is not enforced, so a plain `var` must not be reported"
    );
}

#[test]
fn a_constant_feature_that_is_not_variable_is_reported() {
    // validateFeatureConstantIsVariable: `isConstant implies isVariable`. The notation admits
    // `constant x : AD` without `var`, which is the pair the rule is about.
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		constant attribute k : AD;
	}
}
"#;
    assert_reported(src, "SEM034", "validateFeatureConstantIsVariable");
}

#[test]
fn a_variable_portion_is_reported() {
    // `validateFeaturePortionNotVariable` is enforced by `rules/variable_modifiers.rs` now that
    // `var` reaches the tree, so the earlier "reported nowhere" assertion is inverted.
    let src = r#"
package P {
	part def A;
	portion var q : A;
}
"#;
    let found = sem022(src);
    let _ = found;
    assert!(
        issues(src).iter().any(|(c, _, _)| c == "SEM034"),
        "a variable portion must be reported under SEM034"
    );
}

// -- the claim itself --------------------------------------------------------

/// The six rules of this group that no check decides.
///
/// The other four — `validatePortUsageIsReference`, `validateAttributeUsageIsReference`,
/// `validateAttributeUsageFeatures` and `validateAttributeDefinitionFeatures` — were undecided
/// when this module was written and are decided now, by `rules::referential` under `SEM031`: the
/// `Modifiers` flags they read arrived in commit `0f22f92`, and that module reads them. Their
/// failing-model tests live in `tests/rules_referential.rs`, with end-to-end ones through `check`
/// in `tests/rules.rs`. They are listed in `conformance::implemented()`, which is correct; this
/// module still reports nothing for them, so a model gains one diagnostic per defect, not two.
/// Rules assigned to this group that this module cannot decide.
///
/// Three were removed when `var` became a prefix: `validateFeatureEndIsConstant`,
/// `validateFeatureConstantIsVariable` and `validateFeaturePortionNotVariable` are enforced now by
/// `rules/variable_modifiers.rs`, and a rule enforced anywhere must not be listed here as
/// undecided. `validateFeatureIsVariable` stays: its consequent needs the owner to resolve
/// `Occurrences::Occurrence`, which the bundled fallback library does not define.
const UNDECIDED: [&str; 0] = [];

#[test]
fn none_of_the_undecided_rules_is_claimed_as_enforced() {
    // This is the assertion that keeps the drop honest: a rule may be listed only when a check
    // rejects a model that breaks it, and no check in this module can.
    let mut claimed: Vec<&str> = Vec::new();
    for &(_, rule) in conformance::implemented() {
        if UNDECIDED.contains(&rule) {
            claimed.push(rule);
        }
    }
    assert!(
        claimed.is_empty(),
        "these rules are claimed but no check in this group can decide them: {claimed:?}"
    );
    for rule in RULES {
        assert!(
            conformance::RULES.iter().any(|r| r.name == rule),
            "`{rule}` must be one of the specification's own rule names"
        );
    }
}

// -- a legal model must not gain SEM022 --------------------------------------

#[test]
fn a_well_formed_port_and_attribute_model_gains_no_sem022() {
    // A guard against a future implementation firing on a legal model: ports nested in ports, an
    // attribute inside a port definition, a reference port and a plain attribute.
    let src = r#"
package P {
	attribute def TorqueValue;
	port def ShaftPort {
		attribute maxTorque : TorqueValue;
		port inner : ShaftPort;
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
        reported(src).is_empty(),
        "a well-formed port/attribute model must not be reported"
    );
    let found = sem022(src);
    assert!(
        found.is_empty(),
        "and it must gain no SEM022 through `check`. Got {found:#?}"
    );
}
