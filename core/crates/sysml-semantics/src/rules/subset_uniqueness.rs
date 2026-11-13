//! `subset-uniqueness` — a subset may not drop uniqueness its target has.
//!
//! `validateSubsettingUniquenessConformance`: `subsettedFeature.isUnique implies
//! subsettingFeature.isUnique`. A subsetting narrows; it cannot widen.
//!
//! ```text
//! part base : Wheel[0..*];               // unique values
//! part sub : Wheel[0..*] nonunique :> base;   // rejected: wider than what it narrows
//! ```
//!
//! # How it reads
//!
//! `nonunique` is written after the name or after the type clause, and `Element::modifiers
//! .is_nonunique` records it; uniqueness is the absence of the word. The rule resolves each
//! supertype and compares the two flags.
//!
//! Two limits worth stating. A supertype that does not resolve is skipped — a dangling name is a
//! reference problem, and guessing its uniqueness would double the diagnostic. And `ordered` is
//! not modelled at all: this tree records `nonunique` but not its counterpart, so no rule about
//! ordering can be written yet.
//!
//! The report code this group reports under is `SEM064`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM064";

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
        // A distinctive subsetting only creates a problem when it drops uniqueness.
        if el.modifiers.is_nonunique == false {
            return;
        }
        for text in &el.supertypes {
            let head = text.split(',').next().unwrap_or(text).trim();
            let Resolution::Found(target) = index.resolve(el.id, head) else {
                continue;
            };
            if index.modifiers_of(target).is_nonunique {
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
                    "`{}` is nonunique but narrows `{target_name}`, which is unique; a subset \
                     may not drop uniqueness (validateSubsettingUniquenessConformance)",
                    qualified(el, index)
                ),
            ));
        }
    });
    out
}
