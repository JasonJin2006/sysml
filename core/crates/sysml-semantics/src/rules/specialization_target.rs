//! `specialization-target` — a conjugated type may not specialize.
//!
//! `validateSpecificationSpecificNotConjugated`: `not specific.isConjugated`, read with the
//! metamodel's own vocabulary: in a `Specialization` the *specific* type is the one that
//! specializes, so the rule forbids a conjugated type from being the subtype. Conjugation flips a
//! type's directions, so `B conjugates A` says `B` is `A` with every `in` turned to `out`; a
//! conjugated type is a view of another, and the notation has no way to write what it would mean to
//! build on that view.
//!
//! ```text
//! port def FuelOut;
//! port def FuelIn conjugates FuelOut;
//! port def Broken :> FuelIn;              // legal: `Broken` is the specific type and it is not
//!                                         // conjugated
//! port def Both conjugates FuelOut :> FuelIn;   // rejected: `Both` is the specific type and it is
//!                                               // conjugated
//! ```
//!
//! # How it reads
//!
//! `Element::conjugates` holds the targets of `conjugates T` / `~T`, and `Element::specializes` the
//! written `:>` clause. The second matters: `Element::supertypes` is the union a member walk needs,
//! and a conjugation is an edge of it, so asking that field would make every conjugated type look
//! like it specializes something.
//!
//! This module used to ask the opposite question — whether the *supertype* is conjugated — and
//! reported `class D1 specializes C1` where `C1 conjugates C`. The pilot's own direction model
//! writes exactly that and states no error for it
//! (`Redefinition_DirectionConformance_invalid.kerml.xt:50`), which is what the OCL says too.
//!
//! # The two spellings
//!
//! The relation arrives two ways and both are read. A declaration writes the general type after
//! `:>`, so the specific type is the element itself. A statement writes both:
//! `subtype C specializes B;` (`Specialization_invalid.kerml.xt:30`). The statement lowers to a
//! `Kind::Specialization` element with the specific end in `from` and the general end in `to`,
//! neither of which is a member of the enclosing namespace.
//!
//! The statement's diagnostic is placed on the statement, not on the type it names: the type was
//! written elsewhere and its own declarations are not what is wrong.
//!
//! The report code this group reports under is `SEM063`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM063";

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
        // `subtype C specializes B;` states the same relation the `:>` clause does, with the
        // specific type named rather than declared. The statement is what the pilot asserts this
        // rule on (`Specialization_invalid.kerml.xt:30`), so it is read here: resolve the
        // specific end and ask the same question about it.
        if el.kind == Kind::Specialization {
            let Some(text) = el.from.clone() else {
                return;
            };
            let Resolution::Found(specific) = index.resolve(el.id, &text) else {
                return;
            };
            if !index.is_conjugated(specific) {
                return;
            }
            let specific_name = index
                .qualified
                .get(specific)
                .cloned()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| text.clone());
            out.push(Violation::new(
                CODE,
                el,
                specific_name.clone(),
                format!(
                    "`{specific_name}` is conjugated and also specializes `{}`; a conjugated type \
                     is a view of another, not a base to build on \
                     (validateSpecificationSpecificNotConjugated)",
                    el.to.clone().unwrap_or_default()
                ),
            ));
            return;
        }
        // The OCL is `not specific.isConjugated`, and in a `Specialization` the *specific* type is
        // the one that specializes — so the rule forbids a conjugated type from being the subtype,
        // not from being the supertype. Reading it the other way reported every
        // `class D1 specializes C1` where `C1 conjugates C`, which is a shape the pilot's own
        // direction model writes and states no error for
        // (`Redefinition_DirectionConformance_invalid.kerml.xt:50`).
        if el.conjugates.is_empty() {
            return;
        }
        // The written clause, not `supertypes`: a conjugation is itself an edge of that union, so
        // asking it would make every conjugated type look like it specializes something.
        if el.specializes.is_empty() {
            return;
        }
        let targets: Vec<String> = el
            .specializes
            .iter()
            .map(|text| text.split(',').next().unwrap_or(text).trim().to_string())
            .collect();
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is conjugated and also specializes {}; a conjugated type is a view of \
                 another, not a base to build on (validateSpecificationSpecificNotConjugated)",
                qualified(el, index),
                targets.join(", ")
            ),
        ));
    });
    out
}
