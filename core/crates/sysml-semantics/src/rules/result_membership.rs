//! `result-membership` — a behaviour states at most one result.
//!
//! `validateExpressionResultExpressionMembership` and
//! `validateFunctionResultExpressionMembership`: `membership->selectByKind(ResultExpressionMembership)
//! ->size() <= 1`. A function or an expression produces one value; two result memberships would
//! name two, and nothing says which the caller receives.
//!
//! ```text
//! calc def Total {
//!     return : Integer = 1;     // the result
//!     return : Integer = 2;     // rejected
//! }
//! ```
//!
//! # How it reads
//!
//! `return : Integer = 1;` lowers to `Kind::Parameter` named `return` (see `lower.rs`, where the
//! return declaration becomes a parameter with the fixed name), so the count is the children of a
//! function that carry that name. Only behaviours are checked: a `return` anywhere else is already
//! reported by the parameter rules.
//!
//! The report code this group reports under is `SEM058`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM058";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// A function: something with a result at all.
fn is_function(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::CalcDef
            | Kind::CalcUsage
            | Kind::ConstraintDef
            | Kind::FunctionDef
            | Kind::ExpressionDef
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_function(&el.kind) {
            return;
        }
        let returns: Vec<usize> = index
            .children
            .get(el.id)
            .map(|children| {
                children
                    .iter()
                    .filter(|&&child| {
                        // `clause` is set by the lowering when the member came from a `return`
                        // declaration; the name alone is not enough, because
                        // `return a : Integer = 1` names the slot `a`, not `return`.
                        index.kind[child] == Kind::Parameter
                            && (index.name[child] == "return"
                                || index.clause_of(child) == Some("return"))
                    })
                    .copied()
                    .collect::<Vec<usize>>()
            })
            .unwrap_or_default();
        // One report per `return` past the first, each on that declaration: the pilot asserts at
        // `return r2 : T;` (`CalculationUsage_Invalid2.sysml.xt:45`), so a report on the function
        // is both in the wrong place and a spurious one at the function line.
        //
        // This is the *parameter* rule: `featureMembership->selectByKind(ReturnParameterMembership)
        // ->size() = 1`, which the reference checks as at most one owned
        // (`KerMLValidator.xtend:1068-1070`, `:1086-1088`). It used to be credited to the
        // result-*expression* rules below, and the two parameter rules were listed as not
        // applicable on the strength of a `>= 1` half that inheritance always satisfies.
        for &extra in returns.iter().skip(1) {
            out.push(Violation {
                code: CODE,
                element: index.qualified[extra].clone(),
                line: index.line[extra],
                message: format!(
                    "`{}` is a further return parameter of `{}`; a function produces one (validateExpressionResultParameterMembership / validateFunctionResultParameterMembership)",
                    index.qualified[extra],
                    qualified(el, index)
                ),
                severity: super::Severity::Error,
            });
        }
        // `validateParameterMembershipParameterDirection`: `ownedMemberParameter.direction =
        // parameterDirection()`, and a `ReturnParameterMembership` answers `out`. `return in r :
        // Real;` is writable — `ReturnParameterMember : 'return' UsageElement` lets the element
        // carry a direction (`SysML.xtext:1961-1963`, and `ReturnFeatureMember` in KerML) — and the
        // reference reports it (`KerMLValidator.xtend:1059-1062`). The other parameter memberships
        // the notation writes (payloads, arguments, node and subject parameters) take no direction
        // prefix, so a return is the only place a model can state the wrong one.
        for &ret in &returns {
            let direction = index.direction_of(ret);
            if matches!(
                direction,
                Some(sysml_model::Direction::In) | Some(sysml_model::Direction::InOut)
            ) {
                out.push(Violation {
                    code: CODE,
                    element: index.qualified[ret].clone(),
                    line: index.line[ret],
                    message: format!(
                        "`{}` is a return parameter written `{}`; a return parameter's direction \
                         is out (validateParameterMembershipParameterDirection)",
                        index.qualified[ret],
                        match direction {
                            Some(sysml_model::Direction::In) => "in",
                            _ => "inout",
                        }
                    ),
                    severity: super::Severity::Error,
                });
            }
        }
        // "Only one (owned or inherited) result expression is allowed": a bare expression in a
        // function or expression body *is* the result. `function G :> F { 2 }` inherits `F`'s `1`,
        // so it has two (`ResultExpressionMembership_Invalid.kerml.xt:26`).
        let is_result =
            |id: usize| index.kind[id] == Kind::DefaultReference && index.name[id] == "<anonymous>";
        let mut inherited_results = 0usize;
        let mut frontier: Vec<usize> = index.supertypes_of_public(el.id);
        let mut seen: std::collections::HashSet<usize> = frontier.iter().copied().collect();
        while let Some(sup) = frontier.pop() {
            inherited_results += index
                .children
                .get(sup)
                .map(|children| children.iter().filter(|&&c| is_result(c)).count())
                .unwrap_or(0);
            for next in index.supertypes_of_public(sup) {
                if seen.insert(next) {
                    frontier.push(next);
                }
            }
        }
        let own_results: Vec<usize> = index
            .children
            .get(el.id)
            .map(|children| children.iter().copied().filter(|&c| is_result(c)).collect())
            .unwrap_or_default();
        let total = inherited_results + own_results.len();
        if total > 1 {
            if let Some(&last) = own_results.last() {
                out.push(Violation {
                    code: CODE,
                    element: index.qualified[last].clone(),
                    line: index.line[last],
                    message: format!(
                        "`{}` is one of {total} result expressions; only one is allowed \
                         (validateExpressionResultExpressionMembership / \
                         validateFunctionResultExpressionMembership)",
                        index.qualified[last]
                    ),
                    severity: super::Severity::Error,
                });
            } else {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` inherits {total} result expressions; only one is allowed \
                         (validateExpressionResultExpressionMembership / \
                         validateFunctionResultExpressionMembership)",
                        qualified(el, index)
                    ),
                ));
            }
        }
    });
    out
}
