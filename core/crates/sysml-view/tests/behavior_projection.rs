use sysml_view::graph::{NodeSymbol, View};

fn project(text: &str, view: View) -> sysml_view::Graph {
    let loaded = sysml_syntax::load(text).expect("model parses");
    sysml_view::project(&loaded.roots, view, sysml_view::Mode::Extended)
}

#[test]
fn named_control_nodes_keep_their_symbols() {
    let graph = project(
        "package P { action def A { action work; decision choose; fork split; join sync; merge again; } }",
        View::ActionFlow,
    );
    for (name, symbol) in [
        ("choose", NodeSymbol::Decision),
        ("split", NodeSymbol::Fork),
        ("sync", NodeSymbol::Join),
        ("again", NodeSymbol::Merge),
    ] {
        let node = graph
            .nodes
            .iter()
            .find(|node| node.name == name)
            .expect(name);
        assert_eq!(node.symbol, symbol, "{name}");
    }
}

#[test]
fn first_names_a_succession_source_not_an_initial_node() {
    for (text, view) in [
        (
            "package P { action def A { action a; action b; first a; then b; } }",
            View::ActionFlow,
        ),
        (
            "package P { state def M { state a; state b; first a; then b; } }",
            View::StateTransition,
        ),
    ] {
        let graph = project(text, view);
        assert_eq!(graph.nodes.len(), 3, "{view:?}");
        assert!(!graph.nodes.iter().any(|node| node.name == "first"));
        assert!(!graph
            .nodes
            .iter()
            .any(|node| node.symbol == NodeSymbol::Initial));
        assert_eq!(graph.edges.len(), 1, "{view:?}");
        let edge = &graph.edges[0];
        assert_eq!(graph.nodes[edge.from].name, "a");
        assert_eq!(graph.nodes[edge.to].name, "b");
    }
}

#[test]
fn an_empty_state_entry_followed_by_a_succession_is_the_initial_node() {
    let graph = project(
        "package P { state def M { entry; then off; state off; state on; transition off then on; } }",
        View::StateTransition,
    );
    let initial = graph
        .nodes
        .iter()
        .position(|node| node.symbol == NodeSymbol::Initial)
        .expect("entry node");
    let edge = graph
        .edges
        .iter()
        .find(|edge| edge.from == initial)
        .expect("initial transition");
    assert_eq!(graph.nodes[edge.to].name, "off");
    assert!(graph.nodes[initial].element.is_some());
    assert!(!graph
        .nodes
        .iter()
        .any(|node| node.attributes.iter().any(|row| row == "entry /")));
}

#[test]
fn then_done_reaches_one_unbound_terminal_in_the_state_frame() {
    let graph = project(
        "package P { state def M { state a; then done; state b; then done; } }",
        View::StateTransition,
    );
    let terminals: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.symbol == NodeSymbol::Done)
        .collect();
    assert_eq!(terminals.len(), 1);
    let (terminal, node) = terminals[0];
    assert_eq!(node.symbol.keyword(), "done");
    assert_eq!(node.name, "done");
    assert_eq!(node.bare, "done");
    assert!(node.element.is_none());
    let parent = node.parent.expect("owning frame");
    assert!(graph.nodes[parent].children.contains(&terminal));
    assert_eq!(graph.edges.len(), 2);
    assert!(graph.edges.iter().all(|edge| edge.to == terminal));
}

#[test]
fn a_declared_done_state_is_not_replaced_by_a_terminal() {
    let graph = project(
        "package P { state def M { state a; then 'done'; state 'done'; } }",
        View::StateTransition,
    );
    let node = graph
        .nodes
        .iter()
        .find(|node| node.name == "done")
        .expect("declared state");
    assert_eq!(node.symbol, NodeSymbol::Rectangle);
    assert!(node.element.is_some());
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn action_done_does_not_synthesize_a_state_terminal() {
    let graph = project(
        "package P { action def A { action a; then done; } }",
        View::ActionFlow,
    );
    assert!(!graph
        .nodes
        .iter()
        .any(|node| node.symbol == NodeSymbol::Done));
}

#[test]
fn nested_states_are_frames_with_one_identity_and_their_real_parent() {
    let graph = project(
        "package P { state def M { state outer { state inner; state next; transition inner then next; } state idle; transition idle then outer; } }",
        View::StateTransition,
    );
    let outer: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name == "outer")
        .collect();
    assert_eq!(outer.len(), 1);
    let (outer_index, outer) = outer[0];
    assert!(outer.is_frame);
    let parent = outer.parent.expect("outer machine");
    assert_eq!(graph.nodes[parent].name, "M");
    let inner = graph
        .nodes
        .iter()
        .find(|node| node.name == "inner")
        .expect("inner state");
    assert_eq!(inner.parent, Some(outer_index));
    let groups = sysml_view::states::groups_of(&graph);
    assert_eq!(groups.len(), 2);
    assert!(groups
        .iter()
        .find(|group| group.node == parent)
        .unwrap()
        .members
        .contains(&outer_index));
    let identities: Vec<_> = graph.nodes.iter().filter_map(|node| node.element).collect();
    let unique: std::collections::BTreeSet<_> = identities.iter().collect();
    assert_eq!(identities.len(), unique.len());
}

#[test]
fn state_subactions_stay_in_compartment_rows() {
    let graph = project(
        "package P { state def M { state active { entry action boot; do action sample; exit action stop; } state idle; transition idle then active; } }",
        View::StateTransition,
    );
    let active = graph
        .nodes
        .iter()
        .find(|node| node.name == "active")
        .expect("active state");
    assert_eq!(active.attributes, ["entry boot", "do sample", "exit stop"]);
    assert!(!graph
        .nodes
        .iter()
        .any(|node| matches!(node.name.as_str(), "boot" | "sample" | "stop")));
    assert_eq!(sysml_view::states::frame_title(active), 84.0);
}

#[test]
fn only_actors_default_to_a_stick_figure() {
    let graph = project(
        "package P { part def Person; use case def C { actor user : Person; stakeholder owner : Person; } }",
        View::General,
    );
    let actor = graph
        .nodes
        .iter()
        .find(|node| node.name == "user")
        .expect("actor");
    let stakeholder = graph
        .nodes
        .iter()
        .find(|node| node.name == "owner")
        .expect("stakeholder");
    assert_eq!(actor.symbol, NodeSymbol::Actor);
    assert_eq!(stakeholder.symbol, NodeSymbol::Rectangle);
}

#[test]
fn a_transition_keeps_trigger_guard_and_effect_together() {
    let mut loaded = sysml_syntax::load(
        "package P { state def M { state idle; state active; transition begin first idle then active; } }",
    ).expect("model parses");
    let transition = loaded.roots[0].children[0]
        .children
        .iter_mut()
        .find(|element| element.kind == sysml_model::Kind::Transition)
        .expect("transition");
    transition.trigger_text = Some("accept command".to_string());
    transition.constraint = Some("ready".to_string());
    transition.effect_text = Some("beginSurvey".to_string());
    let graph = sysml_view::states::states(&loaded.roots).graph;
    assert_eq!(
        graph.edges[0].label.as_deref(),
        Some("accept command [ready] / beginSurvey")
    );
}
