//! Failing-model tests for `validateAnnotationAnnotatedElementOwnership` (`SEM121`).
//!
//! The reference reports an annotation that its annotated element owns but that owns no annotating
//! element (`KerMLValidator.xtend:411-416`). A comment written `comment c about c` is such an
//! annotation: it is owned by `c` and is about `c`. The notation reaches the rule only that way.

use sysml_semantics::check;

/// The `SEM121` findings in a model, as `(code, element)`.
fn ownership_findings(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM121")
        .map(|issue| (issue.code.to_string(), issue.at))
        .collect()
}

/// Every message a model produces, whatever its code.
fn all_messages(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_comment_about_itself_must_own_an_annotating_element() {
    let found = ownership_findings("package P {\n\tcomment c about c /* self */\n}\n");
    assert!(
        found
            .iter()
            .any(|(code, at)| code == "SEM121" && at.ends_with("::c")),
        "the comment is about itself and must be reported under SEM121: {found:?}"
    );
}

#[test]
fn a_comment_about_itself_by_its_qualified_name_is_reported() {
    // `P::c` names the same comment as `c` does inside `P`.
    let found = ownership_findings("package P {\n\tcomment c about P::c /* self */\n}\n");
    assert!(
        found.iter().any(|(code, _)| code == "SEM121"),
        "a qualified self-reference is the same annotation: {found:?}"
    );
}

#[test]
fn a_comment_about_another_element_is_not_reported() {
    let found =
        ownership_findings("package P {\n\tfeature x;\n\tcomment c about x /* other */\n}\n");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_unnamed_comment_about_an_element_is_not_reported() {
    // An unnamed comment cannot name itself, so no about target can be it.
    let found =
        ownership_findings("package P {\n\tfeature x;\n\tcomment about x /* anonymous */\n}\n");
    assert!(found.is_empty(), "{found:?}");
}
