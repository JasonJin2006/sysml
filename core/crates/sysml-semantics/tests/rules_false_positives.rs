//! Three false positives, each fixed at its cause and each guarded here.
//!
//! The spurious direction is the expensive one: a diagnostic the official suite does not assert is
//! either a model we wrongly reject or a report we should not have made. The suite went from 10 to
//! 0, and these are the three causes.
//!
//! # 1. An ambiguous qualified name is not a resolution failure
//!
//! `feature A: A::A_alias;` where `A` names four things. The specification resolves by taking the
//! first match — `resolveLocal` and `resolveVisible` both end `memberships->first()`
//! (`generated.rs:9711`, `:9717`) — and `validateNamespaceDistinguishibility` is the rule that
//! reports the collision, as a warning. Treating the ambiguity as a failure rejected a model the
//! suite states as clean. The first segment's candidates are now carried forward and filtered by
//! the segments after it, which is what the name itself says.
//!
//! # 2. A metadata tag between `assume` and its expression is not part of the expression
//!
//! `assume #g constraint b;` put `#g` where the expression reader expected a term, so it produced
//! `Expr::Opaque` and the evaluator said "expression form is not evaluated". The tag is metadata on
//! the membership.
//!
//! # 3. A guard already reported as non-Boolean is not evaluated as well
//!
//! `transition … if "test"` is reported by `validateTransitionFeatureMembershipGuardExpression`.
//! The evaluator then added "constraint cannot be decided: string values are not evaluated" at the
//! transition's own line — one wrong guard, two diagnostics, the second at a line with no
//! assertion.

use sysml_semantics::{check, Severity};

fn errors(src: &str) -> Vec<(String, u32, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.line, i.message.clone()))
        .collect()
}

fn warnings(src: &str) -> Vec<(String, u32, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .map(|i| (i.code.to_string(), i.line, i.message.clone()))
        .collect()
}

#[test]
fn a_qualified_name_narrows_an_ambiguous_first_segment() {
    // One of the four `A`s declares `A_alias`, so the rest of the path decides which was meant.
    // Reported before the fix as `error[REF001]: ambiguous: A::A_alias` on both lines.
    let found = errors(
        r#"package test {
    classifier A {}
    package A {
        classifier A {}
        alias A_alias for A;
    }
    package A {}
    feature A: A::A_alias;
    feature B: A::A_alias;
}"#,
    );
    let ambiguous: Vec<_> = found
        .iter()
        .filter(|(_, _, m)| m.contains("ambiguous"))
        .collect();
    assert!(
        ambiguous.is_empty(),
        "an ambiguous first segment narrowed by a later one must resolve: {ambiguous:?}"
    );
}

// The positive control for the narrowing lives in `sysml-resolve/tests/qualified_narrowing.rs`:
// unresolved type references are reported by the CLI, not by `sysml_semantics::check`, so the
// control has to ask the resolver directly.

#[test]
fn a_metadata_tag_between_assume_and_its_expression_is_skipped() {
    // All three positions of `#g`. Before the fix, two of them produced
    // "assumption cannot be decided: expression form is not evaluated".
    for src in [
        r#"package p {
    constraint b;
    requirement r2 {
        subject;
        #g assume constraint b;
    }
}"#,
        r#"package p {
    constraint b;
    requirement r2 {
        subject;
        assume #g constraint b;
    }
}"#,
    ] {
        let found = warnings(src);
        assert!(
            !found
                .iter()
                .any(|(_, _, m)| m.contains("expression form is not evaluated")),
            "a metadata tag must not make the expression opaque: {found:?}"
        );
    }
}

#[test]
fn a_string_guard_is_reported_once_not_twice() {
    // The error is the rule that owns the judgement; the warning would be the evaluator restating
    // it at a different line.
    let src = r#"package p {
    part def A;
    action def AB {
        state S1;
        state S2;
        transition first S1 if "test" then S2;
    }
}"#;
    let errs = errors(src);
    assert!(
        errs.iter().any(|(_, _, m)| m.contains("Boolean")),
        "the guard must be reported as non-Boolean: {errs:?}"
    );
    let warns = warnings(src);
    assert!(
        !warns
            .iter()
            .any(|(_, _, m)| m.contains("string values are not evaluated")),
        "the same guard must not also be reported as undecidable: {warns:?}"
    );
}
