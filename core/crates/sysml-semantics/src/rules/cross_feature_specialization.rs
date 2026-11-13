//! `cross-feature-specialization` — a redefined end's cross feature specializes the one it replaces.
//!
//! `validateFeatureCrossFeatureSpecialization`: `crossFeature <> null implies
//! ownedRedefinition.redefinedFeature.crossFeature->forAll(f | f <> null implies
//! crossFeature.specializes(f))`. When an end redefines another end and both cross something, the
//! crossed features stand in the same relation as the ends that cross them: an end that crosses
//! something unrelated to what the end it replaces crosses has changed what the connection means.
//!
//! ```text
//! assoc A1 { end x : C1 crosses y.b;  end y : C2 crosses y.b; }
//! assoc A2 :> A1 { end x : C1 crosses y.c;  end y : C2 crosses y.b; }   // `c` vs `b`
//! ```
//!
//! # How it reads
//!
//! An end redefines another in two ways, and both are followed here:
//!
//! * the end states `redefines e` itself — [`Index::redefines_of`];
//! * the association specializes another, so its same-named end redefines that one's
//!   (`assoc A2 :> A1` gives `A2::x` a redefinition of `A1::x`).
//!
//! [`derived::cross_feature`] answers both sides, so the comparison is between ids and
//! `Index::is_subtype` is the `specializes` the OCL asks for.
//!
//! # What is not decided
//!
//! The OCL's `f <> null` guard skips a redefined end that crosses nothing, and so does this. An end
//! whose cross feature does not resolve is skipped for the same reason a chain with a hole is: the
//! reference pass reports the name, and guessing the relation would report the same defect twice.
//!
//! The report code this group reports under is `SEM073`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use crate::derived;

use super::Violation;

const CODE: &str = "SEM073";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The ends this one redefines: written `redefines e`, or inherited from a specialized association.
fn redefined_ends(index: &Index, id: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for text in index.redefines_of(id) {
        let head = text.split(',').next().unwrap_or(text).trim();
        if head.is_empty() {
            continue;
        }
        if let Resolution::Found(found) = index.resolve(index.clause_scope_of(id), head) {
            out.push(found);
        }
    }
    // `assoc A2 :> A1`: A2's end redefines A1's end of the same name.
    let Some(owner) = index.parent.get(id).copied().flatten() else {
        return out;
    };
    let name = &index.name[id];
    for sup in index.supertypes_of_public(owner) {
        for candidate in index.member_of(sup, name) {
            if !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }
    out
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        let Some(own) = derived::cross_feature(index, el.id) else {
            return;
        };
        for redefined in redefined_ends(index, el.id) {
            // The OCL's `f <> null` guard: an end that crosses nothing says nothing to compare.
            let Some(inherited) = derived::cross_feature(index, redefined) else {
                continue;
            };
            if own == inherited || index.is_subtype(own, inherited) {
                continue;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` crosses `{}`, which does not specialize `{}` — the cross feature of the \
                     `{}` it redefines (validateFeatureCrossFeatureSpecialization)",
                    qualified(el, index),
                    index.qualified[own],
                    index.qualified[inherited],
                    index.qualified[redefined]
                ),
            ));
        }
    });
    out
}
