//! `association-ends` — how many ends a relation must have.
//!
//! An association states a relationship between things, so it needs at least two of them:
//!
//! ```text
//! assoc Ownership {
//!     end owner : Person;
//!     end owned : Car;
//! }
//! ```
//!
//! `validateAssociationRelatedTypes` — `not isAbstract implies relatedType->size() >= 2`.
//! `relatedType` is the type of each end, and every end has one, so the count is the number of
//! ends. An abstract association is exempt: it is the pattern the concrete ones fill in.
//!
//! # What the tree can and cannot say
//!
//! `end a : A` lowers to `Kind::KermlFeature` (or `Kind::PortUsage` when its owner is an
//! interface) with `is_end` set, and its `type_ref` holds `A`; the owner decides the kind, not the
//! word `end` (`lower.rs:536-544`). An end written without a type is still an end, and still
//! contributes a `relatedType`: the specification gives it an implied
//! `Links::Link::participant` subsetting. Counting only the ends that write a type made the
//! `noErrors` model `AssociationTest_EndRedefinitionGoodCase.kerml.xt` — whose two ends are both
//! written bare — look like an association with no ends.
//!
//! `assoc` and `assoc struct` both declare an association. `assoc struct` lowers to
//! [`Kind::AssocStructureDef`], one identity that is both an `Association` and a `Structure`; it is
//! not an `AttributeUsage`, and `validateAssociationRelatedTypes` is stated of `Association`, so it
//! is counted here like the plain `assoc` spelling. The standard library's own `Objects::LinkObject`
//! and `Objects::BinaryLinkObject` are written that way.
//!
//! `validateAssociationBinarySpecialization` is a different rule about how many ends contradict a
//! claimed base, and `binary_specialization.rs` reports it under `SEM078`; both read the same
//! effective ends. This module used to say the binary rule stayed out because the Kernel Semantic
//! Library was not loaded, which is no longer true.
//!
//! `validateAssociationStructureIntersection` is a further constraint on the combined kind; it is
//! a different question from this arity count and is not enforced here.
//!
//! # The anonymous cross form, and why it had to move before it could be counted
//!
//! `end [1..*] feature y` — the bound between the marker and the kind keyword — used to lower as
//! two members: an attribute named `end`, then `feature y`. That made
//! `AssociationTest_EndRedefinitionGoodCase.kerml.xt` look like a one-ended association.
//!
//! The bound is not the end's: the specification fixes an end at 1..1 and puts this number on the
//! owned cross feature (2024-12 pilot release notes). So the parser reads it in the prefix run and
//! files it as `Element::cross_multiplicity`, the head takes it from there, and
//! `validateFeatureEndMultiplicity` never sees it. The named form —
//! `end connectingHub [0..1] part hub : Hub`, the one the specification's own figure writes —
//! still collapses the cross feature into the end, and is reported as an end bound; that is the
//! false positive `rules_feature_multiplicity.rs` records, and fixing it means modelling the owned
//! cross feature as its own element rather than as a field.
//!
//! The report code this group reports under is `SEM049`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM049";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether the element is an association, declared `assoc`, `assoc def` or `assoc struct`.
///
/// `assoc struct` is [`Kind::AssocStructureDef`] — one identity that is both an `Association` and
/// a `Structure` — and `validateAssociationRelatedTypes` is stated of `Association`, so it is
/// subject to this rule like any other association.
fn is_association(kind: &Kind) -> bool {
    matches!(kind, Kind::AssocDef | Kind::AssocStructureDef)
}

/// The ends of `id` that are still in force, including the ones it inherits.
///
/// `relatedType` is the type of each end, and **every end has one**: the specification gives an
/// end written with no type an implied subsetting of `Links::Link::participant`, which is also why
/// `end x;` is legal (`AssociationTest_EndRedefinitionGoodCase.kerml.xt`, a `noErrors` model, has
/// two of them). So the count is the number of ends, not the number that write a type; counting
/// only the written ones reported that model as "0 typed end(s)".
///
/// The query is [`Index::effective_ends_of`], which walks the supertype closure by *identity* and
/// then removes the inherited ends that a local redefinition replaces. This module used to
/// deduplicate by name instead, which answers a different question in both directions:
///
///   * it counted a replaced end as a second one when the replacement was positional and named
///     differently — `assoc R :> Base { end c : C; }` where `Base`'s only end is `a` has *one* end
///     in force, because the end at position 0 redefines it; the names differ, so the old count
///     said two;
///   * it collapsed two genuinely distinct inherited ends that happen to share a name, so
///     `assoc R :> Left, Right` with an `x` in each counted one end where it relates two;
///   * it treated two anonymous ends as one, because the lowering gives every unnamed element the
///     same synthesised name `<anonymous>` (`lower.rs:581-584`) and a repeated non-empty name was
///     read as a redeclaration.
///
/// Identity gets all three right: the same end reached twice is one entry, and an entry a
/// redefinition targets is removed whatever it is called.
///
/// The count is deliberately not capped at two: the rule asks whether there are *at least* two, and
/// a ternary association is legal here — its error, if any, is `SEM078`'s question about
/// `Links::BinaryLink`, which is a different rule about a different clause.
fn ends_in_force(index: &Index, id: usize) -> usize {
    index.effective_ends_of(id).len()
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_association(&el.kind) || el.modifiers.is_abstract {
            return;
        }
        let ends = ends_in_force(index, el.id);
        if ends >= 2 {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a concrete association with {ends} end(s); a non-abstract \
                 association must relate at least two types \
                 (validateAssociationRelatedTypes)",
                qualified(el, index)
            ),
        ));
    });
    out
}
