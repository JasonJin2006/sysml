//! `Element::is_default` — the value was written with `default`.
//!
//! `validateFeatureValueOverriding` is the only specification rule that reads the difference between a
//! plain value and a default one:
//!
//! ```text
//! featureWithValue.redefinition.redefinedFeature
//!   ->closure(redefinition.redefinedFeature).valuation->forAll(isDefault)
//! ```
//!
//! Before this flag existed the parser read the word `default` and used it only to decide whether an
//! expression followed, so `= 42` and `default = 42` lowered to identical elements and the rule had
//! nothing to test. These tests pin that the three spellings now differ, and that a `default` with no
//! value is not a value clause at all.

use sysml_model::{Element, Kind};

fn lowered(source: &str) -> sysml_syntax::Loaded {
    sysml_syntax::load_project(&[source], true).expect("model parses")
}

fn find_kind<'a>(roots: &'a [Element], kind: &Kind) -> &'a Element {
    fn go<'a>(elements: &'a [Element], kind: &Kind) -> Option<&'a Element> {
        for element in elements {
            if &element.kind == kind {
                return Some(element);
            }
            if let Some(found) = go(&element.children, kind) {
                return Some(found);
            }
        }
        None
    }
    go(roots, kind).unwrap_or_else(|| panic!("no element of kind {kind:?}"))
}

/// The feature named `a`, which is what carries the value clause in every case below.
fn feature_a<'a>(roots: &'a [Element]) -> &'a Element {
    fn go<'a>(elements: &'a [Element]) -> Option<&'a Element> {
        for element in elements {
            if element.name == "a" {
                return Some(element);
            }
            if let Some(found) = go(&element.children) {
                return Some(found);
            }
        }
        None
    }
    go(roots).expect("no element named `a`")
}

const HEADER: &str = "package P {\n\tattribute def T;\n";
const FOOTER: &str = "}\n";

fn model(clause: &str) -> String {
    format!("{HEADER}\tattribute a : T {clause};\n{FOOTER}")
}

#[test]
fn a_plain_value_is_not_a_default() {
    // Positive control for the whole group: `= 42` must carry a value and no `default` word. If this
    // failed, every other assertion here would be meaningless.
    let loaded = lowered(&model("= 42"));
    let attribute = feature_a(&loaded.roots);
    assert!(
        attribute.value_expr.is_some(),
        "`= 42` must lower to a value expression"
    );
    assert!(
        !attribute.is_default,
        "a plain `= 42` writes no `default`, so the flag must stay false"
    );
    assert!(!attribute.is_initial, "`= 42` is not `:=`");
}

#[test]
fn the_default_equals_form_is_recorded() {
    let loaded = lowered(&model("default = 42"));
    let attribute = feature_a(&loaded.roots);
    assert!(
        attribute.value_expr.is_some(),
        "`default = 42` still has a value"
    );
    assert!(
        attribute.is_default,
        "`default = 42` must be distinguishable from `= 42`"
    );
}

#[test]
fn the_default_prefix_form_is_recorded() {
    // The other spelling: `default 42`, with no `=`. Both are the same `FeatureValue` with
    // `isDefault = true`, so both must set the flag.
    let loaded = lowered(&model("default 42"));
    let attribute = feature_a(&loaded.roots);
    assert!(
        attribute.value_expr.is_some(),
        "`default 42` still introduces a value expression"
    );
    assert!(
        attribute.is_default,
        "`default 42` must set the flag as `default = 42` does"
    );
}

#[test]
fn an_initial_value_is_not_a_default() {
    // `:=` and `default` are different modifiers and only one of them is `isDefault`. Conflating them
    // would make `validateFeatureValueIsInitial` and this rule read each other's flag.
    let loaded = lowered(&model(":= 42"));
    let attribute = feature_a(&loaded.roots);
    assert!(attribute.is_initial, "`:= 42` is an initial value");
    assert!(!attribute.is_default, "`:= 42` is not a default value");
}

#[test]
fn the_two_value_spellings_are_no_longer_identical() {
    // The actual regression: before the flag, these two models lowered to the same element. Comparing
    // the triple rather than one field, so a future change that keeps `is_default` but drops the value
    // still fails here.
    let plain = lowered(&model("= 42"));
    let defaulted = lowered(&model("default = 42"));
    let plain_feature = feature_a(&plain.roots);
    let default_feature = feature_a(&defaulted.roots);
    assert_ne!(
        (plain_feature.is_default, plain_feature.value_expr.is_some()),
        (
            default_feature.is_default,
            default_feature.value_expr.is_some()
        ),
        "`= 42` and `default = 42` must not lower identically any more"
    );
}

#[test]
fn the_attribute_usage_itself_is_still_an_attribute() {
    // Guard against the new field having disturbed the kind. `attribute a : T` is an `AttributeUsage`
    // whether or not it carries a value, and `is_reference` follows the metaclass, not the value clause.
    let loaded = lowered(&model("default = 42"));
    let attribute = find_kind(&loaded.roots, &Kind::AttributeUsage);
    assert!(
        attribute.is_reference,
        "an AttributeUsage is a reference by definition"
    );
    assert!(attribute.is_default);
}
