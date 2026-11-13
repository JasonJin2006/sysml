//! `metadata-annotated-element` — an annotation may only be applied to what it describes.
//!
//! `validateMetadataFeatureAnnotatedElement` requires the annotated element's metaclass to be one
//! the metadata declares. `metadata def A { ref :>> annotatedElement : SysML::AttributeUsage; }`
//! says "A describes attribute usages", so `@A` on an attribute is legal and `@A` on a classifier
//! is not.
//!
//! The rule walks the metadata definition's own `annotatedElement` feature: whatever type that
//! feature states is the set of elements the annotation may describe. When the definition writes
//! no such feature — `metadata def Safety { attribute level : Integer; }` — nothing is constrained,
//! and every element may carry it.
//!
//! # Why this needed a lowering fix first
//!
//! The KerML `metaclass` keyword used to lower to `Kind::KermlType`, so a `metaclass` was not a
//! metadata definition and an annotation naming one was reported as naming the wrong kind of
//! thing. Both spellings — SysML's `metadata def` and KerML's `metaclass` — are now the same kind,
//! which is what makes the pilot's "Cannot annotate Classifier" reachable.
//!
//! The report code is `SEM094`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM094";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The metaclass names a metadata definition says it describes.
///
/// An empty list means the definition constrains nothing: `annotatedElement` is optional, and a
/// metadata definition without one may annotate anything.
fn described_metaclasses(index: &Index, definition: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Some(children) = index.children.get(definition) else {
        return out;
    };
    for &child in children {
        // `ref :>> annotatedElement : T` names the redefinition, not the feature it replaces,
        // so the member is called `ref` and the name to look for is in `redefines`.
        let is_annotated_element = index.name[child] == "annotatedElement"
            || index
                .redefines_of(child)
                .iter()
                .any(|r| r.trim() == "annotatedElement");
        if !is_annotated_element {
            continue;
        }
        if let Some(text) = index.type_of(child) {
            out.push(text.to_string());
        }
    }
    out
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |annotated| {
        for child in &annotated.children {
            if child.kind != Kind::MetadataUsage {
                continue;
            }
            let Some(text) = &child.type_ref else {
                continue;
            };
            let head = text.split(',').next().unwrap_or(text).trim();
            // Same walk `metadata.rs` uses: the annotation element is a member named after the
            // definition it names, so the lookup starts one scope further out.
            let owner = index.parent.get(child.id).copied().flatten();
            let from = owner
                .and_then(|o| index.parent.get(o).copied().flatten())
                .or(owner)
                .unwrap_or(child.id);
            let Resolution::Found(definition) = index.resolve(from, head) else {
                continue;
            };
            if definition == child.id || index.kind[definition] != Kind::MetadataDef {
                continue;
            }
            let described = described_metaclasses(index, definition);
            if described.is_empty() {
                continue;
            }
            // The annotation is legal when the annotated element is one of the kinds the
            // definition describes. `is_kind_of` answers through the metaclass table, so
            // `attribute a` matches `AttributeUsage` and `classifier B` does not.
            // The declaration writes a qualified library name (`SysML::AttributeUsage`); the
            // metaclass table is keyed by the simple name.
            let matches = described.iter().any(|name| {
                let simple = name.rsplit("::").next().unwrap_or(name).trim();
                index.is_kind_of(annotated.id, simple)
            });
            if matches {
                continue;
            }
            out.push(Violation::new(
                CODE,
                child,
                qualified(child, index),
                format!(
                    "`{}` describes {} and cannot annotate `{}`, a {}",
                    index
                        .qualified
                        .get(definition)
                        .cloned()
                        .unwrap_or_else(|| head.to_string()),
                    described.join(", "),
                    qualified(annotated, index),
                    annotated.kind.keyword()
                ),
            ));
        }
    });
    out
}
