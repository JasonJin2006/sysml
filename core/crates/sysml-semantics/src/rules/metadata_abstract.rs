//! `metadata-abstract` — a metadata usage may not name an abstract definition.
//!
//! `validateMetadataFeatureMetaclassNotAbstract`: `not metaclass.isAbstract`. An abstract metadata
//! definition is a base other metadata specialize; annotating with it directly says the element
//! carries metadata without saying which.
//!
//! ```text
//! abstract metadata def Base;
//! metadata def Safety :> Base;
//! part def Brakes { @Safety; ... }        // legal: Safety is concrete
//! part def Brakes2 { @Base; ... }         // rejected: Base says nothing on its own
//! ```
//!
//! # How it reads
//!
//! The metadata usage resolves its target the same way `validateMetadataFeatureMetaclass` does:
//! from the grandparent scope, because `@Name` lowers to an element *named* `Name` inside its
//! owner and a walk from the owner finds the metadata itself. An unresolved target is skipped, and
//! a target that is not a metadata definition at all is left to the metaclass rule rather than
//! reported twice.
//!
//! The report code this group reports under is `SEM065`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM065";

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
        if el.kind != Kind::MetadataUsage {
            return;
        }
        let Some(text) = &el.type_ref else { return };
        let head = text.split(',').next().unwrap_or(text).trim();
        let owner = index.parent.get(el.id).copied().flatten();
        let from = owner
            .and_then(|o| index.parent.get(o).copied().flatten())
            .or(owner)
            .unwrap_or(el.id);
        let Resolution::Found(target) = index.resolve(from, head) else {
            return;
        };
        if target == el.id {
            return;
        }
        // Only a metadata definition is in scope for this rule; anything else is the metaclass
        // rule's business, and reporting it here would double the diagnostic.
        if index.kind[target] != Kind::MetadataDef || !index.modifiers_of(target).is_abstract {
            return;
        }
        let target_name = index
            .qualified
            .get(target)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| head.to_string());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` names `{target_name}`, an abstract metadata definition; annotating with it \
                 says nothing about which metadata applies \
                 (validateMetadataFeatureMetaclassNotAbstract)",
                qualified(el, index)
            ),
        ));
    });
    out
}
