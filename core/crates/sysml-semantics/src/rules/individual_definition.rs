//! `individual-definition` — an occurrence usage names at most one individual definition.
//!
//! `validateOccurrenceUsageIndividualDefinition`: `occurrenceDefinition->selectByKind(...)
//! ->select(isIndividual).size() <= 1`. An individual is one specific thing — *this* vehicle, not
//! the class of vehicles. A usage typed by two individuals would be two specific things at once,
//! and nothing says which.
//!
//! ```text
//! individual part def Vehicle1;
//! individual part def Vehicle2;
//! part one : Vehicle1;              // legal
//! part two : Vehicle1, Vehicle2;    // rejected
//! part three : Vehicle1, Vehicle;   // legal: only one is an individual
//! ```
//!
//! # What it took to read
//!
//! `part two : Vehicle1, Vehicle2` states two types and only the first reached the tree:
//! `Head::target_of` returns one target, and the lowering used it. `Element::type_refs` now keeps
//! the whole list, so the count is possible. `type_ref` still holds the first, so everything that
//! read a single type is unaffected.
//!
//! An unresolved type is skipped: a dangling name is reported as a reference problem, and counting
//! it as an individual would guess.
//!
//! The report code this group reports under is `SEM072`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM072";

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
        if el.type_refs.len() < 2 {
            return;
        }
        let mut individuals = Vec::new();
        for text in &el.type_refs {
            let head = text.split(',').next().unwrap_or(text).trim();
            let Resolution::Found(target) = index.resolve(el.id, head) else {
                continue;
            };
            if index.modifiers_of(target).is_individual {
                individuals.push(
                    index
                        .qualified
                        .get(target)
                        .cloned()
                        .filter(|q| !q.is_empty())
                        .unwrap_or_else(|| head.to_string()),
                );
            }
        }
        if individuals.len() <= 1 {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is typed by {} individuals ({}); an occurrence usage names at most one \
                 (validateOccurrenceUsageIndividualDefinition)",
                qualified(el, index),
                individuals.len(),
                individuals.join(", ")
            ),
        ));
    });
    out
}
