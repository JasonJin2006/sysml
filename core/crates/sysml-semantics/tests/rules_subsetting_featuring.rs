//! `validateSubsettingFeaturingTypes`, and the direction bug it exposed.
//!
//! The rule is `subsettingFeature.canAccess(subsettedFeature)`, which the reference implements as a
//! four-line recursion over featuring types (`FeatureUtil.java:368-381`) behind a guard that skips a
//! subsetted feature stating no featuring type (`KerMLValidator.xtend:825-833`).
//!
//! Implementing it required fixing `featured_within`, which had been asking its subtyping question in
//! the wrong direction. The old version walked the *container's* supertypes and asked whether the
//! feature's *owner* was a subtype of one of them. Every `part def` specializes the library's
//! `Parts::Part`, so `Parts::Part` is always among a container's supertypes and the test answered
//! `true` for any two part definitions. That made the predicate useless for this rule and silently
//! permissive for the one rule already using it.
//!
//! These tests pin both: the rule's own behaviour, and the direction of the predicate underneath it.

use sysml_semantics::check;

/// Check a model and return `code message` strings, with the standard library linked in.
///
/// `load_project` is what the other suites use: it parses the source and loads the library as a single
/// project, which is the only way a qualified path like `P::A::a` resolves the way it does in a real
/// run. Checking `roots` against an empty library reports every library name as unresolved and hides
/// the rule under a pile of `REF001`s.
fn messages(source: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[source], true).expect("the model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .map(|issue| format!("{} {}", issue.code, issue.message))
        .collect()
}

fn mentions(messages: &[String], needle: &str) -> bool {
    messages.iter().any(|message| message.contains(needle))
}

/// The reference's own message for this rule (`KerMLValidator.xtend:224`).
///
/// The rule is identified by its message rather than by its name because the message is what the
/// reference implementation shows a user, and matching it is what makes this the same diagnostic. The
/// pilot's assertion text is that message, so a rule worded differently is one the scorer cannot count
/// however right its logic is.
const FEATURING: &str = "Must be an accessible feature";

#[test]
fn a_subsetted_feature_in_an_unrelated_type_is_reported() {
    // `b` is owned by `B`; `a` is owned by `A`; `B` does not specialize `A`, so `b` cannot reach `a`.
    let model = "\
package P {
    attribute def T;
    part def A { attribute a : T; }
    part def B { attribute b subsets P::A::a : T; }
}
";
    let found = messages(model);
    assert!(
        mentions(&found, FEATURING),
        "a subsetting that cannot reach its target must be reported, got: {found:#?}"
    );
}

#[test]
fn a_subsetted_feature_reached_through_specialization_is_accepted() {
    // `B :> A` puts `A` in `B`'s supertype closure, so `b` reaches `a`.
    let model = "\
package P {
    attribute def T;
    part def A { attribute a : T; }
    part def B :> A { attribute b subsets P::A::a : T; }
}
";
    let found = messages(model);
    assert!(
        !mentions(&found, FEATURING),
        "a reachable subsetting must not be reported, got: {found:#?}"
    );
}

#[test]
fn a_subsetted_feature_with_no_featuring_type_is_skipped() {
    // The validator's guard: `!subsettedFeaturingTypes.isEmpty()`. `a` is owned by the package, not a
    // type, so it has no featuring type and the rule does not apply.
    let model = "\
package P {
    attribute def T;
    attribute a : T;
    part def B { attribute b subsets a : T; }
}
";
    let found = messages(model);
    assert!(
        !mentions(&found, FEATURING),
        "a subsetted feature with no featuring type is out of scope, got: {found:#?}"
    );
}

#[test]
fn a_subsetted_feature_in_the_same_type_is_accepted() {
    let model = "\
package P {
    attribute def T;
    part def B { attribute a : T; attribute b subsets a : T; }
}
";
    let found = messages(model);
    assert!(
        !mentions(&found, FEATURING),
        "a sibling feature is reachable, got: {found:#?}"
    );
}

#[test]
fn the_reverse_specialization_is_also_accepted() {
    // `A :> B` — `a` is owned by `B` and `b`'s featuring type `A` reaches `B` through the clause.
    // This is the case that a reversed subtyping test gets wrong: it would report a legal model.
    let model = "\
package P {
    attribute def T;
    part def B { attribute a : T; }
    part def A :> B { attribute b subsets P::B::a : T; }
}
";
    let found = messages(model);
    assert!(
        !mentions(&found, FEATURING),
        "a subsetting reached through the reverse direction must be accepted, got: {found:#?}"
    );
}

#[test]
fn two_unrelated_part_definitions_are_not_treated_as_related() {
    // The direction bug, pinned directly. Both `A` and `B` specialize the library's `Parts::Part`,
    // so a predicate that asks "is the owner a subtype of one of the container's supertypes" answers
    // `true` for any pair of part definitions. That is what made the rule unable to fire.
    //
    // The observable consequence: `b` subsets `a` across unrelated types and is reported. If this
    // ever stops being reported, the predicate has reverted to answering `true` for everything.
    let model = "\
package P {
    attribute def T;
    part def A { attribute a : T; }
    part def B { attribute b subsets P::A::a : T; }
    part def C { attribute c subsets P::A::a : T; }
}
";
    let found = messages(model);
    let reports = found.iter().filter(|m| m.contains(FEATURING)).count();
    assert_eq!(
        reports, 2,
        "both unrelated subsettings must be reported, got: {found:#?}"
    );
}
