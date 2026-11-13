//! `element-filter` — a filter's condition states a Boolean.
//!
//! `validateElementFilterMembershipConditionIsBoolean`: `condition.result.specializesFromLibrary(
//! 'ScalarValues::Boolean')`. A `filter` chooses which elements a view shows, so its condition is a
//! predicate; a condition whose value is a length or a name selects nothing.
//!
//! ```text
//! view def Safety {
//!     filter @Safety | @Security;    // a Boolean: the tag test, joined
//!     filter mass;                   // rejected when `mass` is a Real
//! }
//! ```
//!
//! # How it reads
//!
//! The condition is [`ExprField::Value`] on the [`Kind::Filter`] element, and the question is
//! [`expr::boolean_result`]. Three answers matter:
//!
//!   * `Some(false)` — the tree can see the type and it is not a Boolean: reported.
//!   * `Some(true)` — a comparison, a logical join, a type test or a Boolean-typed reference.
//!   * `None` — not decidable here, and skipped. `@Safety` on its own is `Opaque` in the tree
//!     (the metadata access is kept as text), so no rule can say what its value is; reporting it
//!     would flag every filter in the corpus.
//!
//! `validateElementFilterMembershipConditionIsModelLevelEvaluable` is the second half: a condition
//! must also be a *constant*, because a filter is read once, when the view is selected, and a value
//! nobody has computed selects nothing. The criterion is
//! [`expr::is_model_level_evaluable`], and `None` there means this layer cannot tell — `filter mass`
//! names a feature whose constancy depends on the feature, and skipping is the only honest answer.
//!
//! The report code this group reports under is `SEM087`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use crate::expr::{self, ExprField, ExprTree};

use super::Violation;

const CODE: &str = "SEM087";

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
    let tree = ExprTree::build(roots);
    super::walk(roots, |el| {
        if el.kind != Kind::Filter {
            return;
        }
        let Some(root) = tree.root_of(el.id, ExprField::Value) else {
            return;
        };
        if expr::boolean_result(index, &tree, root) == Some(false) {
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "the filter condition of `{}` is `{}`, whose value is not a Boolean; a filter \
                     selects with a predicate (validateElementFilterMembershipConditionIsBoolean)",
                    qualified(el, index),
                    tree.node(root).text
                ),
            ));
        }
        if expr::is_model_level_evaluable(&tree, root) == Some(false) {
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "the filter condition of `{}` is `{}`, which cannot be evaluated at model \
                     level; a filter selects with a constant \
                     (validateElementFilterMembershipConditionIsModelLevelEvaluable)",
                    qualified(el, index),
                    tree.node(root).text
                ),
            ));
        }
    });
    out
}
