//! `verification-membership` — `verify` belongs inside an objective.
//!
//! `validateRequirementVerificationMembershipOwningType`: the owner must be a `RequirementUsage`
//! that a `ObjectiveMembership` introduced — that is, the `objective` of a case:
//!
//! ```text
//! analysis case FuelEconomy {
//!     objective target {
//!         verify meetsTarget : FuelEconomyRequirement;   // legal
//!     }
//! }
//! part def Holder {
//!     verify stray : R;                                  // rejected
//! }
//! ```
//!
//! The verification says "this case, examined against this requirement, decides the objective";
//! outside an objective there is no decision for it to make.
//!
//! # The case has to be a verification case
//!
//! The OCL stops at the two conjuncts above, and they are not the whole verdict:
//! `case def VerificationPlan { objective { verify r; } }` satisfies both and is still an error in
//! the reference implementation (`Verification_invalid.sysml.xt:81`), whose message says "the
//! objective of a **verification case**". So the objective's enclosing case is asked for its kind
//! as well, and a generic case rejects the verification the same way a part definition does.
//! Reading the OCL alone matched the two `requirement { verify r; }` lines of that model and missed
//! the third — the same rule, counted twice, because the model states it three times.
//!
//! How far that reaches is what the evidence says and no further. The suite writes `verify` in the
//! objective of a `verification def` (clean) and of a `case def` (error), and nowhere else, so an
//! analysis case stays legal here: it is a case that decides something, and the rule's own test has
//! held that spelling legal since the rule was written.
//!
//! # What it took to read
//!
//! `verify r : R;` had no kind at all: the word was unknown to the dispatch, so it and the name
//! after it fell into the body as two attributes and the verified requirement was lost. It now
//! lowers to `Kind::VerificationUsage`, which is a different kind from `Kind::RequirementUsage` —
//! `Kind::VerificationUsage` (the `verification v : T` keyword). The last two were collapsed
//! onto one kind, and a rule that asks which of them this is needs them apart.
//!
//! The rule's middle conjunct (`owningType.owningFeatureMembership <> null`) cannot fail: the
//! owner is a member of something by construction.
//!
//! The report code this group reports under is `SEM068`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM068";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The case an objective belongs to: the nearest enclosing kind that is a case at all.
///
/// A `case def`, an `analysis case def`, a `verification` and a `use case` are all cases, and the
/// walk stops at the first of them because that is the one the objective belongs to. `None` means
/// the objective sits in something that is not a case — a `requirement def`, a part.
fn enclosing_case(index: &Index, mut id: usize) -> Option<usize> {
    while let Some(parent) = index.parent.get(id).copied().flatten() {
        if matches!(
            index.kind[parent],
            Kind::CaseDef
                | Kind::AnalysisCaseDef
                | Kind::VerificationDef
                | Kind::VerificationUsage
                | Kind::UseCaseDef
                | Kind::UseCaseUsage
        ) {
            return Some(parent);
        }
        id = parent;
    }
    None
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::Verification {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // `owningType.oclIsKindOf(RequirementUsage) and owningType.owningFeatureMembership
        // .oclIsKindOf(ObjectiveMembership)`. The first conjunct is the metamodel question; the
        // second is the projection's stand-in — `Kind::Objective` is the node the lowering builds
        // for an `objective { … }` block, which is what an `ObjectiveMembership` owns.
        //
        // The two conjuncts are not the whole of the pilot's verdict: `case def VerificationPlan {
        // objective { verify r; } }` satisfies both and is still an error there, because the
        // objective has to belong to a *verification* case
        // (`Verification_invalid.sysml.xt:81`, and the message says so in words). Reading the OCL
        // alone matched the two `requirement { verify r; }` lines of that model and missed this
        // one, which is the same line counted twice for the same rule.
        if index.kind[owner].is_metaclass("RequirementUsage")
            && index.kind[owner] == Kind::Objective
        {
            let case = enclosing_case(index, owner);
            // A verification case is where the rule puts it, and an analysis case is the other
            // kind that decides something — the suite writes `verify` in neither place, so the
            // arm that keeps an analysis case legal rests on the rule's own long-standing test
            // rather than on the pilot. The generic `case def` is the one the pilot states as an
            // error, and that is the one this rejects.
            if case.is_some_and(|case| {
                matches!(
                    index.kind[case],
                    Kind::VerificationDef | Kind::VerificationUsage | Kind::AnalysisCaseDef
                )
            }) {
                return;
            }
        }
        let owner_kind = index.kind[owner].keyword();
        let owner_name = index
            .qualified
            .get(owner)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| index.name[owner].clone());
        // Two ways to be in the wrong place, and the report says which one: outside an objective
        // at all, or inside the objective of a case that is not a verification case.
        let where_ = if index.kind[owner] == Kind::Objective {
            match enclosing_case(index, owner) {
                Some(case) => format!(
                    "in the objective of `{}`, a {}",
                    index.qualified[case],
                    index.kind[case].keyword()
                ),
                None => format!("in `{owner_name}`, an objective that belongs to no case"),
            }
        } else {
            format!("inside `{owner_name}`, a {owner_kind}")
        };
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` verifies a requirement {where_}; a verification belongs to the objective of \
                 a verification case (validateRequirementVerificationMembershipOwningType)",
                qualified(el, index)
            ),
        ));
    });
    out
}
