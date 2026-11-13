//! `conjugation` — a type conjugates at most one other type.
//!
//! `validateTypeAtMostOneConjugator`: `ownedRelationship->selectByKind(Conjugation)->size() <= 1`.
//! Conjugation flips a type's directions — an `out` becomes an `in` — so two conjugators would
//! leave every feature facing both ways with no rule for which wins.
//!
//! ```text
//! port def FuelOut;
//! port def FuelIn conjugates FuelOut;      // one
//! port def Bad conjugates FuelOut, FuelOut; // rejected
//! ```
//!
//! # How it reads
//!
//! Both spellings, `conjugates T` and `~T`, produce `RelationOp::Conjugates`, and the lowering
//! keeps them in `Element::conjugates`. A comma list is what makes the count exceed one; the
//! notation has no other way to write a second conjugator.
//!
//! The report code this group reports under is `SEM057`.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM057";

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
        if el.conjugates.len() <= 1 {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` conjugates {} types; a type has at most one conjugator \
                 (validateTypeAtMostOneConjugator)",
                qualified(el, index),
                el.conjugates.len()
            ),
        ));
    });
    out
}
