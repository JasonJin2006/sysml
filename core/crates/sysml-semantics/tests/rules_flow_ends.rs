//! A flow end that names nothing is reported, and a resolvable one is not.
//!
//! `Kind::FlowUsage` used to be whitelisted alongside `Kind::Bind` in `lib.rs`, mapping every
//! non-`NotAccessible` endpoint verdict to `Endpoint::Found`. The effect was that
//! `flow F1 from nosuchthing to alsomissing;` produced **no diagnostic at all**, while the
//! equivalent `connection C1 connect nope1 to nope2;` produced two. That is not a reporting
//! preference, it is a missed violation: `FlowUsage` specializes `Connector` (`generated.rs:1856`),
//! so `validateConnectorRelatedFeatures` reaches a flow, and `relatedFeature` is derived from what
//! each end subsets — an end that names nothing contributes nothing
//! (`generated.rs:8003-8007`, `:2101`).
//!
//! The whitelist was removed. The positive control below is what keeps it from coming back: a flow
//! whose ends resolve must stay silent, so the check cannot be satisfied by reporting everything.
//!
//! # Why the arity rule does not also fire here
//!
//! Because the two would be one finding twice. Of the 33 assertions in the official suite that
//! concern a connector, flow, bind or end, exactly one asks for arity — "Must have at least two
//! related elements", on `Relationship_invalid_relatedElement0`, where the single end it *does*
//! write resolves fine. No assertion anywhere reports arity for an end that names nothing; the
//! reference reports the unresolved name instead ("Cannot identify flow end"). Our `SEM001` is
//! that report, so the arity rule counts written ends and defers the root cause to `SEM001`.

use sysml_semantics::{check, Severity};

/// The `SEM001` messages for unresolved ends, in emission order.
fn unresolved(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM001")
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn a_flow_end_that_names_nothing_is_reported() {
    let found = unresolved(
        r#"package P {
    part def B;
    part b : B;
    flow F from b to nosuchthing;
}"#,
    );
    assert_eq!(
        found.len(),
        1,
        "expected one unresolved flow end: {found:?}"
    );
    assert!(
        found[0].contains("nosuchthing"),
        "the report must name the end that failed: {found:?}"
    );
    assert!(
        found[0].starts_with("flow end"),
        "the report must say which relation it is about: {found:?}"
    );
}

#[test]
fn a_flow_whose_ends_both_name_nothing_is_reported_twice() {
    // The case the whitelist hid completely: no end resolves, so under the old mapping the whole
    // declaration was silent.
    let found = unresolved(
        r#"package P {
    flow F from nosuchthing to alsomissing;
}"#,
    );
    assert_eq!(found.len(), 2, "both ends must be reported: {found:?}");
}

#[test]
fn a_flow_with_resolvable_ends_stays_silent() {
    // Positive control. Without it, "report every end" would pass the tests above.
    let found = unresolved(
        r#"package P {
    part def B;
    part b : B;
    part c : B;
    flow F from b to c;
}"#,
    );
    assert!(
        found.is_empty(),
        "a well-formed flow must be silent: {found:?}"
    );
}

#[test]
fn a_binding_end_that_names_nothing_stays_silent() {
    // `Kind::Bind` keeps the whitelist it shares with nothing else: the pilot leaves
    // `bind a.p = …` alone where `a.p` does not resolve (`valid_ActionUsage.sysml.xt:59`), because a
    // binding's ends are values rather than connector ends. Removing that exemption too would be a
    // change to what a binding means, not a bug fix, and is not made here.
    let found = unresolved(
        r#"package P {
    part def B;
    part b : B;
    bind b.q = b.r;
}"#,
    );
    assert!(
        found.is_empty(),
        "a binding end is not a connector end: {found:?}"
    );
}
