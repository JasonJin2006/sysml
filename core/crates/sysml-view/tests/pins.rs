//! A box a person has placed stays where they put it.
//!
//! The layout decides where everything goes, and a pin is the one exception: dragging a box means
//! it stays there. The test that matters is not "is the box at the saved coordinate" — it is "does
//! the rest of the picture agree with the saved coordinate", because a box that moved while its
//! lines stayed behind is a box that moved and a picture that broke.

use sysml_view::graph;
use sysml_view::layered::Pin;

const MODEL: &str = r#"package P {
    part def Wheel;
    part def Engine;
    part def Car {
        part wheel : Wheel;
        part engine : Engine;
    }
}
"#;

/// The laid-out geometry of the general view, with `pins` applied.
fn layout_of(pins: &[Pin]) -> sysml_view::GeneralGeometry {
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    sysml_view::general_geometry_pinned(&g, sysml_view::layered::Style::Nested, pins)
}

/// The drawn rectangle of the box carrying `label`.
fn box_of(geom: &sysml_view::GeneralGeometry, label: &str, src: &str) -> (f32, f32, f32, f32) {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    geom.boxes
        .iter()
        .find(|&&(i, _, _, _, _)| g.nodes[i].label() == label)
        .map(|&(_, x, y, w, h)| (x, y, w, h))
        .unwrap_or_else(|| panic!("no box called {label}"))
}

/// The page margin `general_geometry` adds to every box's x. A pin is applied before that, so a
/// pin taken from a reported coordinate is that coordinate minus this.
const MARGIN: f32 = sysml_view::PAGE_MARGIN;

#[test]
fn a_pinned_box_is_where_the_pin_says() {
    let unpinned = layout_of(&[]);
    let (x, y, _, _) = box_of(&unpinned, "wheel", MODEL);
    // `general_geometry_pinned` takes a pin in the layout's own frame and adds its page margin when
    // it reports the boxes. The studio converts a saved coordinate by subtracting that margin; the
    // sidecar stores the picture's coordinates, which is what a person sees.
    let pin = Pin {
        node: index_of("wheel"),
        x: x - MARGIN + 300.0,
        y: y + 120.0,
        w: None,
        h: None,
    };
    let pinned = layout_of(&[pin]);
    let (px, py, _, _) = box_of(&pinned, "wheel", MODEL);
    assert!((px - (x + 300.0)).abs() < 1.0, "{x} → {px}");
    assert!((py - (y + 120.0)).abs() < 1.0, "{y} → {py}");
}

#[test]
fn an_unpinned_box_is_left_where_the_layout_put_it() {
    // A pin is about one box. A change that moved the others too would make every drag a re-layout.
    let before = layout_of(&[]);
    let (bx, by, _, _) = box_of(&before, "engine", MODEL);
    let (wx, wy, _, _) = box_of(&before, "wheel", MODEL);
    let after = layout_of(&[Pin {
        node: index_of("wheel"),
        x: wx + 300.0,
        y: wy + 120.0,
        w: None,
        h: None,
    }]);
    let (ax, ay, _, _) = box_of(&after, "engine", MODEL);
    assert!(
        (ax - bx).abs() < 1.0 && (ay - by).abs() < 1.0,
        "{bx},{by} → {ax},{ay}"
    );
}

#[test]
fn a_resized_box_keeps_its_saved_size() {
    let before = layout_of(&[]);
    let (x, y, _, _) = box_of(&before, "wheel", MODEL);
    let pin = Pin {
        node: index_of("wheel"),
        x,
        y,
        w: Some(240.0),
        h: Some(96.0),
    };
    let after = layout_of(&[pin]);
    let (_, _, w, h) = box_of(&after, "wheel", MODEL);
    assert!((w - 240.0).abs() < 1.0, "width {w}");
    assert!((h - 96.0).abs() < 1.0, "height {h}");
}

#[test]
fn the_line_to_a_pinned_box_follows_it() {
    // The point of the whole change. A route is computed from the node positions, so a pin that
    // moved a box without moving its line would leave the line pointing at empty space.
    let before = layout_of(&[]);
    let (x, y, _, _) = box_of(&before, "wheel", MODEL);
    let pin = Pin {
        node: index_of("wheel"),
        x: x + 300.0,
        y: y + 20.0,
        w: None,
        h: None,
    };
    let after = layout_of(&[pin]);

    let ends = |geom: &sysml_view::GeneralGeometry| -> Vec<(f32, f32)> {
        geom.edges
            .iter()
            .flat_map(|(_, points)| points.iter().copied())
            .collect()
    };
    let shifted = ends(&before) != ends(&after);
    assert!(shifted, "no route moved when its box did");
}

#[test]
fn a_box_dragged_below_the_grid_is_still_inside_the_canvas() {
    // The canvas is sized from the boxes. A pin below the last rank is what a person dragging a box
    // down means, and a height taken from the rank grid alone would cut it off.
    let before = layout_of(&[]);
    let (x, _, _, _) = box_of(&before, "wheel", MODEL);
    let pin = Pin {
        node: index_of("wheel"),
        x,
        y: before.height + 400.0,
        w: None,
        h: None,
    };
    let after = layout_of(&[pin]);
    let (_, py, _, ph) = box_of(&after, "wheel", MODEL);
    assert!(
        py + ph <= after.height + 1.0,
        "box ends at {:.0}, canvas is {:.0}",
        py + ph,
        after.height
    );
}

#[test]
fn every_polyline_starts_at_its_graph_source_and_ends_at_its_graph_target() {
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let graph = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    let geom = layout_of(&[]);
    for (index, points) in &geom.edges {
        let edge = &graph.edges[*index];
        let source = geom
            .boxes
            .iter()
            .find(|b| b.0 == edge.from)
            .expect("source");
        let target = geom.boxes.iter().find(|b| b.0 == edge.to).expect("target");
        assert!(on_border(points[0], *source), "source {edge:?}: {points:?}");
        assert!(
            on_border(*points.last().expect("end"), *target),
            "target {edge:?}: {points:?}"
        );
    }
}

#[test]
fn sideways_composite_edges_share_the_owner_attachment() {
    let positions = [
        ("Car", 100.0, 180.0),
        ("wheel", 550.0, 130.0),
        ("engine", 550.0, 260.0),
        ("Wheel", 850.0, 100.0),
        ("Engine", 850.0, 300.0),
    ];
    let pins: Vec<Pin> = positions
        .iter()
        .map(|&(name, x, y)| Pin {
            node: index_of(name),
            x: x - MARGIN,
            y,
            w: None,
            h: None,
        })
        .collect();
    let geom = layout_of(&pins);
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let graph = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    let mut starts = Vec::new();
    for (index, points) in &geom.edges {
        if graph.edges[*index].from == index_of("Car") {
            assert!((points[0].0 - 196.0).abs() < 0.5, "right edge: {points:?}");
            starts.push(points[0].1);
        }
    }
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0], starts[1], "one owner-side diamond: {starts:?}");
}

#[test]
fn a_pinned_line_routes_around_other_boxes_after_crossing_the_rank_grid() {
    let positions = [
        ("Car", 100.0, 180.0),
        ("wheel", 650.0, 180.0),
        ("engine", 650.0, 360.0),
        ("Wheel", 850.0, 100.0),
        ("Engine", 400.0, 180.0),
    ];
    let pins: Vec<Pin> = positions
        .iter()
        .map(|&(name, x, y)| Pin {
            node: index_of(name),
            x: x - MARGIN,
            y,
            w: None,
            h: None,
        })
        .collect();
    let geom = layout_of(&pins);
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let graph = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    for (index, points) in &geom.edges {
        let edge = &graph.edges[*index];
        let source = geom
            .boxes
            .iter()
            .find(|b| b.0 == edge.from)
            .expect("source");
        let target = geom.boxes.iter().find(|b| b.0 == edge.to).expect("target");
        assert!(on_border(points[0], *source), "source: {points:?}");
        assert!(
            on_border(*points.last().expect("end"), *target),
            "target: {points:?}"
        );
        for segment in points.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            assert!(
                (a.0 - b.0).abs() < 0.01 || (a.1 - b.1).abs() < 0.01,
                "orthogonal: {points:?}"
            );
            for &(_, x, y, w, h) in &geom.boxes {
                let cuts = if (a.1 - b.1).abs() < 0.01 {
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
                assert!(!cuts, "line {points:?} crosses box ({x},{y},{w},{h})");
            }
        }
    }
}

fn on_border(p: (f32, f32), b: (usize, f32, f32, f32, f32)) -> bool {
    let (_, x, y, w, h) = b;
    let within = p.0 >= x - 0.5 && p.0 <= x + w + 0.5 && p.1 >= y - 0.5 && p.1 <= y + h + 0.5;
    within
        && ((p.0 - x).abs() < 0.5
            || (p.0 - x - w).abs() < 0.5
            || (p.1 - y).abs() < 0.5
            || (p.1 - y - h).abs() < 0.5)
}

/// The index the projection gives a box, which is what a pin addresses.
fn index_of(label: &str) -> usize {
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    g.nodes
        .iter()
        .position(|n| n.label() == label)
        .unwrap_or_else(|| panic!("no box called {label}"))
}
