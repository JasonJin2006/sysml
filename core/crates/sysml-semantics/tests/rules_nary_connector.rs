//! The n-ary connector and the named include, judged by the checker.
//!
//! An n-ary connector relates every end it lists, so each end is resolved and the arity counts all
//! of them. `include use case i : T;` makes a usage typed by `T`, and `include T` refers to the case
//! it includes; only the second is a reference the include rule checks.

use sysml_semantics::{check, Severity};

/// The error findings a model draws, as (code, message).
fn errors(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| (issue.code.to_string(), issue.message))
        .collect()
}

#[test]
fn an_n_ary_connection_whose_ends_all_resolve_is_clean() {
    let found = errors(
        "package P {\n\tpart car {\n\t\tpart a;\n\t\tpart b;\n\t\tpart e;\n\t\tconnection c connect (a, b, e);\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_unresolved_third_end_is_reported_like_the_other_ends() {
    let found = errors(
        "package P {\n\tpart car {\n\t\tpart a;\n\t\tpart b;\n\t\tconnection c connect (a, b, nope);\n\t}\n}\n",
    );
    assert!(
        found
            .iter()
            .any(|(code, message)| code == "SEM001" && message.contains("`nope`")),
        "{found:?}"
    );
}

#[test]
fn an_anonymous_n_ary_connection_relates_three_ends() {
    let found = errors(
        "package P {\n\tpart car {\n\t\tpart a;\n\t\tpart b;\n\t\tpart e;\n\t\tconnection connect (a, b, e);\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_named_include_states_a_typing_and_refers_to_no_case() {
    let found = errors(
        "package P {\n\tuse case def T;\n\tuse case def U {\n\t\tinclude use case i : T;\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_unnamed_include_must_name_a_use_case_usage() {
    let found =
        errors("package P {\n\tuse case def T;\n\tuse case def U {\n\t\tinclude T;\n\t}\n}\n");
    assert!(found.iter().any(|(code, _)| code == "SEM076"), "{found:?}");
}

#[test]
fn a_named_include_typed_by_a_usage_is_rejected_as_a_typing() {
    let found = errors(
        "package P {\n\tuse case Park;\n\tuse case def U {\n\t\tinclude use case i : Park;\n\t}\n}\n",
    );
    assert!(found.iter().any(|(code, _)| code == "SEM090"), "{found:?}");
}
