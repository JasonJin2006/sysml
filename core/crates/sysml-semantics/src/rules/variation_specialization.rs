//! `variation` — what a variation may specialize, and what its members must be.
//!
//! A `variation` is a definition or usage whose members are the choices: `variation part def
//! Vehicle :> Base { part car; part truck; }`. The specification states three rules over it and
//! this module answers two:
//!
//!   * `validateDefinitionVariationSpecialization` — a variation definition may not specialize
//!     another variation definition. A variation is the set of its own variants; inheriting
//!     another's would make the choice ambiguous rather than richer.
//!   * `validateUsageVariationSpecialization` — the same for a variation usage.
//!
//! # What stays out, and why
//!
//! `validateDefinitionVariationOwnedFeatureMembership` and
//! `validateUsageVariationOwnedFeatureMembership` require every owned member to be a
//! `VariantMembership`. This tree records an element's kind and its owner but not which *membership*
//! introduced it: `part car;` inside a variation is `Kind::Part` with the variation as parent, and
//! nothing says whether the notation reached it through `VariantMember` or `FeatureMember`. A
//! variation body is exactly where both are legal, so the check would have to guess.
//!
//! `validateUsageVariationIsAbstract` is out for a different reason: the notation makes `abstract`
//! and `variation` alternatives of one prefix (`SysML.xtext:491`, `:559`) and the reference
//! implementation derives `isAbstract` from `isVariation` (`build.rs:233-237`), so every
//! conforming variation is already abstract and the rule can never fire.
//!
//! The report code this group reports under is `SEM048`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM048";

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
        if !el.modifiers.is_variation {
            return;
        }
        // A definition answers the Definition rule, a usage the Usage one.
        let rule = if el.kind.is_definition() || matches!(el.kind, Kind::ClassDef) {
            "validateDefinitionVariationSpecialization"
        } else {
            "validateUsageVariationSpecialization"
        };
        // What is read: the *targets* of `:>` — the `general` end. The OCL as printed says
        // `ownedSpecialization.specific`, and `specific` is the specialising end, which is this
        // element itself: read literally, the rule asks whether a variation is a variation, which
        // is the antecedent restated. The constraint's own sentence — "a variation may not
        // specialize any other variation" — is what this check decides. The reference
        // implementation reports these two rules as unevaluable for exactly this reason.
        //
        // A redefinition (`:>>`) is a different relation and is not part of it.
        for text in &el.supertypes {
            let head = text.split(',').next().unwrap_or(text).trim();
            let Resolution::Found(target) = index.resolve(el.id, head) else {
                continue;
            };
            if !index.modifiers_of(target).is_variation {
                continue;
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
                    "`{}` is a variation and specializes `{target_name}`, also a variation; a \
                     variation may not specialize another ({rule})",
                    qualified(el, index)
                ),
            ));
        }
    });
    out
}
