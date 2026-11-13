//! `type-relationships` — the rules on a type's `unions` / `intersects` / `differences` clauses.
//!
//! A classifier may state how its values relate to other types' values:
//!
//! ```text
//! datatype Combined unions A, B;
//! datatype Overlap  intersects C;
//! datatype Rest     differences D;
//! ```
//!
//! The specification states six constraints over those three operators, two per operator and the
//! same pair each time:
//!
//!   * `validateTypeUnioningTypesNotSelf` / `...IntersectingTypesNotSelf` /
//!     `...DifferencingTypesNotSelf` — a type may not name itself. `M unions M` says `M`'s values
//!     are `M`'s values, which defines nothing.
//!   * `validateTypeOwnedUnioningNotOne` / `...IntersectingNotOne` / `...DifferencingNotOne` —
//!     `ownedUnioning->size() <> 1`. One operand is a no-op: `M unions A` alone is just `M`
//!     restricted to `A`, which is what a subsetting says; the operator exists for the set
//!     operation, so a single operand is a mistake in the model's intent.
//!
//! # Why the tree can now answer
//!
//! These clauses are part of the declaration head (`KerML.xtext:340-365`,
//! `TypeRelationshipPart`), not body members. Until the parser read them, `datatype B unions A`
//! left `unions` and `A` in the body as two stray attributes — the model gained two members it
//! never wrote. `Element::unions` / `intersects` / `differences` now hold them, and the six
//! rules are decidable.
//!
//! `disjoint from` is the fourth operator; the specification states no such pair on it, so
//! `Element::disjoints` is kept but no rule reads it.
//!
//! The report code this group reports under is `SEM046`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM046";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The rules one operator answers.
struct Operator {
    clause: &'static str,
    not_self: &'static str,
    not_one: &'static str,
    targets: fn(&Element) -> &Vec<String>,
}

const OPERATORS: &[Operator] = &[
    Operator {
        clause: "unions",
        not_self: "validateTypeUnioningTypesNotSelf",
        not_one: "validateTypeOwnedUnioningNotOne",
        targets: |el| &el.unions,
    },
    Operator {
        clause: "intersects",
        not_self: "validateTypeIntersectingTypesNotSelf",
        not_one: "validateTypeOwnedIntersectingNotOne",
        targets: |el| &el.intersects,
    },
    Operator {
        clause: "differences",
        not_self: "validateTypeDifferencingTypesNotSelf",
        not_one: "validateTypeOwnedDifferencingNotOne",
        targets: |el| &el.differences,
    },
];

/// Whether a kind can carry these clauses. They are stated of a Type, which in this tree is any
/// element that can own members and has a supertype clause.
fn is_type(kind: &Kind) -> bool {
    kind.is_definition()
        || matches!(
            kind,
            Kind::KermlType
                | Kind::ClassDef
                | Kind::DataTypeDef
                | Kind::StructureDef
                | Kind::AssocDef
        )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_type(&el.kind) {
            return;
        }
        for op in OPERATORS {
            let targets = (op.targets)(el);
            if targets.is_empty() {
                continue;
            }
            // `ownedUnioning->size() <> 1`
            if targets.len() == 1 {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` states exactly one `{}` operand; the clause takes two or more \
                         ({})",
                        qualified(el, index),
                        op.clause,
                        op.not_one
                    ),
                ));
            }
            // `unioningType->excludes(self)`
            for text in targets {
                let head = text.split(',').next().unwrap_or(text).trim();
                let Resolution::Found(target) = index.resolve(el.id, head) else {
                    continue;
                };
                if target != el.id {
                    continue;
                }
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` names itself in its `{}` clause ({}); a type may not \
                         {} itself",
                        qualified(el, index),
                        op.clause,
                        op.not_self,
                        op.clause
                    ),
                ));
            }
        }
    });
    out
}
