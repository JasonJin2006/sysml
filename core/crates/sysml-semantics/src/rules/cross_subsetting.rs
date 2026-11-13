//! `cross-subsetting` — a feature that crosses another is an end.
//!
//! `validateCrossSubsettingCrossingFeature`: `crossingFeature.isEnd and
//! crossingFeature.owningType <> null and crossingFeature.owningType <> crossedFeature`.
//!
//! `part p crosses A` says `p` reaches across `A` — it is the far end of a connection through the
//! crossed feature. Only an end can be that: an interior feature has nothing on the other side.
//!
//! ```text
//! part def Vehicle {
//!     part trailerHitch crosses Hitch;      // rejected: not an end
//!     end part hitch crosses Hitch;         // legal
//! }
//! ```
//!
//! # How it reads
//!
//! `crosses T` reaches the tree in `Element::crosses`, and `end` in `Element::is_end`. The rule
//! fires when a feature states a crossing without being an end.
//!
//! An end that owns a crossing feature may not explicitly cross-subset a different feature
//! (SysML section 7.13.2; `validateFeatureCrossingSpecialization`). The lowering preserves an
//! owned crossing feature as an independent element under an OwningMembership, so this check
//! compares identities, not names, types, or source lines. Unresolved targets remain the
//! reference checker's concern, and owned-only ends are legal: their crossing is implicit.
//!
//! The report code this group reports under is `SEM067`.
use crate::derived;
use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM067";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.crosses.is_empty() {
            return;
        }
        if !el.is_end {
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` crosses a feature but is not an end; only an end can be the far side of \
                     a connection (validateCrossSubsettingCrossingFeature)",
                    qualified(el, index)
                ),
            ));
        }
        owned_cross_feature_must_match(el, index, &mut out);
        crossed_feature_is_the_opposite_end(el, index, &mut out);
    });
    out
}

/// An owned crossing feature and an explicit cross feature denote the same element.
/// No explicit or resolved chain means no identity comparison: an owned-only end has an
/// implicit CrossSubsetting in the full model, and an unresolved name is diagnosed elsewhere.
fn owned_cross_feature_must_match(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    let Some(owned) = derived::owned_cross_feature(index, el.id) else {
        return;
    };
    let Some(crossed) = derived::cross_feature(index, el.id) else {
        return;
    };
    if owned == crossed {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` owns crossing feature `{}` but explicitly crosses `{}`; the owned feature must \
             be the cross feature, not a different element with the same name or type \
             (validateFeatureCrossingSpecialization)",
            qualified(el, index),
            index.qualified[owned],
            index.qualified[crossed]
        ),
    ));
}

/// `validateCrossSubsettingCrossedFeature` — a crossing chains through the *other* end.
///
/// `crossingFeature.isEnd and owningType <> null implies let endFeatures = owningType.endFeature,
/// chainingFeatures = crossedFeature.chainingFeature in chainingFeatures->size() = 2 and
/// endFeatures->size() = 2 implies chainingFeatures->at(1) = endFeatures->excluding(crossingFeature)->at(1)`.
///
/// `crosses y.b` states the crossed feature as a two-step chain, and the first step names the end
/// the crossing goes through. On a binary association that has to be the *other* end: `end x
/// crosses y.b` navigates from `y`, while `end y crosses y.b` navigates from itself, which is not a
/// connection between two things.
///
/// Both premises are on the tree: the chain is the authored text of `crosses`, and the ends are the
/// association's children carrying `is_end`. The rule is silent when either fails — the OCL guards
/// both with `size() = 2`, and a model with one end or a single-name crossing states nothing about
/// this relation.
fn crossed_feature_is_the_opposite_end(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !el.is_end {
        return;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        return;
    };
    let ends: Vec<usize> = index
        .children
        .get(owner)
        .map(|c| {
            c.iter()
                .copied()
                .filter(|&id| index.is_end_of(id))
                .collect()
        })
        .unwrap_or_default();
    if ends.len() != 2 {
        return;
    }
    let Some(crossed) = el.crosses.first() else {
        return;
    };
    let mut segments = crossed.split('.').map(str::trim).filter(|s| !s.is_empty());
    let Some(first) = segments.next() else {
        return;
    };
    if segments.next().is_none() {
        // A single-name crossing states no chain, so `chainingFeatures->size() = 2` fails.
        return;
    }
    let sysml_resolve::Resolution::Found(through) = index.resolve(el.id, first) else {
        return;
    };
    if through == el.id {
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` crosses `{crossed}`, which starts at itself; a crossing chains through the \
                 opposite end (validateCrossSubsettingCrossedFeature)",
                qualified(el, index)
            ),
        ));
        return;
    }
    // The first chain step must be the opposite end itself, not a different member that merely
    // shares its source line or is owned by it.
    if ends.contains(&through) {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` crosses `{crossed}`, whose first step is `{}`, not an end of `{}` \
             (validateCrossSubsettingCrossedFeature)",
            qualified(el, index),
            index.qualified[through],
            index.qualified[owner]
        ),
    ));
}
