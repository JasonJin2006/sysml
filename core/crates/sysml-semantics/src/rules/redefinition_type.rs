//! `redefinition-type` — a redefinition says no more than the feature it replaces.
//!
//! `validateRedefinitionDirectionConformance` has a sibling the specification never states: the
//! type of a redefinition must conform to the type of the feature it redefines. It is **not** one
//! of the 180 OCL constraints — the metamodel assumes values conform and says nothing about the
//! types that would make them — which is why type checking is something a commercial
//! implementation advertises as a feature beyond the standard.
//!
//! ```text
//! part def Engine;
//! part def Wheel;
//! part def Car {
//!     part engine : Engine;
//!     part :>> engine : Wheel;   // rejected: a Wheel is not an Engine
//! }
//! ```
//!
//! Narrowing is legal: a redefinition typed by a subtype of the original type says the same thing
//! more precisely, and `conforms` walks the specialization closure so it accepts exactly that.
//!
//! # Why this is a warning
//!
//! A model that breaks it is still well formed by the specification's own rules; what it is not is
//! type-sound. The reference implementation draws the same line for the one type rule it states as
//! a warning (`Bound features should have conforming types`), and a caller that treats every legal
//! model as broken stops reading.
//!
//! # What it does not decide
//!
//! The target is looked up where the direction rule looks for it, in what the owner *inherits*
//! (`Index::inherited_member_of`), so a redefinition reached through a conjugation is skipped
//! rather than judged — the same gap `redefinition_direction.rs` records. A feature that writes no
//! type, or writes more than one, is skipped too: the first is the type of the thing it replaces,
//! and the second is an intersection this layer does not compute. An unresolved type is a
//! reference problem, reported elsewhere.
//!
//! The report code this group reports under is `SEM096`.

use sysml_model::Element;
use sysml_resolve::Index;

use crate::typing;

use super::Violation;

const CODE: &str = "SEM096";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The single type a feature declares, resolved. `None` when it declares none or several.
fn one_type(index: &Index, id: usize) -> Option<usize> {
    let types = index.types_of(id);
    if types.len() != 1 {
        return None;
    }
    let head = types[0].trim();
    if head.is_empty() {
        return None;
    }
    index.resolve(id, head).found()
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // Only a feature can redefine another. A usage that writes no name and no `:>>` has
        // nothing to look up.
        if !el.kind.is_feature() {
            return;
        }
        // An association end's type is judged by `validateAssociationEndTypes` against what the
        // association relates, and an end that redefines an end in a specializing association
        // changes that relation rather than only narrowing a feature. The pilot's own model
        // (`AssociationTest_EndTypes_invalid.kerml.xt:38`) states one arity error for
        // `assoc A2 specializes A1 { end x : C2; }` and nothing about the type, so judging it here
        // would be a second opinion the reference does not give.
        if el.is_end {
            return;
        }
        if el.redefines.is_empty() && el.name.is_empty() {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // A declaration that repeats an inherited member's name is a redefinition too — the
        // metamodel derives it from the name collision, not from the keyword — so both spellings
        // are collected, an explicit `:>>` first.
        let mut clauses: Vec<String> = el.redefines.clone();
        if clauses.is_empty() && index.inherited_member_of(owner, &el.name).is_some() {
            clauses.push(el.name.clone());
        }
        let Some(actual) = one_type(index, el.id) else {
            return;
        };
        for text in &clauses {
            let head = text.split(',').next().unwrap_or(text).trim();
            let Some(target) = index.inherited_member_of(owner, head) else {
                continue;
            };
            let Some(expected) = one_type(index, target) else {
                continue;
            };
            if typing::conforms(index, actual, expected) {
                continue;
            }
            out.push(Violation::warning(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` is a `{}` but redefines `{head}`, a `{}`; a redefinition must conform to \
                     the type it replaces",
                    qualified(el, index),
                    index.qualified[actual],
                    index.qualified[expected]
                ),
            ));
        }
    });
    out
}
