//! `state-subaction` — `entry` / `do` / `exit` belong to a state.
//!
//! A state may name three actions: one that runs on entry, one while it is active, one on exit.
//!
//! ```text
//! state def Running {
//!     entry action startUp;
//!     do action run;
//!     exit action shutDown;
//! }
//! ```
//!
//! `validateStateSubactionMembershipOwningType` — the owner must be a `StateDefinition` or a
//! `StateUsage`. The three keywords describe a state's behaviour, so anywhere else they describe
//! nothing: a `part def` with an `entry action` has no state to enter.
//!
//! # How it reads
//!
//! `entry action a;` lowers to `Kind::StateAction` whose name is the keyword (`entry`) and whose
//! first child is the action. The owner is the parent in the tree, so the check is the owner's
//! kind.
//!
//! `validateStateSubactionMembershipKind` (`kind` must match the keyword) is not here: the keyword
//! *is* the record, so the field and the word would always agree.
//!
//! `validateStateUsageParallelSubactions` and its Definition twin need
//! `nestedAction.incomingTransition`, which would mean resolving successions to the states they
//! connect; a succession's ends are authored text here.
//!
//! The report code this group reports under is `SEM051`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM051";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// A state in either form: `owningType.oclIsKindOf(StateDefinition) or
/// owningType.oclIsKindOf(StateUsage)`.
fn is_state(kind: &Kind) -> bool {
    kind.is_metaclass("StateDefinition") || kind.is_metaclass("StateUsage")
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::StateAction {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if is_state(&index.kind[owner]) {
            return;
        }
        let owner_kind = index.kind[owner].keyword();
        let owner_name = index
            .qualified
            .get(owner)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| index.name[owner].clone());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` states a `{}` action but `{owner_name}` is a {owner_kind}, not a state \
                 (validateStateSubactionMembershipOwningType)",
                qualified(el, index),
                el.name
            ),
        ));
    });
    out
}
