//! `parallel-state` — a parallel state's substates do not follow one another.
//!
//! `validateStateDefinitionParallelSubactions` and `validateStateUsageParallelSubactions`:
//! `isParallel implies ownedAction.incomingTransition->isEmpty()`. A parallel state runs its
//! substates side by side; a succession between them says one waits for the other, which is the
//! opposite of what `parallel` states.
//!
//! ```text
//! state parallel Running {
//!     state pump;
//!     state sensor;
//!     first pump then sensor;    // rejected: these two do not sequence
//! }
//! ```
//!
//! # How it reads
//!
//! `state parallel s` puts `parallel` between the kind keyword and the name. The prefix loop had
//! no arm for it, so the word became the state's *name* and the real name was lost — a state
//! called `parallel` owning a stray attribute. `Prefixes::is_parallel` now carries it, and
//! `Modifiers::is_parallel` keeps it on the tree.
//!
//! The check is over successions in the state's own body: `first a then b;` lowers to
//! `Kind::Transition` beside the states, and its ends are authored text matched against the
//! substates' names. A transition whose ends do not name two of this state's own substates is not
//! counted, since it is not a succession *between* them.
//!
//! The report code this group reports under is `SEM059`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM059";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// A bare end name, or `None` when the end is qualified, dotted or carries a bound.
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

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !matches!(el.kind, Kind::StateDef | Kind::StateUsage) || !el.modifiers.is_parallel {
            return;
        }
        let Some(children) = index.children.get(el.id) else {
            return;
        };
        // The substates' own names: a succession counts only when both ends name one of them.
        let substates: Vec<&str> = children
            .iter()
            .filter(|&&child| {
                matches!(
                    index.kind[child],
                    Kind::StateDef | Kind::StateUsage | Kind::PseudoState
                )
            })
            .map(|&child| index.name[child].as_str())
            .collect();
        for &child in children {
            if index.kind[child] != Kind::Transition {
                continue;
            }
            // `nestedAction.incomingTransition->isEmpty() and
            // nestedAction.outgoingTransition->isEmpty()` — either direction is enough. A bare
            // `then S1_2;` writes only a target; its source is the enclosing state, which is the
            // incoming transition the OCL forbids. Requiring both ends to name a substate skipped
            // exactly that spelling (`TransitionUsage_invalid.sysml.xt:46`).
            let from = index.end_text(child, true).and_then(bare);
            let to = index.end_text(child, false).and_then(bare);
            let names_a_substate = from.is_some_and(|f| substates.contains(&f))
                || to.is_some_and(|t| substates.contains(&t));
            if !names_a_substate {
                // A transition between two things that are not this state's substates is not a
                // succession inside it.
                continue;
            }
            // A bare `then X;` names the enclosing state as its source, so an end that is not a
            // substate says nothing about where the succession runs from — only the substate it
            // reaches is worth naming.
            let from_sub = from.filter(|f| substates.contains(f));
            let to_sub = to.filter(|t| substates.contains(t));
            let ends = match (from_sub, to_sub) {
                (Some(f), Some(t)) => format!("`{f}` before `{t}`"),
                (None, Some(t)) => format!("into `{t}`"),
                (Some(f), None) => format!("out of `{f}`"),
                (None, None) => continue,
            };
            // Reported on the transition, not on the state: the transition is the declaration to
            // remove, and the pilot writes the assertion at `then S1_2;`. The walk hands back
            // ids, so the line comes from the index.
            out.push(Violation {
                code: CODE,
                element: qualified(el, index),
                line: index.line[child],
                message: format!(
                    "`{}` is parallel but a succession runs {ends}; substates of a parallel state \
                     do not follow one another (validateStateDefinitionParallelSubactions / \
                     validateStateUsageParallelSubactions)",
                    qualified(el, index)
                ),
                severity: super::Severity::Error,
            });
        }
    });
    out
}
