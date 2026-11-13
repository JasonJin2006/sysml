//! `variation-members` — a variation's members are all variants.
//!
//! `validateDefinitionVariationOwnedFeatureMembership` and
//! `validateUsageVariationOwnedFeatureMembership`: a variation has no owned feature membership
//! other than the variants, which are `VariantMembership`s.
//!
//! ```text
//! variation part def Fuel {
//!     variant part petrol;      // an alternative
//!     variant part diesel;
//! }
//! variation part def Bad {
//!     variant part petrol;
//!     part tank;                // rejected: a variation is a choice, not a container
//! }
//! ```
//!
//! A variation's body *is* the list of alternatives. An ordinary member would describe the
//! variation itself rather than one of its choices, and there is no reading for it: the variation
//! is not an instance that could have a part.
//!
//! # How it reads
//!
//! `variant part x;` keeps `Kind::PartUsage` and sets `Element::is_variant`; a bare `variant v;`
//! keeps `Kind::Variant`. So the check is: for an element whose `is_variation` flag is set, every
//! child must be a variant member, which is the flag rather than the kind.
//!
//! Members that are not features at all are exempt. A `doc` comment, an import or a nested
//! namespace is not an owned *feature* membership, and the OCL counts only those. The kinds
//! exempted here are the ones this tree constructs for such members.
//!
//! The report code this group reports under is `SEM071`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM071";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Members that are not features: they say nothing about the variation's shape.
fn is_not_a_feature(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Package | Kind::Import | Kind::Alias | Kind::Dependency | Kind::Filter
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !el.modifiers.is_variation {
            return;
        }
        let rule = if el.kind.is_definition() {
            "validateDefinitionVariationOwnedFeatureMembership"
        } else {
            "validateUsageVariationOwnedFeatureMembership"
        };
        for child in &el.children {
            if child.is_variant || is_not_a_feature(&child.kind) {
                continue;
            }
            out.push(Violation::new(
                CODE,
                child,
                qualified(child, index),
                format!(
                    "`{}` is an ordinary member of the variation `{}`; a variation's members are \
                     all variants ({rule})",
                    qualified(child, index),
                    qualified(el, index)
                ),
            ));
        }
    });
    out
}
