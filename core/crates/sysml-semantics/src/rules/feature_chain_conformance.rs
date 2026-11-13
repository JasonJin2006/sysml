//! `feature-chain` — what a chained access reads.
//!
//! `validateFeatureChainExpressionConformance`: `argument->notEmpty() implies
//! targetFeature.isFeaturedWithin(argument->first().result)`. `part v : V; v->mass` reads `mass`
//! out of `v`, so `mass` has to be something `v`'s type provides; otherwise the path reads a
//! feature from a value that does not have it.
//!
//! ```text
//! part def V { attribute mass : Real; }
//! part v : V;
//! attribute a : Real = v->mass;     // legal
//! attribute b : Real = v->nowhere;  // rejected
//! ```
//!
//! # How it reads
//!
//! [`ExprKind::Chain`](crate::expr::ExprKind::Chain) is the node, its first child is the base, and
//! [`expr::result_of`](crate::expr::result_of) answers both ends: the base denotes `v`, and the
//! chain denotes the feature the operator names. When the operator names nothing, `result_of`
//! answers `None` and the chain is the violation — the operator is stored as text rather than as a
//! resolved reference, so the reference pass never sees it and nothing else reports it.
//!
//! # What it cannot say
//!
//! The OCL compares against `isFeaturedWithin`, which follows the *argument's* result type. This
//! projection asks the base's own members and then its type's, which is the same relation one step
//! deep; a chain whose base is itself a chain is followed through [`expr::result_of`] rather than
//! re-answered.
//!
//! The report code this group reports under is `SEM095`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use crate::expr::{self, ExprKind, ExprTree};

use super::Violation;

const CODE: &str = "SEM095";

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
        for &root in tree.roots_of(el.id) {
            for id in tree.preorder(root) {
                let node = tree.node(id);
                let ExprKind::Chain { op, .. } = &node.kind else {
                    continue;
                };
                // `->size()` and friends are sequence operations on the library's collections, not
                // feature accesses: the operator is a function of the base's value, not a member of
                // it. Only a chain whose base resolves to a feature is judged.
                let Some(base) = node.children.first().copied() else {
                    continue;
                };
                let Some(container) = expr::result_of(index, &tree, base) else {
                    continue;
                };
                if expr::result_of(index, &tree, id).is_some() {
                    continue;
                }
                // A sequence operation is not a feature access. `frontWheel->#(1)` indexes and
                // `x->forAll{…}` quantifies; both name functions of the base's *value* from the
                // kernel library, and neither is a member its type provides. Resolving the
                // operator against the library is what tells them apart.
                //
                // The library resolution is not enough on its own: a model that imports nothing
                // cannot resolve `size` anywhere, so `parts->size()` was reported as reading a
                // feature `parts` does not have. The language's own function table answers the
                // same question without needing an import.
                let op_name = op.split('\u{28}').next().unwrap_or(op).trim();
                if !is_plain_name(op)
                    || crate::is_builtin(op_name)
                    || names_a_library_operation(index, container, op)
                {
                    continue;
                }
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` reads `{op}` from `{}`, which provides no such feature \
                         (validateFeatureChainExpressionConformance)",
                        node.text,
                        tree.node(base).text
                    ),
                ));
            }
        }
    });
    out
}

/// Whether `op`, applied to `container`, names a kernel-library function.
///
/// `x->size()`, `x->#(1)` and `x->forAll { … }` are operations over the value of `x`, not features
/// of its type. The library declares them as functions, so a name that resolves in the library and
/// is not a feature of the container is one of these.
fn names_a_library_operation(index: &Index, container: usize, op: &str) -> bool {
    let name = op.split('\u{28}').next().unwrap_or(op).trim();
    if name.is_empty() {
        return false;
    }
    let Resolution::Found(found) = index.resolve(container, name) else {
        return false;
    };
    index.is_library(found)
}

/// Whether the operator is a feature name at all.
///
/// `frontWheel->#(1)` applies the kernel library's indexing operator, whose spelling is a single
/// symbol and not a name any type could declare. Only an identifier can name a feature, so anything
/// else is an operation over the base's value.
fn is_plain_name(op: &str) -> bool {
    let head = op.split('\u{28}').next().unwrap_or(op).trim();
    let mut chars = head.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}
