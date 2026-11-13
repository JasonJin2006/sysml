//! `cross-feature-type` — a feature and the feature it crosses have the same type.
//!
//! `validateFeatureCrossFeatureType`: `crossFeature <> null implies
//! crossFeature.type->asSet() = type->asSet()`. `part p : A crosses X` says `p` reaches across `X`
//! and is itself of type `A`; a crossing whose two sides disagree about what is being connected
//! describes a link between two different kinds of thing, which is what a connection with two ends
//! already does.
//!
//! ```text
//! part def Vehicle { end part hitch : Hitch crosses Hitch; }     // legal
//! part def Vehicle { end part hitch : Hitch crosses Other; }     // rejected
//! ```
//!
//! # How it reads
//!
//! Both sides are `Feature::type`, the *derived* property (`deriveFeatureType`), through
//! [`crate::derived::types`]. Reading the written `: T` clause instead is not the same question, and
//! the difference was measured:
//!
//! ```text
//! part def A;
//! part def B :> A;
//! part def Holder {
//!     end part x : A, B;              // type(x) = {B}: `A` is a redundant supertype
//!     end part y : B crosses x;       // type(y) = {B}
//! }
//! ```
//!
//! The two sides are equal, so the rule states nothing; the earlier projection took `x`'s *first*
//! written type (`A`), compared it with `B`, and reported `SEM073` — a false positive on a legal
//! model. The mirrored case (`y : A, B crosses x` with `x : A`, where the derived sets are `{B}` and
//! `{A}`) is a real violation that the old "any written type matches" test missed.
//!
//! # A plain name is either a feature or a type
//!
//! `crosses x` names the cross feature itself; `crosses Hitch` names a type. The OCL distinguishes
//! them by what `crossFeature` resolves to, and so does this: a name that resolves to a feature is
//! compared as a feature, and only a name that resolves to something else is read as a type. The
//! earlier code treated every undotted name as a type and compared it against the written type
//! text, so `end y : B crosses x` was compared with the *string* `x` and reported on every model
//! that used the feature form.
//!
//! A feature with no type, or a crossing with no target, says nothing to compare. That guard is
//! deliberate and narrower than the OCL: `type` is legitimately empty for an untyped feature, and
//! the corpus's `crosses` models never assert a type error for one.
//!
//! The report code this group reports under is `SEM073`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM073";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The qualified names of a set of type ids, for the message.
fn names(index: &Index, ids: &[usize]) -> String {
    if ids.is_empty() {
        return "no type".to_string();
    }
    ids.iter()
        .map(|&id| index.qualified[id].clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Set equality, in either order: `crossFeature.type->asSet() = type->asSet()`.
fn same_set(one: &[usize], other: &[usize]) -> bool {
    one.iter().all(|x| other.contains(x)) && other.iter().all(|x| one.contains(x))
}

/// The feature an undotted `crosses <name>` clause names, when the name denotes a feature.
///
/// `crosses Hitch` where `Hitch` is a `part def` denotes a type and answers `None`; the caller then
/// compares against that type. A name that does not resolve answers `None` too, and the caller
/// leaves it to the reference pass — an unresolved name is not a type mismatch.
fn named_feature(index: &Index, id: usize) -> Option<usize> {
    let scope = index.clause_scope_of(id);
    for text in index.crosses_of(id) {
        let head = text.split(',').next().unwrap_or(text).trim();
        if head.is_empty() || head.contains('.') {
            // A dotted path is `cross_feature`'s question, not this one.
            continue;
        }
        if let Resolution::Found(target) = index.resolve(scope, head) {
            if target != id && index.kind[target].is_feature() {
                return Some(target);
            }
        }
    }
    None
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.crosses.is_empty() || el.type_refs.is_empty() {
            return;
        }
        let own = crate::derived::types(index, el.id);
        // `crosses y.b` names a chain and `crosses x` names a feature; both go through
        // `crossFeature`. Only a name that denotes a type is compared as a type.
        let theirs = match crate::derived::cross_feature(index, el.id)
            .or_else(|| named_feature(index, el.id))
        {
            Some(feature) => {
                let theirs = crate::derived::types(index, feature);
                // A crossed feature with no type states nothing to compare, for the same reason the
                // guard above does: `type` is legitimately empty when nothing types the feature.
                // The OCL's `={}` reading would report `end y : C2 crosses x.y1` where `y1` is
                // `feature y1 [0..1] featured by C1` — `AssociationTest_CrossFeatures_invalid`
                // line 56 — and the reference reports the cross-subsetting rule there instead,
                // asserting nothing about types. Reporting it was measured as this file's only
                // spurious diagnostic.
                if theirs.is_empty() {
                    return;
                }
                theirs
            }
            None => {
                let scope = index.clause_scope_of(el.id);
                let mut targets = Vec::new();
                for text in &el.crosses {
                    let head = text.split(',').next().unwrap_or(text).trim();
                    if let Resolution::Found(target) = index.resolve(scope, head) {
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                }
                // Nothing resolved: the reference pass reports the name.
                if targets.is_empty() {
                    return;
                }
                targets
            }
        };
        if same_set(&own, &theirs) {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is typed `{}` but the feature it crosses is typed `{}`; a feature and the \
                 feature it crosses share a type (validateFeatureCrossFeatureType)",
                qualified(el, index),
                names(index, &own),
                names(index, &theirs)
            ),
        ));
    });
    out
}
