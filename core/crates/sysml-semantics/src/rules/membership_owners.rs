//! `membership-owners` — which element may own which membership.
//!
//! A membership is not a free-floating declaration: `subject`, `objective`, `actor` and
//! `stakeholder` are all memberships whose meaning is fixed by the element that owns them. The
//! specification states one rule per membership, each of the form "the owning type must be one of
//! these metaclasses", and the tree has both halves: the membership keeps its own kind
//! (`Kind::Subject`, `Kind::Objective`, `Kind::Frame`) and `Index::parent` gives the owner.
//!
//! What is decided here:
//!
//!   * `validateObjectiveMembershipOwningType` — an objective may only be owned by a case.
//!   * `validateSubjectMembershipOwningType` — a subject may only be owned by a requirement, a
//!     case or a concern.
//!
//! `validateEndFeatureMembershipIsEnd` is not here: the `end` word is recorded on the feature
//! itself and the membership is that same word, so there is no second reading to compare and no
//! model can reach the rule.
//!
//! # What is not, and why the same reason keeps recurring
//!
//! Several of these rules name `RequirementUsage` or `CaseUsage` as the owner. This kernel
//! lowers a usage to the same `Kind` as its definition — `K::Case => Kind::CaseDef` regardless of
//! `is_definition` (`lower.rs:225-234`), and the same for `Requirement`, `AnalysisCase`,
//! `UseCase` and `VerificationCase`. So "owned by a CaseUsage" cannot be tested apart from "owned
//! by a CaseDefinition", and a rule that must distinguish them is left out rather than decided
//! against the wrong population. The rules below are the ones whose owner list is a single
//! `oclIsKindOf` over both spellings, so the conflation does not change the answer.
//!
//! The report code this group reports under is `SEM038`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM038";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The kinds that count as a case, definition or usage.
///
/// `Kind::CaseDef` covers both spellings (see the module note), which is exactly what
/// `validateObjectiveMembershipOwningType` needs: its OCL lists `CaseDefinition` and `CaseUsage`
/// as two alternatives of one question.
fn is_case(kind: &Kind) -> bool {
    // `VerificationCaseDefinition` is "a CaseDefinition for the purpose of verification"
    // (generated.rs:1010), and the same of its usage, so both spellings count. This kernel
    // lowers `verification def X` and `verification x : X` to two kinds, so both are listed.
    matches!(
        kind,
        Kind::CaseDef
            | Kind::AnalysisCaseDef
            | Kind::UseCaseDef
            | Kind::UseCaseUsage
            | Kind::VerificationDef
            | Kind::VerificationUsage
    )
}

/// The kinds that may own a `subject`.
fn can_own_subject(kind: &Kind) -> bool {
    is_case(kind)
        || matches!(
            kind,
            Kind::RequirementDef | Kind::RequirementUsage | Kind::ConcernDef
            // An objective is a RequirementUsage in the metamodel
            // (`ObjectiveMembership::ownedObjectiveRequirement`), so a subject written inside one
            // is owned by a requirement and legal. `10a-Analysis.sysml:48` writes exactly that:
            // `objective : MassAnalysisObjective { subject = MassAnalysisCase::result; }`.
                | Kind::Objective
        )
}

/// `validateObjectiveMembershipOwningType` — an objective is owned by a case.
fn objective_owner(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::Objective {
        return;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            "an `objective` is owned by no type; it may only be owned by a case".to_string(),
        ));
        return;
    };
    if is_case(&index.kind[owner]) {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "an `objective` may only be owned by a case, but `{}` is a {}",
            index.qualified[owner],
            index.kind[owner].keyword()
        ),
    ));
}

/// `validateSubjectMembershipOwningType` — a subject is owned by a requirement, case or concern.
fn subject_owner(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::Subject {
        return;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        // A subject at the top of a package is owned by a namespace, which the rule excludes.
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            "a `subject` is owned by no type; it may only be owned by a requirement, a case or a \
             concern"
                .to_string(),
        ));
        return;
    };
    if can_own_subject(&index.kind[owner]) {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "a `subject` may only be owned by a requirement, a case or a concern, but `{}` is a {}",
            index.qualified[owner],
            index.kind[owner].keyword()
        ),
    ));
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        objective_owner(el, index, &mut out);
        subject_owner(el, index, &mut out);
    });
    out
}
