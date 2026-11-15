use sysml_view::{graph::NodeSymbol, states, Mode, View};

fn project(text: &str, view: View) -> sysml_view::Graph {
    let loaded = sysml_syntax::load(text).expect("parses");
    sysml_view::project(&loaded.roots, view, Mode::Extended)
}

#[test]
fn mixed_notation_sample_keeps_state_and_action_frames_separate() {
    let text = include_str!("../../../examples/showcase/notation.sysml");
    let state = project(text, View::StateTransition);
    assert!(!state.nodes.iter().any(|n| n.name == "SurveyCycle"));
    assert!(state.nodes.iter().any(|n| n.symbol == NodeSymbol::Initial));
    assert!(state.nodes.iter().any(|n| n.symbol == NodeSymbol::Done));
    let action = project(text, View::ActionFlow);
    assert!(!action.nodes.iter().any(|n| n.name == "Mission"));
    assert!(action
        .nodes
        .iter()
        .any(|n| n.symbol == NodeSymbol::Decision));
    for graph in [&state, &action] {
        let svg = sysml_view::render(graph, "notation");
        assert!(!svg.contains("NaN"));
    }
}

#[test]
fn nested_machine_geometry_has_one_frame_inside_its_real_parent() {
    let graph = project("package P { state def M { state idle; state active { entry action boot; state sampling; state transmitting; transition sampling then transmitting; } transition idle then active; } }", View::StateTransition);
    let geo = sysml_view::states_geometry(&graph, &states::groups_of(&graph));
    assert_eq!(geo.frames.len(), 2);
    let outer = geo.frames[0];
    let inner = geo.frames[1];
    assert_eq!(graph.nodes[inner.0].name, "active");
    assert!(inner.1 > outer.1 && inner.2 > outer.2);
    assert!(inner.1 + inner.3 < outer.1 + outer.3);
    assert!(inner.2 + inner.4 < outer.2 + outer.4);
    assert!(!geo.boxes.iter().any(|b| b.0 == inner.0));
    for &(i, x, y, w, h) in &geo.boxes {
        let parent = graph.nodes[i].parent.expect("machine");
        let &(_, fx, fy, fw, fh) = geo
            .frames
            .iter()
            .find(|b| b.0 == parent)
            .expect("parent geometry");
        assert!(x >= fx && y >= fy + states::frame_title(&graph.nodes[parent]));
        assert!(x + w <= fx + fw && y + h <= fy + fh);
    }
    let svg = sysml_view::render(&graph, "nested");
    assert!(svg.contains("state actions") && svg.contains("entry boot"));
}

#[test]
fn parallel_and_reverse_transitions_have_distinct_orthogonal_routes() {
    let graph = project("package P { state def M { state a; state b; transition one first a then b; transition two first a then b; transition back first b then a; } }", View::StateTransition);
    let geo = sysml_view::states_geometry(&graph, &states::groups_of(&graph));
    assert_eq!(geo.edges.len(), 3);
    let routes: Vec<_> = geo.edges.values().collect();
    assert_ne!(routes[0], routes[1]);
    let reversed: Vec<_> = routes[2].iter().copied().rev().collect();
    assert_ne!(routes[0], &reversed);
    for route in routes {
        assert!(route
            .windows(2)
            .all(|p| (p[1].0 - p[0].0).abs() < 0.01 || (p[1].1 - p[0].1).abs() < 0.01));
    }
}

#[test]
fn action_successions_follow_ranks_instead_of_source_order_on_a_circle() {
    let graph = project("package P { action def A { action finish; action middle; action startWork; first startWork then middle; first middle then finish; } }", View::ActionFlow);
    let geo = sysml_view::states_geometry(&graph, &states::groups_of(&graph));
    let y = |name| {
        geo.boxes
            .iter()
            .find(|b| graph.nodes[b.0].name == name)
            .expect("action")
            .2
    };
    assert!(y("startWork") < y("middle") && y("middle") < y("finish"));
}

#[test]
fn control_nodes_render_as_symbols_and_actors_have_room_for_the_figure() {
    let graph = project("package P { action def A { action work; decision choose; fork split; join sync; merge again; } }", View::ActionFlow);
    let svg = sysml_view::render(&graph, "controls");
    for symbol in ["decision", "fork", "join", "merge"] {
        assert!(svg.contains(&format!("data-symbol=\"{symbol}\"")), "{svg}");
    }
    let graph = project("package P { part def Person; use case def C { actor user : Person; stakeholder owner : Person; } }", View::General);
    let actor = graph
        .nodes
        .iter()
        .find(|n| n.symbol == NodeSymbol::Actor)
        .expect("actor");
    assert!(sysml_view::node_size(actor).1 >= 130.0);
    let stakeholder = graph
        .nodes
        .iter()
        .find(|n| n.name == "owner")
        .expect("stakeholder");
    assert_eq!(stakeholder.symbol, NodeSymbol::Rectangle);
}
