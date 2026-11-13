//! The sequence view, held to what the model states about an exchange.
//!
//! A message has a sender (the part it is written inside) and a receiver (the feature named after
//! `to`). Nothing else in the model pairs them, and the view must not invent the pairing: `accept`
//! is a separate statement, so it is not drawn as the other half of a `send`.

use sysml_syntax::load;

const EXCHANGE: &str = r#"package P {
    part def Ground;
    part def Drone;
    part ground : Ground;
    part drone : Drone {
        action report {
            send ping to ground;
            send status to ground;
        }
    }
}
"#;

fn pairs(roots: &[sysml_model::Element]) -> Vec<(String, String, String)> {
    let view = sysml_view::sequence::sequence(roots);
    let name = |i: usize| view.graph.nodes[i].label().to_string();
    view.graph
        .edges
        .iter()
        .map(|e| {
            (
                name(e.from),
                name(e.to),
                e.label.clone().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn a_send_is_an_arrow_from_its_part_to_the_part_it_names() {
    let loaded = load(EXCHANGE).expect("parses");
    let edges = pairs(&loaded.roots);
    assert_eq!(
        edges,
        vec![
            (
                "drone".to_string(),
                "ground".to_string(),
                "ping".to_string()
            ),
            (
                "drone".to_string(),
                "ground".to_string(),
                "status".to_string()
            ),
        ],
        "each send is one arrow, labelled with what it carries"
    );
}

#[test]
fn only_parts_that_exchange_a_message_are_lifelines() {
    let loaded = load(
        r#"package P {
    part def A;
    part def B;
    part a : A;
    part b : B;
    part bystander : A {
        action idle { }
    }
}
"#,
    )
    .expect("parses");
    // No sends at all: the picture is empty rather than a row of lifelines for parts that never
    // speak, and the empty state of the canvas says so.
    let view = sysml_view::sequence::sequence(&loaded.roots);
    assert_eq!(view.graph.nodes.len(), 0);
    assert_eq!(view.graph.edges.len(), 0);
}

/// A send written outside any part has no lifeline to leave from.
#[test]
fn a_send_outside_a_part_is_not_drawn() {
    let loaded = load(
        r#"package P {
    part def Ground;
    part ground : Ground;
    action def Loose {
        send ping to ground;
    }
}
"#,
    )
    .expect("parses");
    let view = sysml_view::sequence::sequence(&loaded.roots);
    assert!(
        view.graph.edges.is_empty(),
        "an action definition is not a lifeline; the send is skipped, not attached to the package"
    );
}

/// A message to a name that is not a part is skipped: the receiver has no lifeline.
#[test]
fn a_send_to_an_unknown_feature_is_not_drawn() {
    let loaded = load(
        r#"package P {
    part def Drone;
    part drone : Drone {
        action report {
            send ping to nowhere;
        }
    }
}
"#,
    )
    .expect("parses");
    let view = sysml_view::sequence::sequence(&loaded.roots);
    assert!(view.graph.edges.is_empty());
    assert!(
        view.graph.nodes.is_empty(),
        "and the sender alone is not a picture"
    );
}

/// Time runs down the page, so the rows follow the source order and not the tree order.
#[test]
fn messages_are_ordered_by_where_they_are_written() {
    let loaded = load(
        r#"package P {
    part def Ground;
    part def Drone;
    part ground : Ground;
    part drone : Drone {
        action first { send a to ground; }
        action second { send b to ground; }
    }
}
"#,
    )
    .expect("parses");
    let edges = pairs(&loaded.roots);
    assert_eq!(
        edges
            .iter()
            .map(|(_, _, label)| label.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

/// The layout puts each message on its own row, below the headers, in order.
#[test]
fn messages_stack_down_the_page() {
    let loaded = load(EXCHANGE).expect("parses");
    let view = sysml_view::sequence::sequence(&loaded.roots);
    let geo = sysml_view::sequence_geometry(&view.graph);
    let ys: Vec<f32> = geo.edges.values().map(|points| points[0].1).collect();
    assert_eq!(ys.len(), 2);
    assert!(
        ys[0] < ys[1],
        "the later message is lower on the page: {ys:?}"
    );
    let header_bottom = geo.boxes[0].2 + 46.0;
    assert!(ys[0] > header_bottom, "messages start below the headers");
    assert!(ys[1] < geo.height, "and stay inside the picture");
}
