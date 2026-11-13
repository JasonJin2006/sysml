//! The package structure view, held to the question it exists to answer.
//!
//! The general view drops packages entirely, so `Top::Sub::Deep` and a flat model draw the same
//! picture. This view keeps the namespace tree, and these tests are the properties that make it
//! worth having: nesting survives, a type is not mistaken for a package, and every box is drawn
//! inside the frame that owns it.

use sysml_syntax::load;
use sysml_view::graph::{Graph, View};
use sysml_view::packages::{frames_of, packages, packages_geometry, Frame, Packages};
use sysml_view::Mode;

const NESTED: &str = r#"package Top {
    package Sub {
        part def Engine;
        package Deep {
            part def Bolt;
        }
    }
    part def Car;
}
"#;

fn label_of<'a>(p: &'a Packages, frame: &Frame) -> &'a str {
    p.graph.nodes[frame.node].label()
}

fn frame_named<'a>(p: &'a Packages, name: &str) -> &'a Frame {
    p.frames
        .iter()
        .find(|f| f.node != usize::MAX && label_of(p, f) == name)
        .unwrap_or_else(|| panic!("no frame named {name}"))
}

#[test]
fn nested_packages_stay_nested() {
    let loaded = load(NESTED).expect("parses");
    let p = packages(&loaded.roots);

    // The synthetic root plus Top, Sub and Deep. `Car`, `Engine` and `Bolt` are members.
    assert_eq!(p.frames.len(), 4, "root + three packages");
    assert_eq!(
        p.frames[0].node,
        usize::MAX,
        "index 0 is the synthetic root"
    );
    assert_eq!(frame_named(&p, "Top").depth, 1);
    assert_eq!(frame_named(&p, "Sub").depth, 2);
    assert_eq!(frame_named(&p, "Deep").depth, 3);

    let deep = frame_named(&p, "Deep");
    let sub = frame_named(&p, "Sub");
    assert_eq!(
        deep.parent,
        Some(sub.node),
        "Deep's parent is Sub, by graph index"
    );

    let member_names = |frame: &Frame| -> Vec<String> {
        frame
            .members
            .iter()
            .map(|&m| p.graph.nodes[m].label().to_string())
            .collect()
    };
    assert_eq!(member_names(frame_named(&p, "Top")), vec!["Car"]);
    assert_eq!(member_names(sub), vec!["Engine"]);
    assert_eq!(member_names(deep), vec!["Bolt"]);
}

/// A type with features must not be drawn as a container.
///
/// The frame renderer identifies a frame by the package flag. When it guessed instead — "a node
/// with children is a frame" — a `part def` with an attribute drew as a package named «part def»,
/// and its attribute drew as a sibling of the definition inside the enclosing package.
#[test]
fn a_definition_is_a_box_and_its_features_are_not_members() {
    let loaded = load(
        r#"package P {
    part def Drone { attribute m : Real = 5; }
}
"#,
    )
    .expect("parses");
    let p = packages(&loaded.roots);

    assert_eq!(p.frames.len(), 2, "root + P");
    assert!(
        p.frames
            .iter()
            .all(|f| f.node == usize::MAX || p.graph.nodes[f.node].is_frame),
        "only packages are frames"
    );
    assert_eq!(
        p.graph.nodes.len(),
        2,
        "P and Drone; the attribute is not drawn"
    );
    let drone = p
        .graph
        .nodes
        .iter()
        .find(|n| n.label() == "Drone")
        .expect("Drone is drawn");
    assert!(!drone.is_frame, "a part def is not a package");
    assert!(
        !drone.children.iter().any(|&c| c < p.graph.nodes.len()),
        "the attribute is not among the drawn nodes"
    );
}

/// Every box and every nested frame sits inside the frame that owns it.
///
/// This is the property a reader checks by eye, and the one that breaks first when the sizing or
/// the placement pass changes: the picture then claims an element belongs to a package it is not
/// in.
#[test]
fn everything_is_drawn_inside_its_frame() {
    let loaded = load(NESTED).expect("parses");
    let p = packages(&loaded.roots);
    let geo = packages_geometry(&p.graph, &p.frames);

    let rect = |node: usize| -> (f32, f32, f32, f32) {
        geo.frames
            .iter()
            .chain(geo.boxes.iter())
            .find(|(i, ..)| *i == node)
            .map(|(_, x, y, w, h)| (*x, *y, *w, *h))
            .unwrap_or_else(|| panic!("no rectangle for node {node}"))
    };

    for frame in &p.frames {
        if frame.node == usize::MAX {
            continue;
        }
        let (fx, fy, fw, fh) = rect(frame.node);
        for &member in &frame.members {
            let (x, y, w, h) = rect(member);
            assert!(
                x >= fx && y >= fy && x + w <= fx + fw && y + h <= fy + fh,
                "{} is not inside {}: box ({x},{y},{w},{h}) frame ({fx},{fy},{fw},{fh})",
                p.graph.nodes[member].label(),
                p.graph.nodes[frame.node].label()
            );
        }
        for &child in &frame.children {
            let node = p.frames[child].node;
            let (x, y, w, h) = rect(node);
            assert!(
                x >= fx && y >= fy && x + w <= fx + fw && y + h <= fy + fh,
                "frame {} is not inside {}",
                p.graph.nodes[node].label(),
                p.graph.nodes[frame.node].label()
            );
        }
    }
}

/// The canvas receives a graph and rebuilds the frames, so the rebuild has to agree with the
/// projection. A disagreement would draw one tree and report another.
#[test]
fn the_canvas_projection_rebuilds_the_same_tree() {
    let loaded = load(NESTED).expect("parses");
    let projected: Graph = sysml_view::project(&loaded.roots, View::Packages, Mode::Extended);
    let rebuilt = frames_of(&projected);
    let p = packages(&loaded.roots);

    assert_eq!(rebuilt.len(), p.frames.len());
    for (a, b) in rebuilt.iter().zip(&p.frames) {
        assert_eq!(a.node, b.node, "same frames in the same order");
        assert_eq!(a.depth, b.depth);
        assert_eq!(a.members, b.members);
    }
}

/// Containment is drawn as frames, not as lines, so the view emits no edges.
///
/// An edge from a package to its contents would put a line where the rectangle already says it,
/// and the general view already draws ownership as `*--` for callers that want the edge form.
#[test]
fn the_tree_is_drawn_as_frames_not_edges() {
    let loaded = load(NESTED).expect("parses");
    let p = packages(&loaded.roots);
    assert!(p.graph.edges.is_empty(), "no relationships in this view");
    assert_eq!(p.frames.len(), 4, "the tree is carried by the frames");
}

/// An import is a visibility statement, not a member of the namespace tree.
#[test]
fn imports_are_not_packages_and_are_not_members() {
    let loaded = load(
        r#"package P {
    import ScalarValues::*;
    part def Thing;
}
"#,
    )
    .expect("parses");
    let p = packages(&loaded.roots);
    assert_eq!(p.frames.len(), 2, "root + P");
    assert_eq!(p.graph.nodes.len(), 2, "P and Thing");
    assert!(
        p.graph.nodes.iter().all(|n| n.label() != "*"),
        "the wildcard import is not drawn"
    );
}
