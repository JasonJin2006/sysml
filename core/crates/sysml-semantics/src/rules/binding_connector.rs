//! `binding-connector` — a binding joins exactly two features, and they hold one value.
//!
//! Two checks live here, and they have **different bases**, which matters for traceability:
//!
//!   * `validateBindingConnectorIsBinary`: `relatedFeature->size() = 2` — an OCL rule of the
//!     normative metamodel (`generated.rs` rule table). Reported as an error under `SEM060`.
//!   * bound features hold the same value, so their types must conform — this is the class's own
//!     normative prose, not a `validate*` OCL body: `BindingConnector` "requires its relatedFeatures
//!     to identify the same things (have the same values)" (`generated.rs:110-111`). The 180-rule
//!     table contains exactly one `BindingConnector` rule, the binary one above (measured: one
//!     `metaclass: ElementKind::BindingConnector` entry), so no OCL constraint states the type
//!     requirement. Reported as a **warning** under the same `SEM060` topic code, with its basis
//!     named in the message, because a warning is what a legal-but-almost-certainly-wrong model
//!     deserves and because the message must not claim an OCL rule that does not exist.
//!
//! `bind a = b` states that two features always hold the same value; a third end has no reading —
//! it is neither an equality of two nor a chain the notation defines.
//!
//! ```text
//! bind fuelLevel = tank.level;         // two ends
//! bind a = b = c;                      // rejected (arity)
//! bind a = b;                          // warned when A and B do not conform
//! ```
//!
//! # How it reads
//!
//! The binding's two ends are `Element::from` and `Element::to`. A third was previously not read
//! at all: the parser stopped after the second and the rest of the line fell into the body as a
//! stray member, so the model silently gained an element instead of a diagnostic. The parser now
//! collects those ends into `Statement::Bind::extra`, the lowering keeps them in
//! `Element::extra_ends`, and this rule reports them.
//!
//! The arity report code is `SEM060`; the type-conformance warning shares it, and its message names
//! its basis because no OCL rule states it.

use std::collections::HashMap;

use sysml_model::{BinaryOp, Element, Kind, UnaryOp};
use sysml_resolve::{Index, Resolution};

use crate::expr::{self, ExprId, ExprKind, ExprTree};
use crate::{derived, typing};

use super::Violation;

/// The OCL rule `validateBindingConnectorIsBinary` — at most two related features.
const CODE: &str = "SEM060";

/// The class-semantics check that bound features hold conforming types.
///
/// This shares `SEM060` with the arity rule deliberately: a report code groups rules by *topic*
/// here, not one-to-one (`SEM020` carries ten subject/objective rules, `SEM044` six reference
/// rules), so grouping two `bind` findings under one code is the house pattern, not a collision.
///
/// What is *not* shared is the basis, and the message says so: the arity rule implements the OCL
/// constraint `validateBindingConnectorIsBinary`, this one implements the class's own normative
/// prose. The 180-rule table contains exactly one `BindingConnector` rule (measured: one
/// `metaclass: ElementKind::BindingConnector` entry), so no OCL body states the type requirement.
/// A reader who follows the message back to the specification must not be told a `validate*` rule
/// says this, which is why the message names the class semantics instead of citing a rule name.
const CODE_CONFORMS: &str = CODE;

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The type a `bind` end names.
fn end_type(index: &Index, from: usize, end: &str) -> Option<usize> {
    let text = end.trim();
    // `bind b1.a = b2.a2;` names both ends with a dotted path, and the single-name lookup asks
    // whether `b1.a` is declared in this scope, which it never is — so the conformance check below
    // never ran for the ordinary spelling (`BindingConnector_invalid0.sysml.xt:41`).
    let found = if text.contains('.') {
        let path = sysml_model::NameRef::new(text, index.line[from]);
        index.resolve_path(from, &path)
    } else {
        index.resolve(from, text)
    };
    let Resolution::Found(id) = found else {
        return None;
    };
    let type_text = index.type_of(id)?.to_string();
    let head = type_text
        .split(',')
        .next()
        .unwrap_or(&type_text)
        .trim()
        .to_string();
    match index.resolve(id, &head) {
        Resolution::Found(target) => Some(target),
        _ => None,
    }
}

/// Operators are implicit invocations of the library functions, even without an import of the
/// operator's package (reference ExpressionUtil::getOperatorQualifiedNames). Resolve the signature
/// once per symbol; the argument check reads parameter types, not an inferred result type.
fn operator_function(index: &Index, symbol: &str) -> Option<usize> {
    ["BaseFunctions", "DataFunctions", "ControlFunctions"]
        .iter()
        .find_map(|package| {
            let name = format!("{package}::{symbol}");
            index.qualified.iter().position(|q| q == &name)
        })
}

fn operand_types(index: &Index, tree: &ExprTree, id: ExprId) -> Vec<usize> {
    let node = tree.node(id);
    if let ExprKind::Ref(name) = &node.kind {
        if let Some(target) = expr::resolve_from(index, node.owner, &name.text) {
            return derived::types(index, target);
        }
        return Vec::new();
    }
    typing::type_of(index, tree, id).into_iter().collect()
}

fn operator_bindings(
    el: &Element,
    index: &Index,
    tree: &ExprTree,
    functions: &mut HashMap<&'static str, Option<usize>>,
    out: &mut Vec<Violation>,
) {
    for &root in tree.roots_of(el.id) {
        for id in tree.preorder(root) {
            let node = tree.node(id);
            let symbol = match node.kind {
                ExprKind::Binary(
                    op @ (BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Rem
                    | BinaryOp::Pow
                    | BinaryOp::Less
                    | BinaryOp::LessEq
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEq
                    | BinaryOp::Eq
                    | BinaryOp::NotEq
                    | BinaryOp::Xor),
                ) => op.symbol(),
                ExprKind::Unary(op @ (UnaryOp::Plus | UnaryOp::Minus | UnaryOp::Not)) => {
                    op.symbol()
                }
                // Unit application and short-circuit control operators have different implicit
                // binding shapes; do not guess that every child binds to a data parameter.
                _ => continue,
            };
            let Some(function) = *functions
                .entry(symbol)
                .or_insert_with(|| operator_function(index, symbol))
            else {
                continue;
            };
            for (&operand, parameter) in node
                .children
                .iter()
                .zip(derived::input_parameters(index, function))
            {
                let actual = operand_types(index, tree, operand);
                let expected = derived::types(index, parameter);
                if actual.is_empty() || expected.is_empty() {
                    continue;
                }
                let compatible = |one: &[usize], other: &[usize]| {
                    one.iter()
                        .all(|&a| other.iter().any(|&b| index.is_subtype(a, b)))
                };
                if compatible(&actual, &expected) || compatible(&expected, &actual) {
                    continue;
                }
                let names = |types: &[usize]| {
                    types
                        .iter()
                        .map(|&t| index.qualified[t].as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                out.push(Violation::warning(
                    CODE_CONFORMS,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` implicitly binds argument `{}` of type `{}` to `{}` of type `{}`; \
                         bound features should have conforming types (BindingConnector's \
                         same-values semantics)",
                        node.text,
                        tree.node(operand).text,
                        names(&actual),
                        index.qualified[parameter],
                        names(&expected)
                    ),
                ));
            }
        }
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    let tree = ExprTree::build(roots);
    let mut functions = HashMap::new();
    super::walk(roots, |el| {
        operator_bindings(el, index, &tree, &mut functions, &mut out);
        if el.kind == Kind::Bind {
            // "Bound features should have conforming types": the two ends hold one value, so one
            // type must be the other or a subtype. `bind b = c;` with `C specializes B` is legal;
            // `bind a = b;` with `A` and `B` unrelated is not
            // (`BindingConnector_Invalid3.sysml.xt:57`).
            if let (Some(from), Some(to)) = (&el.from, &el.to) {
                if let (Some(left), Some(right)) =
                    (end_type(index, el.id, from), end_type(index, el.id, to))
                {
                    if left != right
                        && !index.is_subtype(left, right)
                        && !index.is_subtype(right, left)
                    {
                        out.push(Violation::warning(
                            CODE_CONFORMS,
                            el,
                            qualified(el, index),
                            format!(
                                "`{}` binds features of unrelated types `{}` and `{}`; a binding \
                                 requires its ends to identify the same values, so one type must \
                                 conform to the other (BindingConnector's own semantics, not a \
                                 `validate*` OCL rule)",
                                qualified(el, index),
                                index.qualified[left],
                                index.qualified[right]
                            ),
                        ));
                    }
                }
            }
        }
        if el.kind != Kind::Bind || el.extra_ends.is_empty() {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "a binding states {} ends; a binding joins exactly two \
                 (validateBindingConnectorIsBinary)",
                2 + el.extra_ends.len()
            ),
        ));
    });
    out
}
