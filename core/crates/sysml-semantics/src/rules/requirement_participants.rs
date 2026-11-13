//! `requirement-participants` — who may be an actor or a stakeholder.
//!
//! Two membership keywords name the people around a requirement or a case:
//!
//! ```text
//! requirement def BrakeDistance {
//!     stakeholder regulator;      // interested in the requirement
//!     actor driver;               // takes part in the case
//! }
//! ```
//!
//!   * `validateStakeholderMembershipOwningType` — the owner must be a `RequirementUsage` or a
//!     `RequirementDefinition`.
//!   * `validateActorMembershipOwningType` — the owner must be a requirement, a case, or a use
//!     case. A `part def` has nobody to interest and nobody to take part.
//!
//! # What it took to read
//!
//! `stakeholder owner;` had no keyword arm, so the word and the name after it fell into the body
//! as two attributes and the membership vanished. Both keywords now lower to their own kinds
//! (`Kind::Stakeholder`, `Kind::Actor`), which is also more accurate than the old mapping of
//! `actor` to `Kind::Part`: an actor is not a part.
//!
//! The report code this group reports under is `SEM055`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use crate::derived;

use super::Violation;

const CODE: &str = "SEM055";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// What a stakeholder may be owned by: `owningType.oclIsKindOf(RequirementUsage) or
/// owningType.oclIsKindOf(RequirementDefinition)`.
///
/// Asked of the metaclass rather than of a list of kinds, because the list has to grow with every
/// subclass: a `concern def` reaches this rule as [`Kind::ConcernDef`] and is a
/// `RequirementDefinition` (its supertypes are `RequirementDefinition`, `ConstraintDefinition`, …),
/// so the requirement arm covers it without naming it.
fn can_own_stakeholder(kind: &Kind) -> bool {
    kind.is_metaclass("RequirementDefinition") || kind.is_metaclass("RequirementUsage")
}

/// What an actor may be owned by: the rule's four arms,
/// `RequirementUsage or RequirementDefinition or CaseDefinition or CaseUsage`.
///
/// The two case arms cover `use case def` / `use case x`: `UseCaseDefinition` specializes
/// `CaseDefinition` and `UseCaseUsage` specializes `CaseUsage`.
fn can_own_actor(kind: &Kind) -> bool {
    kind.is_metaclass("RequirementDefinition")
        || kind.is_metaclass("RequirementUsage")
        || kind.is_metaclass("CaseDefinition")
        || kind.is_metaclass("CaseUsage")
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        let (allowed, rule, word): (fn(&Kind) -> bool, &str, &str) = match el.kind {
            Kind::Stakeholder => (
                can_own_stakeholder,
                "validateStakeholderMembershipOwningType",
                "stakeholder",
            ),
            Kind::Actor => (can_own_actor, "validateActorMembershipOwningType", "actor"),
            _ => return,
        };
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if allowed(&index.kind[owner]) {
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
                "`{}` is a {word} of `{owner_name}`, a {owner_kind}; a {word} belongs to a \
                 requirement or a case ({rule})",
                qualified(el, index)
            ),
        ));
    });
    out.extend(satisfy_reference(roots, index));
    out
}

/// `validateSatisfyRequirementUsageReference` — `referencedFeatureTarget() <> null implies
/// referencedFeatureTarget().oclIsKindOf(RequirementUsage)`.
///
/// `satisfy r2 by p` states that `p` satisfies `r2`, so `r2` has to be a requirement. The
/// referenced feature is [`derived::referenced_feature_target`], which resolves the target end and
/// follows a chain; the question about it is a metaclass question, so a `concern` or a
/// `viewpoint` — both `RequirementUsage`s — are accepted without being named here.
fn satisfy_reference(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::Satisfy {
            return;
        }
        let Some(target) = derived::referenced_feature_target(index, el.id) else {
            // Nothing is referenced — `satisfy req : R1` names a type — so the OCL's antecedent is
            // false and the rule says nothing.
            return;
        };
        // The OCL asks `oclIsKindOf(RequirementUsage)`. Naming a *definition* in a feature position
        // denotes an implicit usage of it — `satisfy MassBudget by battery` with `requirement def
        // MassBudget` is legal and appears throughout the corpus — so the question is asked of
        // both, and a definition stands for the usage the notation implies.
        if index.is_kind_of(target, "RequirementUsage")
            || index.is_kind_of(target, "RequirementDefinition")
        {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` satisfies `{}`, which is a {}; a satisfy must name a requirement \
                 (validateSatisfyRequirementUsageReference)",
                qualified(el, index),
                index
                    .qualified
                    .get(target)
                    .map(String::as_str)
                    .unwrap_or_default(),
                index.kind[target]
            ),
        ));
    });
    out
}
