use sysml_view::{Mode, View};

const EXAMPLES: &[(&str, &str, View, usize, usize)] = &[
    (
        "general",
        include_str!("../../../examples/showcase/general.sysml"),
        View::General,
        20,
        20,
    ),
    (
        "interconnection",
        include_str!("../../../examples/showcase/interconnection.sysml"),
        View::Interconnection,
        8,
        6,
    ),
    (
        "packages",
        include_str!("../../../examples/showcase/packages.sysml"),
        View::Packages,
        20,
        0,
    ),
    (
        "states",
        include_str!("../../../examples/showcase/states.sysml"),
        View::StateTransition,
        10,
        10,
    ),
    (
        "actions",
        include_str!("../../../examples/showcase/actions.sysml"),
        View::ActionFlow,
        10,
        10,
    ),
    (
        "sequence",
        include_str!("../../../examples/showcase/sequence.sysml"),
        View::Sequence,
        4,
        10,
    ),
    (
        "traceability",
        include_str!("../../../examples/showcase/traceability.sysml"),
        View::Traceability,
        12,
        8,
    ),
];

#[test]
fn showcase_models_have_substantial_drawable_projections() {
    for &(name, text, view, min_nodes, min_edges) in EXAMPLES {
        let loaded = sysml_syntax::load(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let graph = sysml_view::project(&loaded.roots, view, Mode::Extended);
        assert!(
            graph.nodes.len() >= min_nodes,
            "{name}: {} nodes, expected at least {min_nodes}",
            graph.nodes.len()
        );
        assert!(
            graph.edges.len() >= min_edges,
            "{name}: {} edges, expected at least {min_edges}",
            graph.edges.len()
        );
        let svg = sysml_view::render(&graph, name);
        assert!(
            svg.starts_with("<svg") || svg.contains("<svg"),
            "{name}: no SVG"
        );
        assert!(!svg.contains("NaN"), "{name}: invalid geometry");
        assert!(!svg.contains("inf\""), "{name}: infinite geometry");
    }
}

#[test]
fn showcase_tables_and_browser_share_the_same_source_model() {
    let text = include_str!("../../../examples/showcase/traceability.sysml");
    let loaded = sysml_syntax::load(text).expect("showcase model parses");
    let table = sysml_view::table::elements(&loaded.roots);
    let matrix = sysml_view::table::requirement_matrix(&loaded.roots);
    assert!(table.row_count() >= 12);
    assert!(matrix.row_count() >= 6);
    assert!(sysml_view::table::to_csv(&matrix).contains(','));
    assert!(sysml_view::outline(&loaded.roots).contains("requirement"));
}
