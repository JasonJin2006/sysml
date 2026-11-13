//! Failing-model tests for `validateAnnotationAnnotatedElementOwnership` (`SEM121`) on a metadata
//! usage written with the `metadata` keyword and an `about` clause (`SysML.xtext:141-148`).
//!
//! `metadata t : T about t;` is an annotation owned by `t` that is about `t`, and owns no annotating
//! element. The reference reports it (`KerMLValidator.xtend:411-416`).

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

/// The syntax errors a model produces.
fn syntax_errors(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SYN001")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_metadata_usage_about_itself_must_own_an_annotating_element() {
    let found =
        ownership_findings("package P {\n\tmetadata def T;\n\tmetadata t : T about t;\n}\n");
    assert!(
        found
            .iter()
            .any(|(code, at)| code == "SEM121" && at.ends_with("::t")),
        "the metadata usage is about itself and must be reported under SEM121: {found:?}"
    );
}

#[test]
fn a_metadata_usage_about_another_element_is_not_reported() {
    let found = ownership_findings(
        "package P {\n\tmetadata def T;\n\tfeature x;\n\tmetadata t : T about x;\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_metadata_usage_with_an_about_clause_is_legal_notation() {
    let errors = syntax_errors(
        "package P {\n\tmetadata def T;\n\tfeature x;\n\tmetadata t : T about x, P::x;\n}\n",
    );
    assert!(errors.is_empty(), "the about clause must parse: {errors:?}");
}
