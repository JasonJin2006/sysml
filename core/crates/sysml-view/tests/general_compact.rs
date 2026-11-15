use sysml_view::graph::{Arrow, Graph};
use sysml_view::layered::{self, Input, Pin, Style};
use sysml_view::{general_geometry, general_geometry_pinned, Mode, PAGE_MARGIN};

fn graph(text: &str) -> Graph {
    let loaded = sysml_syntax::load(text).expect("parses");
    sysml_view::graph::general(&loaded.roots, Mode::Extended)
}

fn star(count: usize) -> Graph {
    let members = (0..count)
        .map(|i| format!("part member{i};"))
        .collect::<Vec<_>>()
        .join(" ");
    graph(&format!("package P {{ part def Owner {{ {members} }} }}"))
}

fn box_of(geometry: &sysml_view::GeneralGeometry, node: usize) -> (f32, f32, f32, f32) {
    geometry
        .boxes
        .iter()
        .find(|b| b.0 == node)
        .map(|b| (b.1, b.2, b.3, b.4))
        .expect("node rectangle")
}

#[test]
fn disconnected_subgraphs_stay_local_when_the_page_wraps() {
    let members = (0..24)
        .map(|i| format!("part def D{i} {{ part a; part b; }} part u{i} : D{i};"))
        .collect::<Vec<_>>()
        .join(" ");
    let graph = graph(&format!("package P {{ {members} }}"));
    let geometry = general_geometry(&graph, Style::Nested);
    assert_eq!(geometry.boxes.len(), graph.nodes.len());
    assert_eq!(geometry.edges.len(), graph.edges.len());
    assert!(geometry.width <= 1100.0 + 4.0 * PAGE_MARGIN);
    for edge in &graph.edges {
        let source = box_of(&geometry, edge.from);
        let target = box_of(&geometry, edge.to);
        assert!(
            (target.1 - source.1).abs() < 400.0,
            "one component was split across shelves"
        );
    }
}

#[test]
fn a_wide_star_wraps_without_hiding_nodes_or_relationships() {
    let graph = star(28);
    let geometry = general_geometry(&graph, Style::Nested);
    assert_eq!(geometry.boxes.len(), graph.nodes.len());
    assert_eq!(geometry.edges.len(), graph.edges.len());
    assert!(
        geometry.width <= 1100.0 + 4.0 * PAGE_MARGIN,
        "{}",
        geometry.width
    );
    let mut rows: Vec<_> = geometry
        .boxes
        .iter()
        .filter(|b| graph.nodes[b.0].parent.is_some())
        .map(|b| b.2)
        .collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup();
    assert!(rows.len() > 1, "siblings still form one row");
    for (i, &(_, ax, ay, aw, ah)) in geometry.boxes.iter().enumerate() {
        for &(_, bx, by, bw, bh) in &geometry.boxes[i + 1..] {
            assert!(ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay);
        }
    }
}

#[test]
fn notation_layout_is_deterministic_and_preserves_every_projection_node() {
    let graph = graph(include_str!("../../../examples/showcase/notation.sysml"));
    let first = general_geometry(&graph, Style::Nested);
    let second = general_geometry(&graph, Style::Nested);
    assert_eq!(first.boxes, second.boxes);
    assert_eq!(first.edges, second.edges);
    assert_eq!(first.boxes.len(), graph.nodes.len());
    assert_eq!(first.edges.len(), graph.edges.len());
    assert!(first.width <= 1100.0 + 4.0 * PAGE_MARGIN, "{}", first.width);
}

#[test]
fn small_general_graphs_keep_the_original_layered_node_positions() {
    let graph = graph(
        "package P { part def Wheel; part def Car { part front : Wheel; part rear : Wheel; } }",
    );
    let sizes: Vec<_> = graph.nodes.iter().map(sysml_view::node_size).collect();
    let edges: Vec<_> = graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, e)| Input {
            index,
            from: e.from,
            to: e.to,
            kind: e.arrow,
            same_rank: false,
        })
        .collect();
    let order: Vec<_> = (0..sizes.len()).collect();
    let parents: Vec<_> = graph.nodes.iter().map(|n| n.parent).collect();
    let original = layered::build(&sizes, &edges, &order, &parents);
    assert!(original.width < 1100.0);
    let geometry = general_geometry(&graph, Style::Nested);
    for node in original.nodes.iter().filter(|n| n.real.is_some()) {
        let r = box_of(&geometry, node.real.unwrap());
        assert_eq!(r, (node.x + PAGE_MARGIN, node.y, node.w, node.h));
    }
}

#[test]
fn pinned_compact_nodes_and_sizes_do_not_repack_the_other_nodes() {
    let graph = star(28);
    let original = general_geometry(&graph, Style::Nested);
    let index = 3;
    let (x, y, _, _) = box_of(&original, index);
    let pinned = general_geometry_pinned(
        &graph,
        Style::Nested,
        &[Pin {
            node: index,
            x: x - PAGE_MARGIN + 150.0,
            y: y + 800.0,
            w: Some(190.0),
            h: Some(90.0),
        }],
    );
    assert_eq!(box_of(&pinned, index), (x + 150.0, y + 800.0, 190.0, 90.0));
    for &(i, x, y, w, h) in &original.boxes {
        if i != index {
            assert_eq!(box_of(&pinned, i), (x, y, w, h));
        }
    }
}

#[test]
fn variation_groups_are_not_split_when_their_rank_wraps() {
    let extra = (0..20)
        .map(|i| format!("part free{i};"))
        .collect::<Vec<_>>()
        .join(" ");
    let graph=graph(&format!("package P {{ variation part def Choices {{ variant part optionA; variant part optionB; }} {extra} }}"));
    let geometry = general_geometry(&graph, Style::Nested);
    let at = |name| graph.nodes.iter().position(|n| n.name == name).unwrap();
    let y = box_of(&geometry, at("Choices")).1;
    assert_eq!(box_of(&geometry, at("optionA")).1, y);
    assert_eq!(box_of(&geometry, at("optionB")).1, y);
}

#[test]
fn nonownership_relationships_keep_distinct_attachment_slots() {
    let graph =
        graph("package P { part def Shared; part a : Shared; part b : Shared; part c : Shared; }");
    let geometry = general_geometry(&graph, Style::Nested);
    let ends: std::collections::BTreeSet<_> = geometry
        .edges
        .iter()
        .filter(|(i, _)| graph.edges[*i].arrow == Arrow::Typing)
        .map(|(_, p)| {
            let p = p.last().unwrap();
            (p.0.to_bits(), p.1.to_bits())
        })
        .collect();
    assert_eq!(ends.len(), 3);
}

#[test]
fn tree_pins_address_real_node_ids_not_the_sorted_position() {
    let graph = graph("package P { part def Root { part child; } part def Other; }");
    let geometry = general_geometry_pinned(
        &graph,
        Style::Tree,
        &[Pin {
            node: 1,
            x: 200.0,
            y: 500.0,
            w: None,
            h: None,
        }],
    );
    assert_eq!(box_of(&geometry, 1).0, 200.0 + PAGE_MARGIN);
    assert_eq!(box_of(&geometry, 1).1, 500.0);
}

#[test]
fn each_owner_side_shares_a_single_composite_attachment() {
    let graph = star(28);
    let geometry = general_geometry(&graph, Style::Nested);
    let mut starts = Vec::new();
    for (index, points) in &geometry.edges {
        let e = &graph.edges[*index];
        if e.arrow == Arrow::Composite {
            starts.push(points[0]);
        }
    }
    let unique: std::collections::BTreeSet<_> = starts
        .iter()
        .map(|p| (p.0.to_bits(), p.1.to_bits()))
        .collect();
    assert!(
        unique.len() <= 4,
        "more than one diamond per owner side: {}",
        unique.len()
    );
    assert!(unique.len() < starts.len());
}

#[test]
fn compact_routes_are_directed_orthogonal_and_avoid_nonoverlapping_boxes() {
    let graph = star(20);
    let geometry = general_geometry(&graph, Style::Nested);
    for (index, points) in &geometry.edges {
        let edge = &graph.edges[*index];
        for (p, node) in [(points[0], edge.from), (*points.last().unwrap(), edge.to)] {
            let (x, y, w, h) = box_of(&geometry, node);
            assert!(
                p.0 >= x - 0.01 && p.0 <= x + w + 0.01 && p.1 >= y - 0.01 && p.1 <= y + h + 0.01
            );
            assert!(
                (p.0 - x).abs() < 0.01
                    || (p.0 - x - w).abs() < 0.01
                    || (p.1 - y).abs() < 0.01
                    || (p.1 - y - h).abs() < 0.01
            );
        }
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!((a.0 - b.0).abs() < 0.01 || (a.1 - b.1).abs() < 0.01);
            for &(_, x, y, w, h) in &geometry.boxes {
                let crosses = if (a.1 - b.1).abs() < 0.01 {
                    a.1 > y + 0.5
                        && a.1 < y + h - 0.5
                        && a.0.min(b.0) < x + w - 0.5
                        && a.0.max(b.0) > x + 0.5
                } else {
                    a.0 > x + 0.5
                        && a.0 < x + w - 0.5
                        && a.1.min(b.1) < y + h - 0.5
                        && a.1.max(b.1) > y + 0.5
                };
                assert!(!crosses, "edge {index}: {points:?} cuts ({x},{y},{w},{h})");
            }
        }
    }
}
