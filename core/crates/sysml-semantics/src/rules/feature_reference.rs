//! `feature-reference` — a name used as a value names a feature.
//!
//! `validateFeatureReferenceExpressionReferentIsFeature`: the first owned membership of a
//! `FeatureReferenceExpression` that is not a `ParameterMembership` must have a `Feature` as its
//! member. In the notation, a bare name in value position *is* a `FeatureReferenceExpression`, and
//! its referent is what the name resolves to — the specification states that in one sentence: "A
//! `FeatureReferenceExpression` is an `Expression` whose `result` is bound to a `referent`
//! `Feature`".
//!
//! ```text
//! part def Wheel;
//! attribute m : Real = Wheel;     // rejected: `Wheel` is a definition, not a feature
//! part w : Wheel;
//! attribute n : Real = w.mass;    // legal: a feature
//! ```
//!
//! # How it reads
//!
//! [`ExprKind::Ref`](crate::expr::ExprKind::Ref) is the node a bare name or dotted path projects
//! to, and [`ExprKind::metaclasses`] answers `FeatureReferenceExpression` for it. The referent is
//! what the name resolves to through [`expr::result_of`](crate::expr::result_of), which for a
//! reference is exactly what it names.
//!
//! A name that does not resolve is skipped: the reference pass reports it, and a rule about what
//! the name *is* would be reporting the same defect twice.
//!
//! # What is not decided
//!
//! `validateFeatureReferenceExpressionResult` (`result.owningType = self`) needs no check: the
//! specification says an `Expression` "always has a single `result` parameter", so the constraint
//! restates the metaclass. It is recorded as not applicable rather than enforced here.
//!
//! The report code this group reports under is `SEM002`.

use sysml_model::Element;
use sysml_resolve::Index;

use crate::expr::{self, ExprKind, ExprTree};

use super::Violation;

const CODE: &str = "SEM002";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a name resolving to this kind is being used as a value it does not denote.
///
/// Only a *definition* is purely a type: `Wheel` names what a part is typed by, and reading it as a
/// value asks for something the model has no instance of. A usage of any kind denotes a value, and
/// so does a `Variant` — the projection files the latter under `is_definition` because it is
/// declared like one, but a variant member is an alternative that a configuration selects, which is
/// a value.
fn is_type_used_as_value(kind: &sysml_model::Kind) -> bool {
    use sysml_model::Kind;
    matches!(
        kind,
        Kind::PartDef
            | Kind::PortDef
            | Kind::AttributeDef
            | Kind::RequirementDef
            | Kind::VerificationDef
            | Kind::ActionDef
            | Kind::ConstraintDef
            | Kind::RenderingDef
            | Kind::ClassDef
            | Kind::DataTypeDef
            | Kind::StructureDef
            | Kind::AssocDef
            // `assoc struct` is one identity that is both an association and a structure, and both
            // are definitions: `Objects::BinaryLinkObject` names a type, never a value. Omitting it
            // left the one spelling of an association that is also a structure unchecked, so
            // `attribute z = BinaryLinkObject;` read as a feature reference.
            | Kind::AssocStructureDef
            | Kind::ItemDef
            | Kind::OccurrenceDef
            | Kind::StateDef
            | Kind::CalcDef
            | Kind::ConnectionDef
            | Kind::InterfaceDef
            | Kind::AllocationDef
            | Kind::EnumDef
            | Kind::MetadataDef
            | Kind::CaseDef
            | Kind::AnalysisCaseDef
            | Kind::UseCaseDef
            | Kind::ViewDef
            | Kind::ViewpointDef
            | Kind::ConcernDef
            | Kind::KermlType
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    let tree = ExprTree::build(roots);
    super::walk(roots, |el| {
        for &root_id in tree.roots_of(el.id) {
            for id in tree.preorder(root_id) {
                let node = tree.node(id);
                let ExprKind::Ref(name) = &node.kind else {
                    continue;
                };
                // Only a node used *as a value* is a `FeatureReferenceExpression`. Three shapes are
                // not:
                //
                //   * a callee — `F(1)` names something callable, and whether it may be called is
                //     the invocation rules' question;
                //   * a predicate's head, which is a Boolean expression rather than a value;
                //   * a type named by a type test, which names a type by definition.
                if let Some(parent) = node.parent {
                    if tree.node(parent).kind.is_invocation() {
                        continue;
                    }
                }
                if node.parent.is_none() && node.field != expr::ExprField::Value {
                    continue;
                }
                let Some(target) = expr::result_of(index, &tree, id) else {
                    continue;
                };
                // A definition is a type, and a type used as a value names no value: `Wheel`
                // is what a part is *typed by*, not something a feature can read. Everything
                // else — a usage, a variant, a reference — denotes a value, including the shapes
                // this projection files under `is_definition` for the purposes of other rules.
                // A variant is an alternative a configuration selects, which is a value: the
                // reference to it denotes one, so the rule does not fire. A `variant part car`
                // keeps `Kind::PartUsage` now, so the flag is what says so.
                if index.is_variant.get(target).copied().unwrap_or(false) {
                    continue;
                }
                if !is_type_used_as_value(&index.kind[target]) {
                    continue;
                }
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` is used as a value but names `{}`, a {}; a name in value position \
                         must name a feature (validateFeatureReferenceExpressionReferentIsFeature)",
                        name.text,
                        index.qualified[target],
                        index.kind[target].keyword()
                    ),
                ));
            }
        }
    });
    out
}
