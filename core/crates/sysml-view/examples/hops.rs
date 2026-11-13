//! Print the hop points a projection produces, for checking them against the picture.
//!
//! Usage: `cargo run --release --example hops -- <model.sysml>`
//!
//! Reads a model, projects it to the general view, and prints every polyline plus the points at
//! which it is told to hop. Exists because the renderer is the only consumer and a wrong hop is
//! invisible in an assertion about the SVG as a whole.

use std::path::PathBuf;

fn main() {
    let path = std::env::args().nth(1).expect("usage: hops <model.sysml>");
    let path = PathBuf::from(path);
    let text = std::fs::read_to_string(&path).expect("read the model");
    let loaded = sysml_syntax::load_project(&[&text], true).expect("parse the model");
    let graph = sysml_view::project(
        &loaded.roots,
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    );
    let geom = sysml_view::general_geometry(&graph, sysml_view::layered::Style::Nested);
    let lines: Vec<Vec<(f32, f32)>> = geom.edges.iter().map(|(_, p)| p.clone()).collect();
    let hops = sysml_view::jumps::hops(&lines);
    for ((edge, points), hop) in geom.edges.iter().zip(&hops) {
        println!("edge {edge}: {points:?}");
        if !hop.is_empty() {
            println!("    hops {hop:?}");
        }
    }
}
