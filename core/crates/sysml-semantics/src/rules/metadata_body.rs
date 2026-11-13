//! `metadata-body` — the members a metadata usage writes are redefinitions, not declarations.
//!
//! `validateMetadataFeatureBody`, read for the two clauses this tree can answer:
//!
//! ```text
//! @Safety {
//!     isMandatory = true;      // legal: a redefinition of the metadata definition's feature
//!     colour = red;            // rejected: nothing of that name to redefine
//! }
//! ```
//!
//! A metadata body fills in the features its metadata definition declares. Writing a name the
//! definition does not have declares a new feature instead, which the annotation has no schema for;
//! that is why the OCL asks for `declaredName = null` (the member is anonymous, its identity being
//! the redefinition) and `redefinition.redefinedFeature->size() = 1` (it redefines exactly one).
//!
//! # How it reads
//!
//! `@A { x = 3; }` lowers to a `Kind::MetadataUsage` whose children are the body members. A member
//! that states `redefines T` is judged by whether `T` resolves; a member that writes no
//! redefinition and carries a name is a declaration, and is reported.
//!
//! # The third clause
//!
//! `f.valuation.value.isModelLevelEvaluable`: a body fills in a value a reader of the annotation
//! takes as given, so the value cannot depend on anything computed later. This tree keeps the value
//! twice — as text on the feature, and as an expression in
//! [`ExprTree`](crate::expr::ExprTree) — and the expression is what the clause reads, through
//! [`expr::is_model_level_evaluable`](crate::expr::is_model_level_evaluable).
//!
//! The report code this group reports under is `SEM045`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use crate::expr::{self, ExprField, ExprTree};

use super::Violation;

const CODE: &str = "SEM045";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Judge one metadata body: every feature in it must redefine a feature of `definition`.
///
/// A member that carries a body is the definition of *that* body — `@A { u { v = 1; bad; } }` fills
/// in the feature `u` of `A`, and `u`'s own members (`feature v : Integer`, or the features of `u`'s
/// type) are what `v` and `bad` have to redefine. Recursing here is what reaches the nested body:
/// `u` lowers to an ordinary feature, not to a [`Kind::MetadataUsage`], so a walk over metadata
/// usages alone never visited it and the pilot's "Must redefine an owning-type feature" on `bad;`
/// (`MetadataFeature_invalid.kerml.xt:74`, `MetadataUsage_Invalid.sysml.xt:90`) had nothing to
/// match.
fn judge_body(
    index: &Index,
    tree: &ExprTree,
    definition: usize,
    owner: &Element,
    members: &[Element],
    out: &mut Vec<Violation>,
) {
    for member in members {
        // A nested declaration (a metadata usage inside one, an import) is not a body member in
        // the sense this rule counts.
        if !member.kind.is_feature() {
            continue;
        }
        let redefines = index.redefines_of(member.id);
        // A body member redefines exactly one feature. The notation writes that two ways: an
        // explicit `y redefines x`, and the bare `x = 3` the corpus and the pilot both use, where
        // the name itself is the redefinition target. Both are one redefinition.
        if redefines.len() > 1 {
            out.push(Violation::new(
                CODE,
                member,
                qualified(member, index),
                format!(
                    "`{}` in the body of `{}` redefines {} features; a body member redefines exactly one (validateMetadataFeatureBody)",
                    member.name,
                    qualified(owner, index),
                    redefines.len()
                ),
            ));
            continue;
        }
        // Whichever spelling, the thing being redefined has to be a feature of the metadata
        // definition. The pilot states this as "Must redefine an owning-type feature" at the member
        // that names nothing of the sort.
        let target = match redefines.first() {
            Some(text) => text.trim().to_string(),
            None => member.name.clone(),
        };
        if !target.is_empty() && index.member_of(definition, &target).is_empty() {
            out.push(Violation::new(
                CODE,
                member,
                qualified(member, index),
                format!(
                    "`{}` in the body of `{}` redefines `{target}`, which the metadata definition does not declare (validateMetadataFeatureBody)",
                    member.name,
                    qualified(owner, index)
                ),
            ));
        }
        // The value has to be a constant. `x = ~3` and `z = f((as A).y)` both need an evaluator to
        // say what they are worth; an annotation is read as written.
        if let Some(value) = tree.root_of(member.id, ExprField::Value) {
            if expr::is_model_level_evaluable(tree, value) == Some(false) {
                out.push(Violation::new(
                    CODE,
                    member,
                    qualified(member, index),
                    format!(
                        "the value of `{}` in the body of `{}` cannot be evaluated at model level; an annotation states values, not computations (validateMetadataFeatureBody)",
                        member.name,
                        qualified(owner, index)
                    ),
                ));
            }
        }
        // The body of a member is judged against the member the *definition* declares, not against
        // the member in the annotation: `@A { u { v = 1; bad; } }` fills in `A::u`, and it is
        // `A::u`'s features that `v` and `bad` have to redefine. Judging against the annotation's
        // own `u` finds `bad` among its children and calls it declared.
        if !member.children.is_empty() {
            let counterpart = index.member_of(definition, &member.name).first().copied();
            let nested = match counterpart {
                Some(counterpart) => counterpart,
                None => {
                    let scope = index
                        .parent
                        .get(member.id)
                        .copied()
                        .flatten()
                        .unwrap_or(member.id);
                    match index.type_of(member.id) {
                        Some(typing) => match index.resolve_excluding(scope, typing, member.id) {
                            Resolution::Found(definition) => definition,
                            _ => member.id,
                        },
                        None => member.id,
                    }
                }
            };
            judge_body(index, tree, nested, member, &member.children, out);
        }
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    let tree = ExprTree::build(roots);
    super::walk(roots, |el| {
        if el.kind != Kind::MetadataUsage {
            return;
        }
        // The metadata definition the usage names — `@A { … }` is typed by `A`, and `A`'s features
        // are what the body may fill in.
        //
        // Resolved from the *owner*, not from the usage: `resolve` starts at the element it is
        // given, and a metadata usage named `A` sitting in a body is itself a member called `A`,
        // so asking from the usage answers the usage. That shadowing is a real defect in the
        // reference pass, but it is not this rule's to fix, and the owner is the scope the name was
        // written in either way.
        let Some(typing) = index.type_of(el.id) else {
            return;
        };
        let scope = index.parent.get(el.id).copied().flatten().unwrap_or(el.id);
        let Resolution::Found(definition) = index.resolve_excluding(scope, typing, el.id) else {
            return;
        };
        judge_body(index, &tree, definition, el, &el.children, &mut out);
    });
    out
}
