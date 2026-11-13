//! `transition-trigger` — a trigger belongs to a transition out of a state.
//!
//! `validateTransitionUsageTriggerActions`: `source <> null and not
//! source.oclIsKindOf(StateUsage) implies triggerAction->isEmpty()`. A transition that starts
//! anywhere but a state has nothing to wait in: an `accept` there would say "when this arrives,
//! leave the state", and there is no state to leave.
//!
//! ```text
//! state def Machine {
//!     state idle;
//!     state running;
//!     transition t first idle accept start then running;   // legal: idle is a state
//! }
//! ```
//!
//! # How it reads
//!
//! A transition's source is authored text (`Element::from`) and its trigger is the text after
//! `accept` (`Element::trigger_text`). The source resolves in the transition's own scope, so the
//! check is: resolve `from`, and if it lands on something that is not a state while a trigger was
//! written, report. An unresolvable source is left alone — that is a reference problem, reported
//! elsewhere, and guessing its kind would turn one diagnostic into two.
//!
//! The report code this group reports under is `SEM061`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use crate::expr::{self, ExprField, ExprTree};

use super::Violation;

const CODE: &str = "SEM061";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// A bare end name, or `None` when it is qualified or carries a bound.
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
        if el.kind != Kind::Transition {
            return;
        }
        // No trigger written: the rule says nothing.
        if el.trigger_text.is_none() {
            return;
        }
        // `source <> null` — an unnamed transition has no source to judge.
        let Some(source) = el.from.as_deref().and_then(bare) else {
            return;
        };
        let Resolution::Found(target) = index.resolve(el.id, source) else {
            return;
        };
        if matches!(index.kind[target], Kind::StateDef | Kind::StateUsage) {
            return;
        }
        let source_kind = index.kind[target].keyword();
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` writes a trigger but starts at `{source}`, a {source_kind}; a trigger \
                 belongs to a transition out of a state \
                 (validateTransitionUsageTriggerActions)",
                qualified(el, index)
            ),
        ));
    });
    let tree = ExprTree::build(roots);
    super::walk(roots, |el| {
        if el.kind != Kind::Transition {
            return;
        }
        guard_is_boolean(el, index, &tree, &mut out);
        trigger_argument(el, index, &tree, &mut out);
    });
    out
}

/// `validateTransitionFeatureMembershipGuardExpression` — the guard's value has to be a Boolean.
///
/// The rule states three things of a guard: it is an `Expression`, its `result` is a Boolean, and
/// that result has multiplicity 1..1. The first is settled by the production (`if` owns an
/// `OwnedExpression`), the second is [`expr::boolean_result`], and the third has no counterpart in
/// the tree — an expression here is a value, not a feature with a multiplicity — so it is left
/// undecided rather than guessed.
///
/// Only a provably non-Boolean guard is reported: a `when`-style condition the tree cannot type is
/// skipped, because reporting an unknown would flag every model that guards on a library function.
fn guard_is_boolean(el: &Element, index: &Index, tree: &ExprTree, out: &mut Vec<Violation>) {
    let Some(root) = tree.root_of(el.id, ExprField::Predicate) else {
        return;
    };
    if expr::boolean_result(index, tree, root) != Some(false) {
        return;
    }
    let mut violation = Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "the guard of `{}` is `{}`, whose value is not a Boolean; a transition's `if` states a \
             condition (validateTransitionFeatureMembershipGuardExpression)",
            qualified(el, index),
            tree.node(root).text
        ),
    );
    // Report on the guard's own line, not the `transition` keyword's. A transition may write its
    // guard several lines below where it starts — `transition` / `first S2_1` / `if "test"` /
    // `then S2_2` — and Xpect asserts this rule `at "if \"test\""`, the guard's line
    // (`TransitionUsage_invalid.sysml.xt:60`). With `el.line` the report landed three lines above
    // the assertion and could never satisfy it.
    if let Some(line) = el.predicate_line {
        violation.line = line;
    }
    out.push(violation);
}

/// `validateTriggerInvocationExpressionWhenArgument`, `…AtArgument` and `…AfterArgument` — what
/// the trigger's argument has to be.
///
/// `when` takes a feature reference whose value is a Boolean (`argument->at(1).result` specializes
/// `ScalarValues::Boolean`); `at` takes one whose value is a time instant (`Time::TimeInstantValue`);
/// `after` takes a duration.
///
/// # `after`, and how much of it is decided
///
/// The OCL asks three things of an `after` argument: its result conforms to
/// `Quantities::ScalarQuantityValue`, a feature of it redefines `TensorQuantityValue::mRef` through
/// a redefinition closure, and that feature specializes `ISQBase::DurationUnit`. Only the last is
/// decidable here, and the notation gives it directly: `after 5 [min]` lowers to a `Binary(Unit)`
/// whose right side is the unit name, so the question is whether that name denotes something
/// specializing `DurationUnit` — a type test `Index::is_subtype` answers. `after 5 [kg]` is
/// reported; `after 5 [min]` is not.
///
/// The other two need the value to be a typed expression and `mRef` to be a resolvable element;
/// neither is in the tree. Neither is in the corpus either — the validation models contain no
/// `after` trigger, so nothing outside this file states what those two clauses would reject.
fn trigger_argument(el: &Element, index: &Index, tree: &ExprTree, out: &mut Vec<Violation>) {
    let Some(kind) = el.trigger_kind.as_deref() else {
        return;
    };
    let Some(root) = tree.root_of(el.id, ExprField::Trigger) else {
        return;
    };
    match kind {
        "when" => {
            if expr::boolean_result(index, tree, root) != Some(false) {
                return;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` triggers `when {}`, whose value is not a Boolean; a `when` waits for a \
                     condition (validateTriggerInvocationExpressionWhenArgument)",
                    qualified(el, index),
                    tree.node(root).text
                ),
            ));
        }
        "at" => {
            let Some(argument) = expr::result_of(index, tree, root) else {
                return;
            };
            let Some(type_text) = index.type_of(argument) else {
                return;
            };
            let Some(instant) = library_named(index, "TimeInstantValue") else {
                return;
            };
            let Some(typed) = resolve(index, argument, type_text) else {
                return;
            };
            if index.is_subtype(typed, instant) {
                return;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` triggers `at {}`, which is typed by `{}`, a {}; an `at` waits for a time \
                     instant (validateTriggerInvocationExpressionAtArgument)",
                    qualified(el, index),
                    tree.node(root).text,
                    index.qualified[typed],
                    index.kind[typed]
                ),
            ));
        }
        "after" => {
            // `after <quantity>` waits a length of time, and the OCL asks three things of the
            // argument: its result conforms to `Quantities::ScalarQuantityValue`, a feature of it
            // redefines `TensorQuantityValue::mRef`, and that feature specializes
            // `ISQBase::DurationUnit`.
            //
            // Only the first is decidable here. The tree carries `5 [min]` as a `Binary(Unit)`
            // whose right side is the unit name, so the question becomes whether that name denotes
            // something specializing `DurationUnit` — a type test, which `Index::is_subtype`
            // answers. The other two need `mRef` to be a resolvable element and the value to be a
            // typed expression; neither is in the tree, and neither is in the corpus: the
            // validation models contain no `after` trigger at all.
            let unit = match &tree.node(root).kind {
                expr::ExprKind::Binary(sysml_model::BinaryOp::Unit) => {
                    tree.node(root).children.get(1).copied()
                }
                _ => None,
            };
            let Some(unit) = unit else {
                return;
            };
            let expr::ExprKind::Ref(name) = &tree.node(unit).kind else {
                return;
            };
            let Some(duration) = library_named(index, "DurationUnit") else {
                return;
            };
            let sysml_resolve::Resolution::Found(unit_id) = index.resolve(el.id, &name.text) else {
                // The unit does not resolve, which the reference pass reports.
                return;
            };
            if index.is_subtype(unit_id, duration) {
                return;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` triggers `after {}`, whose unit `{}` does not specialize `DurationUnit`; \
                     an `after` waits for a duration (validateTriggerInvocationExpressionAfterArgument)",
                    qualified(el, index),
                    tree.node(root).text,
                    index.qualified[unit_id]
                ),
            ));
        }
        _ => {}
    }
}

/// The library element with this leaf name, if the library is loaded.
fn library_named(index: &Index, name: &str) -> Option<usize> {
    (0..index.len()).find(|&id| index.name[id] == name)
}

/// Resolve a type reference written on `owner`.
fn resolve(index: &Index, owner: usize, text: &str) -> Option<usize> {
    let head = text.split(',').next().unwrap_or(text).trim();
    match index.resolve(owner, head) {
        Resolution::Found(found) => Some(found),
        _ => None,
    }
}
