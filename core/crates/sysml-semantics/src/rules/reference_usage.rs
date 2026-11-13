//! `reference-usage` — a performing or exhibiting usage names the right kind.
//!
//! Two rules, one shape:
//!
//!   * `validatePerformActionUsageReference` — a `perform` must name an action.
//!   * `validateExhibitStateUsageReference` — an `exhibit` must name a state.
//!
//! Each is `referencedFeatureTarget() <> null implies
//! referencedFeatureTarget().oclIsKindOf(<the metaclass>)`: when the usage names a target, that
//! target must be of the right kind.
//!
//! # What the target is
//!
//! `PerformActionUsageDeclaration` is `OwnedReferenceSubsetting FeatureSpecializationPart? |
//! ActionUsageKeyword UsageDeclaration?` (`SysML.xtext:1411-1425`), and `ExhibitStateUsage` is the
//! same shape (`SysML.xtext:1840-1848`). Only the first alternative writes a reference, and the
//! lowering keeps it in [`Element::performs`] as the whole dotted path. With the keyword
//! (`perform action x : T`, `exhibit state s : T`) the statement declares a name and references
//! nothing, so the antecedent is false and the rule is silent about what types it.
//!
//! That is a change from reading `Element::type_ref`: `perform A.B :>> C` names a feature inside a
//! performance, and taking `C` for the type made the performance name itself. The `: T` clause is
//! still a typing question — `typed_by_definition` asks it of the feature a reference denotes.
//!
//! `validateIncludeUseCaseUsageReference` is the same rule for `include`, and it cannot run here:
//! `include uc;` lowers to `Kind::UseCaseDef`, the same kind as `use case uc;`.
//!
//! The report code this group reports under is `SEM044`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM044";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// What a report calls a usage that declares nothing by: the reference it denotes.
///
/// `perform b;`, `exhibit s.sa;` and `event a;` are anonymous — the notation gives their elements
/// no `declaredName` — so the index hands them their owner's qualified name. The name a reader can
/// act on is the feature the statement denotes.
fn denoted_name(el: &Element, index: &Index, reference: &str) -> String {
    if el.name != "<anonymous>" {
        return qualified(el, index);
    }
    match index.parent.get(el.id).copied().flatten() {
        Some(owner) if !index.qualified[owner].is_empty() => {
            format!("{}::{reference}", index.qualified[owner])
        }
        _ => reference.to_string(),
    }
}

/// Whether `id` is one of `accepted`, or reaches one through specialization.
fn is_kind_of(index: &Index, id: usize, accepted: &[Kind]) -> bool {
    let mut frontier = vec![id];
    let mut seen = std::collections::HashSet::from([id]);
    while let Some(current) = frontier.pop() {
        if accepted.contains(&index.kind[current]) {
            return true;
        }
        for next in index.debug_supertypes(current) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    false
}

fn expected(el: &Element) -> Option<(&'static str, &'static [&'static str])> {
    match el.kind {
        Kind::Perform => Some((
            "validatePerformActionUsageReference",
            &["ActionUsage", "ActionDefinition"],
        )),
        Kind::ExhibitState => Some((
            "validateExhibitStateUsageReference",
            &["StateUsage", "StateDefinition"],
        )),
        // `event a;` — an event occurrence is an occurrence usage, so what it denotes must be one
        // too. A definition is accepted for the reason the satisfy rule accepts one: a definition
        // in a feature position stands for the implicit usage of it.
        Kind::EventOccurrenceUsage => Some((
            "validateEventOccurrenceUsageReference",
            &["OccurrenceUsage", "OccurrenceDefinition"],
        )),
        _ => None,
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        let Some((rule, accepted)) = expected(el) else {
            return;
        };
        // The reference, not the type clause: see the module note. A performance with no reference
        // (`perform action x : T`) is not this rule's subject.
        let Some(text) = el.performs.as_deref() else {
            return;
        };
        // The lowering nests a host under a dotted performance and gives the host the same
        // clause; the outer element is the one the statement is about.
        if !el.redefines.is_empty() {
            return;
        }
        let head = text.split(',').next().unwrap_or(text).trim();
        // A dotted reference names a member of what the first segment resolves to, so the
        // single-name lookup is the wrong question — the same reason
        // `validateAssignmentActionUsageReferent` uses `resolve_path`.
        let path = sysml_model::NameRef::new(head, el.line);
        let Resolution::Found(target) = index.resolve_path(el.id, &path) else {
            return;
        };
        // `oclIsKindOf` asks about the target's own metaclass, not about what types it: `ref b : B`
        // with `action def B` is a `ReferenceUsage`, and the pilot marks `perform b;` "Must
        // reference an action" (`ActionUsage_invalid.sysml.xt:58`). Walking the supertype closure
        // answered for `B` instead and let it through.
        if accepted
            .iter()
            .any(|metaclass| index.kind[target].is_metaclass(metaclass))
        {
            return;
        }
        let self_name = denoted_name(el, index, head);
        out.push(Violation::new(
            CODE,
            el,
            self_name.clone(),
            format!(
                "`{self_name}` denotes the {} `{}`; {rule} requires the target to be {}",
                index.kind[target].keyword(),
                index.qualified[target],
                accepted
                    .iter()
                    .map(|m| format!("`{m}`"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
        ));
    });
    out.extend(transition_effects(roots, index));
    out.extend(assignment_referent(roots, index));
    out.extend(assignment_referent_may_vary(roots, index));
    out.extend(assert_reference(roots, index));
    out
}

/// `validateAssertConstraintUsageReference` — `referencedFeatureTarget() <> null implies
/// referencedFeatureTarget().oclIsKindOf(ConstraintUsage)`.
///
/// `assert constraint ac : C` asserts a constraint, and `assert path;` names one; either way what
/// it names has to be a constraint. The projection keeps the name in `type_ref` (the `: C` form) or
/// in the target end (the bare `assert path;` form), so the question is what that name resolves to.
/// A definition is accepted for the reason the satisfy rule accepts one: a definition in a feature
/// position stands for the implicit usage of it.
fn assert_reference(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::AssertConstraint {
            return;
        }
        // The typed form `assert constraint c : T;` is judged by `typed_by_definition`'s
        // AssertConstraint row, which asks the same question and also counts the types. Reporting
        // it here as well gave `assert constraint aConstraint0 : ABlock;` two errors where the
        // pilot states one (`ConstraintUsage_Invalid.sysml.xt:42`). The bare `assert path;` form
        // has no type, so it is this rule's alone.
        if el.type_ref.is_some() {
            return;
        }
        let Some(text) = el.to.as_deref() else {
            return;
        };
        let head = text.split(',').next().unwrap_or(text).trim();
        let Resolution::Found(target) = index.resolve(el.id, head) else {
            return;
        };
        if index.is_kind_of(target, "ConstraintUsage")
            || index.is_kind_of(target, "ConstraintDefinition")
        {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` asserts `{}`, a {}; an assertion names a constraint \
                 (validateAssertConstraintUsageReference)",
                qualified(el, index),
                index.qualified[target],
                index.kind[target].keyword()
            ),
        ));
    });
    out
}

/// `validateAssignmentActionUsageReferent` — an assignment's referent is a feature.
///
/// `ownedMembership->exists(not oclIsKindOf(OwningMembership) and
/// memberElement.oclIsKindOf(Feature))`: `assign x := v` writes to a feature, through a membership
/// that references it rather than owning it. The projection keeps the left-hand side as the
/// element's source end (`from`), so the question is what that name resolves to.
fn assignment_referent(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::AssignmentActionUsage {
            return;
        }
        let Some(lhs) = el.from.as_deref() else {
            return;
        };
        let head = lhs.split(',').next().unwrap_or(lhs).trim();
        // A dotted referent (`assign i.b := null`) names a member of what the first segment
        // resolves to, so the single-name lookup is the wrong question: it asks whether `i.b` is
        // a name in scope, which it never is. `resolve_path` follows the dot into the feature's
        // type, which is where `b` would have to be declared.
        let name = sysml_model::NameRef::new(head, el.line);
        let Resolution::Found(target) = index.resolve_path(el.id, &name) else {
            return;
        };
        if index.kind[target].is_feature() {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` assigns to `{}`, a {}; an assignment writes a feature \
                 (validateAssignmentActionUsageReferent)",
                qualified(el, index),
                index.qualified[target],
                index.kind[target].keyword()
            ),
        ));
    });
    out
}

/// `validateAssignmentActionUsage` — an assignment writes a feature that may vary in time.
///
/// `Usage::may_time_vary` for the feature at `id`, which is a whole derivation and not a flag — see
/// [`Index::may_vary_of`], where `deriveUsageMayTimeVary` is implemented.
///
/// This function used to answer a second time, falling back to "the owner's kind owns a lifetime"
/// when the derivation said no. That fallback was wrong in a way the pilot model shows: `item i {
/// action d; }` gives `d` an owner whose kind is an occurrence, so the fallback said `d` varies and
/// `assign i.d := null` passed — while the derivation excludes it, because a `composite` feature
/// that specializes `Actions::Action` is structure rather than a changing value. Two answers to one
/// question is the defect; the derivation is the one the specification states.
fn may_vary(index: &Index, id: usize) -> bool {
    index.may_vary_of(id)
}
///
/// `referent <> null implies referent.featureTarget.mayTimeVary`. `mayTimeVary` redefines KerML's
/// `is_variable` (`Usage.may_time_vary`): a feature is assigned because its value changes, and a
/// feature that is not variable has one value for the lifetime of its owner. Writing to it is a
/// statement the model has no slot for.
///
/// ```text
/// occurrence def Engine {
///     attribute var rpm : Integer;
///     attribute rating : Integer;
///     action tune { assign rpm := 1; }      // legal: `rpm` varies
///     action rate { assign rating := 1; }   // rejected: `rating` does not
/// }
/// ```
///
/// # How it reads
///
/// The referent is the left-hand side, resolved the same way as
/// `validateAssignmentActionUsageReferent` does — through `resolve_path`, because `assign i.b := v`
/// names a member of `i`. `featureTarget` is `chainingFeature->last()` or the feature itself, and
/// this projection resolves a chain to its endpoint already, so the referent id *is* the target.
///
/// The rule is silent when the referent does not resolve: the reference pass reports that name,
/// and a rule about its variability would be reporting the same defect twice.
fn assignment_referent_may_vary(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::AssignmentActionUsage {
            return;
        }
        let Some(lhs) = el.from.as_deref() else {
            return;
        };
        let head = lhs.split(',').next().unwrap_or(lhs).trim();
        if head.is_empty() {
            return;
        }
        let name = sysml_model::NameRef::new(head, el.line);
        let Resolution::Found(target) = index.resolve_path(el.id, &name) else {
            return;
        };
        if may_vary(index, target) {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` assigns to `{}`, which is not `var`; an assigned feature must be able to \
                 change (validateAssignmentActionUsage)",
                qualified(el, index),
                index.qualified[target]
            ),
        ));
    });
    out
}
/// `validatePerformActionUsageReference`, read off a transition's `do` clause.
///
/// The clause is a `PerformedActionUsage`: `EffectBehaviorMember` owns an `EffectBehaviorUsage`,
/// which is `EmptyActionUsage | PerformedActionUsage` (`SysML.xtext:1909-1921`). The projection
/// keeps the name on the transition as [`Element::effect_text`] rather than building the perform,
/// so the question the perform rule asks — does the referenced feature name an action — is asked
/// of the clause here.
///
/// It is that rule and not `validateTransitionFeatureMembershipEffectAction`, whose subject is an
/// `ActionUsage` by production (`EffectBehaviorUsage returns SysML::ActionUsage`) and so can never
/// fail: what a model can get wrong is the *reference*, and this is the rule that says so.
fn transition_effects(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::Transition {
            return;
        }
        let Some(text) = el.effect_text.as_deref() else {
            return;
        };
        // `do drive()` names the behaviour it calls, and `do send x to y` is recorded as a message
        // without a name; neither is a reference this rule can ask about.
        let head = text.split('(').next().unwrap_or(text).trim();
        if head.is_empty() || head == "message" {
            return;
        }
        let Resolution::Found(target) = index.resolve(el.id, head) else {
            return;
        };
        if is_kind_of(index, target, &[Kind::ActionDef, Kind::ActionUsage]) {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "the `do` effect of `{}` names `{}`, a {}; validatePerformActionUsageReference \
                 requires a `perform` to name an action",
                qualified(el, index),
                index.qualified[target],
                index.kind[target].keyword()
            ),
        ));
    });
    out
}
