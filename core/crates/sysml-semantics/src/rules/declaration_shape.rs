//! `declaration-shape` — three rules about what a declaration's own form must be.
//!
//! Each of these reads one keyword or one child count, and each was unreadable until the lowering
//! layer kept what the parser had already seen:
//!
//! `validateEnumerationDefinitionIsVariation` is not here. Every `enum def` in the official
//! validation models is written without `variation` and is valid, so the rule states a derived
//! property the notation sets on its own rather than something an author must write. Reading it
//! off the tree rejected six conforming models.
//!
//!   * `validateFlowDefinitionFlowEnds` — a `flow def` has at most two ends, since a flow runs
//!     from one thing to another.
//!
//! `validateReferenceUsageIsReference` is not here: `ref part r` lowers to `Kind::Part` with
//! `modifiers.is_reference` set, and there is no `ReferenceUsage` kind to ask. The flag is the
//! only record of the word, so a rule that reads it against itself could never fire.
//!
//! The report code this group reports under is `SEM050`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM050";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// How many ends the flow inherits from its supertypes.
///
/// `flow def I :> H` already has `H`'s `a`, so `I`'s own `a` and `b` fill the two slots and its
/// `c` is the third (`FlowUsage_Invalid.sysml.xt:47-53`).
fn inherited_ends(index: &Index, id: usize) -> usize {
    let mut count = 0;
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut frontier: Vec<usize> = index.supertypes_of_public(id);
    seen.extend(frontier.iter().copied());
    while let Some(sup) = frontier.pop() {
        // The library is excluded: `Flows::Flow` and its siblings declare ends, so counting
        // them would make every flow in every model inherit the limit already.
        if !index.is_library(sup) {
            if let Some(children) = index.children.get(sup) {
                count += children.iter().filter(|&&c| index.is_end_of(c)).count();
            }
        }
        for next in index.supertypes_of_public(sup) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    count
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // A flow runs from one thing to another, so at most two ends.
        //
        // Reported at the end that has to go, not at the flow that owns it: the pilot asserts at
        // `end c;` (`FlowUsage_Invalid.sysml.xt:53`), and a diagnostic on the container is also a
        // spurious one at the container line. A flow that inherits more than two and writes none
        // of its own has nowhere else to be reported, so it is reported on itself
        // (`flow def J :> I;`, `:57`).
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if index.kind[owner] != Kind::Flow {
            return;
        }
        let own: Vec<usize> = index
            .children
            .get(owner)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter(|&child| index.is_end_of(child))
                    .collect()
            })
            .unwrap_or_default();
        let inherited = inherited_ends(index, owner);
        if inherited + own.len() <= 2 {
            return;
        }
        // One report, on the last end the flow writes: the pilot asserts once for
        // `flow def I { end a; end b; end c; }`, not once per end past the limit.
        let surplus: Vec<usize> = own
            .iter()
            .copied()
            .skip(2usize.saturating_sub(inherited))
            .collect();
        let Some(&last) = surplus.last() else {
            return;
        };
        if el.id != last {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a further end of `{}`; a flow has at most two \
                 (validateFlowDefinitionFlowEnds)",
                qualified(el, index),
                index.qualified[owner]
            ),
        ));
    });
    // A flow that writes no ends of its own has no child to report at, so it is reported on
    // itself: `flow def J :> I;` inherits the three ends `I` writes
    // (`FlowUsage_Invalid.sysml.xt:57`). The walk above cannot reach that case — it fires when it
    // reaches a particular element, and the only elements it can reach here are ends, which such a
    // flow does not have.
    super::walk(roots, |el| {
        if index.kind[el.id] != Kind::Flow {
            return;
        }
        let own = index
            .children
            .get(el.id)
            .map(|children| children.iter().filter(|&&c| index.is_end_of(c)).count())
            .unwrap_or(0);
        if own > 0 {
            return;
        }
        let inherited = inherited_ends(index, el.id);
        if inherited <= 2 {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` inherits {inherited} ends; a flow has at most two \
                 (validateFlowDefinitionFlowEnds)",
                qualified(el, index)
            ),
        ));
    });
    out
}
