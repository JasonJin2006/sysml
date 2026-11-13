//! The state transition view, held to the fact the general view loses.
//!
//! In the general view `state off`, `state on` and `transition off then on` are siblings: three
//! boxes hanging off the machine with no edge between them. The one thing the diagram exists to
//! show — off goes to on — is the thing it cannot show. These tests are that claim and the three
//! ways a projection can get it wrong: reading the wrong end, inventing an edge to a box that is
//! not drawn, and treating a package as a machine.

use sysml_syntax::load;
use sysml_view::graph::{Arrow, View};
use sysml_view::states::{groups_of, states};

const MACHINE: &str = r#"package P {
    state def Power {
        state off;
        state on;
        state fault;
        transition off then on;
        transition on then fault;
        transition fault then off;
    }
    part def Drone;
}
"#;

#[test]
fn a_transition_is_an_edge_between_the_states_it_names() {
    let loaded = load(MACHINE).expect("parses");
    let view = states(&loaded.roots);
    let labels: Vec<&str> = view.graph.nodes.iter().map(|n| n.label()).collect();
    assert_eq!(
        labels,
        vec!["Power", "off", "on", "fault"],
        "one frame, three states"
    );
    assert_eq!(view.graph.edges.len(), 3, "three transitions, three edges");

    let name = |i: usize| view.graph.nodes[i].label();
    let mut pairs: Vec<(String, String)> = view
        .graph
        .edges
        .iter()
        .map(|e| (name(e.from).to_string(), name(e.to).to_string()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("fault".to_string(), "off".to_string()),
            ("off".to_string(), "on".to_string()),
            ("on".to_string(), "fault".to_string()),
        ],
        "the edges are the transitions, not the machine owning them"
    );
    assert!(
        view.graph
            .edges
            .iter()
            .all(|e| e.arrow == Arrow::Transition),
        "a transition draws as an arrow, not as an undirected line"
    );
}

/// The bug this view was written after: a package that contains a machine is not a machine.
///
/// Counting a `state def` as a state made every enclosing package a frame with the machine
/// nested inside it, so the picture claimed the package was a state machine.
#[test]
fn a_package_holding_a_machine_is_not_a_machine() {
    let loaded = load(MACHINE).expect("parses");
    let view = states(&loaded.roots);
    assert_eq!(view.groups.len(), 1, "Power, and nothing else");
    assert_eq!(
        view.graph.nodes[view.groups[0].node].label(),
        "Power",
        "the frame is the state def"
    );
    assert!(
        view.graph.nodes.iter().all(|n| n.label() != "P"),
        "the package is not drawn at all: it is not a state machine"
    );
}

/// A transition whose end names something not drawn is left out, not drawn to nowhere.
#[test]
fn a_transition_to_an_undrawn_state_is_skipped() {
    let loaded = load(
        r#"package P {
    state def M {
        state a;
        transition a then missing;
        transition a then a;
    }
}
"#,
    )
    .expect("parses");
    let view = states(&loaded.roots);
    assert_eq!(
        view.graph.edges.len(),
        1,
        "only the self-transition survives"
    );
    assert_eq!(view.graph.edges[0].from, view.graph.edges[0].to);
}

/// The layout keeps every state inside its own machine's frame, and no two states overlap.
#[test]
fn states_sit_inside_their_frame_without_overlapping() {
    let loaded = load(MACHINE).expect("parses");
    let view = states(&loaded.roots);
    let groups = groups_of(&view.graph);
    let geo = sysml_view::states_geometry(&view.graph, &groups);

    let rect_of = |node: usize| {
        geo.boxes
            .iter()
            .chain(geo.frames.iter())
            .find(|(i, ..)| *i == node)
            .map(|(_, x, y, w, h)| (*x, *y, *w, *h))
            .unwrap_or_else(|| panic!("no rectangle for node {node}"))
    };

    for machine in &groups {
        let (fx, fy, fw, fh) = rect_of(machine.node);
        for &state in &machine.members {
            let (x, y, w, h) = rect_of(state);
            assert!(
                x >= fx && y >= fy && x + w <= fx + fw && y + h <= fy + fh,
                "{} is outside {}",
                view.graph.nodes[state].label(),
                view.graph.nodes[machine.node].label()
            );
        }
    }

    let boxes: Vec<usize> = view.groups.iter().flat_map(|m| m.members.clone()).collect();
    for (i, &a) in boxes.iter().enumerate() {
        for &b in &boxes[i + 1..] {
            let (ax, ay, aw, ah) = rect_of(a);
            let (bx, by, bw, bh) = rect_of(b);
            assert!(
                ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay,
                "{} overlaps {}",
                view.graph.nodes[a].label(),
                view.graph.nodes[b].label()
            );
        }
    }

    // Every edge has two ends on the border of its boxes, not a zero-length stub.
    for (index, points) in &geo.edges {
        assert_eq!(points.len(), 2, "edge {index} is a single segment");
        let length =
            ((points[1].0 - points[0].0).powi(2) + (points[1].1 - points[0].1).powi(2)).sqrt();
        assert!(
            length > 10.0,
            "edge {index} is {length} long: the boxes are touching"
        );
    }
}

/// The projection and the canvas rebuild the same machines, or they draw different pictures.
#[test]
fn the_canvas_rebuilds_the_same_machines() {
    let loaded = load(MACHINE).expect("parses");
    let view = states(&loaded.roots);
    let projected = sysml_view::project(
        &loaded.roots,
        View::StateTransition,
        sysml_view::Mode::Extended,
    );
    let rebuilt = groups_of(&projected);
    assert_eq!(rebuilt.len(), view.groups.len());
    for (a, b) in rebuilt.iter().zip(&view.groups) {
        assert_eq!(a.node, b.node);
        assert_eq!(a.members, b.members);
    }
}

/// The action flow view: the same shape, a different set of node kinds.
///
/// `action survey`, `action plan` and `succession survey then plan` are three sibling boxes in the
/// general view, exactly as states are. What makes an action flow is the succession, so the view
/// has to agree with the state view about everything except which children are nodes.
#[test]
fn an_action_flow_draws_its_successions() {
    let loaded = load(
        r#"package P {
    action def Mission {
        action survey;
        action plan;
        action done;
        succession survey then plan;
        succession plan then done;
    }
}
"#,
    )
    .expect("parses");
    let view = sysml_view::states::actions(&loaded.roots);
    let labels: Vec<&str> = view.graph.nodes.iter().map(|n| n.label()).collect();
    assert_eq!(labels, vec!["Mission", "survey", "plan", "done"]);
    assert_eq!(view.graph.edges.len(), 2, "two successions, two edges");
    let name = |i: usize| view.graph.nodes[i].label();
    let mut pairs: Vec<(String, String)> = view
        .graph
        .edges
        .iter()
        .map(|e| (name(e.from).to_string(), name(e.to).to_string()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("plan".to_string(), "done".to_string()),
            ("survey".to_string(), "plan".to_string())
        ]
    );
    assert_eq!(view.groups.len(), 1, "one action definition, one frame");
}

/// A group's nodes are only its direct behaviour children: an attribute is not an action.
#[test]
fn an_action_flow_does_not_draw_attributes_as_nodes() {
    let loaded = load(
        r#"package P {
    action def Mission {
        attribute duration : Real = 10;
        action survey;
        action plan;
        succession survey then plan;
    }
}
"#,
    )
    .expect("parses");
    let view = sysml_view::states::actions(&loaded.roots);
    let labels: Vec<&str> = view.graph.nodes.iter().map(|n| n.label()).collect();
    assert_eq!(
        labels,
        vec!["Mission", "survey", "plan"],
        "the attribute is not drawn"
    );
}

/// A quoted name is one name, however it is written.
///
/// `first 'a 1' then 'b 2';` states the same succession as `first a1 then b2;`. The end keeps its
/// quotes and may contain a space or a `.` inside them; the element it names has neither. Reading
/// the end as a plain `rsplit('.')` returned `1'`, the lookup missed, and every edge vanished —
/// which is what a whole model of Chinese names set in quotes looks like.
#[test]
fn a_succession_between_quoted_names_is_still_an_edge() {
    let loaded = load(
        r#"package P {
    action def 'flow' {
        action 'a 1';
        action 'b 2';
        first 'a 1' then 'b 2';
    }
}
"#,
    )
    .expect("parses");
    let view = sysml_view::states::actions(&loaded.roots);
    let labels: Vec<&str> = view.graph.nodes.iter().map(|n| n.label()).collect();
    assert_eq!(labels, vec!["flow", "a 1", "b 2"]);
    assert_eq!(
        view.graph.edges.len(),
        1,
        "the quoted succession is an edge"
    );
    let name = |i: usize| view.graph.nodes[i].label();
    let edge = &view.graph.edges[0];
    assert_eq!((name(edge.from), name(edge.to)), ("a 1", "b 2"));
}
