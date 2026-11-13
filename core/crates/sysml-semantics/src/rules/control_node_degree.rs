//! `control-node-degree` — how many successions may meet at a node.
//!
//! A control node's purpose fixes its arity. `decision` and `fork` take one arrival and fan out;
//! `merge` and `join` take many and produce one. The specification states six rules; three have a
//! bound this tree can count:
//!
//!   * `validateDecisionNodeIncomingSuccessions` — a decision has at most one incoming.
//!   * `validateForkNodeIncomingSuccessions` — a fork has at most one incoming.
//!   * `validateMergeNodeOutgoingSuccessions` — a merge has at most one outgoing.
//!   * `validateJoinNodeOutgoingSuccessions` — a join has at most one outgoing.
//!
//! # How the count is made, and what that costs
//!
//! A succession lowers to `Kind::Transition` with `from` and `to` as **text** — the names as
//! written, not references to element ids — and a control node stores the feature it names in
//! `Element::to`. So the count is: transitions in the same owner whose `to` equals this node's
//! `to`. Same owner is what makes it sound: an action body is a scope, and the notation resolves a
//! bare name within one. Two nodes in different actions may share a name and are counted apart.
//!
//! A dotted or qualified end (`a.b then c`) is skipped rather than compared: text matching would
//! confuse two different `b`s.
//!
//! # The two rules that stay out
//!
//! `validateDecisionNodeOutgoingSuccessions` and `validateMergeNodeIncomingSuccessions` bound the
//! *multiplicity* of the successions' ends (`0..1`), not their number. An end's multiplicity is
//! written `[n] a` and the connector-end parser strips it; nothing keeps it.
//!
//! The report code this group reports under is `SEM047`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM047";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// A bare end name, or `None` when the end is qualified or dotted.
///
/// The end may carry its own multiplicity (`[0..1] m`); that bound is not part of the name.
fn bare(end: &str) -> Option<&str> {
    let mut text = end.trim();
    if let Some(rest) = text.strip_prefix('[') {
        let close = rest.find(']')?;
        text = rest[close + 1..].trim();
    }
    if text.is_empty() || text.contains('.') || text.contains("::") {
        return None;
    }
    Some(text)
}

/// The multiplicity written at the start of an end, if any.
fn end_multiplicity(end: &str) -> Option<&str> {
    let text = end.trim();
    let rest = text.strip_prefix('[')?;
    let close = rest.find(']')?;
    Some(&text[..close + 2])
}

/// Which bound a node keyword answers, and in which direction.
fn bound(keyword: &str) -> Option<(&'static str, Direction)> {
    match keyword {
        "decision" => Some((
            "validateDecisionNodeIncomingSuccessions",
            Direction::Incoming,
        )),
        "fork" => Some(("validateForkNodeIncomingSuccessions", Direction::Incoming)),
        "merge" => Some(("validateMergeNodeOutgoingSuccessions", Direction::Outgoing)),
        "join" => Some(("validateJoinNodeOutgoingSuccessions", Direction::Outgoing)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Incoming,
    Outgoing,
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::ControlNode {
            return;
        }
        // The keyword, not the name: a control node's name is the declared name
        // (`merge continue` declares `continue`), so the kind of node is only in `control_kind`.
        // A model that writes `merge;` with no name falls back to the keyword as its name.
        let keyword = el.control_kind.as_deref().unwrap_or(&el.name);
        // The two `ControlNode` bounds apply to every kind of control node and are checked
        // before the per-kind limit, which only four keywords answer.
        control_node_end_bounds(el, Direction::Incoming, index, &mut out);
        control_node_end_bounds(el, Direction::Outgoing, index, &mut out);
        let Some((rule, direction)) = bound(keyword) else {
            return;
        };
        // The end text names the node: a named node by its name, an unnamed one by its keyword.
        let Some(target) = el.to.as_deref().and_then(bare) else {
            return;
        };
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        let count = index
            .children
            .get(owner)
            .map(|siblings| {
                siblings
                    .iter()
                    .filter(|&&child| index.kind[child] == Kind::Transition)
                    .filter(|&&child| {
                        // A transition's ends are text; read them off the element.
                        // A succession's `from` is where it starts and `to` where it ends, so an
                        // arrival at this node is a transition whose *target* is the node's
                        // feature, and a departure is one whose *source* is.
                        let end = match direction {
                            Direction::Incoming => index.end_text(child, false),
                            Direction::Outgoing => index.end_text(child, true),
                        };
                        end.and_then(bare) == Some(target)
                    })
                    .count()
            })
            .unwrap_or(0);
        let limit_rule = match direction {
            Direction::Incoming => "incoming",
            Direction::Outgoing => "outgoing",
        };
        if count <= 1 {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` has {count} {limit_rule} successions; a {keyword} may have at most one \
                 ({rule})",
                qualified(el, index)
            ),
        ));
    });
    multiplicity_bounds(roots, index, &mut out);
    out
}

/// `validateDecisionNodeOutgoingSuccessions` and `validateMergeNodeIncomingSuccessions`.
///
/// Both bound the *multiplicity of the ends*, not their number: a decision's departures must each
/// be `0..1` (one branch may not be taken at all) and a merge's arrivals likewise. The bound is
/// written `first [0..1] a then [1] b` and the parser keeps it in the end's text, so it is readable
/// here while it was not before.
fn multiplicity_bounds(roots: &[Element], index: &Index, out: &mut Vec<Violation>) {
    super::walk(roots, |el| {
        if el.kind != Kind::ControlNode {
            return;
        }
        let keyword = el.control_kind.as_deref().unwrap_or(&el.name);
        let (rule, direction) = match keyword {
            "decision" => (
                "validateDecisionNodeOutgoingSuccessions",
                Direction::Outgoing,
            ),
            "merge" => ("validateMergeNodeIncomingSuccessions", Direction::Incoming),
            _ => return,
        };
        let Some(target) = el.to.as_deref().and_then(bare) else {
            return;
        };
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        let Some(siblings) = index.children.get(owner) else {
            return;
        };
        for &child in siblings {
            if index.kind[child] != Kind::Transition {
                continue;
            }
            let (end, side) = match direction {
                Direction::Incoming => (index.end_text(child, false), "target"),
                Direction::Outgoing => (index.end_text(child, true), "source"),
            };
            let Some(end) = end else { continue };
            if bare(end) != Some(target) {
                continue;
            }
            let Some(bound) = end_multiplicity(end) else {
                continue;
            };
            if bound == "[0..1]" || bound == "[0]" {
                continue;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "a succession meeting `{}` states its {side} multiplicity as `{bound}`; \
                     the {} rule requires `0..1` ({rule})",
                    qualified(el, index),
                    el.name
                ),
            ));
        }
    });
}

/// `ControlNode` requires the near end of every succession through it to be exactly 1.
///
/// `validateControlNodeIncomingSuccessions` reads `connectorEnd->at(2)` — the target of an
/// incoming succession — and demands 1..1; the outgoing rule reads `connectorEnd->at(1)` and
/// demands the same of the source. A succession that writes no multiplicity states 1..1 by
/// default, so only an end that spells a different bound can violate it.
///
/// These two rules were listed as unevaluable for two rounds on the belief that they contradicted
/// the merge and decision rules. They do not: this pair reads the near end (the control node's own
/// side) and the merge/decision pair reads the far end. Comparing the OCL bodies side by side is
/// what settled it.
///
/// The reference implementation runs none of the four and files SYSML21-306, for a reason that is
/// not a contradiction: `validateDecisionNodeOutgoingSuccessions` and
/// `validateMergeNodeIncomingSuccessions` pass `connectorEnd->at(n)` to `multiplicityHasBounds`
/// where their siblings pass `connectorEnd->at(n).multiplicity`, handing a connector end to a
/// predicate that reads a multiplicity. Mended the way the siblings are written, that pair is false
/// of every decision and merge node in the corpus, because an end that writes no bound states 1..1
/// and those two ask for 0..1. Every rule in this module reads the bound an end *writes* and skips
/// the rest, which is why all four hold here: a narrower reading than the OCL, not a different one.
fn control_node_end_bounds(
    el: &Element,
    direction: Direction,
    index: &Index,
    out: &mut Vec<Violation>,
) {
    let rule = match direction {
        Direction::Incoming => "validateControlNodeIncomingSuccessions",
        Direction::Outgoing => "validateControlNodeOutgoingSuccessions",
    };
    let Some(target) = el.to.as_deref().and_then(bare) else {
        return;
    };
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        return;
    };
    let Some(siblings) = index.children.get(owner) else {
        return;
    };
    for &child in siblings {
        if index.kind[child] != Kind::Transition {
            continue;
        }
        // The near end: the target of an arrival, the source of a departure. The far end is the
        // other one and belongs to the merge/decision rules.
        let (near, far) = match direction {
            Direction::Incoming => (index.end_text(child, false), index.end_text(child, true)),
            Direction::Outgoing => (index.end_text(child, true), index.end_text(child, false)),
        };
        // Only a succession that reaches this node is judged by it.
        if far.and_then(bare) == Some(target) {
            continue;
        }
        if near.and_then(bare) != Some(target) {
            continue;
        }
        let Some(end) = near else { continue };
        // A written bound must be 1. `[1]` and `[1..1]` are the same statement.
        let Some(bound) = end_multiplicity(end) else {
            continue;
        };
        if bound == "[1]" || bound == "[1..1]" {
            continue;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` has an {} succession whose near end is `{bound}`; a control node's own end \
                 must be 1..1 ({rule})",
                qualified(el, index),
                match direction {
                    Direction::Incoming => "incoming",
                    Direction::Outgoing => "outgoing",
                }
            ),
        ));
    }
}
