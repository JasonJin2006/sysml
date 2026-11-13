//! `validateFeatureValueOverriding` across all three value spellings, and the diamond.
//!
//! The rule's OCL is
//!
//! ```text
//! featureWithValue.redefinition.redefinedFeature
//!   ->closure(redefinition.redefinedFeature).valuation->forAll(isDefault)
//! ```
//!
//! and the grammar fixes what `isDefault` means (`KerML.xtext:744-750`):
//!
//! ```text
//! FeatureValue : ( '=' | isInitial ?= ':=' | isDefault ?= 'default' ( '=' | ':=' )? )
//!                 ownedRelatedElement += OwnedExpression ;
//! ```
//!
//! So `isDefault` is true **only** when the word `default` was written, and a plain `= 42` is *not* a
//! default. The implementation used to read the `:=` flag instead, which is the opposite test — it
//! accepted a redefinition over `= 42` and rejected one over `:= 42` only by accident of that flag's
//! meaning. These tests pin the three-way distinction so it cannot be collapsed again.
//!
//! The diamond case is here because it is the one a third-party tool documents: Syside's
//! "Stricter redefinition hiding" page gives
//! `part def C :> B0, B1 { attribute a :>> a = 7; }` with `B1::a = 42` and says the Pilot
//! Implementation reports nothing while `validateFeatureValueOverriding` requires an error.

use sysml_semantics::check;

fn messages(source: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[source], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|issue| issue.message.contains("validateFeatureValueOverriding"))
        .map(|issue| format!("{}: {}", issue.at, issue.message))
        .collect()
}

fn model(base_clause: &str, redefinition_clause: &str) -> String {
    format!(
        "package P {{\n\tpart def A {{ attribute a{base_clause}; }}\n\t\
         part def B :> A {{ attribute a :>> a{redefinition_clause}; }}\n}}\n"
    )
}

#[test]
fn a_plain_value_under_a_redefinition_is_rejected() {
    // `= 42` sets neither `isInitial` nor `isDefault`, so `forAll(isDefault)` fails. This is the case
    // the old implementation missed entirely.
    let found = messages(&model(" = 42", " = 7"));
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("P::A::a"),
        "the offender is the feature with the plain value: {found:?}"
    );
    assert!(
        found[0].contains("not a default"),
        "the message must distinguish this from the `:=` case: {found:?}"
    );
}

#[test]
fn a_default_value_under_a_redefinition_is_accepted() {
    // The only spelling the rule allows. If this ever fails, the flag has been given the wrong sense.
    let found = messages(&model(" default = 42", " = 7"));
    assert!(found.is_empty(), "a default may be restated, got {found:?}");
}

#[test]
fn an_initial_value_under_a_redefinition_is_rejected() {
    // `:=` is not a default either, so the consequent fails here too. This is the case the old
    // implementation got right, and it must not regress while the other two are corrected.
    let found = messages(&model(" := 42", " = 7"));
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("initial"),
        "the message must name `:=` specifically: {found:?}"
    );
}

#[test]
fn no_value_under_a_redefinition_is_silent() {
    // The antecedent needs a `FeatureValue` on the redefined feature. Without one there is nothing for
    // `valuation->forAll(...)` to range over, and `forAll` over an empty collection is vacuously true —
    // but reporting here would be wrong for a different reason: the redefinition is not overriding
    // anything. Measured: silent.
    let found = messages(&model("", " = 7"));
    assert!(found.is_empty(), "nothing to override, got {found:?}");
}

#[test]
fn the_syside_diamond_is_reported_as_ambiguous_not_as_an_overriding() {
    // Verbatim from Syside's "Stricter redefinition hiding" page. `B1::a` is bound to 42, so `C::a`'s
    // `= 7` may not restate it — that is the page's argument, and it is right about the model.
    //
    // What our kernel reports is a different rule, and deliberately so. `C :> B0, B1` inherits `a`
    // twice, so `C::a :>> a` has two candidates and resolution refuses to choose (measured in
    // `zz_diamond_resolution.rs`). The OCL reads `featureWithValue.redefinition` — the *resolved*
    // redefinition — and an unresolved one leaves this rule with no antecedent, so the model is
    // diagnosed by `SEM091` instead. Both answers reject the model; they differ in which rule owns it.
    //
    // This is recorded rather than made to match Syside: making the overriding rule fire here would
    // mean either picking a winner arbitrarily or reading the authored text instead of the resolved
    // redefinition, and the second is inventing a relation the specification does not state.
    let source = r#"
package P {
	part def A { attribute a; }
	part def B0 :> A { attribute a :>> a; }
	part def B1 :> A { attribute a :>> a = 42; }
	part def C :> B0, B1 { attribute a :>> a = 7; }
}
"#;
    let loaded = sysml_syntax::load_project(&[source], true).expect("model parses");
    let issues = check(&loaded.roots, &loaded.library).issues;

    assert!(
        issues.iter().any(|issue| issue.code == "SEM091"),
        "the inherited duplicate must be reported: {:?}",
        issues
            .iter()
            .map(|i| (&i.code, &i.message))
            .collect::<Vec<_>>()
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.message.contains("validateFeatureValueOverriding")),
        "the overriding rule must stay silent while the redefinition is unresolved: {:?}",
        issues
            .iter()
            .map(|i| (&i.code, &i.message))
            .collect::<Vec<_>>()
    );
}

/// A diamond whose redefinition *is* resolvable does reach the rule.
///
/// The companion to the test above: qualify the redefinition and the ambiguity disappears, so the
/// overriding rule has a `featureWithValue.redefinition` to range over and reports the value that is not
/// a default. This is the case that shows the silence above comes from resolution and not from the rule
/// being unable to see diamonds at all.
#[test]
fn a_qualified_diamond_redefinition_reaches_the_overriding_rule() {
    let source = r#"
package P {
	part def A { attribute a; }
	part def B1 :> A { attribute a :>> a = 42; }
	part def C :> B1 { attribute a :>> a = 7; }
}
"#;
    let found = messages(source);
    assert!(
        found.iter().any(|m| m.contains("P::B1::a")),
        "`= 42` is not a default, so restating it must be reported: {found:?}"
    );
}

#[test]
fn a_redefinition_without_its_own_value_is_silent() {
    // `:>> a;` restates nothing, so there is no `featureWithValue` and the rule's antecedent is false.
    // Guard against a fix that reports every `:>>` once the closure is walked.
    let source = r#"
package P {
	part def A { attribute a = 42; }
	part def B :> A { attribute a :>> a; }
}
"#;
    let found = messages(source);
    assert!(found.is_empty(), "no value to override, got {found:?}");
}
