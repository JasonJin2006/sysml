//! Failing-model tests for `validateInstantiationExpressionInstantiatedType` (`SEM122`).
//!
//! An instantiation expression has an instantiated type only when the name it reads names something.
//! The reference reports the missing type on the expression (`KerMLValidator.xtend:1164-1166`). The
//! kernel reports the unreadable name as `SEM003` at the same place, and `SEM122` is the rule's own
//! report beside it.

use sysml_semantics::check;

/// The `SEM122` and `SEM003` findings in a model, as `(code, line)`.
fn instantiation_findings(src: &str) -> Vec<(String, u32)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM122" || issue.code == "SEM003")
        .map(|issue| (issue.code.to_string(), issue.line))
        .collect()
}

#[test]
fn a_constructor_of_a_missing_type_is_reported_beside_the_unreadable_name() {
    let found = instantiation_findings(
        "package P {\n\tattribute def Vehicle;\n\tcalc def MakeMissing {\n\t\treturn : Vehicle = new Missing();\n\t}\n}\n",
    );
    let line = found
        .iter()
        .find(|(code, _)| code == "SEM122")
        .map(|(_, line)| *line)
        .unwrap_or_else(|| panic!("SEM122 must be reported: {found:?}"));
    assert!(
        found.iter().any(|(code, l)| code == "SEM003" && *l == line),
        "the unreadable name is still reported on the same line: {found:?}"
    );
}

#[test]
fn a_call_to_a_missing_function_reports_the_invoked_type() {
    let found = instantiation_findings(
        "package P {\n\tattribute def Vehicle;\n\tcalc def CallMissing {\n\t\treturn : Vehicle = missingFn(1);\n\t}\n}\n",
    );
    assert!(
        found.iter().any(|(code, _)| code == "SEM122"),
        "a call whose callee names nothing has no invoked type: {found:?}"
    );
}

#[test]
fn a_constructor_of_a_defined_type_reports_nothing() {
    let found = instantiation_findings(
        "package P {\n\tattribute def Vehicle;\n\tcalc def MakeKnown {\n\t\treturn : Vehicle = new Vehicle();\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_call_to_a_defined_calculation_reports_nothing() {
    let found = instantiation_findings(
        "package P {\n\tcalc def F {\n\t\treturn : Integer = 1;\n\t}\n\tcalc def G {\n\t\treturn : Integer = F();\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}
