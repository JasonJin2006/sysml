//! `view-expose-import` — where an `expose` may be written, and how an import that sits at the
//! root of the model must be marked.
//!
//! Seven rules were assigned to this group. Two are decided here:
//!
//!   * `validateExposeOwningNamespace` —
//!     `importOwningNamespace.oclIsType(ViewUsage)`, "The importOwningNamespace of an Expose must
//!     be a ViewUsage."
//!   * `validateImportTopLevelVisibility` —
//!     `importOwningNamespace.owner = null implies visibility = VisibilityKind::private`, "A
//!     top-level Import (that is, one that is owned by a root Namespace) must have a visibility
//!     of private."
//!
//! # What the projection decides, and what it does not
//!
//! `expose Q;` lowers to [`Kind::Expose`] under the element that owned the statement
//! (`sysml-syntax/src/lower.rs:611-616`), so its owning namespace is `index.parent`. A view
//! definition and a view usage are separate kinds (`lower.rs` splits `K::View` on `is_definition`)
//! and both are accepted here, because the OCL's `oclIsType(ViewUsage)` and `oclIsType
//! (ViewDefinition)` two rules both only ask that *a* view owns the expose. The reference grammar
//! does not let the two apart in the other direction either: `Expose` appears only in
//! `ViewBodyItem` (`research/upstream/SysML.xtext:2355-2364`), never in `ViewDefinitionBodyItem`
//! (`:2329-2333`), so an expose under a view definition is not a legal model to begin with.
//!
//! A viewpoint is not a view: `ViewpointUsage specializes RequirementUsage`
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:1779`), not `ViewUsage`, so an
//! expose owned by a viewpoint is reported like one owned by a package.
//!
//! For the import rule, "top-level" is read the way the OCL reads it: the namespace that owns the
//! import has no owner of its own, which in this tree is an import whose `index.parent` is `None`.
//! An import inside a top-level package is therefore not top-level — that package has an owner
//! (the model's root namespace), so `importOwningNamespace.owner` is not null. That is also the
//! reading under which the standard library conforms; it writes `public import ISQBase::*` inside
//! a top-level package (`research/.venv/Lib/site-packages/sysmlpy/library/domain/Quantities and
//! Units/ISQ.sysml:11-22`).
//!
//! An absent prefix is the language default, which for an import is `private`
//! (`sysml-model/src/lib.rs:331-333` and `:790-792`), so only an authored `public` or `protected`
//! is reported.
//!
//! # Rules this group cannot decide
//!
//! Five of the seven reach for something the element tree does not carry. They are not checked
//! here rather than checked vacuously, and `conformance::implemented()` must not list them.
//!
//!   * `validateExposeVisibility` (`visibility = VisibilityKind::protected`) — an expose's
//!     authored visibility is dropped twice over. `parse_member` reads the prefix
//!     (`sysml-parser/src/parser.rs:563`) but hands it only to `import()`
//!     (`parser.rs:625-626`); the `expose` branch at `parser.rs:631-632` receives nothing, and
//!     `lower_relationship` builds the `Expose` without setting `Element::visibility`
//!     (`lower.rs:611-616`). `Element::visibility` is populated only by `with_import`
//!     (`sysml-model/src/lib.rs:918-922`), whose one caller is the import branch
//!     (`lower.rs:601`). So no element in the tree records an expose's visibility at all. The
//!     reference notation has no other value to write either: `ExposeVisibilityKind` is
//!     `protected = 'expose'` (`research/upstream/SysML.xtext:2366-2372`), so the only value the
//!     grammar can produce is the one the rule demands.
//!   * `validateExposeIsImportAll` (`isImportAll`) — no such field exists; the struct at
//!     `sysml-model/src/lib.rs:721-850` has none. `import_shape` is set only on `Kind::Import`
//!     (`lower.rs:601`) and the expose branch sets nothing (`lower.rs:611-616`). The metamodel
//!     defaults `Expose::isImportAll` to true (`generated.rs:2219`), unlike `Import::isImportAll`,
//!     which defaults to false (`generated.rs:2325`), and the grammar writes `all` only on
//!     `ImportPrefix` (`research/upstream/KerML.xtext:168-171`), never on an expose. A model this
//!     layer reads therefore always has `isImportAll = true` on an expose.
//!   * `validateViewUsageOnlyOneViewRendering` and `validateViewDefinitionOnlyOneViewRendering`
//!     (`featureMembership->selectByKind(ViewRenderingMembership)->size() <= 1`) — the metaclass
//!     `ViewRenderingMembership` is not on the tree, and nothing records which membership owns a
//!     rendering. Three constructs lower to the single kind [`Kind::RenderingDef`]:
//!     `render r : R` (a `ViewRenderingMember`, `research/upstream/SysML.xtext:2335-2343`),
//!     `rendering r : R` (an ordinary usage member) and `rendering def R` (a `RenderingDefinition`,
//!     which `DefinitionElement` admits, `:301-331`). `element_kind` maps `K::Rendering` to one
//!     kind without consulting `is_definition` (`lower.rs:237`), and the parser folds the
//!     spellings `"rendering" | "render"` into one `K::Rendering` (`parser.rs:1004`). Counting
//!     `Kind::RenderingDef` children would therefore report a legal view body that declares a
//!     rendering definition or writes a plain rendering usage as if it had two renderings.
//!   * `validateEnumerationDefinitionIsVariation` (`isVariation`) — no such field exists.
//!     `Prefixes::is_variation` is parsed (`parser.rs:271-274`, `sysml-parser/src/ast.rs:158`) and
//!     never read by the lowering layer, which takes only `is_end` and `direction` from the
//!     prefixes (`lower.rs:281`, `:298`, `:338`, `:391`). The metamodel defaults `isVariation` to
//!     true for an `EnumerationDefinition` (`generated.rs:2205`), and `EnumerationDefinition`
//!     takes no `DefinitionPrefix` (`research/upstream/SysML.xtext:767-770`), so the reference
//!     notation cannot write the flag on one at all. Our parser refuses `variation enum def M` as a
//!     syntax error, so no tree this rule reads carries the flag.

use sysml_model::{Element, Kind, Visibility};
use sysml_resolve::Index;

use super::Violation;

/// The code every rule in this group reports under.
const CODE: &str = "SEM025";

/// Whether a kind is a view.
///
/// `view def V` and `view v : V` are separate kinds now, and both may own an expose, so both pass
/// this test. `ViewpointUsage` and `ViewpointDefinition` are not views and do not.
fn is_view(kind: &Kind) -> bool {
    matches!(kind, Kind::ViewDef | Kind::ViewUsage)
}

/// The qualified name the index assigned to an element id.
fn qualified_at(index: &Index, id: usize) -> String {
    index.qualified.get(id).cloned().unwrap_or_default()
}

/// The qualified name of an element itself, falling back to its own name.
fn qualified(index: &Index, el: &Element) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .unwrap_or_else(|| el.name.clone())
}

/// `validateExposeOwningNamespace` — an expose may only be owned by a view usage.
fn check_expose(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    let target = el.to.as_deref().unwrap_or(el.name.as_str());
    match index.parent.get(el.id).copied().flatten() {
        // A view, or a view definition, which the tree cannot tell apart.
        Some(owner) if is_view(&index.kind[owner]) => {}
        Some(owner) => {
            let message = format!(
                "`expose {target}` is owned by `{}`, a {}; an expose may only be owned by a ViewUsage",
                qualified_at(index, owner),
                index.kind[owner].keyword()
            );
            out.push(Violation::new(CODE, el, qualified(index, el), message));
        }
        None => {
            let message = format!(
                "`expose {target}` is owned by no namespace; an expose may only be owned by a ViewUsage"
            );
            out.push(Violation::new(CODE, el, qualified(index, el), message));
        }
    }
}

/// `validateImportTopLevelVisibility` — an import at the root of the model must be private.
fn check_import(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    // Top-level means the owning namespace has no owner: an import written outside every package.
    if index.parent.get(el.id).copied().flatten().is_some() {
        return;
    }
    let Some(visibility) = el.visibility else {
        // Absent means the language default, which for an import is private.
        return;
    };
    if visibility == Visibility::Private {
        return;
    }
    let message = format!(
        "the top-level import of `{}` is `{}`; a top-level import must be private",
        el.name,
        visibility.keyword()
    );
    out.push(Violation::new(CODE, el, qualified(index, el), message));
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| match el.kind {
        Kind::Expose => check_expose(el, index, &mut out),
        Kind::Import => check_import(el, index, &mut out),
        _ => {}
    });
    out
}
