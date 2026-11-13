//! `association-end` — an end of an association names exactly one type.
//!
//! `validateAssociationEndTypes`: `ownedEndFeature->forAll(type->size() = 1)`. An end states what
//! kind of thing sits at that side of the link, and an end that names two types says it sits at two
//! sides at once. An end that names none is legal: its type is implied (see below).
//!
//! # Which direction is reported, and why only one
//!
//! The rule reads `type->size() = 1`, so both zero and more-than-one are violations on its face.
//! Only the second is reported here.
//!
//! **More than one** is readable from `Element::type_refs`, which keeps every typing target
//! (`lower.rs` fills it from `Head::targets_of`). This module used to say the direction was out of
//! reach because the lowering kept only the first target — true before `type_refs` existed, and
//! the comment stayed after it did. `end z : A, B` is reported, and the pilot's
//! `AssociationTest_EndTypes_invalid.kerml.xt:33` asserts exactly that.
//!
//! **Zero** is not: the specification gives every end an implied subsetting of
//! `Links::Link::participant`, and the reference implementation materialises it, so `end x;` is
//! legal (`AssociationTest_EndRedefinitionGoodCase.kerml.xt` is that spelling under `noErrors`).
//!
//! The report code this rule reports under is `SEM041`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM041";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether the element is an owned end feature of an association.
///
/// `is_end` rather than the end's own kind is the test, because the owner decides what a bare
/// `end y;` becomes: only an interface end defaults to `Kind::PortUsage`, while a general
/// connector or association end is a KerML `Feature` (`lower.rs:536-544`, which is what
/// `sysml-syntax`'s `tests/loading.rs::bare_end_kinds_follow_the_owner_without_rewriting_explicit_kinds`
/// pins). Reading the end's kind instead would therefore judge one owner and skip the others.
///
/// `Kind::AssocStructureDef` is the single `AssociationStructure` identity — an `assoc struct` is
/// both an `Association` and a `Structure` — so its ends are `ownedEndFeature` exactly as an
/// `assoc`'s are. Leaving it out made the rule silent for the one SysML spelling of an association
/// that the standard library itself uses (`Objects::LinkObject`).
fn is_association_end(el: &Element, index: &Index) -> bool {
    if !el.is_end {
        return false;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        return false;
    };
    // Only an *Association* owns the ends this constraint is about (`metaclass: Association`).
    // A connection usage is a `Connector`, a feature: `connection de : DE, BC connect b to c;`
    // gives `de` two ends that each redefine an end of `DE` and one of `BC`, and two types per end
    // is what typing a connector by two associations means. The pilot leaves that line clean
    // (`ConnectionUsage_Invalid.sysml.xt:55`); counting `Kind::Connection` here reported it twice.
    // A `connection def` is an `AssociationStructure`, so its ends stay in scope.
    // KerML writes `assoc A1 { … }` with no `def`, so the kind alone decides; `is_definition` is
    // false there.
    matches!(
        index.kind[owner],
        Kind::ConnectionDef | Kind::AssocDef | Kind::AssocStructureDef
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_association_end(el, index) {
            return;
        }
        // `ownedEndFeature->forAll(type->size() = 1)`. `type` is not the written clause: it is
        // `deriveFeatureType` — "the union of the types of its typings and the types of the
        // Features it subsets, with all redundant supertypes removed"
        // (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs`, `deriveFeatureType`).
        //
        // That difference decides two ends of `AssociationTest_EndTypes_invalid.kerml.xt`. A
        // redeclaration inside a subtype redefines the inherited feature of the same name, so
        // `assoc A2 specializes A1 { end x : C2; }` has the types `{C2, C1}` — two, and the pilot
        // reports it — while `assoc A3 specializes A1 { end x : C3; }` with `C3 specializes C1` has
        // `{C3, C1}`, whose `C1` is redundant, so it is one type and the pilot reports nothing.
        // Counting the written clause alone reported neither.
        //
        // The derivation itself lives in [`crate::derived::types`], because two other enforced
        // rules ask the same question (`validateFeatureCrossFeatureType`,
        // `validateMetadataFeatureMetaclass`). It used to be written out here, which meant one rule
        // followed subsetting and the other two read the written clause — the three disagreed about
        // what "the type" of a feature is.
        let maximal = crate::derived::types(index, el.id);
        if maximal.len() > 1 {
            let named: Vec<String> = maximal
                .iter()
                .map(|&id| index.qualified[id].clone())
                .collect();
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` is an end of an association and names {} types ({}); an end states \
                     exactly one (validateAssociationEndTypes)",
                    qualified(el, index),
                    maximal.len(),
                    named.join(", ")
                ),
            ));
        }
        // An end that states none leaves it implied: the specification gives each end of an
        // association or connector an implied subsetting of `Links::Link::participant`
        // (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/implied.rs:144-186`), and the
        // reference implementation materialises it so this rule does not fire on `end x;`.
        // `AssociationTest_EndRedefinitionGoodCase.kerml.xt` is that spelling under `noErrors`.
    });
    out
}
