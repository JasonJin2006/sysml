//! A function owns at most one return parameter, and a return parameter's direction is `out`.
//!
//! `validateExpressionResultParameterMembership`, `validateFunctionResultParameterMembership` and
//! `validateParameterMembershipParameterDirection` were listed as not applicable. The first two
//! because inheritance always supplies one result — but the reference checks *owned* return
//! memberships, at most one (`KerMLValidator.xtend:1068-1070`, `:1086-1088`), and a model can write
//! two. The third because the notation was said to fix the direction — but `return in r : Real;` is
//! writable, and the parser read `in` as the parameter's name and `r` as a separate attribute.

use sysml_semantics::{check, Severity};

fn sem058(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM058" && issue.severity == Severity::Error)
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_second_return_parameter_violates_validate_expression_result_parameter_membership() {
    let found = sem058(
        "package P { private import ScalarValues::*; \
         calc def C { return a : Real; return b : Real; } \
         function F { return x : Integer; return y : Integer; } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found.iter().all(
            |m| m.contains("validateExpressionResultParameterMembership")
                && m.contains("validateFunctionResultParameterMembership")
        ),
        "{found:?}"
    );
}

#[test]
fn one_return_parameter_or_an_inherited_one_is_enough() {
    // `calc def F1;` inherits `Calculation::result`, which is why the `>= 1` half never fails.
    let found = sem058(
        "package P { private import ScalarValues::*; calc def F1; calc def F2 { return r : Real; } }",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_return_parameter_written_in_violates_validate_parameter_membership_parameter_direction() {
    let found = sem058(
        "package P { private import ScalarValues::*; \
         calc def C { return in r : Real; } calc def D { return inout s : Real; } \
         calc def E { return out t : Real; } calc def G { return u : Real; } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .any(|m| m.contains("C::r") && m.contains("written `in`")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|m| m.contains("D::s") && m.contains("written `inout`")),
        "{found:?}"
    );
}

#[test]
fn a_directed_return_is_one_parameter_not_a_parameter_named_after_the_direction() {
    let loaded = sysml_syntax::load_project(&["calc def C { return in r : Real; }"], false)
        .expect("model parses");
    let names: Vec<&str> = loaded.roots[0]
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["r"]);
}
