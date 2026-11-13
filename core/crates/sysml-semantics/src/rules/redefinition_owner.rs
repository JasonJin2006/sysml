//! `redefinition-featuring` — a redefinition must add a featuring type.
//!
//! `validateRedefinitionFeaturingTypes` builds two sets — each feature's featuring types plus
//! `Base::Anything` for the implicit one — and states `<>`: the sets differ. The specification's
//! own sentence is the plain reading: "The redefiningFeature of a Redefinition must have at least
//! one featuringType that is not also a featuringType of the redefinedFeature."
//!
//! ```text
//! part def Base { part x; part y redefines x; }   // rejected: both live in Base
//! part def Base { part x; }
//! part def Sub :> Base { part y redefines x; }    // legal: Sub adds a featuring type
//! part wheel : Wheel;
//! part wheel1 redefines wheel;                    // rejected: neither has a featuring type
//! ```
//!
//! # What the OCL says, and what is done here
//!
//! `Base::Anything` is not in the library this tree loads, so the set comparison cannot be
//! reproduced literally. What it decides is whether the two features share an owner: two sets of
//! the form `{owner, Anything}` are equal exactly when the owners are, and `{Anything}` equals
//! itself, which is the package-level case.
//!
//! Two different owners always give different sets, so a redefinition that names a feature it does
//! not inherit is *not* reported here even when the owners are unrelated. That shape is a
//! subsetting question, and another rule judges it.
//!
//! An `alias a for B::x` is followed to the feature it names: its featuring type is that of the
//! aliased feature, not of the type that owns the alias declaration.
//!
//! A clause that resolves to the member it declares (`ref :>> x` inside `x`) is left alone: that
//! is a dangling reference, not a featuring-type question.
//!
//! The report code this group reports under is `SEM075`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM075";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is a definition — the only kind of owner a subtype can inherit from.
fn is_definition(kind: &sysml_model::Kind) -> bool {
    kind.is_definition()
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    // An `alias a for B::x` is a name for another feature. Its featuring type is that of the
    // feature it names, not the type that owns the alias declaration, so a redefinition naming an
    // alias has to be compared against the aliased feature's owner. The alias's target text is on
    // its element, which the walk below already visits.
    let mut aliases: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    super::walk(roots, |el| {
        if el.kind == sysml_model::Kind::Alias {
            if let Some(text) = &el.to {
                aliases.insert(el.id, text.clone());
            }
        }
    });
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.redefines.is_empty() {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // A featuring type is a *type*. An owner that is neither one nor a package — an anonymous
        // expression inside an action, a constraint body — contributes nothing to the comparison;
        // the OCL walks past it to the enclosing type. Skipping is the same walk done lazily: the
        // rule cannot fire on such an owner either way.
        let owner_is_package = index.kind[owner] == sysml_model::Kind::Package;
        for text in &el.redefines {
            let head = text.split(',').next().unwrap_or(text).trim();
            // `:>> x` names the `x` the owner *inherits*, and the order matters in both
            // directions. Resolution from the redefining feature answers the feature itself when
            // the redefinition repeats the name (`part cyl :>> cyl` resolves to the new `cyl`),
            // and resolution from the owner answers the owner's own member when both live in the
            // same type. What the clause means is the one inherited: asked first, with the plain
            // lookup kept for a qualified target (`:>> Other::z`) and for a package-level feature.
            let target = match index.inherited_member_of(owner, head) {
                Some(found) => found,
                None => match index.resolve(el.id, head) {
                    Resolution::Found(found) => found,
                    _ => continue,
                },
            };
            // A clause that names the member it declares — `ref :>> x` inside `x` — resolves to
            // itself. That is a dangling reference (`Couldn't resolve reference to Feature 'x'`),
            // not a featuring-type question, and comparing a feature with itself would report
            // every such line as a same-type redefinition.
            if target == el.id {
                continue;
            }
            // Follow an alias chain to the feature it names. A cap of eight keeps a malformed
            // cycle from spinning; the corpus never nests aliases deeper than one.
            let mut target = target;
            for _ in 0..8 {
                let Some(text) = aliases.get(&target) else {
                    break;
                };
                match index.resolve(target, text) {
                    Resolution::Found(next) if next != target => target = next,
                    _ => break,
                }
            }
            // Both ends at package level: neither feature has a featuring type, so the OCL's two
            // sets are `{Anything}` on both sides and the rule's `<>` fails. A package-level
            // feature has nothing to be redefined in — there is no type for the new one to belong
            // to (`Redefinition_OwningType_Invalid.sysml.xt:31`).
            let target_is_package = index
                .parent
                .get(target)
                .copied()
                .flatten()
                .is_some_and(|id| index.kind[id] == sysml_model::Kind::Package);
            if owner_is_package && target_is_package {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` redefines `{head}`, and both are package-level features; a feature \
                         owned by a package has no featuring type, so it cannot be redefined \
                         (validateRedefinitionFeaturingTypes)",
                        qualified(el, index)
                    ),
                ));
                continue;
            }
            if owner_is_package != target_is_package {
                continue;
            }
            let Some(target_owner) = index.parent.get(target).copied().flatten() else {
                continue;
            };
            // A featuring type must be something a subtype can inherit from — a definition. A
            // usage cannot be specialised, so a redefinition inside one is not a featuring-type
            // question. The official 3c model writes `ref :>> trailerHitch` inside an anonymous
            // `connect ... to ...`; that owner lowers to an action with no authored name, and
            // comparing it against a part definition reports a violation the OCL never sees
            // because the walk never reaches such an owner in the first place.
            if !is_definition(&index.kind[owner]) || !is_definition(&index.kind[target_owner]) {
                continue;
            }
            let owner_name = index
                .qualified
                .get(owner)
                .cloned()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| index.name[owner].clone());
            // The same featuring type on both sides is the other way the OCL's `<>` fails: every
            // featuring type of the redefinition is also one of the redefined feature, so no
            // type is added. `part def Base { part x; part y redefines x; }` is that shape — the
            // new feature belongs to the type that already holds the old one.
            if owner == target_owner {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` redefines `{head}`, which belongs to the same type `{owner_name}`; a \
                         redefinition must add a featuring type the redefined feature does not \
                         have (validateRedefinitionFeaturingTypes)",
                        qualified(el, index)
                    ),
                ));
                continue;
            }
            // Two different owners is the ordinary case and the rule says nothing about them.
            // The OCL compares the two featuring-type *sets*: `{Sub, Anything}` against
            // `{Base, Anything}` differs, so the invariant holds whether or not one owner
            // inherits from the other. A redefinition naming a feature it does not inherit is a
            // different complaint, judged by the rules that read subsetting.
            let _ = index.is_subtype(owner, target_owner);
        }
    });
    out
}
