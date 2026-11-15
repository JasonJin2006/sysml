//! That a long edge is drawn as a line rather than as a detour.
//!
//! An edge spanning more than one rank used to be a chain of lane hops: one virtual node per rank,
//! each hop turning a corner. Drawn, that is a wire that leaves its column, crosses the picture and
//! comes back — and it was invisible in the crossing count the ordering optimises, so nothing ever
//! corrected it.
//!
//! These tests measure the drawn geometry, not the layout: a route that says the right thing but
//! wanders is the defect being guarded against.

use sysml_view::graph;

const MODEL: &str = r#"package P {
    private import ScalarValues::*;
    part def Vehicle {
        attribute mass : Real = 1200;
    }
    requirement def Heavy;
    requirement def Slow;
    part def System {
        part vehicle : Vehicle;
        attribute topSpeed : Real = 30;
    }
    satisfy Heavy by System::vehicle;
    satisfy Slow by System::vehicle;
}
"#;

/// The drawn polylines of the general view, one per relationship.
fn routes(src: &str) -> Vec<Vec<(f32, f32)>> {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    let geom = sysml_view::general_geometry(&g, sysml_view::layered::Style::Nested);
    geom.edges.into_iter().map(|(_, points)| points).collect()
}

/// How far a polyline travels.
fn travel(points: &[(f32, f32)]) -> f32 {
    points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).abs() + (w[1].1 - w[0].1).abs())
        .sum()
}

#[test]
fn no_edge_wanders_far_beyond_the_picture() {
    // A line that travels more than three times the canvas width is not describing a relationship
    // between two boxes, it is describing a route around them.
    let loaded = sysml_syntax::load(MODEL).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    let geom = sysml_view::general_geometry(&g, sysml_view::layered::Style::Nested);
    let width = geom.width;
    for points in routes(MODEL) {
        if points.len() < 2 {
            continue;
        }
        let distance = travel(&points);
        assert!(
            distance < width * 3.0,
            "an edge travels {distance:.0} on a {width:.0} wide picture: {points:?}"
        );
    }
}

#[test]
fn a_satisfy_edge_reaches_down_to_its_requirement() {
    // The relationship that made this worth fixing: a requirement sits below the part that
    // satisfies it, so the line should run down, not out and back.
    let list = routes(MODEL);
    assert!(!list.is_empty(), "the model has relationships");
    let widest = list
        .iter()
        .max_by(|a, b| {
            travel(a)
                .partial_cmp(&travel(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("at least one");
    // Its horizontal span is the part of the route a reader notices, so it is what is bounded.
    let xs: Vec<f32> = widest.iter().map(|p| p.0).collect();
    let span =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!(
        span < 400.0,
        "the longest edge spans {span:.0} horizontally: {widest:?}"
    );
}

#[test]
fn a_route_is_a_polyline_with_a_start_and_an_end() {
    // Whatever computes it, the result has to be a sequence of points a renderer can stroke.
    for points in routes(MODEL) {
        assert!(points.len() >= 2, "a route needs two points: {points:?}");
        for pair in points.windows(2) {
            let orthogonal =
                (pair[0].0 - pair[1].0).abs() < 0.01 || (pair[0].1 - pair[1].1).abs() < 0.01;
            assert!(
                orthogonal,
                "a segment is not axis-aligned: {:?} to {:?}",
                pair[0], pair[1]
            );
        }
    }
}
