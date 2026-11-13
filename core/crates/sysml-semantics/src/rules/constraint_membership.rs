//! `constraint-membership` — a `require` / `assume` clause belongs to a requirement.
//!
//! `validateRequirementConstraintMembershipOwningType`: the owner must be a
//! `RequirementDefinition` or a `RequirementUsage`.
//!
//! ```text
//! requirement def BrakeDistance {
//!     require constraint stopping < limit;    // legal
//! }
//! part def Holder {
//!     require constraint stray;               // rejected: nothing here is a requirement
//! }
//! ```
//!
//! # What it took to read
//!
//! `require constraint c1;` produced no element at all: the clause's *expression* went into the
//! requirement's `constraint` field and the name was dropped, so `c1` was unreachable and no rule
//! could ask who owned it. A named clause now lowers to `Kind::ConstraintUsage` as well. The
//! joined expression still lands on the requirement, so the checkers that read it are unaffected.
//!
//! `validateRequirementConstraintMembershipIsComposite` is not here: the constraint is a member of
//! the requirement by construction, and this tree records no separate composite flag to contradict
//! that.
//!
//! The report code this group reports under is `SEM062`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM062";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// What may own a `require` / `assume` clause:
/// `owningType.oclIsKindOf(RequirementDefinition) or owningType.oclIsKindOf(RequirementUsage)`.
///
/// An `objective` is covered by the second arm: the projection gives it
/// [`Kind::Objective`], which stands for a `RequirementUsage`.
fn can_own(kind: &Kind) -> bool {
    kind.is_metaclass("RequirementDefinition") || kind.is_metaclass("RequirementUsage")
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // Only a `require` / `assume` clause is a RequirementConstraintMembership. A plain
        // `constraint c { ... }` lowers to the same kind and may sit anywhere.
        if el.kind != Kind::ConstraintUsage || el.clause.is_none() {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if can_own(&index.kind[owner]) {
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
                "`{}` is a constraint owned by `{owner_name}`, a {owner_kind}; a require or \
                 assume clause belongs to a requirement \
                 (validateRequirementConstraintMembershipOwningType)",
                qualified(el, index)
            ),
        ));
    });
    out
}
