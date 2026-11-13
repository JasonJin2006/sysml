//! `invocation` — a call names something callable.
//!
//! `validateInvocationExpressionInstantiatedType`: `instantiatedType.oclIsKindOf(Behavior) or
//! instantiatedType.oclIsKindOf(Feature) and instantiatedType.type->exists(oclIsKindOf(Behavior))
//! and instantiatedType.type->size(1)`. A call supplies arguments to a behaviour; a name that is a
//! part, a value or a state has nothing to supply them to.
//!
//! ```text
//! calc def Double { in x : Integer; return : Integer = x; }
//! attribute good = Double(2);          // legal: a calculation
//! part def Wheel;
//! attribute bad = Wheel(2);            // rejected: a part definition is not callable
//! ```
//!
//! # How it reads
//!
//! The call sites are already on the tree: `attribute r = Bad()` lowers to
//! `Expr::Call { callee: Ref("Bad"), args }` in `Element::value_expr`, and a constraint's
//! expression is in `Element::predicate`. This walks both and resolves each callee from the
//! element's own scope — the same scope the reference checker uses, so a call to a name that does
//! not resolve is left to that check rather than reported twice.
//!
//! The kinds accepted are the behaviours this tree has. A `Feature` whose type is a behaviour —
//! the second half of the OCL — is the `perform`-style call, which the tree expresses as a
//! `Kind::Perform` or an action-typed feature; those resolve through `supertypes_of` to an action
//! definition and are accepted by the same test.
//!
//! # What is not covered
//!
//! `validateInvocationExpressionOwnedFeatures` bounds the *direction of the argument bindings*
//! (`ownedFeature->forAll(f | f <> result implies f.direction = in)`). An invocation's owned
//! features are the parameter redefinitions the notation materialises for each argument, and this
//! tree keeps an argument as an `Expr`, not as a feature — so there is nothing to read a direction
//! from. It is left out rather than approximated from the callee's parameter list, which would
//! report the callee rather than the call.
//!
//! The report code this group reports under is `SEM083`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use crate::expr::{ExprId, ExprTree};

use super::Violation;

const CODE: &str = "SEM083";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is something an invocation can supply arguments to.
///
/// `validateInvocationExpressionInstantiatedType`:
/// `instantiatedType.oclIsKindOf(Behavior) or instantiatedType.oclIsKindOf(Feature) and
/// instantiatedType.type->exists(oclIsKindOf(Behavior)) and instantiatedType.type->size(1)`.
///
/// The two metaclass questions are asked of the hierarchy, so a `calc def`, an `action def`, a
/// requirement and a case all reach the first arm through `Behavior`, while a `calc x : C` or a
/// `perform action` reaches the second as a `Feature`. The `type->exists(oclIsKindOf(Behavior))`
/// conjunct of the second arm is **not** decided here: `instantiatedType` is a derived property
/// (TODO, Layer 2), and until it exists the rule reads the callee's own kind rather than guessing
/// its type. That under-reports — a feature typed by something that is not a behaviour is accepted
/// — which is the direction a missed violation is preferred in.
///
/// A requirement is callable from a verification case: the case's verdict is the requirement's
/// result, and `RequirementDefinition specializes ConstraintDefinition specializes … Function
/// specializes Behavior`, so the first arm covers it.
fn is_callable(kind: &Kind) -> bool {
    if kind.is_metaclass("Behavior") || kind.is_metaclass("Feature") {
        return true;
    }
    // The kernel function library declares its operators as KerML type declarations, so a call to
    // `sum` or `round` resolves to one. Which KerML classifier it is was not kept — `function`,
    // `datatype` and `class` share `Kind::KermlType` — so no metaclass question can answer it, and
    // refusing them would report every library call in every model.
    *kind == Kind::KermlType
}

/// Every callee in an expression, in authored order.
///
/// Read from the expression id space rather than by walking [`Expr`] again: a call is a node whose
/// first child is its callee, and [`ExprTree::preorder`] visits every node once.
fn callees<'a>(tree: &'a ExprTree, root: ExprId, out: &mut Vec<&'a sysml_model::NameRef>) {
    for id in tree.preorder(root) {
        let node = tree.node(id);
        if node.kind != crate::expr::ExprKind::Call {
            continue;
        }
        if let Some(callee) = node.children.first() {
            if let crate::expr::ExprKind::Ref(name) = &tree.node(*callee).kind {
                out.push(name);
            }
        }
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    let tree = ExprTree::build(roots);
    super::walk(roots, |el| {
        let mut found: Vec<&sysml_model::NameRef> = Vec::new();
        for root in tree.roots_of(el.id) {
            callees(&tree, *root, &mut found);
        }
        for name in found {
            let Resolution::Found(target) = index.resolve(el.id, &name.text) else {
                continue;
            };
            if is_callable(&index.kind[target]) {
                continue;
            }
            // A feature typed by a behaviour is the second half of the OCL; resolving the call
            // through the feature's own type is what `supertypes_of` already does.
            if index
                .type_of(target)
                .and_then(|t| {
                    let head = t.split(',').next().unwrap_or(t).trim();
                    match index.resolve(target, head) {
                        Resolution::Found(typed) => Some(typed),
                        _ => None,
                    }
                })
                .is_some_and(|typed| is_callable(&index.kind[typed]))
            {
                continue;
            }
            let target_kind = index.kind[target].keyword();
            let target_name = index
                .qualified
                .get(target)
                .cloned()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| name.text.clone());
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` calls `{target_name}`, a {target_kind}; a call supplies arguments to a \
                     behaviour (validateInvocationExpressionInstantiatedType)",
                    qualified(el, index)
                ),
            ));
        }
    });
    out
}
