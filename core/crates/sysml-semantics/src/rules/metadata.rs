//! `metadata` — what a metadata usage may name.
//!
//! `validateMetadataFeatureMetaclass`: `type->selectByKind(Metaclass).size() = 1`. A metadata
//! usage attaches information to an element, and the thing it names has to be a metadata
//! definition — the metaclass that says what the annotation means. A usage naming an ordinary
//! part has no schema for its contents.
//!
//! `@Safety` constructs `Kind::Metadata` with `type_ref = "Safety"` (`lower.rs`), and
//! `metadata def Safety` constructs `Kind::MetadataDef`, so both halves are on the tree.
//!
//! # What is not decided
//!
//! `validateMetadataFeatureMetaclassNotAbstract` (`not metaclass.isAbstract`) is decidable in
//! form — `is_abstract` is on `Modifiers` — but it is not here because it reads the *metaclass's*
//! abstractness, and a metadata definition authored `abstract metadata def` is a legal base for
//! others to specialize. Deciding it against the definition a usage happens to name would report
//! every usage of an abstract base, which is what the notation is for.
//!
//! `validateMetadataFeatureBody` and `validateMetadataFeatureAnnotatedElement` both navigate
//! `ownedFeature->closure(...)` and a global `Metaobjects::Metaobject::annotatedElement`, neither
//! of which exists in this tree.
//!
//! The report code this rule reports under is `SEM045`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

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

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::MetadataUsage {
            return;
        }
        let Some(text) = &el.type_ref else {
            // `@` with no name is a parse accident, reported elsewhere.
            return;
        };
        let head = text.split(',').next().unwrap_or(text).trim();
        // `@Safety` lowers to an element *named* `Safety`, so a walk starting at the element finds
        // itself before the definition it means. Resolving from the owner is what a reader does:
        // `@Safety` in `P`'s body names `P`'s scope, where the metadata definition is visible.
        let owner = index.parent.get(el.id).copied().flatten();
        // The metadata element is a *member* of its owner named after the definition it names, so
        // a walk from the owner finds the metadata itself. Start one scope further out and skip
        // this element: that is the scope the authored name was written against.
        let from = owner
            .and_then(|o| index.parent.get(o).copied().flatten())
            .or(owner)
            .unwrap_or(el.id);
        let Resolution::Found(target) = index.resolve(from, head) else {
            // An unresolved metadata name is the reference pass's report.
            return;
        };
        if target == el.id {
            return;
        }
        // `oclIsKindOf(Metaclass)`: a metadata definition, or something specializing one.
        if is_metaclass(index, target) {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` names `{}`, a {}; a metadata usage must name a metadata definition \
                 (validateMetadataFeatureMetaclass)",
                qualified(el, index),
                index.qualified[target],
                index.kind[target].keyword()
            ),
        ));
    });
    out
}

/// Whether the target is a metadata definition or specializes one.
fn is_metaclass(index: &Index, id: usize) -> bool {
    let mut frontier = vec![id];
    let mut seen = std::collections::HashSet::from([id]);
    while let Some(current) = frontier.pop() {
        if index.kind[current] == Kind::MetadataDef {
            return true;
        }
        for next in index.debug_supertypes(current) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    false
}
