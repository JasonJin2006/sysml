//! `return-membership` — a `return` belongs to a function or an expression.
//!
//! `validateReturnParameterMembershipOwningType` and
//! `validateResultExpressionMembershipOwningType` state the same bound: the owner of a result
//! membership must be a `Function` or an `Expression`.
//!
//! ```text
//! calc def Total {
//!     return : Integer = 1;      // legal: a function has a result
//! }
//! part def Holder {
//!     return : Integer = 1;      // rejected: nothing here produces a value
//! }
//! ```
//!
//! # How it reads
//!
//! `return : T = v` lowers to `Kind::Parameter` with `direction = Out`, the authored name (or
//! `return` when none was written), and `clause = "return"`. The clause is the marker that
//! separates a result membership from an ordinary `in`/`out` parameter, which is what makes this
//! rule decidable where `validateParameterMembershipOwningType` was not: that one turns on `in` /
//! `out` direction, and `lower.rs` gives a port's payload feature the same direction marker as a
//! calculation's parameter, so the two cannot be told apart.
//!
//! The kinds accepted here are the ones this tree has for "produces a value": `calc` in either
//! form and a `constraint`, whose body is an expression. `Expression` has no kind of its own — an
//! expression is an `Expr`, not an `Element` — so there is no owner of that shape to accept.
//!
//! The report code this group reports under is `SEM069`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM069";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

fn produces_value(kind: &Kind) -> bool {
    // `owningType.oclIsKindOf(Function) or owningType.oclIsKindOf(Expression)`, asked of the
    // metaclass hierarchy: a `calc` reaches `Function` as a `CalculationDefinition` and `Expression`
    // as a `CalculationUsage`; a case reaches them through `CaseDefinition`/`CaseUsage`, and a
    // `constraint` through `ConstraintDefinition`. The seven-kind list this replaces had to name
    // each of those relations; the table already carries them.
    kind.is_metaclass("Function") || kind.is_metaclass("Expression")
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.clause.as_deref() != Some("return") {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if produces_value(&index.kind[owner]) {
            return;
        }
        let owner_kind = index.kind[owner].keyword();
        let owner_name = index
            .qualified
            .get(owner)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| index.name[owner].clone());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a result membership owned by `{owner_name}`, a {owner_kind}; only a \
                 function or an expression has a result \
                 (validateReturnParameterMembershipOwningType / \
                 validateResultExpressionMembershipOwningType)",
                qualified(el, index)
            ),
        ));
    });
    out
}

// `validateExpressionResultParameterMembership` and `validateFunctionResultParameterMembership` —
// `featureMembership->selectByKind(ReturnParameterMembership)->size() = 1` — are **not** decided
// here. A function whose body computes its value has a result parameter the tool derives, and this
// projection builds no such element: counting the `return` clauses a model writes reported every
// analysis case and every calculation in the official corpus (twenty declarations, all legal), which
// is what an absent implicit element looks like from the outside. The blocker the ledger records is
// `ReturnParameterMembership`, meaning the element, not the property.
