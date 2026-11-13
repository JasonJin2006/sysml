//! A qualified name narrows an ambiguous first segment, and still fails when nothing matches.
//!
//! `feature A: A::A_alias;` where `A` names a classifier, two packages and a feature. The
//! specification resolves by taking the first match — `resolveLocal` and `resolveVisible` both end
//! `memberships->first()` (`generated.rs:9711`, `:9717`) — and `validateNamespaceDistinguishibility`
//! is the rule that reports the collision, as a warning. Reporting the ambiguity as a resolution
//! *failure* rejected a model the official suite states as clean.
//!
//! The fix carries the candidates forward and filters them with the segments after them, which is
//! what the name itself says: only one of the four `A`s declares `A_alias`.
//!
//! Both directions are asserted here. A narrowing that turned every miss into a success would be a
//! worse defect than the false positive it replaced, and it would be invisible in the pilot score,
//! because a model whose name is silently accepted produces no diagnostic to count.

use sysml_resolve::Resolution;

fn resolve_in(src: &str, name: &str, reference: &str) -> Resolution {
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index
        .name
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("{name} not found"));
    index.resolve(id, reference)
}

const AMBIGUOUS_A: &str = r#"package test {
    classifier A {}
    package A {
        classifier A {}
        alias A_alias for A;
    }
    package A {}
    feature B;
}"#;

#[test]
fn a_later_segment_picks_one_of_several_candidates() {
    // `B` resolves `A::A_alias`: three `A`s are candidates, one declares `A_alias`.
    let verdict = resolve_in(AMBIGUOUS_A, "B", "A::A_alias");
    assert!(
        matches!(verdict, Resolution::Found(_)),
        "the rest of the path decides which `A` was meant, got {verdict:?}"
    );
}

#[test]
fn a_member_no_candidate_provides_is_unresolved() {
    // Positive control. Nothing declares `nowhere`, so narrowing must leave the name unresolved
    // rather than silently succeeding.
    let verdict = resolve_in(AMBIGUOUS_A, "B", "A::nowhere");
    assert!(
        matches!(verdict, Resolution::Unresolved),
        "a member no candidate provides must stay unresolved, got {verdict:?}"
    );
}

#[test]
fn an_ambiguity_no_later_segment_can_settle_is_still_reported() {
    // A bare `A` has no later segment to narrow it, so the ambiguity survives and is reported. The
    // fix must not make every ambiguous name resolve.
    let verdict = resolve_in(AMBIGUOUS_A, "B", "A");
    assert!(
        matches!(verdict, Resolution::Ambiguous(_)),
        "a bare ambiguous name must stay ambiguous, got {verdict:?}"
    );
}

#[test]
fn two_candidates_that_both_provide_the_segment_stay_ambiguous() {
    // Both `A`s declare `shared`, so the path does not decide between them.
    let src = r#"package test {
    package A { classifier shared {} }
    package A { classifier shared {} }
    feature B;
}"#;
    let verdict = resolve_in(src, "B", "A::shared");
    assert!(
        matches!(verdict, Resolution::Ambiguous(_)),
        "two candidates that both provide the name must stay ambiguous, got {verdict:?}"
    );
}
