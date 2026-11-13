//! `case-membership` — the subjects and objectives a case or a requirement declares.
//!
//! Ten rules from the specification's table:
//!
//!   * at most one subject, written
//!     `featureMembership->selectByKind(SubjectMembership)->size() <= 1` on `CaseUsage`,
//!     `CaseDefinition`, `RequirementUsage` and `RequirementDefinition`;
//!   * at most one objective, written
//!     `featureMembership->selectByKind(ObjectiveMembership)->size() <= 1` on `CaseUsage` and
//!     `CaseDefinition`;
//!   * the subject is the first input, written
//!     `input->notEmpty() and input->first() = subjectParameter` on the same four
//!     subject-bearing metaclasses.
//!
//! All ten are decided here. The two objective rules count [`Kind::Objective`] children, and
//! `objective o { … }` keeps that kind (`sysml-syntax/src/lower.rs:1586-1602`); it used to lower
//! to [`Kind::RequirementUsage`], which made an objective indistinguishable from a nested
//! requirement and held these two rules back. Counting nested requirement usages instead would
//! report every case that owns one, which is a legal and ordinary shape.
//!
//! # What the element tree can decide
//!
//! `subject x : T` lowers to [`Kind::Subject`] (`sysml-syntax/src/lower.rs:947-956`), and a
//! directed feature `in x : T` lowers to [`Kind::Parameter`] with its direction set
//! (`lower.rs:298-303` for the kind, `lower.rs:391-395` for the direction). Both the subject
//! count and the order of the inputs are therefore readable off the element's own children.
//!
//! `input` is not a stored field; it is `deriveTypeInput` — "those of its features that have a
//! direction of `in` or `inout`" (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:8111-8115`).
//! The subject is one of them although it writes no direction: a `SubjectMembership` is a
//! `ParameterMembership` (`generated.rs:1755`) whose `parameterDirection()` is
//! `FeatureDirectionKind::_'in'` (`generated.rs:9294-9297`). That reading lives in
//! [`crate::derived::input_parameters`], which counts [`Kind::Subject`] as an input for exactly
//! this reason; this module asks it rather than keeping a second copy. Order is authored order,
//! because `input` is an ordered sequence derived from the ordered `feature`.
//!
//! # One predicate, several metaclasses
//!
//! The tree does not keep case definitions and case usages apart: `case def X` and `case x : X`
//! both lower to [`Kind::CaseDef`] (`lower.rs:225`, which ignores `is_definition`). The same
//! holds for `analysis case` (`lower.rs:226`) and `use case` (`lower.rs:234`). The four subject
//! rules split into two predicates — one for the case side, one for the requirement side — so one
//! walk decides both rules of a pair.
//!
//! The ruled metaclasses' own specializations are covered by the same predicates, because the
//! metamodel states them
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs`):
//!
//!   * `AnalysisCaseDefinition specializes CaseDefinition` (`:1612`) and `AnalysisCaseUsage
//!     specializes CaseUsage` (`:1613`);
//!   * `UseCaseDefinition` (`:1770`), `VerificationCaseDefinition` (`:1773`) and their usages
//!     (`:1771`, `:1774`) specialize the case pair;
//!   * `ConcernDefinition specializes RequirementDefinition` (`:1634`), `ConcernUsage specializes
//!     RequirementUsage` (`:1635`);
//!   * `ViewpointDefinition specializes RequirementDefinition` (`:1778`), `ViewpointUsage
//!     specializes RequirementUsage` (`:1779`).
//!
//! `concern` and `viewpoint` each keep one kind for both the definition and the usage
//! (`lower.rs:224` and `:236`), so both spellings reach the requirement predicate.
//!
//! A calculation is *not* ruled: `CaseDefinition specializes CalculationDefinition`
//! (`generated.rs:1628`), so a calc is the supertype and the rule does not reach it.
//!
//! # Where the projection is coarser than the metamodel
//!
//! `frame k : K` is a `FramedConcernMembership`, which specializes
//! `RequirementConstraintMembership` (`generated.rs:1678`) and is *not* a `ParameterMembership`.
//! It lowers to [`Kind::Frame`], not [`Kind::Subject`], precisely so the subject count here does
//! not include it: `RequirementBodyItem` admits a subject and a frame side by side
//! (`SysML.xtext:2039-2046`), so counting both reported a legal requirement.
//!
//! An `actor` lowers to a plain [`Kind::Part`] (`sysml-parser/src/parser.rs:985-986`), so it
//! enters `input` only when it writes a direction. `ActorMembership` is a `ParameterMembership`
//! (`generated.rs:1609`) and therefore an input in the metamodel, so an actor written before the
//! subject and without `in` is a violation this check misses. A missed violation, not a false
//! one.
//!
//! `featureMembership` is `ownedFeatureMembership->union(inheritedMembership->selectByKind(
//! FeatureMembership))` (`generated.rs:8147-8150`). The tree does not model inherited
//! memberships, so a case definition that inherits a subject from a supertype and writes a second
//! one is counted as having one. Again a missed violation, never a false one.
//!
//! # Zero subjects is not a violation
//!
//! The position rule's `input->notEmpty()` guards the ordering rather than demanding a subject:
//! read as a demand it would reject every requirement that declares none, and the sibling rule
//! explicitly allows that with `size() <= 1`. The official validation corpus writes subjectless
//! requirements — `requirement def <'2'> ReliabilityRequirement;` at
//! `research/upstream/release/sysml/src/validation/08-Requirements/8-Requirements.sysml:88` — so a
//! violation there would be a false positive on a model the specification treats as well formed.
//! The check therefore applies once a subject exists.
//!
//! # Rules this group cannot decide
//!
//! `validateCaseUsageOnlyOneObjective` and `validateCaseDefinitionOnlyOneObjective` reach for
//! `ObjectiveMembership` (`generated.rs:1717`), which the element tree does not have.
//! `objective o { ... }` lowers to [`Kind::RequirementUsage`] (`lower.rs:933-946`);
//! [`Kind::Objective`] exists as a variant (`sysml-model/src/lib.rs:55-56`) but is constructed
//! nowhere — `grep -rn "Kind::Objective" core/crates/` finds only the enum declaration, its
//! `keyword()` arm and its `group()` arm. A case body may legitimately hold nested requirement
//! usages, and nothing distinguishes them from objective memberships, so counting them would
//! report legal models.
//!
//! Keying on the lowered name is not a way round it: an unnamed objective takes the literal name
//! `objective` (`lower.rs:935-940`), but a named one takes its own name, and a model may legally
//! call a requirement usage `objective`. Counting by name would be a guess about what the author
//! meant, which is the failure this layer exists to avoid.
//!
//! `conformance::implemented()` must not list either rule.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

/// The code every rule in this group reports under.
const CODE: &str = "SEM020";

/// The kinds that are a `CaseDefinition` or a `CaseUsage`, directly or by specialization.
///
/// `AnalysisCaseDefinition`/`Usage` (`generated.rs:1612-1613`), `UseCaseDefinition`/`Usage`
/// (`:1770-1771`) and `VerificationCaseDefinition`/`Usage` (`:1773-1774`) all specialize the case
/// pair, so they are ruled by the same statement.
fn is_case(kind: &Kind) -> bool {
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

/// The kinds that are a `RequirementDefinition` or a `RequirementUsage`, directly or by
/// specialization.
///
/// `ConcernDefinition`/`Usage` (`generated.rs:1634-1635`) and `ViewpointDefinition`/`Usage`
/// (`:1778-1779`) specialize the requirement pair. The tree keeps one kind for the definition and
/// the usage of each (`lower.rs:224`, `:236`), so both spellings are covered here.
fn is_requirement(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::RequirementDef | Kind::RequirementUsage | Kind::ConcernDef | Kind::ViewpointDef
    )
}

/// The subject declarations the element owns, in authored order.
///
/// `ownedFeatureMembership` is what the count ranges over, and the element's own children are
/// exactly its owned features.
fn subjects_of(el: &Element) -> Vec<&Element> {
    el.children
        .iter()
        .filter(|child| child.kind == Kind::Subject)
        .collect()
}

/// How many objectives the declaration inherits along its supertype closure.
///
/// `objectiveMembership` is inherited like any other membership, so the count a case has to obey
/// is over the whole objective set, not only the members it writes. Each *provider* counts once
/// per objective it declares: `Occurrences::Occurrence` declares `self` four times and that is
/// still one member for a duplicate-name warning, but here the rule counts objectives, and one
/// object per declaration is what the OCL sizes.
fn inherited_objectives(index: &Index, id: usize) -> usize {
    let mut count = 0;
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut frontier: Vec<usize> = index.supertypes_of_public(id);
    seen.extend(frontier.iter().copied());
    while let Some(sup) = frontier.pop() {
        // The library is excluded: `Cases::Case` declares an `objective`, so counting it would
        // put one on every case in every model and make `case def C { objective o1; }` a
        // two-objective case. The pilot reports that model clean.
        if !index.is_library(sup) {
            count += index
                .children
                .get(sup)
                .map(|kids| {
                    kids.iter()
                        .filter(|&&k| index.kind[k] == Kind::Objective)
                        .count()
                })
                .unwrap_or(0);
        }
        for next in index.supertypes_of_public(sup) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    count
}

/// The qualified name the index assigned to an element, falling back to its own name.
fn qualified_at(index: &Index, el: &Element) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .unwrap_or_else(|| el.name.clone())
}

/// The inputs a declaration has from itself and from the model's own supertypes.
///
/// [`crate::derived::input_parameters_with_inherited`] counts the library too, because
/// `validateAcceptActionUsageParameters` needs the parameters `Transfers::AcceptAction` supplies.
/// This rule must not: `Cases::Case` and `Requirements::RequirementCheck` declare a `subject`, so
/// counting the library's gives every case and every requirement an inherited subject, and a legal
/// `case def C { in p : A; }` would report `p` as preceding it. What the rule is about is the
/// order the *model* writes, so the walk stops at the library.
fn own_inputs_with_inherited(index: &Index, el: &Element) -> Vec<usize> {
    let mut out = crate::derived::input_parameters(index, el.id);
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::from([el.id]);
    let mut frontier: Vec<usize> = index.supertypes_of_public(el.id);
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for owner in frontier {
            if !seen.insert(owner) || index.is_library(owner) {
                continue;
            }
            for param in crate::derived::input_parameters(index, owner) {
                // A redefinition in the subtype replaces the inherited one; the name decides.
                if !out.iter().any(|&p| index.name[p] == index.name[param]) {
                    out.push(param);
                }
            }
            next.extend(index.supertypes_of_public(owner));
        }
        frontier = next;
    }
    out
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_case(&el.kind) && !is_requirement(&el.kind) {
            return;
        }
        let qualified = qualified_at(index, el);
        let subjects = subjects_of(el);

        // validateCaseUsageOnlyOneSubject, validateCaseDefinitionOnlyOneSubject,
        // validateRequirementUsageOnlyOneSubject, validateRequirementDefinitionOnlyOneSubject —
        // all four say `featureMembership->selectByKind(SubjectMembership)->size() <= 1`.
        // One report per subject past the first, each at that subject: the pilot suite asserts at
        // `subject s2;`, `subject s4;` and so on — every declaration that has to go, not the case
        // that owns them.
        for extra in subjects.iter().skip(1) {
            let at = qualified_at(index, extra);
            let message =
                format!("`{at}` is a further subject of `{qualified}`; at most one is allowed");
            out.push(Violation::new(CODE, extra, at, message));
        }

        // validateCaseUsageOnlyOneObjective, validateCaseDefinitionOnlyOneObjective — a case
        // owns at most one `objective`. Decidable now that `objective o { }` keeps its own kind
        // instead of lowering to a requirement usage: a nested requirement in a case body is legal
        // and must not be counted.
        //
        // The count ranges over the *whole* objective set, not only the ones this declaration
        // writes. `objectiveMembership` is inherited along the supertype closure, and the pilot
        // asserts on exactly that: `case c: C { objective o3; objective o4; }` reports `o3` —
        // not just `o4` — because `C` already declares `o1`. A case that writes none of its own
        // and inherits two (`case c1 : C1;` where `C1` inherits `C`'s `o1` and adds `o5`) is
        // reported at the declaration itself, which is the line the assertion names.
        let inherited = inherited_objectives(index, el.id);
        let own: Vec<&Element> = el
            .children
            .iter()
            .filter(|child| child.kind == Kind::Objective)
            .collect();
        // How many of the declaration's own objectives count. Two rules meet here:
        //
        //   * `validateCaseUsageOnlyOneObjective` / `validateCaseDefinitionOnlyOneObjective`
        //     count the inherited objectives too. `case c: C { objective o3; objective o4; }`
        //     reports *both* declarations, because `C` already contributes one — so with at least
        //     one inherited, every own objective is past the first allowed one. `case def C`
        //     alone reports only its second, because nothing is inherited.
        //   * A verification case is not in that pair. `verification vehicleMassTest : MassTest`
        //     in `9-Verification-simplified.sysml:52-58` writes one objective while `MassTest`
        //     declares one, and that model conforms; its own rule set is separate.
        let counts_inherited = matches!(el.kind, Kind::CaseDef | Kind::AnalysisCaseDef);
        // A declaration that writes none of its own and inherits more than one allowed has nowhere
        // else to be reported: the pilot writes the assertion on the declaration line itself
        // (`case c1 : C1;` in `CaseSubjectObjective_Invalid.sysml.xt:85`).
        if counts_inherited && own.is_empty() && inherited > 1 {
            out.push(Violation::new(
                CODE,
                el,
                qualified.clone(),
                format!("`{qualified}` inherits {inherited} objectives; at most one is allowed"),
            ));
        }
        let skip = if counts_inherited {
            1usize.saturating_sub(inherited)
        } else {
            1
        };
        for extra in own.iter().skip(skip) {
            let at = qualified_at(index, extra);
            let message =
                format!("`{at}` is a further objective of `{qualified}`; at most one is allowed");
            out.push(Violation::new(CODE, extra, at, message));
        }
        // validateCaseUsageSubjectParameterPosition, validateCaseDefinitionSubjectParameterPosition,
        // validateRequirementUsageSubjectParameterPosition,
        // validateRequirementDefinitionSubjectParameterPosition — all four say
        // `input->notEmpty() and input->first() = subjectParameter`.
        //
        // `input->notEmpty()` guards the ordering: with no subject there is nothing to place
        // first, and an element without a subject is legal (see the module note).
        let Some(&subject) = subjects.first() else {
            // A usage that writes no subject of its own inherits its type's, and the inherited
            // one can be out of order too: `case def C1 :> C { in x; subject s5; }` makes
            // `case c1 : C1;` a case whose subject follows a parameter, and the pilot states that
            // on the usage line itself (`CaseSubjectObjective_Invalid.sysml.xt:85`). Only the
            // written list was read, so the usage had no subject and the rule had nothing to
            // place — the same rule, reported for the definition and missed for its usage.
            let inputs = own_inputs_with_inherited(index, el);
            let Some(&first) = inputs.first() else {
                return;
            };
            if index.kind[first] == Kind::Subject {
                return;
            }
            let Some(inherited_subject) = inputs
                .iter()
                .copied()
                .find(|&param| index.kind[param] == Kind::Subject)
            else {
                return;
            };
            out.push(Violation::new(
                CODE,
                el,
                qualified.clone(),
                format!(
                    "`{qualified}` inherits the subject `{}`, which is not its first input; `{}` \
                     precedes it",
                    index.name[inherited_subject], index.name[first]
                ),
            ));
            return;
        };
        // The subject is itself an input, so this is `Some` whenever a subject exists.
        let Some(&first_id) = crate::derived::input_parameters(index, el.id).first() else {
            return;
        };
        let first = el
            .children
            .iter()
            .find(|child| child.id == first_id)
            .expect("a derived id names a child");
        // Every `Subject` is an input, so the first input is the subject exactly when it is a
        // `Subject` — which, since `subject` is the first `Subject` child, is `subject` itself.
        // A preceding `in`/`inout` parameter is the violation the rule names.
        if first.kind != Kind::Subject {
            // Reported at the subject, as the pilot suite does: the subject is the declaration
            // whose position is wrong.
            let at = qualified_at(index, subject);
            let message = format!(
                "the subject `{}` of `{qualified}` is not its first input; `{}` precedes it",
                subject.name, first.name
            );
            out.push(Violation::new(CODE, subject, at, message));
        }
    });
    out
}
