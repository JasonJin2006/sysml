//! `control-flow` — the constraints a control node's context places on it.
//!
//! A control node (`merge`, `decision`, `join`, `fork`) is a point where a behaviour's flow
//! branches or converges. The specification states where one may appear and, in four rules, how
//! many successions may lead into or out of it.
//!
//! Decided here:
//!
//!   * `validateControlNodeOwningType` — the owner must be an action, definition or usage.
//!     Readable from `Index::parent` and `Kind`: the tree constructs `Kind::ControlNode` for all
//!     four keywords (`lower.rs:845`) and keeps the owner.
//!   * `validateControlNodeIsComposite` — `isComposite`, the effective value the lowering fills.
//!
//! Not decided, and why — all four are the same missing fact:
//!
//! `validateControlNodeIncomingSuccessions`, `...OutgoingSuccessions`,
//! `validateDecisionNodeIncomingSuccessions` and `validateMergeNodeOutgoingSuccessions` count the
//! successions attached to the node. A `succession a then b` lowers to `Kind::Transition` with
//! `from`/`to` as *text*, not as references to element ids, and a plain `a then b` inside an
//! action body does not lower to a node at all. Counting "successions whose target is this node"
//! would mean matching names, and two nodes in different actions can share a name. So the counts
//! are not available.
//!
//! The report code this rule reports under is `SEM040`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM040";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// `validateControlNodeOwningType` — a control node lives in an action.
fn owning_type(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::ControlNode {
        return;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is owned by no type; a control node must be owned by an action definition \
                 or an action usage (validateControlNodeOwningType)",
                qualified(el, index)
            ),
        ));
        return;
    };
    // `owningType.oclIsKindOf(ActionDefinition) or owningType.oclIsKindOf(ActionUsage)`, asked of
    // the metaclass: a state definition is an `ActionDefinition` and a state usage an `ActionUsage`,
    // and a control node nested in another node is an `ActionUsage` too, so one pair of questions
    // covers the four shapes the list used to name.
    let ok = index.kind[owner].is_metaclass("ActionDefinition")
        || index.kind[owner].is_metaclass("ActionUsage");
    if ok {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` is a control node owned by `{}`, a {}; it must be owned by an action definition \
             or an action usage (validateControlNodeOwningType)",
            qualified(el, index),
            index.qualified[owner],
            index.kind[owner].keyword()
        ),
    ));
}

/// `validateControlNodeIsComposite` — `isComposite`.
///
/// A control node is part of the action it sits in, so the effective `isComposite` the lowering
/// fills must be true. With the derivation in `fill_feature_defaults` that is what a control node
/// owned by a type gets; the declaration that states otherwise is one nothing owns — `merge m;` at
/// package level — and there `isComposite` is false because a package is not a type.
///
/// That model also breaks `validateControlNodeOwningType` just above, and both rules are genuinely
/// violated by it: the node is outside any action, and being outside one is exactly why it is not
/// composite. Two diagnostics for one declaration, recorded here rather than hidden — the same
/// overlap SEM021/SEM033 carries for an end's multiplicity.
fn is_composite(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::ControlNode {
        return;
    }
    if el.is_composite {
        return;
    }
    let owner = match index.parent.get(el.id).copied().flatten() {
        Some(owner) => format!("`{}`, a {}", index.qualified[owner], index.kind[owner]),
        None => "nothing".to_string(),
    };
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` is a control node that is not composite; it is owned by {owner}, and a control \
             node is part of the action it sits in (validateControlNodeIsComposite)",
            qualified(el, index)
        ),
    ));
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        owning_type(el, index, &mut out);
        is_composite(el, index, &mut out);
    });
    out
}
