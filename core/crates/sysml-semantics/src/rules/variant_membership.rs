//! `variant-membership` — a `variant` belongs to a variation.
//!
//! `validateVariantMembershipOwningNamespace`: the namespace owning a variant must be a variation,
//! either a definition that is a variation or a usage that is one.
//!
//! ```text
//! variation part def Vehicle {
//!     variant part car;        // the alternatives of the choice
//!     variant part truck;
//! }
//! part def Plain {
//!     variant part stray;      // rejected: nothing here is a choice
//! }
//! ```
//!
//! # How it reads
//!
//! `variant part car;` keeps `Kind::PartUsage` and sets `Element::is_variant` (the parser records
//! the `variant` prefix, and the lowering carries it as a flag rather than as the kind), so the
//! flag identifies it as a variant membership without a separate membership node. A bare
//! `variant v;` has no inner kind and keeps `Kind::Variant`. The owner is the parent, and
//! `modifiers.is_variation` is the flag the `variation` prefix set.
//!
//! The report code this group reports under is `SEM054`.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM054";

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
        if !el.is_variant {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // `Definition.isVariation` or `Usage.isVariation` — the flag is the same on both.
        if index.modifiers_of(owner).is_variation {
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
                "`{}` is a variant of `{owner_name}`, a {owner_kind} that is not a variation; a \
                 variant belongs to a variation (validateVariantMembershipOwningNamespace)",
                qualified(el, index)
            ),
        ));
    });
    out
}
