//! `view-rendering` — a view states at most one rendering.
//!
//! `validateViewUsageOnlyOneViewRendering` and `validateViewDefinitionOnlyOneViewRendering`: a
//! view's `ownedRelationship->selectByKind(ViewRenderingMembership)->size() <= 1`. A rendering
//! says how the view's content is presented; two says the view has two presentations, which the
//! specification separates into two views.
//!
//! Both halves are on the tree: a view constructs `Kind::ViewDef` or `Kind::ViewUsage`
//! (`lower.rs` splits on the `def` keyword), and `render r;` constructs `Kind::RenderingDef`.
//! Neither is guarded by a
//! membership element — the keyword *is* the membership — so counting children of that kind is
//! the count the rule asks for.
//!
//! The report code this group reports under is `SEM043`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM043";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is a view.
///
/// `owningType.oclIsKindOf(ViewDefinition) or owningType.oclIsKindOf(ViewUsage)`: the two rules
/// are stated with the same bound over the same relationship, so a view definition and a view
/// usage are both asked. They are distinct kinds now (`view def X` and `view v : V`), which is why
/// both questions are asked instead of one standing in for the other.
fn is_view(kind: &Kind) -> bool {
    kind.is_metaclass("ViewDefinition") || kind.is_metaclass("ViewUsage")
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_view(&el.kind) {
            return;
        }
        // Only a `render` member is a `ViewRenderingMembership`. `rendering r : R` is an ordinary
        // usage member, and `rendering def R` a definition: neither is one of the memberships
        // `featureMembership->selectByKind(ViewRenderingMembership)->size() <= 1` counts.
        let renderings: Vec<&Element> =
            el.children.iter().filter(|child| child.is_render).collect();
        if renderings.len() <= 1 {
            return;
        }
        // Reported on every rendering past the first, not on the view: the view is what is
        // legal in shape, and the second `render` is the declaration to remove. The pilot states
        // it that way — `render rendering r2;` carries the assertion.
        for extra in renderings.iter().skip(1) {
            out.push(Violation::new(
                CODE,
                extra,
                qualified(extra, index),
                format!(
                    "`{}` states another rendering; a view states at most one \
                     (validateViewUsageOnlyOneViewRendering / \
                     validateViewDefinitionOnlyOneViewRendering)",
                    qualified(el, index)
                ),
            ));
        }
    });
    out.extend(owning_type(roots, index));
    out
}

/// `validateViewRenderingMembershipOwningType` — a rendering belongs to a view.
///
/// The same tree shape answers it: a `render` member's owner is its parent, and only a view may be
/// one. A `render` inside a `part def` presents nothing. A plain `rendering r` is an ordinary
/// usage member, so owning one is not a `ViewRenderingMembership` and the constraint has nothing
/// to say about it — the pilot's `rendering r;` at package scope carries no assertion.
fn owning_type(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !el.is_render {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        if is_view(&index.kind[owner]) {
            return;
        }
        let owner_kind = index.kind[owner].keyword();
        let owner_name = index
            .qualified
            .get(owner)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| index.name[owner].clone());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a rendering owned by `{owner_name}`, a {owner_kind}; a rendering belongs \
                 to a view (validateViewRenderingMembershipOwningType)",
                qualified(el, index)
            ),
        ));
    });
    out
}
