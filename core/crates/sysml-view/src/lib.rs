//! View projection and SVG rendering.
//!
//! The projections are checked against `sysmlpy` 0.97.1 rather than against memory: see
//! `graph.rs` for the clause-by-line citations, and `core/tools/parity.py` for the check that
//! compares this crate's node and edge list with the reference implementation's PlantUML text.
//!
//! Two views are produced. The general view is a flat node-and-edge graph — packages flattened,
//! every element one node, ownership `*--`, typing `--:|>` labelled `types`. The interconnection
//! view is a nested box diagram — only the part usages a connection reaches, attributes in a
//! compartment as `+ name : Type`, ports on the boundary, connectors as thick arrowless lines.

pub mod graph;
pub mod jumps;
pub mod layered;
pub mod packages;
pub mod sequence;
pub mod states;
pub mod style;
mod svg;
pub mod table;
pub mod trace;

use sysml_model::{Element, Kind};

pub use graph::{Arrow, Graph, Mode, View};

pub use svg::{
    general_geometry, interconnection_geometry, node_size, sequence_geometry, states_geometry,
    traceability_geometry, GeneralGeometry, InterconnectionGeometry, SequenceGeometry,
    StateGeometry, TraceGeometry,
};
/// Geometry as data, for callers that draw the projection themselves.
///
/// The SVG renderer and an editor both call these, so a model has one picture rather than two
/// that can disagree. See `svg::general_geometry` for why the split is where it is.
pub use trace::derivations;

/// Project the element tree onto one of the views.
pub fn project(roots: &[Element], view: View, mode: Mode) -> Graph {
    match view {
        View::General => graph::general(roots, mode),
        View::Interconnection => graph::interconnection(roots, mode),
        // The traceability view has no compat form: it is an addition, and there is no reference
        // implementation to mirror. The mode argument is ignored rather than pretending.
        View::Traceability => trace::traceability(roots).graph,
        View::Packages => packages::packages(roots).graph,
        View::StateTransition => states::states(roots).graph,
        View::ActionFlow => states::actions(roots).graph,
        View::Sequence => sequence::sequence(roots).graph,
    }
}

/// Project a model with everything the filter did not keep removed.
///
/// The filter is a set of element ids, computed by the caller: which elements carry an annotation is
/// a semantic question, and this crate has no business deciding it. An element stays when it is kept
/// itself or when something inside it is — a filtered diagram that dropped a container would cut
/// everything under it out of the picture as well.
pub fn project_filtered(
    roots: &[Element],
    view: View,
    mode: Mode,
    keep: &std::collections::HashSet<usize>,
) -> Graph {
    project(&keep_only(roots, keep), view, mode)
}

/// The same pruning, for a caller that wants the tree rather than a projection (`outline`).
///
/// Ids are reassigned in pre-order afterwards. The resolver and every view index elements *by id*,
/// and `Index::build` requires them dense and in order — a pruned tree carrying the original ids is
/// sparse, and the first lookup into it reads past the end.
pub fn keep_only(roots: &[Element], keep: &std::collections::HashSet<usize>) -> Vec<Element> {
    fn walk(el: &Element, keep: &std::collections::HashSet<usize>) -> Option<Element> {
        let children: Vec<Element> = el.children.iter().filter_map(|c| walk(c, keep)).collect();
        if !keep.contains(&el.id) && children.is_empty() {
            return None;
        }
        let mut copy = el.clone();
        copy.children = children;
        Some(copy)
    }
    let mut out: Vec<Element> = roots.iter().filter_map(|r| walk(r, keep)).collect();
    renumber_ids(&mut out);
    out
}

/// Give a pruned tree dense ids, in pre-order.
///
/// The resolver and every view index elements *by id*, and `Index::build` requires them dense and in
/// order. A tree that dropped elements still carries the ids they had, so the first lookup into it
/// reads past the end — which is how a filtered view panicked before this existed. Every pruning
/// path has to end here.
pub fn renumber_ids(roots: &mut [Element]) {
    fn go(el: &mut Element, next: &mut usize) {
        el.id = *next;
        *next += 1;
        for child in &mut el.children {
            go(child, next);
        }
    }
    let mut next = 0;
    for root in roots {
        go(root, &mut next);
    }
}

/// Canonical text form of a projection, used by the parity check and by tests that should not
/// depend on pixel geometry.
pub fn to_lines(g: &Graph) -> Vec<String> {
    graph::to_lines(g)
}

pub fn render(g: &Graph, title: &str) -> String {
    render_styled(g, title, layered::Style::Nested)
}

/// Render with an explicit layout style. `Nested` is what `render` has always done; `Tree` puts
/// every node on its own indented row, which is the other style the notation defines.
pub fn render_styled(g: &Graph, title: &str, style: layered::Style) -> String {
    svg::render(g, title, style)
}

pub fn render_view(roots: &[Element], view: View, mode: Mode, title: &str) -> String {
    render(&project(roots, view, mode), title)
}

///  with an explicit layout style.
pub fn render_view_styled(
    roots: &[Element],
    view: View,
    mode: Mode,
    title: &str,
    style: layered::Style,
) -> String {
    render_styled(&project(roots, view, mode), title, style)
}

/// Plain-text outline, used by the CLI and by tests that should not depend on SVG layout.
pub fn outline(roots: &[Element]) -> String {
    let mut out = String::new();
    fn rec(el: &Element, depth: usize, out: &mut String) {
        out.push_str(&"  ".repeat(depth));
        // An `@Safety { … }` annotation is anonymous and typed by Safety; printing the placeholder
        // would make the tree read `metadata <anonymous> : Safety` where the model says
        // `@Safety`. The type is the name a reader is looking for.
        let printed = match el.kind {
            Kind::MetadataUsage if el.name == "<anonymous>" => {
                el.type_ref.as_deref().unwrap_or(&el.name)
            }
            // `event a.areal;` and `perform b.a;` declare nothing: they denote a feature that
            // already exists, so the feature they denote is what a reader is looking for.
            // Printing the placeholder read `event occurrence <anonymous>` for every line of
            // `17b-Sequence-Modeling.sysml` and `perform <anonymous>` for every performance.
            Kind::EventOccurrenceUsage | Kind::Perform | Kind::ExhibitState
                if el.name == "<anonymous>" =>
            {
                el.performs.as_deref().unwrap_or(&el.name)
            }
            // `expose Power::*;` has no name of its own: the target is the whole statement.
            Kind::Expose => el.to.as_deref().unwrap_or("…"),
            _ => el.name.as_str(),
        };
        // A variation point and its alternatives are written with a prefix word, and the prefix
        // is what tells a reader this declaration is one of them: `variation part config` prints
        // as `variation part config`, not `part config`. The word is a flag on the element, not
        // part of its kind, so it is printed here rather than by `keyword_as`.
        if el.modifiers.is_variation {
            out.push_str("variation ");
        }
        if el.is_variant && el.kind != Kind::Variant {
            out.push_str("variant ");
        }
        out.push_str(el.kind.keyword_as(el.is_definition));
        // A relationship names its ends rather than itself (`subtype C specializes B` prints as
        // `specialization [C -> B]`), so an empty name must not leave a gap before the rest.
        if !printed.is_empty() {
            out.push(' ');
            out.push_str(printed);
        }
        if let Some(shape) = el.import_shape {
            // An import prints its target plus the suffix that says which form was written:
            // `import ISQ::*`, `import ScalarValues::Real`, `import Q::*::**`.
            out.push_str(shape.suffix());
        } else if let Some(t) = &el.type_ref {
            // `port p : ~FuelPort` — the `~` is a conjugation of the type, and the outline is
            // the one place a reader compares the declaration to the source. Printing the type
            // without it made a conjugated port read as an ordinary one.
            let conjugated = el.conjugates.iter().any(|c| c == t);
            out.push_str(&format!(" : {}{t}", if conjugated { "~" } else { "" }));
        }
        if let Some(v) = el.visibility {
            out.push_str(&format!(" ({})", v.keyword()));
        }
        if let Some(d) = &el.direction {
            out.push_str(&format!(" ({})", d.keyword()));
        }
        if let Some(c) = &el.constraint {
            out.push_str(&format!(" {{{c}}}"));
        }
        if let (Some(from), Some(to)) = (&el.from, &el.to) {
            out.push_str(&format!(" [{from} -> {to}]"));
        }
        out.push('\n');
        for c in &el.children {
            rec(c, depth + 1, out);
        }
    }
    for r in roots {
        rec(r, 0, &mut out);
    }
    out
}
