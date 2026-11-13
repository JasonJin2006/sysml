//! `invocation-parameters` — what a call's arguments redefine.
//!
//! Two rules over one relation, plus their constructor twin:
//!
//! - `validateInvocationExpressionParameterRedefinition`: `input->forAll(inp |
//!   inp.ownedRedefinition.redefinedFeature->intersection(parameters)->size() = 1)`. Each argument
//!   redefines exactly one input parameter of the callee.
//! - `validateInvocationExpressionNoDuplicateParameterRedefinition`: two different arguments must
//!   not redefine the same parameter.
//!
//! ```text
//! calc def F { in x : Real; in y : Real; return : Real = x + y; }
//! attribute ok  = F(x = 1, y = 2);
//! attribute dup = F(x = 1, x = 2);      // both arguments write `x`
//! attribute bad = F(z = 1, y = 2);      // `z` is not a parameter of `F`
//! ```
//!
//! # How it reads
//!
//! [`expr::arguments_of`](crate::expr::arguments_of) resolves each written name against the
//! instantiated type, so two arguments are "the same parameter" when they resolve to one id rather
//! than when they are spelled alike. An argument that names nothing the callee declares is
//! `validateInvocationExpressionParameterRedefinition`'s case: its `intersection` is empty, so the
//! count is zero rather than one.
//!
//! A positional argument names no feature and is skipped; the specification's relation is stated
//! over the redefinition, which a positional argument does not write.
//!
//! The report code this group reports under is `SEM095`.

use sysml_model::Element;
use sysml_resolve::Index;

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
                if !matches!(node.kind, ExprKind::Call | ExprKind::Constructor { .. }) {
                    continue;
                }
                // `intersection(parameters)->size() = 1` fails for an argument whose name the
                // callee does not declare. That argument is dropped by `arguments_of`, so it is
                // found by counting the names instead. The count comes first because a call whose
                // every name is unknown resolves to no argument at all — and that is the case the
                // guard used to skip, the one where nothing at all names a parameter.
                let written = node.arg_names.iter().filter(|n| n.is_some()).count();
                if written == 0 {
                    continue;
                }
                let arguments = expr::arguments_of(index, &tree, id);
                if written > arguments.len() {
                    out.push(Violation::new(
                        CODE,
                        el,
                        qualified(el, index),
                        format!(
                            "`{}` passes {} named argument(s) but only {} name a parameter of what \
                             it instantiates (validateInvocationExpressionParameterRedefinition)",
                            node.text,
                            written,
                            arguments.len()
                        ),
                    ));
                }
                // Two arguments redefining one parameter. The specification states this twice,
                // once for an invocation and once for a constructor, so the report names the rule
                // that belongs to the node it is about.
                let duplicate_rule = match node.kind {
                    ExprKind::Constructor { .. } => {
                        "validateConstructorExpressionNoDuplicateFeatureRedefinition"
                    }
                    _ => "validateInvocationExpressionNoDuplicateParameterRedefinition",
                };
                for (i, first) in arguments.iter().enumerate() {
                    for second in arguments.iter().skip(i + 1) {
                        if first.redefines != second.redefines {
                            continue;
                        }
                        out.push(Violation::new(
                            CODE,
                            el,
                            qualified(el, index),
                            format!(
                                "`{}` redefines `{}` twice; each argument redefines one \
                                 ({duplicate_rule})",
                                node.text, index.qualified[first.redefines]
                            ),
                        ));
                    }
                }
            }
        }
        // The constructor twin, same relation over `instantiatedType.feature`: the loop above
        // already covers it, because `arguments_of` answers for both node kinds and the
        // duplicate rule is stated identically for `ConstructorExpression`.
    });
    out
}
